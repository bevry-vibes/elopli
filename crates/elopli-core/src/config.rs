use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

/// The application registry path. The CLI may prefer a local file, but GUI and
/// App Intent processes must use this stable path regardless of their working
/// directory.
pub fn platform_config_path() -> PathBuf {
	if let Some(config) = dirs::config_dir() {
		return config.join("elopli").join("devices.json");
	}
	if let Some(home) = dirs::home_dir() {
		return home.join(".config").join("elopli").join("devices.json");
	}
	PathBuf::from("devices.json")
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
	platform_config_path()
}

fn fingerprint(bytes: &[u8]) -> u64 {
	let mut hasher = DefaultHasher::new();
	bytes.hash(&mut hasher);
	hasher.finish()
}

fn file_fingerprint(path: &Path) -> Option<u64> {
	fs::read(path).ok().map(|bytes| fingerprint(&bytes))
}

fn validate_device(device: &Device) -> Result<Device> {
	let name = device.name.trim();
	let host = device.host.trim();
	if name.is_empty() {
		return Err(ElopliError::Config("device name must not be empty".into()));
	}
	if host.is_empty() {
		return Err(ElopliError::Config("device host must not be empty".into()));
	}
	if device.port == 0 {
		return Err(ElopliError::Config(format!(
			"device {name} has an invalid port"
		)));
	}
	Ok(Device {
		name: name.to_string(),
		host: host.to_string(),
		port: device.port,
	})
}

struct ConfigLock {
	file: File,
}

impl Drop for ConfigLock {
	fn drop(&mut self) {
		let _ = self.file.unlock();
	}
}

#[derive(Debug, Clone)]
pub struct Config {
	pub path: PathBuf,
	pub devices: Vec<Device>,
	fingerprint: Option<u64>,
}

impl Config {
	pub fn new(path: PathBuf) -> Self {
		Self {
			path,
			devices: Vec::new(),
			fingerprint: None,
		}
	}

