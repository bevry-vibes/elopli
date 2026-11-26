use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use elopli_core::parse_duration;
use elopli_session::{LightDevice, LinkKind, LinkTarget, REFRESH_INTERVAL, SessionSnapshot};
use gtk::glib::value::ToValue;
use gtk::prelude::*;
use gtk::{gdk, gio, glib, pango};

use crate::autostart;
use crate::sheets::{self, Sheet, SheetKind};
use crate::slider::{Slider, SliderKind};
use crate::theme;
use crate::tray::{Tray, UiEvent};
use crate::worker::{Command, Worker};

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const PANEL_WIDTH: i32 = 460;
const PANEL_HEIGHT: i32 = 640;

thread_local! {
	static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) };
}

pub fn activate(application: &gtk::Application, start_hidden: bool) {
	if let Some(app) = APP.with(|slot| slot.borrow().clone()) {
		app.show();
		return;
	}

	gtk::Window::set_default_icon_name("com.bevry.elopli");
	theme::install();
	theme::install_icons();
	let (ui_tx, ui_rx) = mpsc::channel();
	match App::new(application, ui_tx) {
		Ok(app) => {
			APP.with(|slot| *slot.borrow_mut() = Some(Rc::clone(&app)));
			app.build();
			app.start_timers(ui_rx);
			if start_hidden {
				app.hide();
			} else {
				app.show();
			}
			if let Ok(path) = std::env::var("ELOPLI_SCREENSHOT") {
				let app = Rc::clone(&app);
				glib::timeout_add_local_once(Duration::from_millis(900), move || {
					app.screenshot(Path::new(&path));
					app.quit();
				});
			}
		}
		Err(error) => show_fatal(application, &error),
	}
}

fn show_fatal(application: &gtk::Application, message: &str) {
	let window = gtk::ApplicationWindow::builder()
		.application(application)
		.title("Elopli")
		.default_width(PANEL_WIDTH)
		.build();
	let label = gtk::Label::new(Some(&format!("Elopli could not start.\n\n{message}")));
	label.set_wrap(true);
	label.set_margin_top(24);
	label.set_margin_bottom(24);
	label.set_margin_start(24);
	label.set_margin_end(24);
	window.set_child(Some(&label));
	window.present();
}

pub(crate) struct App {
	application: gtk::Application,
	pub(crate) window: gtk::ApplicationWindow,
	header: gtk::HeaderBar,
	cards_box: gtk::Box,
	empty: gtk::Box,
	error_label: gtk::Label,
	footer: gtk::Label,
	worker: Worker,
	snapshot: RefCell<SessionSnapshot>,
	cards: RefCell<Vec<Rc<Card>>>,
	sheet: RefCell<Option<Rc<Sheet>>>,
	tray: RefCell<Option<Tray>>,
	autostart_action: RefCell<Option<gio::SimpleAction>>,
	local_error: RefCell<Option<String>>,
	ctrl: Cell<bool>,
	last_revision: Cell<u64>,
	quitting: Cell<bool>,
}

impl App {
	fn new(application: &gtk::Application, ui_tx: Sender<UiEvent>) -> Result<Rc<Self>, String> {
		let worker = Worker::start()?;
		let snapshot = worker.snapshot();

		let window = gtk::ApplicationWindow::builder()
			.application(application)
			.title("Elopli")
			.default_width(PANEL_WIDTH)
			.default_height(PANEL_HEIGHT)
			.build();
		window.add_css_class("elopli-panel");
		window.set_hide_on_close(true);
		window.set_resizable(false);
		window.set_icon_name(Some("com.bevry.elopli"));

		let header = gtk::HeaderBar::new();
		header.set_show_title_buttons(true);
		header.set_decoration_layout(Some(":close"));
		let title = gtk::Label::new(Some("Elopli"));
		title.add_css_class("elopli-title");
		title.set_xalign(0.0);
		title.set_margin_start(4);
		header.pack_start(&title);
		window.set_titlebar(Some(&header));

		let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
		let error_label = gtk::Label::new(None);
		error_label.add_css_class("elopli-error");
		error_label.set_wrap(true);
		error_label.set_xalign(0.0);
		error_label.set_margin_top(6);
		error_label.set_margin_start(12);
		error_label.set_margin_end(12);
		error_label.set_visible(false);
		outer.append(&error_label);

		let scrolled = gtk::ScrolledWindow::builder()
			.vexpand(true)
			.hscrollbar_policy(gtk::PolicyType::Never)
			.build();
		let cards_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
		cards_box.set_margin_top(6);
		cards_box.set_margin_bottom(6);
		cards_box.set_margin_start(12);
		cards_box.set_margin_end(12);
		scrolled.set_child(Some(&cards_box));
		outer.append(&scrolled);

		let footer = gtk::Label::new(Some("Ctrl-click to link · drag a card to reorder."));
		footer.add_css_class("elopli-hint");
		footer.set_xalign(0.0);
		footer.set_margin_top(4);
		footer.set_margin_bottom(10);
		footer.set_margin_start(12);
		footer.set_margin_end(12);
		outer.append(&footer);

		window.set_child(Some(&outer));

		let empty = gtk::Box::new(gtk::Orientation::Vertical, 8);
		empty.set_valign(gtk::Align::Center);
		empty.set_vexpand(true);
		empty.set_margin_top(40);
		empty.set_margin_bottom(40);
		let empty_title = gtk::Label::new(Some("No lights configured"));
		empty_title.set_margin_top(24);
		let empty_hint = gtk::Label::new(Some(
			"Add devices to the registry, or scan the local network.",
		));
		empty_hint.add_css_class("elopli-hint");
		empty_hint.set_wrap(true);
		empty_hint.set_justify(gtk::Justification::Center);
		empty_hint.set_margin_bottom(24);
		empty.append(&empty_title);
		empty.append(&empty_hint);

		let tray = Tray::start(ui_tx, autostart::enabled()).ok();
		let app = Rc::new(Self {
			application: application.clone(),
			window,
			header,
			cards_box,
			empty,
			error_label,
			footer,
			worker,
			snapshot: RefCell::new(snapshot),
			cards: RefCell::new(Vec::new()),
			sheet: RefCell::new(None),
			tray: RefCell::new(tray),
			autostart_action: RefCell::new(None),
			local_error: RefCell::new(None),
			ctrl: Cell::new(false),
			last_revision: Cell::new(0),
			quitting: Cell::new(false),
		});
		Ok(app)
	}

