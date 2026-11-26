use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use elopli_core::{PowerOnBehavior, warmth_to_mired};
use elopli_session::{DiscoveryRow, DiscoveryState};
use gtk::prelude::*;
use gtk::{gdk, glib};

use crate::app::App;
use crate::slider::{RangeSlider, Slider, SliderKind};
use crate::worker::Command;

#[derive(Debug, Clone)]
pub enum SheetKind {
	Rename { name: String },
	PowerOn { name: String },
	Pulse { names: Vec<String> },
	Sweep { names: Vec<String> },
	Discovery,
}

pub struct Sheet {
	pub window: gtk::Window,
	content: Content,
}

enum Content {
	Rename,
	PowerOn,
	Pulse(PulseContent),
	Sweep(SweepContent),
	Discovery(DiscoveryContent),
}

struct PulseContent {
	names: Vec<String>,
	message: gtk::Label,
	primary: gtk::Button,
	started: Rc<Cell<bool>>,
	stopping: Rc<Cell<bool>>,
	seen: Rc<Cell<bool>>,
	started_at: Rc<Cell<Option<Instant>>>,
}

struct SweepContent {
	names: Vec<String>,
	message: gtk::Label,
	primary: gtk::Button,
	started: Rc<Cell<bool>>,
	stopping: Rc<Cell<bool>>,
	seen: Rc<Cell<bool>>,
	started_at: Rc<Cell<Option<Instant>>>,
}

struct DiscoveryContent {
	app: Weak<App>,
	list: gtk::ListBox,
	spinner: gtk::Spinner,
	scan: gtk::Button,
	empty: gtk::Label,
	fingerprint: RefCell<String>,
}

impl Sheet {
	pub fn sync(&self, app: &App) {
		match &self.content {
			Content::Pulse(content) => content.sync(app),
			Content::Sweep(content) => content.sync(app),
			Content::Discovery(content) => content.sync(app),
			Content::Rename | Content::PowerOn => {}
		}
	}

	pub fn cancel_animation(&self, app: &App) {
		match &self.content {
			Content::Pulse(content) => {
				if content.started.get() && !content.stopping.get() {
					app.command(Command::CancelAnimations(content.names.clone()));
				}
			}
			Content::Sweep(content) => {
				if content.started.get() && !content.stopping.get() {
					app.command(Command::CancelAnimations(content.names.clone()));
				}
			}
			Content::Rename | Content::PowerOn | Content::Discovery(_) => {}
		}
	}
}

pub fn open(app: &Rc<App>, kind: SheetKind) -> Rc<Sheet> {
	match kind {
		SheetKind::Rename { name } => open_rename(app, &name),
		SheetKind::PowerOn { name } => open_power_on(app, &name),
		SheetKind::Pulse { names } => open_pulse(app, names),
		SheetKind::Sweep { names } => open_sweep(app, names),
		SheetKind::Discovery => open_discovery(app),
	}
}

impl PulseContent {
	fn sync(&self, app: &App) {
		if !self.started.get() {
			return;
		}
		let snapshot = app.current_snapshot();
		let pulsing = self
			.names
			.iter()
			.any(|name| snapshot.pulsing.contains(name));
		if pulsing {
			self.seen.set(true);
		}
		if !pulsing && self.stopping.get() {
			self.finish("Stopped.");
			return;
		}
		if !pulsing && self.seen.get() {
			self.finish("Done.");
			return;
		}
		if !pulsing
			&& !self.seen.get()
			&& self
				.started_at
				.get()
				.is_some_and(|at| at.elapsed() > Duration::from_secs(2))
		{
			self.finish("Done.");
		}
	}

	fn finish(&self, message: &str) {
		self.started.set(false);
		self.stopping.set(false);
		self.seen.set(false);
		self.started_at.set(None);
		self.message.set_text(message);
		self.primary.set_label("Pulse");
		self.primary.set_sensitive(true);
	}
}

impl SweepContent {
	fn sync(&self, app: &App) {
		if !self.started.get() {
			return;
		}
		let snapshot = app.current_snapshot();
		let pulsing = self
			.names
			.iter()
			.any(|name| snapshot.pulsing.contains(name));
		if pulsing {
			self.seen.set(true);
		}
		if !pulsing && self.stopping.get() {
			self.finish("Stopped.");
			return;
		}
		if !pulsing && self.seen.get() {
			self.finish("Done.");
			return;
		}
		if !pulsing
			&& !self.seen.get()
			&& self
				.started_at
				.get()
				.is_some_and(|at| at.elapsed() > Duration::from_secs(2))
		{
			self.finish("Done.");
		}
	}

