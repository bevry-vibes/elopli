use crate::error::{ElopliError, Result};
use crate::pulse::{FRAME_INTERVAL_MS, distribute_durations};
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
			duration_ms: 1_000,
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
		if !(100..=3_600_000).contains(&self.duration_ms) {
			return Err(ElopliError::InvalidValue(
				"sweep duration must be between 100 ms and 1 hour".into(),
			));
		}
		if let Some(brightness) = self.brightness
			&& !(2..=100).contains(&brightness)
		{
			return Err(ElopliError::InvalidValue(
				"sweep brightness must be between 2 and 100".into(),
			));
		}
		let segments = self.count.saturating_mul(2).saturating_add(1);
		let minimum = segments.saturating_mul(FRAME_INTERVAL_MS);
		if self.duration_ms < minimum {
			return Err(ElopliError::InvalidValue(format!(
				"sweep duration must be at least {minimum} ms for {} sweeps at the {FRAME_INTERVAL_MS} ms frame limit",
				self.count
			)));
		}
		Ok(())
	}

	/// Durations for every segment. The first move to the warm end and the
	/// final return are weighted by their distance from their targets. The sum
	/// equals `duration_ms`.
	///
	/// The final segment also carries any brightness restoration, so its
	/// weight is the larger of the temperature and brightness distances.
	pub(crate) fn segment_durations(
		&self,
		original_temperature: u16,
		original_brightness: u8,
		original_on: bool,
	) -> Result<Vec<u32>> {
		let segments = self.count.saturating_mul(2).saturating_add(1) as usize;
		let warm = f64::from(warmth_to_mired(100.0)?);
		let cool = f64::from(warmth_to_mired(0.0)?);
		let span = (warm - cool).max(1.0);
		let original = f64::from(original_temperature);
		let start_brightness =
			self.brightness
				.unwrap_or(if original_on { original_brightness } else { 2 });
		let final_brightness = if original_on { original_brightness } else { 0 };
		let brightness_span = 100.0;
		let mut weights = vec![1.0; segments];
		weights[0] = ((warm - original).abs() / span).clamp(0.0, 1.0);
		let final_temperature = ((original - cool).abs() / span).clamp(0.0, 1.0);
		let final_brightness = (f64::from(start_brightness.abs_diff(final_brightness))
			/ brightness_span)
			.clamp(0.0, 1.0);
		weights[segments - 1] = final_temperature.max(final_brightness);
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
		let durations = config.segment_durations(231, 50, true).unwrap();
		assert_eq!(durations.len(), 3);
		assert!(durations[0] < durations[1]);
		assert!(durations[2] < durations[1]);
		assert_eq!(
			durations.iter().map(|value| u64::from(*value)).sum::<u64>(),
			3_000
		);

		let warm_start = config.segment_durations(344, 50, true).unwrap();
		assert!(warm_start[0] <= 100);
		assert_eq!(
			warm_start
				.iter()
				.map(|value| u64::from(*value))
				.sum::<u64>(),
			3_000
		);
	}

	#[test]
	fn final_segment_weights_brightness_restoration() {
		let config = SweepConfig {
			count: 1,
			duration_ms: 3_000,
			brightness: Some(2),
		};
		let durations = config.segment_durations(231, 100, true).unwrap();
		assert!(durations[2] > durations[0]);
	}

	#[test]
	fn duration_must_cover_every_endpoint_at_the_frame_limit() {
		let config = SweepConfig {
			count: 3,
			duration_ms: 300,
			brightness: None,
		};
		assert!(config.validate().is_err());
	}
}