	fn build(self: &Rc<Self>) {
		self.install_actions();
		self.build_header();
		self.connect_window();
		self.sync_snapshot();
	}

	fn build_header(self: &Rc<Self>) {
		let refresh = tool_button("view-refresh-symbolic", "Refresh");
		{
			let app = Rc::downgrade(self);
			refresh.connect_clicked(move |_| {
				if let Some(app) = app.upgrade() {
					app.worker.send(Command::Refresh);
				}
			});
		}
		let all_on = tool_button("display-brightness-symbolic", "All On");
		{
			let app = Rc::downgrade(self);
			all_on.connect_clicked(move |_| {
				if let Some(app) = app.upgrade() {
					app.worker.send(Command::SetAllOn(true));
				}
			});
		}
		let all_off = tool_button("weather-clear-night-symbolic", "All Off");
		{
			let app = Rc::downgrade(self);
			all_off.connect_clicked(move |_| {
				if let Some(app) = app.upgrade() {
					app.worker.send(Command::SetAllOn(false));
				}
			});
		}
		let discover = tool_button("system-search-symbolic", "Discover Lights");
		{
			let app = Rc::downgrade(self);
			discover.connect_clicked(move |_| {
				if let Some(app) = app.upgrade() {
					app.open_sheet(SheetKind::Discovery);
					app.worker.send(Command::Scan);
				}
			});
		}

		let settings_menu = gio::Menu::new();
		settings_menu.append(Some("Launch at Login"), Some("app.autostart"));
		settings_menu.append(Some("Reveal Config"), Some("app.reveal-config"));
		settings_menu.append(Some("Quit Elopli"), Some("app.quit"));
		let settings = gtk::MenuButton::builder()
			.icon_name("emblem-system-symbolic")
			.tooltip_text("Settings")
			.menu_model(&settings_menu)
			.build();
		settings.add_css_class("flat");
		settings.set_size_request(30, 30);
		settings.set_direction(gtk::ArrowType::None);

		// Title on the left, actions centered, window controls on the right.
		let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
		actions.set_halign(gtk::Align::Center);
		actions.append(&refresh);
		actions.append(&discover);
		actions.append(&all_on);
		actions.append(&all_off);
		actions.append(&settings);
		self.header.set_title_widget(Some(&actions));
	}

	fn connect_window(self: &Rc<Self>) {
		{
			let app = Rc::downgrade(self);
			self.window.connect_close_request(move |_| {
				if let Some(app) = app.upgrade() {
					app.hide();
				}
				glib::Propagation::Stop
			});
		}

		let keys = gtk::EventControllerKey::new();
		{
			let app = Rc::downgrade(self);
			keys.connect_key_pressed(move |_, key, _, _| {
				let Some(app) = app.upgrade() else {
					return glib::Propagation::Proceed;
				};
				match key {
					gdk::Key::Escape => {
						if app.sheet.borrow().is_some() {
							app.close_sheet();
						} else {
							app.hide();
						}
						glib::Propagation::Stop
					}
					gdk::Key::Control_L | gdk::Key::Control_R => {
						app.ctrl.set(true);
						glib::Propagation::Proceed
					}
					_ => glib::Propagation::Proceed,
				}
			});
		}
		{
			let app = Rc::downgrade(self);
			keys.connect_key_released(move |_, key, _, _| {
				if matches!(key, gdk::Key::Control_L | gdk::Key::Control_R)
					&& let Some(app) = app.upgrade()
				{
					app.release_ctrl();
				}
			});
		}
		self.window.add_controller(keys);
	}