	fn finish(&self, message: &str) {
		self.started.set(false);
		self.stopping.set(false);
		self.seen.set(false);
		self.started_at.set(None);
		self.message.set_text(message);
		self.primary.set_label("Sweep");
		self.primary.set_sensitive(true);
	}
}

impl DiscoveryContent {
	fn sync(&self, app: &App) {
		let snapshot = app.current_snapshot();
		let fingerprint = snapshot
			.discovery_rows
			.iter()
			.map(|row| {
				format!(
					"{}|{}|{}|{:?}",
					row.id, row.name, row.needs_update, row.state
				)
			})
			.collect::<Vec<_>>()
			.join("\n");
		if *self.fingerprint.borrow() != fingerprint {
			*self.fingerprint.borrow_mut() = fingerprint;
			while let Some(child) = self.list.first_child() {
				self.list.remove(&child);
			}
			self.empty.set_visible(snapshot.discovery_rows.is_empty());
			for row in &snapshot.discovery_rows {
				self.list.append(&discovery_row(row, &self.app));
			}
		}
		self.spinner.set_spinning(snapshot.scanning);
		self.spinner.set_visible(snapshot.scanning);
		self.scan.set_sensitive(!snapshot.scanning);
	}
}

fn discovery_row(row: &DiscoveryRow, app: &Weak<App>) -> gtk::Box {
	let line = gtk::Box::new(gtk::Orientation::Horizontal, 10);
	line.set_margin_top(4);
	line.set_margin_bottom(4);
	line.set_margin_start(4);
	line.set_margin_end(4);

	let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
	text.set_hexpand(true);
	let name = gtk::Label::new(Some(&row.name));
	name.set_xalign(0.0);
	name.set_ellipsize(gtk::pango::EllipsizeMode::End);
	let detail = gtk::Label::new(Some(&row.detail));
	detail.set_xalign(0.0);
	detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
	detail.add_css_class("elopli-discovery-detail");
	text.append(&name);
	text.append(&detail);
	line.append(&text);

	let button = gtk::Button::new();
	match row.state {
		DiscoveryState::New => {
			button.set_label("Add");
			button.add_css_class("suggested-action");
			if let Some(device) = row.discovered.clone() {
				let app = app.clone();
				button.connect_clicked(move |_| {
					if let Some(app) = app.upgrade() {
						app.command(Command::AddDiscovered(device.clone()));
					}
				});
			}
		}
		DiscoveryState::Added if row.needs_update => {
			button.set_label("Update");
			if let Some(device) = row.discovered.clone() {
				let app = app.clone();
				button.connect_clicked(move |_| {
					if let Some(app) = app.upgrade() {
						app.command(Command::UpdateDiscovered(device.clone()));
					}
				});
			}
		}
		DiscoveryState::Added => {
			button.set_label("Added");
			button.set_sensitive(false);
		}
		DiscoveryState::Missing => {
			button.set_label("Missing");
			button.add_css_class("elopli-missing");
			button.set_sensitive(false);
			button.set_tooltip_text(Some(
				"Power-cycle the light, wait for it to reconnect, then scan again.",
			));
		}
	}
	line.append(&button);
	line
}

