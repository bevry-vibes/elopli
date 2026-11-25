use crate::error::{ElopliError, Result};

/// The default text form used by the CLI and the macOS sheets.
pub const DEFAULT_DURATION: &str = "1.0s";

/// Parse a human duration into milliseconds.
///
/// Accepted forms:
///
/// - `1` or `1.0` — seconds
/// - `1s` or `1.0s` — seconds
/// - `500ms` — milliseconds
/// - `10m` — minutes
/// - `1h` — hours
///
/// The value must be positive and finite. Callers apply their own action-specific
/// minimum and maximum.
pub fn parse_duration(input: &str) -> Result<u32> {
	let input = input.trim();
	if input.is_empty() {
		return Err(ElopliError::InvalidValue(
			"duration must not be empty".into(),
		));
	}

	let lower = input.to_ascii_lowercase();
	let (number, multiplier) = if let Some(value) = lower.strip_suffix("ms") {
		(value.trim(), 1.0)
	} else if let Some(value) = lower.strip_suffix('s') {
		(value.trim(), 1_000.0)
	} else if let Some(value) = lower.strip_suffix('m') {
		(value.trim(), 60_000.0)
	} else if let Some(value) = lower.strip_suffix('h') {
		(value.trim(), 3_600_000.0)
	} else {
		(lower.as_str(), 1_000.0)
	};

	if number.is_empty() {
		return Err(ElopliError::InvalidValue(format!(
			"duration must be a number, optionally followed by s or ms: {input}"
		)));
	}

	let value = number.parse::<f64>().map_err(|_| {
		ElopliError::InvalidValue(format!(
			"duration must be a number, optionally followed by s or ms: {input}"
		))
	})?;
	if !value.is_finite() || value <= 0.0 {
		return Err(ElopliError::InvalidValue(
			"duration must be a positive finite number".into(),
		));
	}

	let milliseconds = value * multiplier;
	if milliseconds < 1.0 || milliseconds > f64::from(u32::MAX) {
		return Err(ElopliError::InvalidValue(format!(
			"duration is out of range: {input}"
		)));
	}
	Ok(milliseconds.round() as u32)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bare_numbers_are_seconds() {
		assert_eq!(parse_duration("1").unwrap(), 1_000);
		assert_eq!(parse_duration("1.5").unwrap(), 1_500);
		assert_eq!(parse_duration("0.5").unwrap(), 500);
	}

	#[test]
	fn seconds_and_milliseconds_are_explicit() {
		assert_eq!(parse_duration("1.0s").unwrap(), 1_000);
		assert_eq!(parse_duration("2s").unwrap(), 2_000);
		assert_eq!(parse_duration("500ms").unwrap(), 500);
		assert_eq!(parse_duration(" 250 ms ").unwrap(), 250);
		assert_eq!(parse_duration("10m").unwrap(), 600_000);
		assert_eq!(parse_duration("1h").unwrap(), 3_600_000);
	}

	#[test]
	fn rejects_invalid_input() {
		for value in ["", "s", "ms", "1min", "1h2", "nan", "inf", "-1", "0"] {
			assert!(parse_duration(value).is_err(), "{value:?} should fail");
		}
	}
}