	fn install_actions(self: &Rc<Self>) {
		let autostart =
			gio::SimpleAction::new_stateful("autostart", None, &autostart::enabled().to_variant());
		{
			let app = Rc::downgrade(self);
			autostart.connect_activate(move |action, _| {
				let Some(app) = app.upgrade() else {
					return;
				};
				let next = !autostart::enabled();
				match autostart::set(next) {
					Ok(()) => {
						action.set_state(&next.to_variant());
						app.update_tray();
					}
					Err(error) => app.set_local_error(error),
				}
			});
		}
		self.application.add_action(&autostart);
		*self.autostart_action.borrow_mut() = Some(autostart);

		let reveal = gio::SimpleAction::new("reveal-config", None);
		{
			let app = Rc::downgrade(self);
			reveal.connect_activate(move |_, _| {
				if let Some(app) = app.upgrade() {
					app.reveal_config();
				}
			});
		}
		self.application.add_action(&reveal);

		let quit = gio::SimpleAction::new("quit", None);
		{
			let app = Rc::downgrade(self);
			quit.connect_activate(move |_, _| {
				if let Some(app) = app.upgrade() {
					app.quit();
				}
			});
		}
		self.application.add_action(&quit);
		self.application
			.set_accels_for_action("app.quit", &["<Control>q"]);

		let group = gio::SimpleActionGroup::new();
		for name in [
			"rename",
			"power-on",
			"pulse",
			"pulse-group",
			"sweep",
			"sweep-group",
		] {
			let action = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
			let app = Rc::downgrade(self);
			let name = name.to_string();
			action.connect_activate(move |_, parameter| {
				let Some(app) = app.upgrade() else {
					return;
				};
				let Some(device) = parameter.and_then(|value| value.get::<String>()) else {
					return;
				};
				let kind = match name.as_str() {
					"rename" => SheetKind::Rename { name: device },
					"power-on" => SheetKind::PowerOn { name: device },
					"pulse" => SheetKind::Pulse {
						names: vec![device],
					},
					"sweep" => SheetKind::Sweep {
						names: vec![device],
					},
					"pulse-group" => SheetKind::Pulse {
						names: app.grouped_names(&device),
					},
					"sweep-group" => SheetKind::Sweep {
						names: app.grouped_names(&device),
					},
					_ => return,
				};
				app.open_sheet(kind);
			});
			group.add_action(&action);
		}
		let identify = gio::SimpleAction::new("identify", Some(glib::VariantTy::STRING));
		{
			let app = Rc::downgrade(self);
			identify.connect_activate(move |_, parameter| {
				if let (Some(app), Some(device)) = (
					app.upgrade(),
					parameter.and_then(|value| value.get::<String>()),
				) {
					app.worker.send(Command::Identify(device));
				}
			});
		}
		group.add_action(&identify);
		self.window.insert_action_group("card", Some(&group));
	}

	fn start_timers(self: &Rc<Self>, ui_rx: Receiver<UiEvent>) {
		let weak = Rc::downgrade(self);
		glib::timeout_add_local(POLL_INTERVAL, move || {
			let Some(app) = weak.upgrade() else {
				return glib::ControlFlow::Break;
			};
			while let Ok(event) = ui_rx.try_recv() {
				app.handle_ui_event(event);
			}
			let revision = app.worker.revision();
			if revision != app.last_revision.get() {
				app.last_revision.set(revision);
				app.sync_snapshot();
			}
			app.sync_sheets();
			glib::ControlFlow::Continue
		});

		let weak = Rc::downgrade(self);
		glib::timeout_add_local(REFRESH_INTERVAL, move || {
			let Some(app) = weak.upgrade() else {
				return glib::ControlFlow::Break;
			};
			if app.window.is_visible() && app.sheet.borrow().is_none() {
				app.worker.send(Command::Refresh);
			}
			glib::ControlFlow::Continue
		});
	}

	fn handle_ui_event(self: &Rc<Self>, event: UiEvent) {
		match event {
			UiEvent::Show => self.show(),
			UiEvent::Toggle => self.toggle(),
			UiEvent::Refresh => self.worker.send(Command::Refresh),
			UiEvent::AllOn => self.worker.send(Command::SetAllOn(true)),
			UiEvent::AllOff => self.worker.send(Command::SetAllOn(false)),
			UiEvent::Discover => {
				self.open_sheet(SheetKind::Discovery);
				self.worker.send(Command::Scan);
			}
			UiEvent::ToggleAutostart => {
				if let Some(action) = self.autostart_action.borrow().as_ref() {
					action.activate(None);
				}
			}
			UiEvent::RevealConfig => self.reveal_config(),
			UiEvent::Quit => self.quit(),
		}
	}

	pub(crate) fn show(&self) {
		self.window.present();
		gtk::prelude::GtkWindowExt::set_focus(&self.window, None::<&gtk::Widget>);
		self.worker.send(Command::Refresh);
		self.ctrl.set(false);
	}

	pub(crate) fn hide(&self) {
		self.close_sheet();
		self.release_ctrl();
		self.window.set_visible(false);
		self.ctrl.set(false);
	}

	fn toggle(&self) {
		if self.window.is_visible() {
			self.hide();
		} else {
			self.show();
		}
	}

	pub(crate) fn quit(&self) {
		if self.quitting.replace(true) {
			return;
		}
		self.close_sheet();
		self.worker.send(Command::CancelAnimations(
			self.snapshot.borrow().pulsing.iter().cloned().collect(),
		));
		if let Some(tray) = self.tray.borrow_mut().take() {
			tray.shutdown();
		}
		self.worker.shutdown();
		self.application.quit();
	}