fn open_rename(app: &Rc<App>, name: &str) -> Rc<Sheet> {
	let (window, body, buttons) = sheet_window(app, "Rename Light");
	let current = app
		.device(name)
		.and_then(|device| device.info)
		.map(|info| info.display_name.trim().to_string())
		.filter(|value| !value.is_empty())
		.unwrap_or_else(|| name.to_string());

	let label = gtk::Label::new(Some("Display name"));
	label.set_xalign(0.0);
	body.append(&label);
	let hint = gtk::Label::new(Some(
		"The name is stored on the light and follows it to every Elopli interface.",
	));
	hint.add_css_class("elopli-hint");
	hint.set_xalign(0.0);
	hint.set_wrap(true);
	body.append(&hint);
	let entry = gtk::Entry::new();
	entry.set_text(&current);
	entry.set_activates_default(true);
	entry.set_max_length(elopli_core::DISPLAY_NAME_MAX_BYTES as i32);
	body.append(&entry);

	let message = message_label();
	body.append(&message);

	let cancel = gtk::Button::with_label("Cancel");
	let save = gtk::Button::with_label("Save");
	save.add_css_class("suggested-action");
	buttons.append(&cancel);
	buttons.append(&save);
	window.set_default_widget(Some(&save));

	{
		let weak = Rc::downgrade(app);
		cancel.connect_clicked(move |_| {
			if let Some(app) = weak.upgrade() {
				app.close_sheet();
			}
		});
	}
	{
		let weak = Rc::downgrade(app);
		let entry = entry.clone();
		let message = message.clone();
		let name = name.to_string();
		save.connect_clicked(move |_| {
			let text = entry.text().to_string();
			match elopli_core::validate_display_name(&text) {
				Ok(value) => {
					if let Some(app) = weak.upgrade() {
						app.command(Command::Rename {
							name: name.clone(),
							new_name: value,
						});
						app.close_sheet();
					}
				}
				Err(error) => message.set_text(&error.to_string()),
			}
		});
	}

	let sheet = Rc::new(Sheet {
		window: window.clone(),
		content: Content::Rename,
	});
	present(&window, app);
	sheet
}

fn open_power_on(app: &Rc<App>, name: &str) -> Rc<Sheet> {
	let (window, body, buttons) = sheet_window(app, "Power On Behaviour");
	let settings = app.device(name).and_then(|device| device.power_on);
	let restore_defaults = gtk::CheckButton::with_label("Restore defaults");
	let restore_last = gtk::CheckButton::with_label("Restore last setting");
	restore_last.set_group(Some(&restore_defaults));
	let defaults = settings
		.map(|settings| settings.behavior == PowerOnBehavior::RestoreDefaults)
		.unwrap_or(false);
	restore_defaults.set_active(defaults);
	restore_last.set_active(!defaults);
	body.append(&restore_last);
	body.append(&restore_defaults);

	let default_row = gtk::Box::new(gtk::Orientation::Vertical, 10);
	default_row.set_margin_top(6);

	let brightness_value = settings.map_or(20, |settings| settings.brightness.max(2));
	let (brightness_row, brightness, brightness_label) =
		labelled_slider("Brightness", SliderKind::Brightness, brightness_value);
	default_row.append(&brightness_row);

	let warmth_value = settings.map_or(35, |settings| settings.warmth());
	let (warmth_row, warmth, warmth_label) =
		labelled_slider("Warmth", SliderKind::Warmth, warmth_value);
	default_row.append(&warmth_row);
	body.append(&default_row);
	default_row.set_sensitive(defaults);

	brightness.set_on_change({
		let label = brightness_label.clone();
		move |value| label.set_text(&format!("{value}%"))
	});
	warmth.set_on_change({
		let label = warmth_label.clone();
		let brightness = brightness.clone();
		move |value| {
			brightness.set_warmth(value);
			label.set_text(&format!("{value}%"));
		}
	});
	{
		let default_row = default_row.clone();
		restore_defaults.connect_toggled(move |button| {
			default_row.set_sensitive(button.is_active());
		});
	}

	let cancel = gtk::Button::with_label("Cancel");
	let save = gtk::Button::with_label("Save");
	save.add_css_class("suggested-action");
	buttons.append(&cancel);
	buttons.append(&save);
	window.set_default_widget(Some(&save));

	{
		let weak = Rc::downgrade(app);
		cancel.connect_clicked(move |_| {
			if let Some(app) = weak.upgrade() {
				app.close_sheet();
			}
		});
	}
	{
		let weak = Rc::downgrade(app);
		let restore_defaults = restore_defaults.clone();
		let brightness = brightness.clone();
		let warmth = warmth.clone();
		let name = name.to_string();
		save.connect_clicked(move |_| {
			let defaults = restore_defaults.is_active();
			let brightness = brightness.value();
			let warmth = warmth.value();
			if let Some(app) = weak.upgrade() {
				app.command(Command::SetPowerOn {
					name: name.clone(),
					behavior: if defaults {
						PowerOnBehavior::RestoreDefaults
					} else {
						PowerOnBehavior::RestoreLast
					},
					brightness: defaults.then_some(brightness),
					temperature: if defaults {
						warmth_to_mired(f64::from(warmth)).ok()
					} else {
						None
					},
				});
				app.close_sheet();
			}
		});
	}

	let sheet = Rc::new(Sheet {
		window: window.clone(),
		content: Content::PowerOn,
	});
	present(&window, app);
	sheet
}

