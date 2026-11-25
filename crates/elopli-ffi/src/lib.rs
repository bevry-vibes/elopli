uniffi::setup_scaffolding!();

use std::sync::{Arc, Mutex};

use elopli_core::{
	AccessoryInfo, Config, Device, DiscoveredDevice, ElopliClient, ElopliError, LightState,
	PowerOnBehavior, PowerOnSettings, PulseConfig, PulseEase, PulseOutcome, Snapshot, SweepConfig,
	TransportMode, Update, WifiInfo, config_path, discover as discover_core, kelvin_to_mired,
	validate_warmth,
};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
	#[error("{message}")]
	Message { message: String },
}

impl From<ElopliError> for FfiError {
	fn from(error: ElopliError) -> Self {
		Self::Message {
			message: error.to_string(),
		}
	}
}

fn message(message: impl Into<String>) -> FfiError {
	FfiError::Message {
		message: message.into(),
	}
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiLightState {
	pub on: bool,
	pub brightness: u8,
	pub warmth: u8,
	pub kelvin: u16,
	pub temperature_mired: u16,
}

impl From<LightState> for FfiLightState {
	fn from(state: LightState) -> Self {
		Self {
			on: state.on,
			brightness: state.brightness,
			warmth: state.warmth(),
			kelvin: state.kelvin(),
			temperature_mired: state.temperature,
		}
	}
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiWifiInfo {
	pub ssid: String,
	pub frequency_mhz: u16,
	pub rssi: i16,
	pub quality: String,
}

impl From<WifiInfo> for FfiWifiInfo {
	fn from(wifi: WifiInfo) -> Self {
		let quality = wifi.quality().to_string();
		Self {
			ssid: wifi.ssid,
			frequency_mhz: wifi.frequency_mhz,
			rssi: wifi.rssi,
			quality,
		}
	}
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiAccessoryInfo {
	pub product: String,
	pub display_name: String,
	pub firmware: String,
	pub board_type: u16,
	pub wifi: Option<FfiWifiInfo>,
}

impl From<AccessoryInfo> for FfiAccessoryInfo {
	fn from(info: AccessoryInfo) -> Self {
		let firmware = info.firmware();
		Self {
			product: info.product_name,
			display_name: info.display_name,
			firmware,
			board_type: info.hardware_board_type,
			wifi: info.wifi.map(FfiWifiInfo::from),
		}
	}
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiPowerOnSettings {
	pub behavior: u8,
	pub behavior_label: String,
	pub brightness: u8,
	pub warmth: u8,
	pub kelvin: u16,
	pub temperature_mired: u16,
}

impl From<PowerOnSettings> for FfiPowerOnSettings {
	fn from(settings: PowerOnSettings) -> Self {
		Self {
			behavior: settings.behavior as u8,
			behavior_label: settings.behavior.label().to_string(),
			brightness: settings.brightness,
			warmth: settings.warmth(),
			kelvin: settings.kelvin(),
			temperature_mired: settings.temperature,
		}
	}
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiSnapshot {
	pub state: FfiLightState,
	pub info: FfiAccessoryInfo,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiPulseOutcome {
	pub completed: bool,
	pub cancellation: Option<String>,
	pub state: FfiLightState,
}

impl From<PulseOutcome> for FfiPulseOutcome {
	fn from(outcome: PulseOutcome) -> Self {
		let completed = outcome.completed();
		let cancellation = outcome
			.cancellation()
			.map(|reason| reason.label().to_string());
		let state = outcome.state().clone().into();
		Self {
			completed,
			cancellation,
			state,
		}
	}
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiDevice {
	pub name: String,
	pub host: String,
	pub port: u16,
}

impl From<Device> for FfiDevice {
	fn from(device: Device) -> Self {
		Self {
			name: device.name,
			host: device.host,
			port: device.port,
		}
	}
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiDiscoveredDevice {
	pub name: String,
	pub host: String,
	pub address: Option<String>,
	pub port: u16,
	pub product: String,
	pub board_type: Option<u16>,
	pub model: Option<String>,
}

impl From<DiscoveredDevice> for FfiDiscoveredDevice {
	fn from(device: DiscoveredDevice) -> Self {
		Self {
			name: device.instance,
			host: device.hostname,
			address: device.address.map(|address| address.to_string()),
			port: device.port,
			product: device.product,
			board_type: device.board_type,
			model: device.model,
		}
	}
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum FfiPulseEase {
	Linear,
	EaseIn,
	EaseOut,
	EaseInOut,
}

impl From<FfiPulseEase> for PulseEase {
	fn from(value: FfiPulseEase) -> Self {
		match value {
			FfiPulseEase::Linear => Self::Linear,
			FfiPulseEase::EaseIn => Self::EaseIn,
			FfiPulseEase::EaseOut => Self::EaseOut,
			FfiPulseEase::EaseInOut => Self::EaseInOut,
		}
	}
}

#[derive(uniffi::Object)]
pub struct FfiClient {
	inner: ElopliClient,
}

#[uniffi::export]
impl FfiClient {
	#[uniffi::constructor]
	pub fn new(
		host: String,
		port: u16,
		timeout_seconds: f64,
		retries: u32,
		transport: String,
	) -> Result<Arc<Self>, FfiError> {
		if timeout_seconds <= 0.0 || !timeout_seconds.is_finite() {
			return Err(message("timeout must be greater than zero"));
		}
		let mode = match transport.as_str() {
			"auto" => TransportMode::Auto,
			"native" => TransportMode::Native,
			"curl" => TransportMode::Curl,
			_ => return Err(message(format!("unknown transport: {transport}"))),
		};
		Ok(Arc::new(Self {
			inner: ElopliClient::new(
				host,
				port,
				std::time::Duration::from_secs_f64(timeout_seconds),
				retries,
				mode,
			),
		}))
	}

	pub fn state(&self) -> Result<FfiLightState, FfiError> {
		Ok(self.inner.state()?.into())
	}

	pub fn set_on(&self, on: bool) -> Result<FfiLightState, FfiError> {
		Ok(self.inner.set_on(on)?.into())
	}

	pub fn set_on_with_brightness(
		&self,
		on: bool,
		brightness: Option<u8>,
	) -> Result<FfiLightState, FfiError> {
		Ok(self
			.inner
			.update(Update {
				on: Some(on),
				brightness,
				..Update::default()
			})?
			.into())
	}

	pub fn toggle(&self) -> Result<FfiLightState, FfiError> {
		Ok(self.inner.toggle()?.into())
	}

	pub fn set_brightness(&self, value: u8) -> Result<FfiLightState, FfiError> {
		Ok(self
			.inner
			.update(Update {
				brightness: Some(value),
				..Update::default()
			})?
			.into())
	}

	pub fn set_warmth(&self, value: u8) -> Result<FfiLightState, FfiError> {
		let temperature = validate_warmth(f64::from(value))?;
		Ok(self
			.inner
			.update(Update {
				temperature: Some(temperature),
				..Update::default()
			})?
			.into())
	}

	pub fn set_kelvin(&self, value: u16) -> Result<FfiLightState, FfiError> {
		let temperature = kelvin_to_mired(f64::from(value))?;
		Ok(self
			.inner
			.update(Update {
				temperature: Some(temperature),
				..Update::default()
			})?
			.into())
	}

	pub fn set_temperature(&self, value: u16) -> Result<FfiLightState, FfiError> {
		Ok(self
			.inner
			.update(Update {
				temperature: Some(value),
				..Update::default()
			})?
			.into())
	}

	pub fn info(&self) -> Result<FfiAccessoryInfo, FfiError> {
		Ok(self.inner.info()?.into())
	}

	pub fn snapshot(&self) -> Result<FfiSnapshot, FfiError> {
		let snapshot: Snapshot = self.inner.snapshot()?;
		Ok(FfiSnapshot {
			state: snapshot.state.into(),
			info: snapshot.info.into(),
		})
	}

	pub fn identify(&self) -> Result<(), FfiError> {
		Ok(self.inner.identify()?)
	}

	pub fn cancel_pulse(&self) -> Result<(), FfiError> {
		Ok(self.inner.cancel_pulse()?)
	}

	pub fn rename(&self, name: String) -> Result<String, FfiError> {
		Ok(self.inner.rename(&name)?)
	}

	pub fn power_on_settings(&self) -> Result<FfiPowerOnSettings, FfiError> {
		Ok(self.inner.power_on_settings()?.into())
	}

	pub fn pulse(
		&self,
		count: u32,
		duration_ms: u32,
		ease: FfiPulseEase,
		value: u8,
	) -> Result<FfiPulseOutcome, FfiError> {
		let outcome = self.inner.pulse(PulseConfig {
			count,
			duration_ms,
			ease: ease.into(),
			value,
		})?;
		Ok(outcome.into())
	}

	pub fn sweep(
		&self,
		count: u32,
		duration_ms: u32,
		ease: FfiPulseEase,
		brightness: Option<u8>,
	) -> Result<FfiPulseOutcome, FfiError> {
		let outcome = self.inner.sweep(SweepConfig {
			count,
			duration_ms,
			ease: ease.into(),
			brightness,
		})?;
		Ok(outcome.into())
	}

	pub fn set_power_on(
		&self,
		behavior: u8,
		brightness: Option<u8>,
		temperature_mired: Option<u16>,
	) -> Result<FfiPowerOnSettings, FfiError> {
		let behavior = PowerOnBehavior::from_u8(behavior)
			.ok_or_else(|| message(format!("unknown power-on behaviour: {behavior}")))?;
		Ok(self
			.inner
			.set_power_on(behavior, brightness, temperature_mired)?
			.into())
	}
}

#[derive(uniffi::Object)]
pub struct FfiConfig {
	inner: Mutex<Config>,
}

#[uniffi::export]
impl FfiConfig {
	#[uniffi::constructor]
	pub fn load(path: Option<String>) -> Result<Arc<Self>, FfiError> {
		let path = path
			.map(std::path::PathBuf::from)
			.unwrap_or_else(|| config_path(None));
		Ok(Arc::new(Self {
			inner: Mutex::new(Config::load(path)?),
		}))
	}

	pub fn path(&self) -> Result<String, FfiError> {
		Ok(self.lock()?.path.display().to_string())
	}

	pub fn devices(&self) -> Result<Vec<FfiDevice>, FfiError> {
		Ok(self
			.lock()?
			.devices
			.iter()
			.cloned()
			.map(FfiDevice::from)
			.collect())
	}

	pub fn find(&self, name: String) -> Result<FfiDevice, FfiError> {
		Ok(self.lock()?.find(&name)?.into())
	}

	pub fn add(&self, name: String, host: String, port: u16) -> Result<bool, FfiError> {
		Ok(self.lock()?.add(Device { name, host, port }))
	}

	pub fn reorder(&self, names: Vec<String>) -> Result<(), FfiError> {
		self.lock()?.reorder(&names)?;
		Ok(())
	}

	pub fn save(&self) -> Result<(), FfiError> {
		Ok(self.lock()?.save()?)
	}
}

impl FfiConfig {
	fn lock(&self) -> Result<std::sync::MutexGuard<'_, Config>, FfiError> {
		self.inner
			.lock()
			.map_err(|_| message("config lock is poisoned"))
	}
}

#[uniffi::export]
pub fn discover(timeout_seconds: f64) -> Result<Vec<FfiDiscoveredDevice>, FfiError> {
	if timeout_seconds <= 0.0 || !timeout_seconds.is_finite() {
		return Err(message("timeout must be greater than zero"));
	}
	Ok(
		discover_core(std::time::Duration::from_secs_f64(timeout_seconds))?
			.into_iter()
			.map(FfiDiscoveredDevice::from)
			.collect(),
	)
}

#[uniffi::export]
pub fn default_config_path() -> String {
	config_path(None).display().to_string()
}
