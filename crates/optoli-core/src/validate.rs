use crate::error::{OptoliError, Result};

pub const BRIGHTNESS_MIN: u8 = 2;
pub const BRIGHTNESS_MAX: u8 = 100;
pub const MIRED_MIN: u16 = 143;
pub const MIRED_MAX: u16 = 344;
pub const KELVIN_MIN: u16 = 2900;
pub const KELVIN_MAX: u16 = 7000;
pub const DISPLAY_NAME_MAX_BYTES: usize = 63;

fn finite(name: &str, value: f64) -> Result<f64> {
	if !value.is_finite() {
		return Err(OptoliError::InvalidValue(format!(
			"{name} must be a finite number"
		)));
	}
	Ok(value)
}

pub fn validate_brightness(value: f64) -> Result<u8> {
	let value = finite("brightness", value)?.round();
	if !(f64::from(BRIGHTNESS_MIN)..=f64::from(BRIGHTNESS_MAX)).contains(&value) {
		return Err(OptoliError::InvalidValue(format!(
			"brightness must be between {BRIGHTNESS_MIN} and {BRIGHTNESS_MAX}"
		)));
	}
	Ok(value as u8)
}

pub fn validate_temperature(value: f64) -> Result<u16> {
	let value = finite("temperature", value)?.round();
	if !(f64::from(MIRED_MIN)..=f64::from(MIRED_MAX)).contains(&value) {
		return Err(OptoliError::InvalidValue(format!(
			"temperature must be between {MIRED_MIN} and {MIRED_MAX} mired"
		)));
	}
	Ok(value as u16)
}

pub fn warmth_to_mired(value: f64) -> Result<u16> {
	let value = finite("warmth", value)?;
	if !(0.0..=100.0).contains(&value) {
		return Err(OptoliError::InvalidValue(
			"warmth must be between 0 and 100".into(),
		));
	}
	Ok((f64::from(MIRED_MIN) + (value / 100.0) * f64::from(MIRED_MAX - MIRED_MIN)).round() as u16)
}

pub fn validate_warmth(value: f64) -> Result<u16> {
	warmth_to_mired(value)
}

pub fn kelvin_to_mired(value: f64) -> Result<u16> {
	let value = finite("kelvin", value)?;
	if !(f64::from(KELVIN_MIN)..=f64::from(KELVIN_MAX)).contains(&value) {
		return Err(OptoliError::InvalidValue(format!(
			"kelvin must be between {KELVIN_MIN} and {KELVIN_MAX}"
		)));
	}
	Ok(((1_000_000.0 / value).round() as u16).clamp(MIRED_MIN, MIRED_MAX))
}

pub fn mired_to_kelvin(value: u16) -> u16 {
	let value = u32::from(value.clamp(MIRED_MIN, MIRED_MAX));
	((1_000_000 + value / 2) / value) as u16
}

pub fn mired_to_warmth(value: u16) -> u8 {
	let value = u32::from(value.clamp(MIRED_MIN, MIRED_MAX));
	let min = u32::from(MIRED_MIN);
	let max = u32::from(MIRED_MAX);
	(((value - min) * 100 + (max - min) / 2) / (max - min)) as u8
}

pub fn validate_display_name(value: &str) -> Result<String> {
	let name = value.trim();
	if name.is_empty() {
		return Err(OptoliError::InvalidValue(
			"display name must not be empty".into(),
		));
	}
	if name.len() > DISPLAY_NAME_MAX_BYTES {
		return Err(OptoliError::InvalidValue(format!(
			"display name must be {DISPLAY_NAME_MAX_BYTES} bytes or fewer"
		)));
	}
	Ok(name.to_string())
}