fn open_pulse(app: &Rc<App>, names: Vec<String>) -> Rc<Sheet> {
	let title = if names.len() > 1 {
		"Pulse Group"
	} else {
		"Pulse"
	};
	let (window, body, buttons) = sheet_window(app, title);
	if names.len() > 1 {
		let detail = gtk::Label::new(Some(&names.join(", ")));
		detail.add_css_class("elopli-hint");
		detail.set_xalign(0.0);
		body.append(&detail);
	}

	let count = gtk::SpinButton::with_range(1.0, 20.0, 1.0);
	count.set_value(3.0);
	count.set_digits(0);
	body.append(&labelled("Count", &count));

	let duration = gtk::Entry::new();
	duration.set_text("1.0s");
	duration.set_activates_default(true);
	body.append(&labelled("Duration", &duration));

	let range_label = gtk::Label::new(Some("Low 0% · Peak 100%"));
	range_label.set_xalign(0.0);
	range_label.add_css_class("elopli-hint");
	let range = RangeSlider::new(0, 100, {
		let label = range_label.clone();
		move |low, peak| label.set_text(&format!("Low {low}% · Peak {peak}%"))
	});
	body.append(&range_label);
	body.append(range.widget());

	let message = message_label();
	message.set_text("A count of 3 means three peak-to-low fades.");
	body.append(&message);

	let close = gtk::Button::with_label("Close");
	let primary = gtk::Button::with_label("Pulse");
	primary.add_css_class("suggested-action");
	buttons.append(&close);
	buttons.append(&primary);
	window.set_default_widget(Some(&primary));

	let started = Rc::new(Cell::new(false));
	let stopping = Rc::new(Cell::new(false));
	let started_at = Rc::new(Cell::new(None));

	{
		let weak = Rc::downgrade(app);
		let names = names.clone();
		let count = count.clone();
		let duration = duration.clone();
		let range = range.clone();
		let message = message.clone();
		let primary = primary.clone();
		let started = Rc::clone(&started);
		let stopping = Rc::clone(&stopping);
		let started_at = Rc::clone(&started_at);
		let primary_button = primary.clone();
		primary.connect_clicked(move |_| {
			let Some(app) = weak.upgrade() else {
				return;
			};
			if started.get() {
				app.command(Command::CancelAnimations(names.clone()));
				stopping.set(true);
				message.set_text("Stopping…");
				primary_button.set_label("Stopping…");
				primary_button.set_sensitive(false);
				return;
			}
			match app.pulse_config(
				count.value() as u32,
				&duration.text(),
				range.low(),
				range.peak(),
			) {
				Ok(config) => {
					app.command(Command::Pulse {
						names: names.clone(),
						config,
					});
					started.set(true);
					stopping.set(false);
					started_at.set(Some(Instant::now()));
					message.set_text("Pulsing…");
					primary_button.set_label("Stop");
				}
				Err(error) => message.set_text(&error),
			}
		});
	}
	{
		let weak = Rc::downgrade(app);
		close.connect_clicked(move |_| {
			if let Some(app) = weak.upgrade() {
				app.close_sheet();
			}
		});
	}

	let sheet = Rc::new(Sheet {
		window: window.clone(),
		content: Content::Pulse(PulseContent {
			names,
			message,
			primary,
			started,
			stopping,
			seen: Rc::new(Cell::new(false)),
			started_at,
		}),
	});
	present(&window, app);
	sheet
}

