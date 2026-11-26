use std::path::PathBuf;

use gtk::gdk;
use gtk::prelude::*;

/// Application CSS. Colours come from the active GTK theme so light and dark
/// desktops stay correct; the link colours are deliberately theme-independent.
pub const CSS: &str = r#"
window.elopli-panel {
    background-color: @theme_bg_color;
}

window.elopli-panel headerbar {
    background-color: transparent;
    background-image: none;
    box-shadow: none;
    border: none;
}

/* Match the libadwaita switch GNOME Settings uses. Plain GTK's Adwaita
   switch uses a dark knob in the dark theme. */
switch slider {
    background-color: #ffffff;
    background-image: none;
    border: 1px solid rgba(0, 0, 0, 0.18);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.18);
}

switch:checked slider {
    background-color: #ffffff;
}

.elopli-title {
    font-size: 17px;
    font-weight: 700;
}

.elopli-card-title {
    font-weight: 600;
}

menubutton.elopli-menu > button {
    background: none;
    background-image: none;
    border: none;
    box-shadow: none;
}

menubutton.elopli-menu > button:focus {
    box-shadow: none;
}

.elopli-hint {
    font-size: 11px;
    opacity: 0.62;
}

.elopli-error {
    color: #ff5c5c;
    font-size: 12px;
}

.elopli-wifi-good {
    color: #33d17a;
}

.elopli-wifi-fair {
    color: #f5c211;
}

.elopli-wifi-weak,
.elopli-wifi-offline {
    color: #ff5c5c;
}

.elopli-card {
    background-color: alpha(@theme_fg_color, 0.045);
    border: 1px solid alpha(@theme_fg_color, 0.10);
    border-radius: 14px;
}

.elopli-card-linked {
    box-shadow: inset 0 0 0 2px alpha(#ff00ff, 0.72);
}

.elopli-card-pending {
    box-shadow: inset 0 0 0 2px #ffc400;
}

.elopli-card-drop {
    box-shadow: inset 0 0 0 2px @theme_selected_bg_color;
}

.elopli-field {
    border: 1px solid transparent;
    border-radius: 9px;
}

.elopli-field-linked {
    box-shadow: inset 0 0 0 1px alpha(#ff00ff, 0.72);
}

.elopli-field-pending {
    box-shadow: inset 0 0 0 1px #ffc400;
}

.elopli-subtitle {
    font-size: 11px;
    opacity: 0.62;
}

.elopli-value {
    font-size: 11px;
    opacity: 0.72;
}

.elopli-temp {
    font-size: 10px;
    opacity: 0.55;
}

.elopli-sheet-message {
    font-size: 12px;
    opacity: 0.75;
}

.elopli-discovery-detail {
    font-size: 11px;
    opacity: 0.62;
}

.elopli-missing {
    color: #ff5c5c;
    font-weight: 600;
}
"#;

/// Icon theme roots that carry the Elopli hicolor set. The first entry is used
/// by the StatusNotifierItem host; all entries are added to GTK's icon theme.
pub fn icon_theme_roots() -> Vec<PathBuf> {
	let mut roots = Vec::new();
	if let Some(path) = std::env::var_os("ELOPLI_ICON_THEME") {
		let path = PathBuf::from(path);
		if path.is_dir() {
			roots.push(path);
		}
	}
	if let Ok(executable) = std::env::current_exe()
		&& let Some(directory) = executable.parent()
	{
		for candidate in [
			directory.join("share/icons"),
			directory.join("../share/icons"),
		] {
			if candidate.is_dir() && !roots.contains(&candidate) {
				roots.push(candidate);
			}
		}
	}
	let development = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons"));
	if development.is_dir() && !roots.contains(&development) {
		roots.push(development);
	}
	roots
}

/// Make the applications and status icons visible to GTK's icon lookup.
pub fn install_icons() {
	let Some(display) = gdk::Display::default() else {
		return;
	};
	let theme = gtk::IconTheme::for_display(&display);
	for root in icon_theme_roots() {
		theme.add_search_path(root);
	}
}

/// Install the application stylesheet on the default display.
pub fn install() {
	if let Some(display) = gdk::Display::default() {
		let provider = gtk::CssProvider::new();
		provider.load_from_data(CSS);
		gtk::style_context_add_provider_for_display(
			&display,
			&provider,
			gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
		);
	}
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
	pub foreground: gdk::RGBA,
	pub background: gdk::RGBA,
	pub accent: gdk::RGBA,
}

impl Palette {
	pub fn for_widget(widget: &impl IsA<gtk::Widget>) -> Self {
		let context = widget.style_context();
		let foreground = context.color();
		let background = context
			.lookup_color("theme_bg_color")
			.unwrap_or_else(|| gdk::RGBA::new(1.0, 1.0, 1.0, 1.0));
		let accent = context
			.lookup_color("accent_color")
			.or_else(|| context.lookup_color("theme_selected_bg_color"))
			.unwrap_or_else(|| gdk::RGBA::new(0.208, 0.518, 0.894, 1.0));
		Self {
			foreground,
			background,
			accent,
		}
	}

	pub fn track(&self) -> gdk::RGBA {
		with_alpha(self.foreground, 0.16)
	}

	pub fn knob_border(&self) -> gdk::RGBA {
		with_alpha(self.background, 0.75)
	}
}

pub fn with_alpha(color: gdk::RGBA, alpha: f64) -> gdk::RGBA {
	gdk::RGBA::new(color.red(), color.green(), color.blue(), alpha as f32)
}

/// Warm on the left, cool on the right, matching the sweep order.
pub fn temperature_color(warmth: f64) -> gdk::RGBA {
	let t = (warmth / 100.0).clamp(0.0, 1.0);
	let cool = (0.55, 0.77, 1.0);
	let white = (1.0, 1.0, 1.0);
	let warm = (1.0, 0.62, 0.29);
	let (from, to, factor) = if t < 0.4 {
		(cool, white, t / 0.4)
	} else {
		(white, warm, (t - 0.4) / 0.6)
	};
	gdk::RGBA::new(
		(from.0 + (to.0 - from.0) * factor) as f32,
		(from.1 + (to.1 - from.1) * factor) as f32,
		(from.2 + (to.2 - from.2) * factor) as f32,
		1.0,
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn temperature_runs_cool_to_warm() {
		let cool = temperature_color(0.0);
		let middle = temperature_color(40.0);
		let warm = temperature_color(100.0);
		assert!(cool.blue() > warm.blue(), "cool is bluer than warm");
		assert!(warm.red() > cool.red(), "warm is redder");
		assert!(middle.red() >= cool.red());
	}

	#[test]
	fn alpha_only_changes_the_alpha() {
		let base = gdk::RGBA::new(0.2, 0.4, 0.6, 1.0);
		let faded = with_alpha(base, 0.1);
		assert_eq!(faded.red(), base.red());
		assert_eq!(faded.green(), base.green());
		assert_eq!(faded.blue(), base.blue());
		assert!(faded.alpha() < base.alpha());
	}
}
