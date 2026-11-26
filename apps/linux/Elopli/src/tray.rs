use std::sync::mpsc::Sender;

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, StandardItem};
use ksni::{Category, MenuItem, Status, ToolTip};

/// UI actions requested from the status-notifier menu. Tray callbacks run on
/// the ksni thread, so they only send these values to the GTK main loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEvent {
	Show,
	Toggle,
	Refresh,
	AllOn,
	AllOff,
	Discover,
	ToggleAutostart,
	RevealConfig,
	Quit,
}

struct Item {
	events: Sender<UiEvent>,
	autostart: bool,
	devices: usize,
	error: Option<String>,
}

impl ksni::Tray for Item {
	fn id(&self) -> String {
		"com.bevry.elopli".into()
	}

	fn title(&self) -> String {
		"Elopli".into()
	}

	fn category(&self) -> Category {
		Category::Hardware
	}

	fn status(&self) -> Status {
		Status::Active
	}

	fn icon_name(&self) -> String {
		"com.bevry.elopli".into()
	}

	fn icon_theme_path(&self) -> String {
		// A portable tarball, an AppImage, and a development checkout each keep
		// the hicolor set in a different place. An installed package uses the
		// system icon path, so an empty value is the correct fallback.
		if let Some(path) = std::env::var_os("ELOPLI_ICON_THEME")
			&& std::path::Path::new(&path).is_dir()
		{
			return path.to_string_lossy().into_owned();
		}
		if let Ok(executable) = std::env::current_exe()
			&& let Some(directory) = executable.parent()
		{
			for candidate in [
				directory.join("share/icons"),
				directory.join("../share/icons"),
			] {
				if candidate.is_dir() {
					return candidate.to_string_lossy().into_owned();
				}
			}
		}
		let development = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons");
		if std::path::Path::new(development).is_dir() {
			development.to_string()
		} else {
			String::new()
		}
	}

	fn tool_tip(&self) -> ToolTip {
		let description = match &self.error {
			Some(error) => error.clone(),
			None => format!(
				"{} light{}",
				self.devices,
				if self.devices == 1 { "" } else { "s" }
			),
		};
		ToolTip {
			title: "Elopli".into(),
			description,
			..Default::default()
		}
	}

	fn activate(&mut self, _x: i32, _y: i32) {
		let _ = self.events.send(UiEvent::Toggle);
	}

	fn secondary_activate(&mut self, _x: i32, _y: i32) {
		let _ = self.events.send(UiEvent::Toggle);
	}

	fn menu(&self) -> Vec<MenuItem<Self>> {
		let mut items = vec![
			action("Show Elopli", UiEvent::Show),
			MenuItem::Separator,
			action("Refresh", UiEvent::Refresh),
			action("All On", UiEvent::AllOn),
			action("All Off", UiEvent::AllOff),
			action("Discover Lights…", UiEvent::Discover),
			MenuItem::Separator,
			CheckmarkItem {
				label: "Launch at Login".into(),
				checked: self.autostart,
				activate: Box::new(|item: &mut Self| {
					let _ = item.events.send(UiEvent::ToggleAutostart);
				}),
				..Default::default()
			}
			.into(),
			action("Reveal Config", UiEvent::RevealConfig),
			MenuItem::Separator,
			action("Quit Elopli", UiEvent::Quit),
		];
		items.shrink_to_fit();
		items
	}
}

fn action(label: &str, event: UiEvent) -> MenuItem<Item> {
	StandardItem {
		label: label.into(),
		activate: Box::new(move |item: &mut Item| {
			let _ = item.events.send(event);
		}),
		..Default::default()
	}
	.into()
}

/// Handle to the background status-notifier service.
pub struct Tray {
	handle: Handle<Item>,
}

impl Tray {
	pub fn start(events: Sender<UiEvent>, autostart: bool) -> Result<Self, String> {
		let item = Item {
			events,
			autostart,
			devices: 0,
			error: None,
		};
		let handle = item
			.assume_sni_available(true)
			.spawn()
			.map_err(|error| error.to_string())?;
		Ok(Self { handle })
	}

	pub fn update(&self, autostart: bool, devices: usize, error: Option<String>) {
		self.handle.update(move |item| {
			item.autostart = autostart;
			item.devices = devices;
			item.error = error;
		});
	}

	pub fn shutdown(&self) {
		self.handle.shutdown().wait();
	}
}
