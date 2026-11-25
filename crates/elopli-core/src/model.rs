use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::net::Ipv4Addr;

use crate::error::{ElopliError, Result};
use crate::validate::{mired_to_kelvin, mired_to_warmth};

pub const DEFAULT_PORT: u16 = 9123;

fn default_port() -> u16 {
	DEFAULT_PORT
}

fn is_default_port(port: &u16) -> bool {
	*port == DEFAULT_PORT
}

/// A light in the local registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Device {
	pub name: String,
	pub host: String,
	#[serde(default = "default_port", skip_serializing_if = "is_default_port")]
	pub port: u16,
}

impl Device {
	pub fn new(name: impl Into<String>, host: impl Into<String>) -> Self {
		Self {
			name: name.into(),
			host: host.into(),
			port: DEFAULT_PORT,
		}
	}

	pub fn to_json(&self) -> Value {
		json!({
			"name": self.name,
			"host": self.host,
			"port": self.port,
		})
	}
}

/// The current state of one light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightState {
	pub on: bool,
	pub brightness: u8,
	pub temperature: u16,
}

impl LightState {
	pub fn warmth(&self) -> u8 {
		mired_to_warmth(self.temperature)
	}

	pub fn kelvin(&self) -> u16 {
		mired_to_kelvin(self.temperature)
	}

	pub fn to_json(&self) -> Value {
		json!({
			"on": self.on,
			"brightness": self.brightness,
			"warmth": self.warmth(),
			"kelvin": self.kelvin(),
			"temperature_mired": self.temperature,
		})
	}
}

/// The light's own WiFi information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiInfo {
	pub ssid: String,
	pub frequency_mhz: u16,
	pub rssi: i16,
}

impl WifiInfo {
	pub fn quality(&self) -> &'static str {
		match self.rssi {
			rssi if rssi >= -50 => "excellent",
			rssi if rssi >= -60 => "good",
			rssi if rssi >= -70 => "fair",
			_ => "weak",
		}
	}

	pub fn to_json(&self) -> Value {
		json!({
			"ssid": self.ssid,
			"frequency_mhz": self.frequency_mhz,
			"rssi": self.rssi,
			"quality": self.quality(),
		})
	}
}

/// Product and network details. Serial and MAC addresses are discarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessoryInfo {
	pub product_name: String,
	pub display_name: String,
	pub firmware_version: String,
	pub firmware_build_number: u32,
	pub hardware_board_type: u16,
	pub wifi: Option<WifiInfo>,
}

impl AccessoryInfo {
	pub fn firmware(&self) -> String {
		format!("{} ({})", self.firmware_version, self.firmware_build_number)
	}

	pub fn to_json(&self) -> Value {
		json!({
			"product": self.product_name,
			"display_name": self.display_name,
			"firmware": self.firmware(),
			"board_type": self.hardware_board_type,
			"wifi": self.wifi.as_ref().map(WifiInfo::to_json),
		})
	}
}

/// The power-on behaviour of the light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PowerOnBehavior {
	RestoreLast = 1,
	RestoreDefaults = 2,
}

impl PowerOnBehavior {
	pub fn from_u8(value: u8) -> Option<Self> {
		match value {
			1 => Some(Self::RestoreLast),
			2 => Some(Self::RestoreDefaults),
			_ => None,
		}
	}

	pub fn label(self) -> &'static str {
		match self {
			Self::RestoreLast => "restore last used",
			Self::RestoreDefaults => "restore defaults",
		}
	}
}

/// The power-on behaviour and the default values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerOnSettings {
	pub behavior: PowerOnBehavior,
	pub brightness: u8,
	pub temperature: u16,
}

impl PowerOnSettings {
	pub fn warmth(&self) -> u8 {
		mired_to_warmth(self.temperature)
	}

	pub fn kelvin(&self) -> u16 {
		mired_to_kelvin(self.temperature)
	}

	pub fn to_json(&self) -> Value {
		json!({
			"power_on_behavior": self.behavior as u8,
			"power_on_behavior_label": self.behavior.label(),
			"power_on_brightness": self.brightness,
			"power_on_temperature_mired": self.temperature,
			"power_on_warmth": self.warmth(),
			"power_on_kelvin": self.kelvin(),
		})
	}
}

/// A state and accessory-info snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
	pub state: LightState,
	pub info: AccessoryInfo,
}

