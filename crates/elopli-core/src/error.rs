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
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
}

impl ElopliError {
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
