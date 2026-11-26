//! Cross-process coordination for lights that are being pulsed.
//!
//! A pulse holds an exclusive lock for its whole run. Any other action first
//! writes a fresh cancellation token and then waits for the lock, so the pulse
//! stops before the action writes. This works across the app, the CLI, and
//! Shortcuts because they all share the same state directory.

use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{ErrorKind, Write};

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::expand_tilde;
use crate::error::{ElopliError, Result};

static NONCE: AtomicU64 = AtomicU64::new(0);

const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const LOCK_POLL: Duration = Duration::from_millis(10);

pub(crate) fn state_directory() -> PathBuf {
	if let Some(path) = std::env::var_os("ELOPLI_STATE_DIR") {
		return expand_tilde(PathBuf::from(path));
	}
	state_directory_from(dirs::config_dir(), dirs::home_dir(), std::env::temp_dir())
}

fn state_directory_from(
	config: Option<PathBuf>,
	home: Option<PathBuf>,
	temporary: PathBuf,
) -> PathBuf {
	if let Some(config) = config {
		return config.join("elopli").join("run");
	}
	if let Some(home) = home {
		return home.join(".config").join("elopli").join("run");
	}
	temporary.join("elopli-run")
}

fn device_key(host: &str, port: u16) -> String {
	let host = host.trim().to_lowercase();
	let safe: String = host
		.chars()
		.take(80)
		.map(|character| {
			if character.is_ascii_alphanumeric() || character == '-' || character == '.' {
				character
			} else {
				'_'
			}
		})
		.collect();
	let mut hasher = DefaultHasher::new();
	host.hash(&mut hasher);
	format!("{safe}-{port}-{:016x}", hasher.finish())
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
		Self::new_in(host, port, state_directory())
	}

	pub(crate) fn new_in(host: &str, port: u16, directory: impl AsRef<Path>) -> Self {
		let key = device_key(host, port);
		let directory = directory.as_ref();
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

	fn write_token(&self, token: &str) -> Result<()> {
		self.prepare()?;
		let mut file = OpenOptions::new()
			.create(true)
			.write(true)
			.truncate(true)
			.open(&self.token_path)?;
		file.write_all(token.as_bytes())?;
		file.sync_all()?;
		Ok(())
	}

	/// Ask any running pulse to stop. The caller should take the lock before
	/// writing its own change so it cannot be overwritten by a pulse step.
	pub(crate) fn cancel(&self) -> Result<()> {
		self.write_token(&next_token())
	}

	pub(crate) fn token(&self) -> Result<String> {
		match fs::read_to_string(&self.token_path) {
			Ok(token) => Ok(token),
			Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
			Err(error) => Err(error.into()),
		}
	}

	/// Claim the light for this action. The token is written before the lock,
	/// then verified after acquisition so a newer action cannot be adopted by
	/// an older one.
	pub(crate) fn acquire(&self) -> Result<(ControlGuard, String)> {
		let token = next_token();
		self.write_token(&token)?;
		let guard = self.lock()?;
		if self.token()? != token {
			return Err(ElopliError::Superseded);
		}
		Ok((guard, token))
	}

	pub(crate) fn lock(&self) -> Result<ControlGuard> {
		self.prepare()?;
		let file = OpenOptions::new()
			.create(true)
			.read(true)
			.write(true)
			.truncate(false)
			.open(&self.lock_path)
			.map_err(|error| {
				ElopliError::Config(format!(
					"cannot open control lock {}: {error}",
					self.lock_path.display()
				))
			})?;
		let deadline = Instant::now() + LOCK_TIMEOUT;
		loop {
			match file.try_lock() {
				Ok(()) => return Ok(ControlGuard { file }),
				Err(std::fs::TryLockError::WouldBlock) => {
					if Instant::now() >= deadline {
						return Err(ElopliError::LockTimeout);
					}
					std::thread::sleep(LOCK_POLL);
				}
				Err(std::fs::TryLockError::Error(error)) => {
					return Err(ElopliError::Config(format!(
						"cannot lock control file {}: {error}",
						self.lock_path.display()
					)));
				}
			}
		}
	}
}

impl Drop for ControlGuard {
	fn drop(&mut self) {
		let _ = self.file.unlock();
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn temp_dir() -> PathBuf {
		let directory = std::env::temp_dir().join(format!(
			"elopli-control-{}-{}",
			std::process::id(),
			SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.map(|duration| duration.as_nanos())
				.unwrap_or_default()
		));
		fs::create_dir_all(&directory).unwrap();
		directory
	}

	#[test]
	fn acquire_serializes_and_writes_a_token() {
		let control = Control::new_in("127.0.0.1", 9123, temp_dir());
		let (guard, token) = control.acquire().unwrap();
		assert_eq!(control.token().unwrap(), token);
		drop(guard);
		let (_guard, next) = control.acquire().unwrap();
		assert_ne!(next, token);
	}

	#[test]
	fn cancel_replaces_the_token() {
		let control = Control::new_in("127.0.0.1", 9123, temp_dir());
		let (guard, token) = control.acquire().unwrap();
		control.cancel().unwrap();
		assert_ne!(control.token().unwrap(), token);
		drop(guard);
	}

	#[test]
	fn state_directory_prefers_the_platform_config_directory() {
		let config = PathBuf::from("config");
		let home = PathBuf::from("home");
		let temporary = PathBuf::from("temp");
		assert_eq!(
			state_directory_from(Some(config.clone()), Some(home.clone()), temporary.clone()),
			config.join("elopli").join("run")
		);
		assert_eq!(
			state_directory_from(None, Some(home.clone()), temporary.clone()),
			home.join(".config").join("elopli").join("run")
		);
		assert_eq!(
			state_directory_from(None, None, temporary.clone()),
			temporary.join("elopli-run")
		);
	}
}