fn open_sweep(app: &Rc<App>, names: Vec<String>) -> Rc<Sheet> {
	let title = if names.len() > 1 {
		"Sweep Group"
	} else {
		"Sweep"
	};
	let (window, body, buttons) = sheet_window(app, title);
	if names.len() > 1 {
		let detail = gtk::Label::new(Some(&names.join(", ")));
		detail.add_css_class("elopli-hint");
		detail.set_xalign(0.0);
		body.append(&detail);
	}

	let count = gtk::SpinButton::with_range(1.0, 20.0, 1.0);
	count.set_value(1.0);
	count.set_digits(0);
	body.append(&labelled("Count", &count));

	let duration = gtk::Entry::new();
	duration.set_text("1.0s");
	duration.set_activates_default(true);
	body.append(&labelled("Duration", &duration));

	let brightness_label = gtk::Label::new(Some("N/A"));
	brightness_label.add_css_class("elopli-value");
	brightness_label.set_width_chars(4);
	brightness_label.set_xalign(1.0);
	let brightness = Slider::new(
		SliderKind::Plain {
			min: 0.0,
			max: 100.0,
		},
		0,
	);
	brightness.set_on_change({
		let label = brightness_label.clone();
		move |value| {
			if value < 2 {
				label.set_text("N/A");
			} else {
				label.set_text(&format!("{value}%"));
			}
		}
	});
	let brightness_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
	let brightness_title = gtk::Label::new(Some("Brightness"));
	brightness_title.set_xalign(0.0);
	brightness_title.set_width_chars(10);
	brightness_row.append(&brightness_title);
	brightness.widget().set_hexpand(true);
	brightness_row.append(brightness.widget());
	brightness_row.append(&brightness_label);
	body.append(&brightness_row);
	let brightness_hint =
		gtk::Label::new(Some("0 means N/A: keep each light's current brightness."));
	brightness_hint.add_css_class("elopli-hint");
	brightness_hint.set_xalign(0.0);
	body.append(&brightness_hint);

	let message = message_label();
	message.set_text("Warm → cool → warm, restoring the original values.");
	body.append(&message);

	let close = gtk::Button::with_label("Close");
	let primary = gtk::Button::with_label("Sweep");
	primary.add_css_class("suggested-action");
	buttons.append(&close);
	buttons.append(&primary);
	window.set_default_widget(Some(&primary));

	let started = Rc::new(Cell::new(false));
	let stopping = Rc::new(Cell::new(false));
	let started_at = Rc::new(Cell::new(None));

	{
		let weak = Rc::downgrade(app);
		let names = names.clone();
		let count = count.clone();
		let duration = duration.clone();
		let brightness = brightness.clone();
		let message = message.clone();
		let primary = primary.clone();
		let started = Rc::clone(&started);
		let stopping = Rc::clone(&stopping);
		let started_at = Rc::clone(&started_at);
		let primary_button = primary.clone();
		primary.connect_clicked(move |_| {
			let Some(app) = weak.upgrade() else {
				return;
			};
			if started.get() {
				app.command(Command::CancelAnimations(names.clone()));
				stopping.set(true);
				message.set_text("Stopping…");
				primary_button.set_label("Stopping…");
				primary_button.set_sensitive(false);
				return;
			}
			match app.sweep_config(count.value() as u32, &duration.text(), brightness.value()) {
				Ok(config) => {
					app.command(Command::Sweep {
						names: names.clone(),
						config,
					});
					started.set(true);
					stopping.set(false);
					started_at.set(Some(Instant::now()));
					message.set_text("Sweeping…");
					primary_button.set_label("Stop");
				}
				Err(error) => message.set_text(&error),
			}
		});
	}
	{
		let weak = Rc::downgrade(app);
		close.connect_clicked(move |_| {
			if let Some(app) = weak.upgrade() {
				app.close_sheet();
			}
		});
	}

	let sheet = Rc::new(Sheet {
		window: window.clone(),
		content: Content::Sweep(SweepContent {
			names,
			message,
			primary,
			started,
			stopping,
			seen: Rc::new(Cell::new(false)),
			started_at,
		}),
	});
	present(&window, app);
	sheet
}

