//! Platform-free session state shared by the native Elopli apps.
//!
//! The session owns the device registry, the link graph, live slider values,
//! refresh revisions, discovery rows, and animation state. It performs network
//! calls through `elopli-core` and leaves threading and presentation to the UI.
//!
//! A UI should keep a `Session` on one worker thread, publish
//! [`SessionSnapshot`] values to its render thread, and send commands back to
//! the worker. Animations and scans can run on child threads so the worker
//! stays responsive.

pub mod link;
pub mod session;
pub mod slider;

pub use link::{LinkGraph, LinkGroup, LinkKind, LinkTarget};
pub use session::{
	DeviceTarget, DiscoveryRow, DiscoveryState, Fetched, RefreshPlan, Session, SessionSnapshot,
	cancel_target, run_pulse, run_sweep,
};
pub use slider::{SLIDER_WRITE_INTERVAL, SliderKey, SliderQueue};

use std::time::Duration;

use elopli_core::{AccessoryInfo, LightState, PowerOnSettings, TransportMode};

/// How long a refresh request may take before it fails.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);
/// How many times a refresh request retries.
pub const DEFAULT_RETRIES: u32 = 2;
/// How often the apps refresh device state while the panel is open.
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
/// How long live slider values stay visible after the last drag event.
pub const LIVE_CLEAR_DELAY: Duration = Duration::from_millis(250);

/// Network settings for the UI session. Native transport is the default so
/// animations always reuse a persistent connection and no console can flash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientOptions {
	pub timeout: Duration,
	pub retries: u32,
	pub transport: TransportMode,
}

impl Default for ClientOptions {
	fn default() -> Self {
		Self {
			timeout: DEFAULT_TIMEOUT,
			retries: DEFAULT_RETRIES,
			transport: TransportMode::Native,
		}
	}
}

/// One light as the UI renders it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightDevice {
	pub name: String,
	pub host: String,
	pub port: u16,
	pub state: Option<LightState>,
	pub info: Option<AccessoryInfo>,
	pub power_on: Option<PowerOnSettings>,
	pub error: Option<String>,
}

impl LightDevice {
	pub fn new(name: impl Into<String>, host: impl Into<String>, port: u16) -> Self {
		Self {
			name: name.into(),
			host: host.into(),
			port,
			state: None,
			info: None,
			power_on: None,
			error: None,
		}
	}

	pub fn is_effectively_on(&self) -> bool {
		self.state
			.is_some_and(|state| state.on && state.brightness >= 2)
	}

	/// The brightness a slider shows. The light is physically off below 2%,
	/// but the slider keeps its last visible value.
	pub fn display_brightness(&self) -> u8 {
		self.state
			.map_or(50, |state| state.brightness.clamp(2, 100))
	}

	pub fn display_warmth(&self) -> u8 {
		self.state.map_or(50, |state| state.warmth())
	}

	/// The card subtitle: the light's hardware name when it differs from the
	/// registry name, then the product name, then the host.
	pub fn subtitle(&self) -> String {
		let display = self
			.info
			.as_ref()
			.map(|info| info.display_name.trim())
			.filter(|name| !name.is_empty());
		let product = self
			.info
			.as_ref()
			.map(|info| info.product_name.trim())
			.filter(|name| !name.is_empty());
		if let Some(display) = display
			&& !display.eq_ignore_ascii_case(&self.name)
		{
			return match product {
				Some(product) => format!("{display} · {product}"),
				None => display.to_string(),
			};
		}
		product.map_or_else(|| self.host.clone(), str::to_string)
	}
}
