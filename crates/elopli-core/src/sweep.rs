use crate::error::{ElopliError, Result};
use crate::pulse::PulseEase;

/// A temperature sweep across the full cool-to-warm range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SweepConfig {
	pub count: u32,
	pub duration_ms: u32,
	pub ease: PulseEase,
	/// `None` keeps the current brightness. `Some` holds this brightness for
	/// the whole sweep and restores the original brightness afterwards.
	pub brightness: Option<u8>,
}

impl Default for SweepConfig {
	fn default() -> Self {
		Self {
			count: 1,
			duration_ms: 3_000,
			ease: PulseEase::Linear,
			brightness: None,
		}
	}
}

impl SweepConfig {
	pub fn validate(&self) -> Result<()> {
		if !(1..=20).contains(&self.count) {
			return Err(ElopliError::InvalidValue(
				"sweep count must be between 1 and 20".into(),
			));
		}
		if !(100..=60_000).contains(&self.duration_ms) {
			return Err(ElopliError::InvalidValue(
				"sweep duration must be between 100 and 60000 ms".into(),
			));
		}
		if let Some(brightness) = self.brightness
			&& !(2..=100).contains(&brightness)
		{
			return Err(ElopliError::InvalidValue(
				"sweep brightness must be between 2 and 100".into(),
			));
		}
		Ok(())
	}
}