	fn sync_snapshot(self: &Rc<Self>) {
		let snapshot = self.worker.snapshot();
		let names = snapshot
			.devices
			.iter()
			.map(|device| device.name.clone())
			.collect::<Vec<_>>();
		let rebuild = {
			let cards = self.cards.borrow();
			cards.len() != names.len()
				|| cards
					.iter()
					.zip(&names)
					.any(|(card, name)| card.name != *name)
		};
		if rebuild {
			{
				let cards = self.cards.borrow();
				for card in cards.iter() {
					self.cards_box.remove(&card.root);
				}
			}
			let mut cards = Vec::new();
			for device in &snapshot.devices {
				let card = Card::new(self, device);
				self.cards_box.append(&card.root);
				cards.push(card);
			}
			*self.cards.borrow_mut() = cards;
		}
		if self.empty.parent().is_none() {
			self.cards_box.append(&self.empty);
		}
		self.empty.set_visible(snapshot.devices.is_empty());

		let cards = self.cards.borrow().clone();
		for card in cards {
			card.update(&snapshot);
		}
		self.resize_to_content();

		let error = snapshot
			.error
			.clone()
			.or_else(|| self.local_error.borrow().clone());
		self.error_label.set_text(error.as_deref().unwrap_or(""));
		self.error_label.set_visible(error.is_some());
		self.footer.set_text(if snapshot.devices.is_empty() {
			"Add devices to devices.json, or use Discover Lights."
		} else {
			"Ctrl-click to link · drag a card to reorder."
		});
		*self.snapshot.borrow_mut() = snapshot;
		self.update_tray();
	}

	fn resize_to_content(&self) {
		let count = self.cards.borrow().len();
		let height = if count == 0 {
			300
		} else {
			(130 + count as i32 * 165).clamp(260, PANEL_HEIGHT)
		};
		self.window.set_default_size(PANEL_WIDTH, height);
	}

	fn update_tray(&self) {
		if let Some(tray) = self.tray.borrow().as_ref() {
			let snapshot = self.snapshot.borrow();
			tray.update(
				autostart::enabled(),
				snapshot.devices.len(),
				snapshot.error.clone(),
			);
		}
	}

	fn sync_sheets(&self) {
		let sheet = self.sheet.borrow().clone();
		if let Some(sheet) = sheet {
			sheet.sync(self);
		}
	}

	pub(crate) fn open_sheet(self: &Rc<Self>, kind: SheetKind) {
		self.close_sheet();
		let sheet = sheets::open(self, kind);
		*self.sheet.borrow_mut() = Some(sheet);
	}

	pub(crate) fn close_sheet(&self) {
		let sheet = self.sheet.borrow_mut().take();
		if let Some(sheet) = sheet {
			sheet.cancel_animation(self);
			sheet.window.destroy();
		}
	}

	pub(crate) fn release_ctrl(&self) {
		if self.ctrl.replace(false) {
			// Always commit: the click that started the pending selection may
			// still be in flight, so the local snapshot can lag the UI.
			self.worker.send(Command::CommitLinks);
		}
	}

	pub(crate) fn toggle_link(&self, name: &str, kind: LinkKind) {
		self.worker.send(Command::ToggleLink {
			name: name.to_string(),
			kind,
		});
	}

	pub(crate) fn grouped_names(&self, name: &str) -> Vec<String> {
		self.snapshot
			.borrow()
			.groups
			.iter()
			.find(|group| group.kind == LinkKind::Card && group.contains(name))
			.map(|group| group.device_names.clone())
			.unwrap_or_else(|| vec![name.to_string()])
	}

	pub(crate) fn pulse_config(
		&self,
		count: u32,
		duration: &str,
		low: u8,
		peak: u8,
	) -> Result<elopli_core::PulseConfig, String> {
		let duration_ms = parse_duration(duration)
			.map_err(|_| "Invalid duration. Use 1.0s, 500ms, 10m, or 1h.".to_string())?;
		let minimum = count.saturating_mul(2).saturating_add(1) * 100;
		if duration_ms < minimum {
			return Err(format!(
				"Duration must be at least {minimum} ms for {count} pulses."
			));
		}
		let config = elopli_core::PulseConfig {
			count,
			duration_ms,
			value: peak,
			low,
		};
		config.validate().map_err(|error| error.to_string())?;
		Ok(config)
	}

	pub(crate) fn sweep_config(
		&self,
		count: u32,
		duration: &str,
		brightness: u8,
	) -> Result<elopli_core::SweepConfig, String> {
		let duration_ms = parse_duration(duration)
			.map_err(|_| "Invalid duration. Use 1.0s, 500ms, 10m, or 1h.".to_string())?;
		let minimum = count.saturating_mul(2).saturating_add(1) * 100;
		if duration_ms < minimum {
			return Err(format!(
				"Duration must be at least {minimum} ms for {count} sweeps."
			));
		}
		let config = elopli_core::SweepConfig {
			count,
			duration_ms,
			brightness: (brightness >= 2).then_some(brightness),
		};
		config.validate().map_err(|error| error.to_string())?;
		Ok(config)
	}

	pub(crate) fn command(&self, command: Command) {
		self.worker.send(command);
	}

	pub(crate) fn current_snapshot(&self) -> SessionSnapshot {
		self.snapshot.borrow().clone()
	}

