//! Core client for Elopli, the open Elgato Key Light controller.

pub mod client;
pub mod config;
mod control;
pub mod discovery;
pub mod duration;
pub mod error;
pub mod model;
pub mod pulse;
pub mod sweep;
pub mod transport;
pub mod validate;

pub use client::ElopliClient;
pub use config::{Config, config_path, expand_tilde, platform_config_path};
pub use discovery::discover;
pub use duration::{DEFAULT_DURATION, parse_duration};
pub use error::{ElopliError, Result};
pub use model::{
	AccessoryInfo, Device, DiscoveredDevice, LightState, PowerOnBehavior, PowerOnSettings,
	Snapshot, Update, WifiInfo,
};
pub use pulse::{PulseCancellation, PulseConfig, PulseOutcome};
pub use sweep::SweepConfig;
pub use transport::TransportMode;
pub use validate::{
	DISPLAY_NAME_MAX_BYTES, kelvin_to_mired, mired_to_kelvin, mired_to_warmth, validate_brightness,
	validate_display_name, validate_temperature, validate_warmth, warmth_to_mired,
};