	pub fn load(path: PathBuf) -> Result<Self> {
		if !path.exists() {
			return Ok(Self::new(path));
		}
		let raw = std::fs::read_to_string(&path).map_err(|error| {
			ElopliError::Config(format!("cannot read {}: {error}", path.display()))
		})?;
		let contents = raw.trim_start_matches('\u{feff}');
		let payload: Value = serde_json::from_str(contents).map_err(|error| {
			ElopliError::Config(format!("cannot parse {}: {error}", path.display()))
		})?;
		let entries = payload
			.get("devices")
			.and_then(Value::as_array)
			.ok_or_else(|| {
				ElopliError::Config(format!("{} must contain a devices list", path.display()))
			})?;
		let mut devices = Vec::new();
		for entry in entries {
			let name = entry.get("name").and_then(Value::as_str).ok_or_else(|| {
				ElopliError::Config(format!(
					"{} contains a device without a name",
					path.display()
				))
			})?;
			let host = entry.get("host").and_then(Value::as_str).ok_or_else(|| {
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
					"{} contains an invalid port for {}",
					path.display(),
					name.trim()
				)));
			}
			devices.push(validate_device(&Device {
				name: name.to_string(),
				host: host.to_string(),
				port: port as u16,
			})?);
		}
		let config = Self {
			path,
			devices,
			fingerprint: Some(fingerprint(raw.as_bytes())),
		};
		config.validate()?;
		Ok(config)
	}

	pub fn reload(&mut self) -> Result<()> {
		let loaded = Self::load(self.path.clone())?;
		*self = loaded;
		Ok(())
	}

	pub fn validate(&self) -> Result<()> {
		let mut names = HashSet::new();
		let mut hosts = HashSet::new();
		for device in &self.devices {
			let device = validate_device(device)?;
			let name = device.name.to_lowercase();
			if !names.insert(name.clone()) {
				return Err(ElopliError::Config(format!(
					"{} contains the device name {name} more than once",
					self.path.display()
				)));
			}
			let host = (device.host.to_lowercase(), device.port);
			if !hosts.insert(host) {
				return Err(ElopliError::Config(format!(
					"{} contains the host {} more than once",
					self.path.display(),
					device.host
				)));
			}
		}
		Ok(())
	}

	fn lock(&self) -> Result<ConfigLock> {
		if let Some(parent) = self.path.parent() {
			fs::create_dir_all(parent).map_err(|error| {
				ElopliError::Config(format!("cannot create {}: {error}", parent.display()))
			})?;
		}
		let lock_path = self.path.with_extension("json.lock");
		let file = OpenOptions::new()
			.create(true)
			.read(true)
			.write(true)
			.truncate(false)
			.open(&lock_path)?;
		let deadline = Instant::now() + Duration::from_secs(5);
		loop {
			match file.try_lock() {
				Ok(()) => break,
				Err(std::fs::TryLockError::WouldBlock) => {
					if Instant::now() >= deadline {
						return Err(ElopliError::LockTimeout);
					}
					std::thread::sleep(Duration::from_millis(10));
				}
				Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
			}
		}
		Ok(ConfigLock { file })
	}

	pub fn save(&mut self) -> Result<()> {
		self.validate()?;
		let _lock = self.lock()?;
		let current = file_fingerprint(&self.path);
		if self.fingerprint.is_some() && current != self.fingerprint {
			return Err(ElopliError::ConfigConflict);
		}
		let payload = json!({
			"devices": self.devices.iter().map(Device::to_json).collect::<Vec<_>>(),
		});
		let contents = serde_json::to_string_pretty(&payload)
			.map_err(|error| ElopliError::Config(error.to_string()))?;
		let contents = format!("{contents}\n");
		let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
		let name = self
			.path
			.file_name()
			.and_then(|value| value.to_str())
			.unwrap_or("devices.json");
		let temporary = parent.join(format!(
			".{name}.tmp-{}-{}",
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.map(|duration| duration.as_nanos())
				.unwrap_or_default()
		));
		let result = (|| -> Result<()> {
			let mut file = OpenOptions::new()
				.create_new(true)
				.write(true)
				.open(&temporary)?;
			file.write_all(contents.as_bytes())?;
			file.sync_all()?;
			drop(file);
			fs::rename(&temporary, &self.path)?;
			Ok(())
		})();
		if result.is_err() {
			let _ = fs::remove_file(&temporary);
		}
		result?;
		self.fingerprint = file_fingerprint(&self.path);
		Ok(())
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

	/// Add a device, or update the host/port of an existing name. Returns true
	/// when the registry changed.
	pub fn add(&mut self, device: Device) -> Result<bool> {
		let device = validate_device(&device)?;
		let existing_index = self
			.devices
			.iter()
			.position(|existing| existing.name.eq_ignore_ascii_case(&device.name));
		if let Some(index) = existing_index {
			if self.devices.iter().enumerate().any(|(other, existing)| {
				other != index
					&& existing.host.eq_ignore_ascii_case(&device.host)
					&& existing.port == device.port
			}) {
				return Err(ElopliError::Config(format!(
					"{} is already registered under another name",
					device.host
				)));
			}
			if self.devices[index].host.eq_ignore_ascii_case(&device.host)
				&& self.devices[index].port == device.port
			{
				return Ok(false);
			}
			self.devices[index].host = device.host;
			self.devices[index].port = device.port;
			return Ok(true);
		}
		if self.devices.iter().any(|existing| {
			existing.host.eq_ignore_ascii_case(&device.host) && existing.port == device.port
		}) {
			return Err(ElopliError::Config(format!(
				"{} is already registered under another name",
				device.host
			)));
		}
		self.devices.push(device);
		Ok(true)
	}

	pub fn reorder(&mut self, names: &[String]) -> Result<()> {
		if names.len() != self.devices.len() {
			return Err(ElopliError::Config(
				"reorder must list every registered device exactly once".into(),
			));
		}
		let mut reordered = Vec::with_capacity(self.devices.len());
		for name in names {
			if reordered
				.iter()
				.any(|device: &Device| device.name.eq_ignore_ascii_case(name))
			{
				return Err(ElopliError::Config(format!(
					"reorder contains {name} more than once"
				)));
			}
			if let Some(index) = self
				.devices
				.iter()
				.position(|device| device.name.eq_ignore_ascii_case(name))
			{
				reordered.push(self.devices.remove(index));
			} else {
				return Err(ElopliError::Config(format!(
					"reorder contains unknown device: {name}"
				)));
			}
		}
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

#[cfg(test)]
mod tests {
	use super::*;

	fn temp_path(name: &str) -> PathBuf {
		let directory = std::env::temp_dir().join(format!(
			"elopli-config-{}-{}",
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.map(|duration| duration.as_nanos())
				.unwrap_or_default()
		));
		fs::create_dir_all(&directory).unwrap();
		directory.join(name)
	}

	#[test]
	fn load_strips_a_utf8_bom() {
		let path = temp_path("devices.json");
		fs::write(
			&path,
			"\u{feff}{\"devices\":[{\"name\":\"Left\",\"host\":\"192.0.2.10\"}]}",
		)
		.unwrap();
		let config = Config::load(path).unwrap();
		assert_eq!(config.devices.len(), 1);
		assert_eq!(config.devices[0].name, "Left");
	}

	#[test]
	fn add_updates_a_case_insensitive_name_and_rejects_host_conflicts() {
		let mut config = Config::new(temp_path("devices.json"));
		assert!(config.add(Device::new("Left", "192.0.2.10")).unwrap());
		assert!(!config.add(Device::new("left", "192.0.2.10")).unwrap());
		assert!(config.add(Device::new("LEFT", "192.0.2.11")).unwrap());
		assert_eq!(config.devices[0].host, "192.0.2.11");
		assert!(config.add(Device::new("Right", "192.0.2.11")).is_err());
	}

	#[test]
	fn save_is_atomic_and_detects_external_changes() {
		let path = temp_path("devices.json");
		let mut config = Config::new(path.clone());
		config.add(Device::new("Left", "192.0.2.10")).unwrap();
		config.save().unwrap();
		assert!(path.exists());
		assert!(!path.with_extension("json.lock").exists() || path.exists());

		let mut first = Config::load(path.clone()).unwrap();
		let mut second = Config::load(path).unwrap();
		second.devices[0].host = "192.0.2.20".to_string();
		second.save().unwrap();
		first.devices[0].host = "192.0.2.30".to_string();
		assert!(matches!(first.save(), Err(ElopliError::ConfigConflict)));
	}

	#[test]
	fn reorder_requires_every_device_once() {
		let mut config = Config::new(temp_path("devices.json"));
		config.add(Device::new("Left", "192.0.2.10")).unwrap();
		config.add(Device::new("Right", "192.0.2.11")).unwrap();
		assert!(config.reorder(&["Right".into(), "Left".into()]).is_ok());
		assert!(config.reorder(&["Right".into()]).is_err());
		assert!(config.reorder(&["Right".into(), "Right".into()]).is_err());
	}
}