fn open_discovery(app: &Rc<App>) -> Rc<Sheet> {
	let (window, body, buttons) = sheet_window(app, "Discover Lights");

	let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
	let hint = gtk::Label::new(Some("Lights on the local network"));
	hint.set_xalign(0.0);
	hint.set_hexpand(true);
	hint.add_css_class("elopli-hint");
	let spinner = gtk::Spinner::new();
	let scan = gtk::Button::with_label("Scan Again");
	header.append(&hint);
	header.append(&spinner);
	header.append(&scan);
	body.append(&header);

	let list = gtk::ListBox::new();
	list.set_selection_mode(gtk::SelectionMode::None);
	list.add_css_class("boxed-list");
	let scrolled = gtk::ScrolledWindow::builder()
		.min_content_height(260)
		.max_content_height(360)
		.child(&list)
		.build();
	body.append(&scrolled);
	let empty = gtk::Label::new(Some(
		"No lights found. Power-cycle a light, wait for it to reconnect, then scan again.",
	));
	empty.add_css_class("elopli-hint");
	empty.set_wrap(true);
	empty.set_xalign(0.0);
	body.append(&empty);

	let close = gtk::Button::with_label("Close");
	buttons.append(&close);
	{
		let weak = Rc::downgrade(app);
		close.connect_clicked(move |_| {
			if let Some(app) = weak.upgrade() {
				app.close_sheet();
			}
		});
	}
	{
		let weak = Rc::downgrade(app);
		scan.connect_clicked(move |_| {
			if let Some(app) = weak.upgrade() {
				app.command(Command::Scan);
			}
		});
	}

	let sheet = Rc::new(Sheet {
		window: window.clone(),
		content: Content::Discovery(DiscoveryContent {
			app: Rc::downgrade(app),
			list,
			spinner,
			scan,
			empty,
			fingerprint: RefCell::new(String::new()),
		}),
	});
	sheet.sync(app);
	present(&window, app);
	sheet
}

fn sheet_window(app: &App, title: &str) -> (gtk::Window, gtk::Box, gtk::Box) {
	let window = gtk::Window::builder()
		.title(title)
		.transient_for(&app.window)
		.modal(true)
		.destroy_with_parent(true)
		.default_width(400)
		.resizable(false)
		.build();
	let header = gtk::HeaderBar::new();
	header.set_show_title_buttons(true);
	let label = gtk::Label::new(Some(title));
	label.add_css_class("elopli-title");
	header.set_title_widget(Some(&label));
	window.set_titlebar(Some(&header));

	let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
	let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
	body.set_margin_top(16);
	body.set_margin_bottom(12);
	body.set_margin_start(16);
	body.set_margin_end(16);
	outer.append(&body);

	let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
	buttons.set_halign(gtk::Align::End);
	buttons.set_margin_top(4);
	buttons.set_margin_bottom(16);
	buttons.set_margin_start(16);
	buttons.set_margin_end(16);
	outer.append(&buttons);
	window.set_child(Some(&outer));
	(window, body, buttons)
}

fn present(window: &gtk::Window, app: &Rc<App>) {
	{
		let weak = Rc::downgrade(app);
		window.connect_close_request(move |_| {
			if let Some(app) = weak.upgrade() {
				app.close_sheet();
			}
			glib::Propagation::Stop
		});
	}
	{
		let weak = Rc::downgrade(app);
		let keys = gtk::EventControllerKey::new();
		keys.connect_key_pressed(move |_, key, _, _| {
			if key == gdk::Key::Escape {
				if let Some(app) = weak.upgrade() {
					app.close_sheet();
				}
				glib::Propagation::Stop
			} else {
				glib::Propagation::Proceed
			}
		});
		window.add_controller(keys);
	}
	window.present();
}

fn labelled(label: &str, widget: &impl IsA<gtk::Widget>) -> gtk::Box {
	let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
	let text = gtk::Label::new(Some(label));
	text.set_xalign(0.0);
	text.set_width_chars(10);
	row.append(&text);
	widget.set_hexpand(true);
	row.append(widget);
	row
}

fn labelled_slider(label: &str, kind: SliderKind, value: u8) -> (gtk::Box, Slider, gtk::Label) {
	let slider = Slider::new(kind, value);
	let value_label = gtk::Label::new(Some(&format!("{value}%")));
	value_label.add_css_class("elopli-value");
	value_label.set_width_chars(4);
	value_label.set_xalign(1.0);
	let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
	let text = gtk::Label::new(Some(label));
	text.set_xalign(0.0);
	text.set_width_chars(10);
	row.append(&text);
	slider.widget().set_hexpand(true);
	row.append(slider.widget());
	row.append(&value_label);
	(row, slider, value_label)
}

fn message_label() -> gtk::Label {
	let label = gtk::Label::new(None);
	label.set_xalign(0.0);
	label.set_wrap(true);
	label.add_css_class("elopli-sheet-message");
	label
}
