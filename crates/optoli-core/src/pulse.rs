use crate::error::{OptoliError, Result};

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
	Both,
}

impl PulseTarget {
	pub fn includes_brightness(self) -> bool {
		matches!(self, Self::Brightness | Self::Both)
	}

	pub fn includes_temperature(self) -> bool {
		matches!(self, Self::Temperature | Self::Both)
	}

	pub fn label(self) -> &'static str {
		match self {
			Self::Brightness => "brightness",
			Self::Temperature => "temperature",
			Self::Both => "both",
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
			ease: PulseEase::EaseInOut,
			target: PulseTarget::Both,
			value: 100,
		}
	}
}

impl PulseConfig {
	pub fn validate(&self) -> Result<()> {
		if !(1..=20).contains(&self.count) {
			return Err(OptoliError::InvalidValue(
				"pulse count must be between 1 and 20".into(),
			));
		}
		if !(50..=10_000).contains(&self.duration_ms) {
			return Err(OptoliError::InvalidValue(
				"pulse duration must be between 50 and 10000 ms".into(),
			));
		}
		if self.value > 100 {
			return Err(OptoliError::InvalidValue(
				"pulse value must be between 0 and 100".into(),
			));
		}
		Ok(())
	}
}
