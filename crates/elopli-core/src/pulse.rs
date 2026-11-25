use crate::error::{ElopliError, Result};
use crate::model::LightState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PulseCancellation {
	/// Another action asked the pulse to stop before changing the light.
	Superseded,
	/// The light changed outside Elopli; that change was preserved.
	ExternalChange,
}

impl PulseCancellation {
	pub fn label(self) -> &'static str {
		match self {
			Self::Superseded => "superseded",
			Self::ExternalChange => "external-change",
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PulseOutcome {
	Completed(LightState),
	Cancelled {
		state: LightState,
		reason: PulseCancellation,
	},
}

impl PulseOutcome {
	pub fn completed(&self) -> bool {
		matches!(self, Self::Completed(_))
	}

	pub fn state(&self) -> &LightState {
		match self {
			Self::Completed(state) | Self::Cancelled { state, .. } => state,
		}
	}

	pub fn cancellation(&self) -> Option<PulseCancellation> {
		match self {
			Self::Completed(_) => None,
			Self::Cancelled { reason, .. } => Some(*reason),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PulseConfig {
	pub count: u32,
	pub duration_ms: u32,
	pub value: u8,
	pub low: u8,
}

/// The firmware fades smoothly for `durationMs` values below 1000 ms.
/// At 1000 ms and above it applies the target instantly. Animations now send
/// immediate keyframes, so this limit only documents the device behavior.
pub const MAX_FADE_MS: u32 = 900;
/// One client-side keyframe interval. Animations must never send faster than
/// this, including across segment boundaries.
pub const FRAME_INTERVAL_MS: u32 = 100;
/// A short low hold, capped so long pulses do not hang at the low.
pub const MAX_LOW_HOLD_MS: u32 = 100;
/// A short floor for a segment whose start is already close to its target.
pub const MIN_SEGMENT_FADE_MS: u32 = 100;

/// A pulse target. `on` is `None` to let brightness decide the power state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PulseTarget {
	pub brightness: u8,
	pub on: Option<bool>,
}

/// One segment of a pulse. `hold_ms` is part of `duration_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PulseSegment {
	pub target: PulseTarget,
	pub duration_ms: u32,
	pub hold_ms: u32,
}

impl Default for PulseConfig {
	fn default() -> Self {
		Self {
			count: 3,
			duration_ms: 1_000,
			value: 100,
			low: 0,
		}
	}
}

impl PulseConfig {
	pub fn validate(&self) -> Result<()> {
		if !(1..=20).contains(&self.count) {
			return Err(ElopliError::InvalidValue(
				"pulse count must be between 1 and 20".into(),
			));
		}
		if !(50..=3_600_000).contains(&self.duration_ms) {
			return Err(ElopliError::InvalidValue(
				"pulse duration must be between 50 ms and 1 hour".into(),
			));
		}
		if !(2..=100).contains(&self.value) {
			return Err(ElopliError::InvalidValue(
				"pulse value must be between 2 and 100".into(),
			));
		}
		if self.low > self.value {
			return Err(ElopliError::InvalidValue(
				"pulse low must not exceed the peak".into(),
			));
		}
		let segments = self.count.saturating_mul(2).saturating_add(1);
		let minimum = segments.saturating_mul(FRAME_INTERVAL_MS);
		if self.duration_ms < minimum {
			return Err(ElopliError::InvalidValue(format!(
				"pulse duration must be at least {minimum} ms for {} pulses at the {FRAME_INTERVAL_MS} ms frame limit",
				self.count
			)));
		}
		Ok(())
	}

	/// Build the target sequence and distance-weighted budgets.
	///
	/// An on light runs `original -> peak -> low -> ... -> original`.
	/// An off light runs `off -> peak -> off` for one count, and
	/// `off -> peak -> low -> peak -> off` for two counts.
	pub(crate) fn plan(&self, original_on: bool, original_brightness: u8) -> Vec<PulseSegment> {
		let peak = self.value.clamp(2, 100);
		let low = self.low.min(peak);
		let start = if original_on { original_brightness } else { 0 };
		let mut targets = vec![PulseTarget {
			brightness: peak,
			on: Some(true),
		}];
		for cycle in 0..self.count {
			if cycle + 1 < self.count {
				targets.push(PulseTarget {
					brightness: low,
					on: None,
				});
				targets.push(PulseTarget {
					brightness: peak,
					on: Some(true),
				});
			} else if original_on {
				targets.push(PulseTarget {
					brightness: low,
					on: None,
				});
				targets.push(PulseTarget {
					brightness: original_brightness,
					on: Some(true),
				});
			} else {
				targets.push(PulseTarget {
					brightness: 0,
					on: None,
				});
			}
		}

		let span = f64::from(peak.saturating_sub(low).max(1));
		let mut current = start;
		let mut weights = Vec::with_capacity(targets.len());
		for target in &targets {
			let distance = f64::from(target.brightness.abs_diff(current));
			weights.push((distance / span).clamp(0.0, 1.0));
			current = target.brightness;
		}

		distribute_durations(self.duration_ms, &weights)
			.into_iter()
			.zip(targets)
			.map(|(duration_ms, target)| {
				let hold_ms = if target.brightness == low {
					(duration_ms / 4).min(MAX_LOW_HOLD_MS)
				} else {
					0
				};
				PulseSegment {
					target,
					duration_ms,
					hold_ms,
				}
			})
			.collect()
	}
}

/// Split a total duration across weighted segments. Every fade keeps a small
/// floor and the result sums to the total.
pub(crate) fn distribute_durations(total: u32, weights: &[f64]) -> Vec<u32> {
	let count = weights.len().max(1);
	let raw_weight_total: f64 = weights.iter().sum();
	let normalized_weights = if raw_weight_total > 0.0 {
		weights.to_vec()
	} else {
		vec![1.0; count]
	};
	let weight_total: f64 = normalized_weights.iter().sum::<f64>().max(0.0001);
	let floor = MIN_SEGMENT_FADE_MS.min(total / count as u32).max(1);
	let mut fades: Vec<u32> = normalized_weights
		.iter()
		.map(|weight| ((total as f64 * weight / weight_total).round() as u32).max(floor))
		.collect();
	let mut sum: u32 = fades.iter().sum();
	while sum > total {
		let Some((index, _)) = fades
			.iter()
			.enumerate()
			.filter(|(_, fade)| **fade > floor)
			.max_by_key(|(_, fade)| **fade)
		else {
			break;
		};
		let excess = (sum - total).min(fades[index] - floor);
		fades[index] -= excess;
		sum -= excess;
	}
	if sum < total
		&& let Some((index, _)) = fades.iter().enumerate().max_by_key(|(_, fade)| **fade)
	{
		fades[index] += total - sum;
	}
	fades
}

#[cfg(test)]
mod tests {
	use super::*;

	fn config(count: u32, duration_ms: u32) -> PulseConfig {
		PulseConfig {
			count,
			duration_ms,
			value: 100,
			low: 0,
		}
	}

	fn targets(plan: &[PulseSegment]) -> Vec<u8> {
		plan.iter()
			.map(|segment| segment.target.brightness)
			.collect()
	}

	fn durations(plan: &[PulseSegment]) -> Vec<u32> {
		plan.iter().map(|segment| segment.duration_ms).collect()
	}

	#[test]
	fn on_count_one_visits_peak_low_original() {
		let plan = config(1, 1_000).plan(true, 50);
		assert_eq!(targets(&plan), vec![100, 0, 50]);
		assert_eq!(durations(&plan), vec![250, 500, 250]);
		assert_eq!(plan[1].hold_ms, 100);
	}

	#[test]
	fn off_count_one_is_off_peak_off() {
		let plan = config(1, 1_000).plan(false, 50);
		assert_eq!(targets(&plan), vec![100, 0]);
		assert_eq!(durations(&plan), vec![500, 500]);
	}

	#[test]
	fn off_count_two_visits_the_minimum() {
		let mut config = config(2, 2_000);
		config.low = 20;
		let plan = config.plan(false, 50);
		assert_eq!(targets(&plan), vec![100, 20, 100, 0]);
		assert!(plan[1].hold_ms > 0);
		assert_eq!(
			plan.iter().map(|segment| segment.duration_ms).sum::<u32>(),
			2_000
		);
	}

	#[test]
	fn on_count_two_returns_to_the_original() {
		let plan = config(2, 2_000).plan(true, 50);
		assert_eq!(targets(&plan), vec![100, 0, 100, 0, 50]);
	}

	#[test]
	fn duration_is_preserved() {
		let plan = config(3, 3_000).plan(true, 50);
		assert_eq!(
			plan.iter().map(|segment| segment.duration_ms).sum::<u32>(),
			3_000
		);
	}

	#[test]
	fn final_restore_is_not_rushed() {
		let plan = config(1, 2_000).plan(true, 95);
		assert!(plan.last().unwrap().duration_ms >= 900);
	}

	#[test]
	fn low_must_not_exceed_the_peak() {
		let mut config = config(1, 100);
		config.low = 101;
		assert!(config.validate().is_err());
	}
}
