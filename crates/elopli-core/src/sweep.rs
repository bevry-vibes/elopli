use crate::error::{ElopliError, Result};
use crate::pulse::distribute_durations;
use crate::validate::warmth_to_mired;

/// A temperature sweep across the full cool-to-warm range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SweepConfig {
	pub count: u32,
	pub duration_ms: u32,
	/// `None` keeps the current brightness. `Some` holds this brightness for
	/// the whole sweep and restores the original brightness afterwards.
	pub brightness: Option<u8>,
}

impl Default for SweepConfig {
	fn default() -> Self {
		Self {
			count: 1,
			duration_ms: 3_000,
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

	/// Durations for every segment. The first move to the warm end and the
	/// final return are weighted by their distance from their targets. The sum
	/// equals `duration_ms`.
	pub fn segment_durations(&self, original_temperature: u16) -> Result<Vec<u32>> {
		let segments = self.count.saturating_mul(2).saturating_add(1) as usize;
		let warm = f64::from(warmth_to_mired(100.0)?);
		let cool = f64::from(warmth_to_mired(0.0)?);
		let span = (warm - cool).max(1.0);
		let original = f64::from(original_temperature);
		let mut weights = vec![1.0; segments];
		weights[0] = ((warm - original).abs() / span).clamp(0.0, 1.0);
		weights[segments - 1] = ((original - cool).abs() / span).clamp(0.0, 1.0);
		Ok(distribute_durations(self.duration_ms, &weights))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn first_and_last_segments_follow_distance() {
		let config = SweepConfig {
			count: 1,
			duration_ms: 3_000,
			brightness: None,
		};
		let durations = config.segment_durations(231).unwrap();
		assert_eq!(durations.len(), 3);
		assert!(durations[0] < durations[1]);
		assert!(durations[2] < durations[1]);
		assert_eq!(
			durations.iter().map(|value| u64::from(*value)).sum::<u64>(),
			3_000
		);

		let warm_start = config.segment_durations(344).unwrap();
		assert!(warm_start[0] <= 100);
		assert_eq!(
			warm_start
				.iter()
				.map(|value| u64::from(*value))
				.sum::<u64>(),
			3_000
		);
	}
}
