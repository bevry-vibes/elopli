use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

pub const fn rgb(red: u8, green: u8, blue: u8) -> u32 {
	0xFF00_0000 | ((red as u32) << 16) | ((green as u32) << 8) | blue as u32
}

pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> u32 {
	((alpha as u32) << 24) | ((red as u32) << 16) | ((green as u32) << 8) | blue as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
	pub dark: bool,
	pub panel_bg: u32,
	pub card_bg: u32,
	pub card_border: u32,
	pub text: u32,
	pub text_secondary: u32,
	pub accent: u32,
	pub link: u32,
	pub pending: u32,
	pub danger: u32,
	pub success: u32,
	pub warning: u32,
	pub track: u32,
	pub toggle_off: u32,
	pub knob: u32,
	pub hover: u32,
	pub pressed: u32,
	pub button_bg: u32,
	pub button_accent: u32,
	pub sheet_backdrop: u32,
	pub edit_bg: u32,
}

impl Theme {
	pub fn detect() -> Self {
		Self::for_dark(is_dark_mode())
	}

	pub fn for_dark(dark: bool) -> Self {
		if dark {
			Self {
				dark,
				panel_bg: rgb(29, 29, 31),
				card_bg: rgb(44, 44, 46),
				card_border: rgb(58, 58, 60),
				text: rgb(245, 245, 247),
				text_secondary: rgb(152, 152, 157),
				accent: rgb(10, 132, 255),
				link: rgb(255, 0, 255),
				pending: rgb(255, 214, 10),
				danger: rgb(255, 69, 58),
				success: rgb(48, 209, 88),
				warning: rgb(255, 159, 10),
				track: rgb(72, 72, 74),
				toggle_off: rgb(72, 72, 74),
				knob: rgb(255, 255, 255),
				hover: rgba(255, 255, 255, 18),
				pressed: rgba(255, 255, 255, 32),
				button_bg: rgba(255, 255, 255, 14),
				button_accent: rgb(10, 132, 255),
				sheet_backdrop: rgba(0, 0, 0, 120),
				edit_bg: rgb(28, 28, 30),
			}
		} else {
			Self {
				dark,
				panel_bg: rgb(245, 245, 247),
				card_bg: rgb(255, 255, 255),
				card_border: rgb(209, 209, 214),
				text: rgb(29, 29, 31),
				text_secondary: rgb(110, 110, 115),
				accent: rgb(0, 122, 255),
				link: rgb(214, 0, 214),
				pending: rgb(191, 144, 0),
				danger: rgb(215, 0, 21),
				success: rgb(36, 138, 61),
				warning: rgb(178, 80, 0),
				track: rgb(209, 209, 214),
				toggle_off: rgb(209, 209, 214),
				knob: rgb(255, 255, 255),
				hover: rgba(0, 0, 0, 12),
				pressed: rgba(0, 0, 0, 24),
				button_bg: rgba(0, 0, 0, 8),
				button_accent: rgb(0, 122, 255),
				sheet_backdrop: rgba(0, 0, 0, 90),
				edit_bg: rgb(255, 255, 255),
			}
		}
	}

	/// Warm on the left, cool on the right, matching the sweep order.
	pub fn temperature_color(warmth: f32) -> u32 {
		let t = (warmth / 100.0).clamp(0.0, 1.0);
		let cool = (0.55f32, 0.77f32, 1.0f32);
		let white = (1.0f32, 1.0f32, 1.0f32);
		let warm = (1.0f32, 0.62f32, 0.29f32);
		let (from, to, factor) = if t < 0.4 {
			(cool, white, t / 0.4)
		} else {
			(white, warm, (t - 0.4) / 0.6)
		};
		rgb(
			((from.0 + (to.0 - from.0) * factor) * 255.0).round() as u8,
			((from.1 + (to.1 - from.1) * factor) * 255.0).round() as u8,
			((from.2 + (to.2 - from.2) * factor) * 255.0).round() as u8,
		)
	}

	pub fn wifi_color(&self, quality: &str) -> u32 {
		match quality {
			"excellent" | "good" => self.success,
			"fair" => self.warning,
			_ => self.danger,
		}
	}
}

pub fn is_dark_mode() -> bool {
	let key = RegKey::predef(HKEY_CURRENT_USER)
		.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
	match key {
		Ok(key) => key.get_value::<u32, _>("AppsUseLightTheme").unwrap_or(1) == 0,
		Err(_) => false,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn temperature_runs_cool_to_warm() {
		let cool = Theme::temperature_color(0.0);
		let middle = Theme::temperature_color(40.0);
		let warm = Theme::temperature_color(100.0);
		assert!(cool & 0xFF > warm & 0xFF, "cool is bluer than warm");
		assert!(warm >> 16 & 0xFF > cool >> 16 & 0xFF, "warm is redder");
		assert!(middle >> 16 & 0xFF >= cool >> 16 & 0xFF);
	}
}
