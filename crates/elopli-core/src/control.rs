//! Cross-process coordination for lights that are being pulsed.
//!
//! A pulse holds an exclusive lock for its whole run. Any other action first
//! writes a fresh cancellation token and then waits for the lock, so the pulse
//! stops before the action writes. This works across the app, the CLI, and
//! Shortcuts because they all share the same state directory.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;

use crate::error::{ElopliError, Result};

static NONCE: AtomicU64 = AtomicU64::new(0);

fn state_directory() -> PathBuf {
	if let Some(path) = std::env::var_os("ELOPLI_STATE_DIR") {
		return PathBuf::from(path);
	}
	if let Some(config) = dirs::config_dir() {
		return config.join("elopli").join("run");
	}
	if let Some(home) = dirs::home_dir() {
		return home.join(".config").join("elopli").join("run");
	}
	std::env::temp_dir().join("elopli-run")
}

fn device_key(host: &str, port: u16) -> String {
	let host: String = host
		.chars()
		.map(|character| {
			if character.is_ascii_alphanumeric() || character == '-' || character == '.' {
				character
			} else {
				'_'
			}
		})
		.collect();
	format!("{host}-{port}")
}

fn next_token() -> String {
	let nanos = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|duration| duration.as_nanos())
		.unwrap_or_default();
	let sequence = NONCE.fetch_add(1, Ordering::Relaxed);
	format!("{}-{nanos}-{sequence}", std::process::id())
}

pub(crate) struct Control {
	lock_path: PathBuf,
	token_path: PathBuf,
}

pub(crate) struct ControlGuard {
	file: File,
}

impl Control {
	pub(crate) fn new(host: &str, port: u16) -> Self {
		let directory = state_directory();
		let key = device_key(host, port);
		Self {
			lock_path: directory.join(format!("{key}.lock")),
			token_path: directory.join(format!("{key}.cancel")),
		}
	}

	fn prepare(&self) -> Result<()> {
		if let Some(parent) = self.lock_path.parent() {
			fs::create_dir_all(parent).map_err(|error| {
				ElopliError::Config(format!("cannot create {}: {error}", parent.display()))
			})?;
		}
		Ok(())
	}

	/// Ask any running pulse to stop. The caller should take the lock before
	/// writing its own change so it cannot be overwritten by a pulse step.
	pub(crate) fn cancel(&self) -> Result<()> {
		self.prepare()?;
		let mut file = OpenOptions::new()
			.create(true)
			.write(true)
			.truncate(true)
			.open(&self.token_path)?;
		file.write_all(next_token().as_bytes())?;
		file.sync_all()?;
		Ok(())
	}

	pub(crate) fn token(&self) -> Result<String> {
		match fs::read_to_string(&self.token_path) {
			Ok(token) => Ok(token),
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
			Err(error) => Err(error.into()),
		}
	}

	pub(crate) fn lock(&self) -> Result<ControlGuard> {
		self.prepare()?;
		let file = OpenOptions::new()
			.create(true)
			.read(true)
			.write(true)
			.open(&self.lock_path)?;
		file.lock_exclusive()?;
		Ok(ControlGuard { file })
	}
}

impl Drop for ControlGuard {
	fn drop(&mut self) {
		let _ = FileExt::unlock(&self.file);
	}
}
