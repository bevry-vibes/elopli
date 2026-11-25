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
pub enum PulseEase {
	Linear,
	EaseIn,
	EaseOut,
	EaseInOut,
}

impl PulseEase {
	pub fn apply(self, t: f64) -> f64 {
		let t = t.clamp(0.0, 1.0);
		match self {
			Self::Linear => t,
			Self::EaseIn => t * t,
			Self::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
			Self::EaseInOut => {
				if t < 0.5 {
					2.0 * t * t
				} else {
					1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
				}
			}
		}
	}

	pub fn label(self) -> &'static str {
		match self {
			Self::Linear => "linear",
			Self::EaseIn => "ease-in",
			Self::EaseOut => "ease-out",
			Self::EaseInOut => "ease-in-out",
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PulseTarget {
	Brightness,
	Temperature,
}

impl PulseTarget {
	pub fn includes_brightness(self) -> bool {
		matches!(self, Self::Brightness)
	}

	pub fn includes_temperature(self) -> bool {
		matches!(self, Self::Temperature)
	}

	pub fn label(self) -> &'static str {
		match self {
			Self::Brightness => "brightness",
			Self::Temperature => "temperature",
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PulseConfig {
	pub count: u32,
	pub duration_ms: u32,
	pub ease: PulseEase,
	pub target: PulseTarget,
	pub value: u8,
}

impl Default for PulseConfig {
	fn default() -> Self {
		Self {
			count: 3,
			duration_ms: 600,
			ease: PulseEase::Linear,
			target: PulseTarget::Brightness,
			value: 100,
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
		if !(50..=10_000).contains(&self.duration_ms) {
			return Err(ElopliError::InvalidValue(
				"pulse duration must be between 50 and 10000 ms".into(),
			));
		}
		if self.value > 100 {
			return Err(ElopliError::InvalidValue(
				"pulse value must be between 0 and 100".into(),
			));
		}
		Ok(())
	}
}
