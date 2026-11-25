use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::error::{ElopliError, Result};
use crate::model::{DEFAULT_PORT, Device};

pub fn expand_tilde(path: impl AsRef<Path>) -> PathBuf {
	let path = path.as_ref();
	if let Ok(rest) = path.strip_prefix("~")
		&& let Some(home) = dirs::home_dir()
	{
		return home.join(rest);
	}
	path.to_path_buf()
}

pub fn config_path(explicit: Option<&Path>) -> PathBuf {
	if let Some(path) = explicit {
		return expand_tilde(path);
	}
	if let Some(path) = std::env::var_os("ELOPLI_CONFIG") {
		return expand_tilde(path);
	}
	let local = PathBuf::from("devices.json");
	if local.exists() {
		return local;
	}
	if let Some(config) = dirs::config_dir() {
		return config.join("elopli").join("devices.json");
	}
	if let Some(home) = dirs::home_dir() {
		return home.join(".config").join("elopli").join("devices.json");
	}
	PathBuf::from("devices.json")
}

#[derive(Debug, Clone)]
pub struct Config {
	pub path: PathBuf,
	pub devices: Vec<Device>,
}

impl Config {
	pub fn load(path: PathBuf) -> Result<Self> {
		if !path.exists() {
			return Ok(Self {
				path,
				devices: Vec::new(),
			});
		}
		let contents = std::fs::read_to_string(&path).map_err(|error| {
			ElopliError::Config(format!("cannot read {}: {error}", path.display()))
		})?;
		let payload: Value = serde_json::from_str(&contents).map_err(|error| {
			ElopliError::Config(format!("cannot parse {}: {error}", path.display()))
		})?;
		let entries = payload
			.get("devices")
			.and_then(Value::as_array)
			.ok_or_else(|| {
				ElopliError::Config(format!("{} must contain a devices list", path.display()))
			})?;
		let mut devices = Vec::new();
		let mut names = HashSet::new();
		for entry in entries {
			let name = entry
				.get("name")
				.and_then(Value::as_str)
				.map(str::trim)
				.filter(|name| !name.is_empty())
				.ok_or_else(|| {
					ElopliError::Config(format!(
						"{} contains a device without a name",
						path.display()
					))
				})?;
			let host = entry
				.get("host")
				.and_then(Value::as_str)
				.map(str::trim)
				.filter(|host| !host.is_empty())
				.ok_or_else(|| {
					ElopliError::Config(format!(
						"{} contains a device without a host",
						path.display()
					))
				})?;
			let port = entry
				.get("port")
				.and_then(Value::as_u64)
				.unwrap_or(u64::from(DEFAULT_PORT));
			if !(1..=u64::from(u16::MAX)).contains(&port) {
				return Err(ElopliError::Config(format!(
					"{} contains an invalid port for {name}",
					path.display()
				)));
			}
			let key = name.to_lowercase();
			if !names.insert(key) {
				return Err(ElopliError::Config(format!(
					"{} contains the device name {name} more than once",
					path.display()
				)));
			}
			devices.push(Device {
				name: name.to_string(),
				host: host.to_string(),
				port: port as u16,
			});
		}
		Ok(Self { path, devices })
	}

	pub fn save(&self) -> Result<()> {
		if let Some(parent) = self.path.parent() {
			std::fs::create_dir_all(parent).map_err(|error| {
				ElopliError::Config(format!("cannot create {}: {error}", parent.display()))
			})?;
		}
		let payload = json!({
			"devices": self.devices.iter().map(Device::to_json).collect::<Vec<_>>(),
		});
		let contents = serde_json::to_string_pretty(&payload)
			.map_err(|error| ElopliError::Config(error.to_string()))?;
		std::fs::write(&self.path, format!("{contents}\n")).map_err(|error| {
			ElopliError::Config(format!("cannot write {}: {error}", self.path.display()))
		})
	}

	pub fn find(&self, name: &str) -> Result<Device> {
		let needle = name.to_lowercase();
		self.devices
			.iter()
			.find(|device| {
				device.name.to_lowercase() == needle || device.host.to_lowercase() == needle
			})
			.cloned()
			.ok_or_else(|| ElopliError::Config(format!("unknown device: {name}")))
	}

	pub fn add(&mut self, device: Device) -> bool {
		if self
			.devices
			.iter()
			.any(|existing| existing.name == device.name || existing.host == device.host)
		{
			return false;
		}
		self.devices.push(device);
		true
	}

	pub fn reorder(&mut self, names: &[String]) -> Result<()> {
		let mut reordered = Vec::with_capacity(self.devices.len());
		for name in names {
			if let Some(index) = self
				.devices
				.iter()
				.position(|device| device.name.eq_ignore_ascii_case(name))
			{
				reordered.push(self.devices.remove(index));
			}
		}
		reordered.append(&mut self.devices);
		self.devices = reordered;
		Ok(())
	}

	pub fn to_json(&self) -> Value {
		json!({
			"path": self.path.display().to_string(),
			"devices": self.devices.iter().map(Device::to_json).collect::<Vec<_>>(),
		})
	}
}