	pub(crate) fn device(&self, name: &str) -> Option<LightDevice> {
		self.snapshot
			.borrow()
			.devices
			.iter()
			.find(|device| device.name == name)
			.cloned()
	}

	fn set_local_error(&self, message: impl Into<String>) {
		let message = message.into();
		self.error_label.set_text(&message);
		self.error_label.set_visible(true);
		*self.local_error.borrow_mut() = Some(message);
	}

	/// Render the panel to a PNG. This is a development and CI hook; the
	/// window manager never sees a screenshot request, so it works on Wayland.
	fn screenshot(&self, path: &Path) {
		use gtk::gdk::prelude::PaintableExt;

		let width = f64::from(self.window.width());
		let height = f64::from(self.window.height());
		let Some(native) = self.window.native() else {
			eprintln!("screenshot: the window is not realized");
			return;
		};
		let Some(renderer) = native.renderer() else {
			eprintln!("screenshot: the window has no renderer");
			return;
		};
		let paintable = gtk::WidgetPaintable::new(Some(&self.window));
		let snapshot = gtk::Snapshot::new();
		paintable.snapshot(&snapshot, width, height);
		let Some(node) = snapshot.to_node() else {
			eprintln!("screenshot: the window produced no render node");
			return;
		};
		let texture = renderer.render_texture(&node, None);
		if let Err(error) = texture.save_to_png(path) {
			eprintln!("screenshot: cannot write {}: {error}", path.display());
		}
	}

	fn reveal_config(&self) {
		let path = self.snapshot.borrow().config_path.clone();
		let target = if path.exists() {
			path
		} else {
			path.parent()
				.map(Path::to_path_buf)
				.unwrap_or_else(|| std::path::PathBuf::from("."))
		};
		let uri = match glib::filename_to_uri(&target, None) {
			Ok(uri) => uri.to_string(),
			Err(error) => {
				self.set_local_error(format!("cannot open {}: {error}", target.display()));
				return;
			}
		};
		if let Err(error) =
			gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>)
		{
			self.set_local_error(format!("cannot open {}: {error}", target.display()));
		}
	}
}

fn tool_button(icon: &str, tooltip: &str) -> gtk::Button {
	let button = gtk::Button::builder()
		.icon_name(icon)
		.tooltip_text(tooltip)
		.build();
	button.add_css_class("flat");
	button.set_size_request(30, 30);
	button
}

fn menu_item(label: &str, action: &str, target: &str) -> gio::MenuItem {
	let item = gio::MenuItem::new(Some(label), Some(action));
	item.set_attribute_value("target", Some(&target.to_variant()));
	item
}

fn is_linked(snapshot: &SessionSnapshot, name: &str, kind: LinkKind) -> bool {
	let in_card_group = snapshot
		.groups
		.iter()
		.any(|group| group.kind == LinkKind::Card && group.contains(name));
	if kind == LinkKind::Card {
		in_card_group
	} else if in_card_group {
		false
	} else {
		snapshot
			.groups
			.iter()
			.any(|group| group.kind == kind && group.contains(name))
	}
}

fn is_pending(snapshot: &SessionSnapshot, name: &str, kind: LinkKind) -> bool {
	snapshot
		.pending_links
		.contains(&LinkTarget::new(name, kind))
}

fn inside_linkable(widget: &gtk::Widget) -> bool {
	let mut current = Some(widget.clone());
	while let Some(widget) = current {
		if widget.has_css_class("elopli-card") {
			return false;
		}
		if widget.has_css_class("elopli-field")
			|| widget.is::<gtk::Button>()
			|| widget.is::<gtk::MenuButton>()
			|| widget.is::<gtk::Switch>()
		{
			return true;
		}
		current = widget.parent();
	}
	false
}

fn apply_link_class(widget: &impl IsA<gtk::Widget>, linked: bool, pending: bool, prefix: &str) {
	let linked_class = format!("{prefix}-linked");
	let pending_class = format!("{prefix}-pending");
	widget.remove_css_class(&linked_class);
	widget.remove_css_class(&pending_class);
	if pending {
		widget.add_css_class(&pending_class);
	} else if linked {
		widget.add_css_class(&linked_class);
	}
}

pub(crate) struct Card {
	name: String,
	root: gtk::Box,
	title: gtk::Label,
	subtitle: gtk::Label,
	power_wrap: gtk::Box,
	power: gtk::Switch,
	wifi: gtk::Button,
	brightness_wrap: gtk::Box,
	brightness: Slider,
	brightness_label: gtk::Label,
	warmth_wrap: gtk::Box,
	warmth: Slider,
	warmth_label: gtk::Label,
	temperature: gtk::Label,
	error: gtk::Label,
	menu: gtk::MenuButton,
	grouped: Cell<Option<bool>>,
	updating: Cell<bool>,
}