/// A partial light update.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Update {
	pub on: Option<bool>,
	pub brightness: Option<u8>,
	pub temperature: Option<u16>,
	pub duration_ms: Option<u32>,
}

impl Update {
	pub fn is_empty(&self) -> bool {
		self.on.is_none()
			&& self.brightness.is_none()
			&& self.temperature.is_none()
			&& self.duration_ms.is_none()
	}
}

/// A device found through mDNS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredDevice {
	pub instance: String,
	pub hostname: String,
	pub address: Option<Ipv4Addr>,
	pub port: u16,
	pub product: String,
	pub board_type: Option<u16>,
	pub model: Option<String>,
}

impl DiscoveredDevice {
	pub fn to_json(&self) -> Value {
		json!({
			"name": self.instance,
			"host": self.hostname,
			"address": self.address.map(|address| address.to_string()),
			"port": self.port,
			"product": self.product,
			"board_type": self.board_type,
			"model": self.model,
		})
	}
}

pub fn parse_state(payload: &Value) -> Result<LightState> {
	let light = payload
		.get("lights")
		.and_then(Value::as_array)
		.and_then(|lights| lights.first())
		.ok_or_else(|| ElopliError::InvalidResponse("response contains no light state".into()))?;
	let on = light.get("on").and_then(Value::as_u64).unwrap_or(0) != 0;
	let brightness = light
		.get("brightness")
		.and_then(Value::as_u64)
		.unwrap_or(0)
		.try_into()
		.map_err(|_| ElopliError::InvalidResponse("brightness is out of range".into()))?;
	let temperature = light
		.get("temperature")
		.and_then(Value::as_u64)
		.unwrap_or(143)
		.try_into()
		.map_err(|_| ElopliError::InvalidResponse("temperature is out of range".into()))?;
	Ok(LightState {
		on,
		brightness,
		temperature,
	})
}

pub fn parse_accessory_info(payload: &Value) -> Result<AccessoryInfo> {
	let get_u64 = |key: &str| payload.get(key).and_then(Value::as_u64);
	let wifi = payload.get("wifi-info").and_then(|wifi| {
		Some(WifiInfo {
			ssid: wifi.get("ssid")?.as_str()?.to_string(),
			frequency_mhz: wifi.get("frequencyMHz")?.as_u64()?.try_into().ok()?,
			rssi: wifi.get("rssi")?.as_i64()?.try_into().ok()?,
		})
	});
	Ok(AccessoryInfo {
		product_name: payload
			.get("productName")
			.and_then(Value::as_str)
			.unwrap_or("")
			.to_string(),
		display_name: payload
			.get("displayName")
			.and_then(Value::as_str)
			.unwrap_or("")
			.to_string(),
		firmware_version: payload
			.get("firmwareVersion")
			.and_then(Value::as_str)
			.unwrap_or("")
			.to_string(),
		firmware_build_number: get_u64("firmwareBuildNumber").unwrap_or(0) as u32,
		hardware_board_type: get_u64("hardwareBoardType").unwrap_or(0) as u16,
		wifi,
	})
}

pub fn parse_power_on_settings(payload: &Value) -> Result<PowerOnSettings> {
	let behavior_value = payload
		.get("powerOnBehavior")
		.and_then(Value::as_u64)
		.ok_or_else(|| ElopliError::InvalidResponse("missing powerOnBehavior".into()))?;
	let behavior = PowerOnBehavior::from_u8(behavior_value as u8)
		.ok_or_else(|| ElopliError::InvalidResponse("unknown powerOnBehavior".into()))?;
	let brightness = payload
		.get("powerOnBrightness")
		.and_then(Value::as_u64)
		.unwrap_or(0)
		.try_into()
		.map_err(|_| ElopliError::InvalidResponse("powerOnBrightness is out of range".into()))?;
	let temperature = payload
		.get("powerOnTemperature")
		.and_then(Value::as_u64)
		.unwrap_or(143)
		.try_into()
		.map_err(|_| ElopliError::InvalidResponse("powerOnTemperature is out of range".into()))?;
	Ok(PowerOnSettings {
		behavior,
		brightness,
		temperature,
	})
}

pub fn parse_error_messages(payload: &Value) -> Vec<String> {
	payload
		.get("errors")
		.and_then(Value::as_array)
		.map(|errors| {
			errors
				.iter()
				.filter_map(|error| error.get("message").and_then(Value::as_str))
				.map(str::to_string)
				.collect()
		})
		.unwrap_or_default()
}
