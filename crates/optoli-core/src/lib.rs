//! Core client for Optoli, the open Elgato Key Light controller.

pub mod client;
pub mod config;
pub mod discovery;
pub mod error;
pub mod model;
pub mod pulse;
pub mod transport;
pub mod validate;

pub use client::OptoliClient;
pub use config::{Config, config_path};
pub use discovery::discover;
pub use error::{OptoliError, Result};
pub use model::{
	AccessoryInfo, Device, DiscoveredDevice, LightState, PowerOnBehavior, PowerOnSettings,
	Snapshot, Update, WifiInfo,
};
pub use pulse::{PulseConfig, PulseEase, PulseTarget};
pub use transport::TransportMode;
pub use validate::{
	DISPLAY_NAME_MAX_BYTES, kelvin_to_mired, mired_to_kelvin, mired_to_warmth, validate_brightness,
	validate_display_name, validate_temperature, validate_warmth, warmth_to_mired,
};