impl Card {
	fn new(app: &Rc<App>, device: &LightDevice) -> Rc<Self> {
		let name = device.name.clone();
		let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
		root.add_css_class("elopli-card");
		let pressed_on_control = Rc::new(Cell::new(false));
		{
			let weak = Rc::downgrade(app);
			let device_name = name.clone();
			let root_weak = root.downgrade();
			let pressed_on_control = Rc::clone(&pressed_on_control);
			let click = gtk::GestureClick::new();
			click.set_button(1);
			click.connect_pressed(move |gesture, _, x, y| {
				let Some(root) = root_weak.upgrade() else {
					return;
				};
				let control = root
					.pick(x, y, gtk::PickFlags::DEFAULT)
					.is_some_and(|picked| inside_linkable(&picked));
				pressed_on_control.set(control);
				let ctrl = gesture
					.current_event_state()
					.contains(gdk::ModifierType::CONTROL_MASK);
				if ctrl && !control {
					if let Some(app) = weak.upgrade() {
						app.toggle_link(&device_name, LinkKind::Card);
						gesture.set_state(gtk::EventSequenceState::Claimed);
					}
				} else {
					// Leave the sequence to the slider, switch, or drag source.
					gesture.set_state(gtk::EventSequenceState::Denied);
				}
			});
			root.add_controller(click);
		}

		let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
		content.set_margin_top(10);
		content.set_margin_bottom(10);
		content.set_margin_start(12);
		content.set_margin_end(12);
		root.append(&content);

		let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
		let title_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
		title_box.set_hexpand(true);
		title_box.set_valign(gtk::Align::Center);
		let title_text = gtk::Box::new(gtk::Orientation::Vertical, 0);
		title_text.set_hexpand(true);
		let title = gtk::Label::new(None);
		title.set_xalign(0.0);
		title.set_ellipsize(pango::EllipsizeMode::End);
		title.set_width_chars(12);
		let subtitle = gtk::Label::new(None);
		subtitle.set_xalign(0.0);
		subtitle.set_ellipsize(pango::EllipsizeMode::End);
		subtitle.add_css_class("elopli-subtitle");
		title_text.append(&title);
		title_text.append(&subtitle);
		title_box.append(&title_text);
		top.append(&title_box);

		{
			let source = gtk::DragSource::new();
			source.set_actions(gdk::DragAction::MOVE);
			let drag_name = name.clone();
			let pressed_on_control = Rc::clone(&pressed_on_control);
			source.connect_prepare(move |_, _, _| {
				if pressed_on_control.get() {
					return None;
				}
				Some(gdk::ContentProvider::for_value(&drag_name.to_value()))
			});
			root.add_controller(source);
		}

		let target = gtk::DropTarget::new(String::static_type(), gdk::DragAction::MOVE);
		{
			let weak = Rc::downgrade(app);
			let onto = name.clone();
			let root = root.downgrade();
			target.connect_drop(move |_, value, _, _| {
				let Ok(dragged) = value.get::<String>() else {
					return false;
				};
				if let Some(root) = root.upgrade() {
					root.remove_css_class("elopli-card-drop");
				}
				if dragged == onto {
					return false;
				}
				if let Some(app) = weak.upgrade() {
					app.command(Command::Move {
						name: dragged,
						onto: onto.clone(),
					});
					return true;
				}
				false
			});
		}
		{
			let root = root.downgrade();
			target.connect_enter(move |_, _, _| {
				if let Some(root) = root.upgrade() {
					root.add_css_class("elopli-card-drop");
				}
				gdk::DragAction::MOVE
			});
		}
		{
			let root = root.downgrade();
			target.connect_leave(move |_| {
				if let Some(root) = root.upgrade() {
					root.remove_css_class("elopli-card-drop");
				}
			});
		}
		root.add_controller(target);

		let wifi = gtk::Button::builder()
			.icon_name("network-wireless-symbolic")
			.tooltip_text("WiFi details")
			.build();
		wifi.add_css_class("flat");
		wifi.set_size_request(30, 30);
		wifi.set_valign(gtk::Align::Center);
		top.append(&wifi);

		let power_wrap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
		power_wrap.add_css_class("elopli-field");
		power_wrap.set_valign(gtk::Align::Center);
		let power = gtk::Switch::new();
		power.set_valign(gtk::Align::Center);
		power_wrap.append(&power);
		{
			let weak = Rc::downgrade(app);
			let device_name = name.clone();
			let link = gtk::GestureClick::new();
			link.set_button(1);
			link.connect_pressed(move |gesture, _, _, _| {
				if gesture
					.current_event_state()
					.contains(gdk::ModifierType::CONTROL_MASK)
					&& let Some(app) = weak.upgrade()
				{
					app.toggle_link(&device_name, LinkKind::Power);
					gesture.set_state(gtk::EventSequenceState::Claimed);
				}
			});
			power_wrap.add_controller(link);
		}
		{
			let weak = Rc::downgrade(app);
			let device_name = name.clone();
			power.connect_active_notify(move |switch| {
				let Some(app) = weak.upgrade() else {
					return;
				};
				if app
					.cards
					.borrow()
					.iter()
					.find(|card| card.name == device_name)
					.is_some_and(|card| card.updating.get())
				{
					return;
				}
				app.command(Command::SetOn {
					name: device_name.clone(),
					on: switch.is_active(),
				});
			});
		}
		top.append(&power_wrap);

		let menu = gtk::MenuButton::builder()
			.icon_name("view-more-symbolic")
			.tooltip_text("Light menu")
			.build();
		menu.add_css_class("flat");
		menu.set_size_request(28, 30);
		menu.set_direction(gtk::ArrowType::None);
		menu.set_valign(gtk::Align::Center);
		top.append(&menu);
		content.append(&top);

		let rows = gtk::Box::new(gtk::Orientation::Vertical, 6);

		let (brightness_wrap, brightness, brightness_label) = slider_row(
			"display-brightness-symbolic",
			SliderKind::Brightness,
			device.display_brightness(),
		);
		rows.append(&brightness_wrap);

		let (warmth_wrap, warmth, warmth_label) = slider_row(
			"com.bevry.elopli-flame-symbolic",
			SliderKind::Warmth,
			device.display_warmth(),
		);
		rows.append(&warmth_wrap);

		let temperature = gtk::Label::new(None);
		temperature.add_css_class("elopli-temp");
		temperature.set_xalign(0.0);
		rows.append(&temperature);

		let error = gtk::Label::new(None);
		error.add_css_class("elopli-error");
		error.set_xalign(0.0);
		error.set_wrap(true);
		error.set_visible(false);
		rows.append(&error);
		content.append(&rows);

		{
			let weak = Rc::downgrade(app);
			let device_name = name.clone();
			let label = brightness_label.clone();
			brightness.set_on_change(move |value| {
				label.set_text(&format!("{value}%"));
				if let Some(app) = weak.upgrade() {
					app.command(Command::QueueBrightness {
						name: device_name.clone(),
						value,
					});
				}
			});
		}
		{
			let weak = Rc::downgrade(app);
			let device_name = name.clone();
			brightness.set_on_link(move || {
				if let Some(app) = weak.upgrade() {
					app.toggle_link(&device_name, LinkKind::Brightness);
				}
			});
		}
		{
			let weak = Rc::downgrade(app);
			let device_name = name.clone();
			let brightness = brightness.clone();
			let label = warmth_label.clone();
			warmth.set_on_change(move |value| {
				brightness.set_warmth(value);
				label.set_text(&format!("{value}%"));
				if let Some(app) = weak.upgrade() {
					app.command(Command::QueueWarmth {
						name: device_name.clone(),
						value,
					});
				}
			});
		}
		{
			let weak = Rc::downgrade(app);
			let device_name = name.clone();
			warmth.set_on_link(move || {
				if let Some(app) = weak.upgrade() {
					app.toggle_link(&device_name, LinkKind::Warmth);
				}
			});
		}

		Rc::new(Self {
			name,
			root,
			title,
			subtitle,
			power_wrap,
			power,
			wifi,
			brightness_wrap,
			brightness,
			brightness_label,
			warmth_wrap,
			warmth,
			warmth_label,
			temperature,
			error,
			menu,
			grouped: Cell::new(None),
			updating: Cell::new(false),
		})
	}

