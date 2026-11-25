use thiserror::Error;

/// Errors from the Elopli core.
#[derive(Debug, Error)]
pub enum ElopliError {
	#[error("connection failed: {0}")]
	Connection(String),
	#[error("HTTP {status}: {message}")]
	Api { status: u16, message: String },
	#[error("invalid response: {0}")]
	InvalidResponse(String),
	#[error("invalid value: {0}")]
	InvalidValue(String),
	#[error("config error: {0}")]
	Config(String),
	#[error("discovery error: {0}")]
	Discovery(String),
	/// The requested operation is not supported by the selected transport.
	#[error("{0}")]
	Unsupported(String),
	/// Another action took control before this action could write.
	#[error("another action superseded this request")]
	Superseded,
	/// The per-light control lock was not released in time.
	#[error("timed out waiting for another action to finish")]
	LockTimeout,
	/// The registry changed on disk after it was loaded.
	#[error("the device registry changed on disk; reload it and retry")]
	ConfigConflict,
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
}

impl ElopliError {
	pub fn kind(&self) -> &'static str {
		match self {
			Self::Connection(_) => "connection",
			Self::Api { .. } => "api",
			Self::InvalidResponse(_) => "invalid_response",
			Self::InvalidValue(_) => "invalid_value",
			Self::Config(_) => "config",
			Self::Discovery(_) => "discovery",
			Self::Unsupported(_) => "unsupported",
			Self::Superseded => "superseded",
			Self::LockTimeout => "lock_timeout",
			Self::ConfigConflict => "config_conflict",
			Self::Io(_) => "io",
		}
	}

	pub fn api(status: u16, messages: &[String]) -> Self {
		let message = messages
			.iter()
			.filter(|message| !message.is_empty())
			.cloned()
			.collect::<Vec<_>>()
			.join("; ");
		Self::Api {
			status,
			message: if message.is_empty() {
				"unknown error".to_string()
			} else {
				message
			},
		}
	}
}

pub type Result<T> = std::result::Result<T, ElopliError>;