	fn update(&self, snapshot: &SessionSnapshot) {
		let Some(device) = snapshot
			.devices
			.iter()
			.find(|device| device.name == self.name)
		else {
			return;
		};
		self.updating.set(true);

		self.title.set_text(device.title());
		self.subtitle.set_text(&device.subtitle());
		if let Some(info) = &device.info
			&& !info.serial_number.is_empty()
		{
			self.subtitle
				.set_tooltip_text(Some(info.serial_number.as_str()));
		} else {
			self.subtitle.set_tooltip_text(None);
		}

		self.power.set_active(device.is_effectively_on());

		let brightness_value = snapshot
			.live_brightness
			.get(&self.name)
			.copied()
			.unwrap_or_else(|| device.display_brightness());
		let warmth_value = snapshot
			.live_warmth
			.get(&self.name)
			.copied()
			.unwrap_or_else(|| device.display_warmth());

		if !self.brightness.is_dragging() {
			self.brightness.set_value(brightness_value);
		}
		if !self.warmth.is_dragging() {
			self.warmth.set_value(warmth_value);
		}
		self.brightness.set_warmth(warmth_value);
		let dragging = self.brightness.is_dragging() || self.warmth.is_dragging();
		self.brightness
			.set_active(device.is_effectively_on() || dragging);
		self.warmth.set_active(true);

		let brightness_off = device
			.state
			.is_some_and(|state| state.on && state.brightness < 2)
			&& !snapshot.live_brightness.contains_key(&self.name);
		let brightness_text = if brightness_off {
			"off".to_string()
		} else {
			format!("{brightness_value}%")
		};
		self.brightness_label.set_text(&brightness_text);
		self.warmth_label.set_text(&format!("{warmth_value}%"));

		if let Some(state) = device.state {
			self.temperature.set_text(&format!(
				"{} mired · {} K",
				state.temperature,
				state.kelvin()
			));
			self.temperature.set_visible(true);
		} else {
			self.temperature.set_visible(false);
		}

		for class in [
			"elopli-wifi-good",
			"elopli-wifi-fair",
			"elopli-wifi-weak",
			"elopli-wifi-offline",
		] {
			self.wifi.remove_css_class(class);
		}
		if let Some(error) = &device.error {
			self.error.set_text(error);
			self.error.set_visible(true);
			self.wifi.set_icon_name("network-wireless-offline-symbolic");
			self.wifi.add_css_class("elopli-wifi-offline");
			self.wifi.set_tooltip_text(Some(&format!(
				"{} · {}\n{error}\nPower-cycle the light, wait for it to reconnect, then refresh.",
				device.title(),
				device.host
			)));
		} else {
			self.error.set_visible(false);
			self.wifi.set_icon_name("network-wireless-symbolic");
			match device
				.info
				.as_ref()
				.and_then(|info| info.wifi.as_ref())
				.map(|wifi| wifi.quality())
			{
				Some("excellent" | "good") => self.wifi.add_css_class("elopli-wifi-good"),
				Some("fair") => self.wifi.add_css_class("elopli-wifi-fair"),
				Some(_) => self.wifi.add_css_class("elopli-wifi-weak"),
				None => {}
			}
			let details = match &device.info {
				Some(info) => {
					let wifi = info.wifi.as_ref().map_or_else(
						|| "WiFi details unavailable".to_string(),
						|wifi| {
							format!(
								"{} · {} MHz · RSSI {} dBm · {}",
								wifi.ssid,
								wifi.frequency_mhz,
								wifi.rssi,
								wifi.quality()
							)
						},
					);
					format!(
						"{}\n{}\n{}\n{}",
						device.title(),
						info.product_name,
						wifi,
						device.host
					)
				}
				None => format!("{}\n{}", device.title(), device.host),
			};
			self.wifi.set_tooltip_text(Some(&details));
		}

		let linked_power = is_linked(snapshot, &self.name, LinkKind::Power);
		let linked_brightness = is_linked(snapshot, &self.name, LinkKind::Brightness);
		let linked_warmth = is_linked(snapshot, &self.name, LinkKind::Warmth);
		apply_link_class(
			&self.root,
			is_linked(snapshot, &self.name, LinkKind::Card),
			is_pending(snapshot, &self.name, LinkKind::Card),
			"elopli-card",
		);
		apply_link_class(
			&self.power_wrap,
			linked_power,
			is_pending(snapshot, &self.name, LinkKind::Power),
			"elopli-field",
		);
		apply_link_class(
			&self.brightness_wrap,
			linked_brightness,
			is_pending(snapshot, &self.name, LinkKind::Brightness),
			"elopli-field",
		);
		apply_link_class(
			&self.warmth_wrap,
			linked_warmth,
			is_pending(snapshot, &self.name, LinkKind::Warmth),
			"elopli-field",
		);

		let grouped = snapshot
			.groups
			.iter()
			.any(|group| group.kind == LinkKind::Card && group.contains(&self.name));
		if self.grouped.get() != Some(grouped) {
			self.grouped.set(Some(grouped));
			self.menu.set_menu_model(Some(&self.menu_model(grouped)));
		}

		self.updating.set(false);
		self.align_power_controls();
	}

	/// Align the on knob with the sliders' rightmost thumb and keep equal
	/// gaps between the WiFi button, the power switch, and the card menu.
	fn align_power_controls(&self) {
		let Some((slider_x, _)) = self
			.brightness
			.widget()
			.translate_coordinates(&self.root, 0.0, 0.0)
		else {
			return;
		};
		let Some((switch_x, _)) = self.power.translate_coordinates(&self.root, 0.0, 0.0) else {
			return;
		};
		let slider_width = f64::from(self.brightness.widget().width());
		let switch_width = f64::from(self.power.width());
		let switch_height = f64::from(self.power.height());
		if slider_width <= 0.0 || switch_width <= 0.0 {
			return;
		}
		let thumb = slider_x + slider_width - crate::slider::INSET;
		let knob = switch_x + switch_width - switch_height / 2.0;
		let current = f64::from(self.power_wrap.margin_end());
		let target = (current + knob - thumb).max(0.0).round();
		if (target - current).abs() <= 0.5 {
			return;
		}
		self.power_wrap.set_margin_end(target as i32);
		self.wifi.set_margin_end(target as i32);
	}

	fn menu_model(&self, grouped: bool) -> gio::Menu {
		let menu = gio::Menu::new();
		menu.append_item(&menu_item("Rename…", "card.rename", &self.name));
		menu.append_item(&menu_item(
			"Power On Behaviour…",
			"card.power-on",
			&self.name,
		));
		menu.append_item(&menu_item("Pulse…", "card.pulse", &self.name));
		if grouped {
			menu.append_item(&menu_item("Pulse Group…", "card.pulse-group", &self.name));
		}
		menu.append_item(&menu_item("Sweep…", "card.sweep", &self.name));
		if grouped {
			menu.append_item(&menu_item("Sweep Group…", "card.sweep-group", &self.name));
		}
		menu.append_item(&menu_item("Identify", "card.identify", &self.name));
		menu
	}
}

fn slider_row(icon: &str, slider_kind: SliderKind, value: u8) -> (gtk::Box, Slider, gtk::Label) {
	let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
	row.add_css_class("elopli-field");
	let image = gtk::Image::from_icon_name(icon);
	image.set_pixel_size(16);
	image.set_valign(gtk::Align::Center);
	row.append(&image);

	let value_label = gtk::Label::new(Some(&format!("{value}%")));
	value_label.add_css_class("elopli-value");
	value_label.set_width_chars(4);
	value_label.set_xalign(1.0);

	let slider = Slider::new(slider_kind, value);
	slider.widget().set_hexpand(true);
	row.append(slider.widget());
	row.append(&value_label);
	(row, slider, value_label)
}
