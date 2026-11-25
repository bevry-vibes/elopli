use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
	Auto,
	Native,
	Curl,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
	pub status: u16,
	pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct TransportError {
	pub message: String,
	pub connection: bool,
}

impl TransportError {
	pub fn connection(message: impl Into<String>) -> Self {
		Self {
			message: message.into(),
			connection: true,
		}
	}

	pub fn protocol(message: impl Into<String>) -> Self {
		Self {
			message: message.into(),
			connection: false,
		}
	}

	pub fn is_connection(&self) -> bool {
		self.connection
	}
}

impl std::fmt::Display for TransportError {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.write_str(&self.message)
	}
}

impl std::error::Error for TransportError {}

pub trait HttpTransport: Send + Sync {
	fn request(
		&self,
		method: &str,
		url: &str,
		body: Option<&[u8]>,
		timeout: Duration,
	) -> Result<HttpResponse, TransportError>;

	/// Whether the transport reuses one connection for every request.
	/// Animations must not open a new connection per keyframe: the light's
	/// HTTP server drops repeated short-lived connections.
	fn persistent(&self) -> bool {
		false
	}
}

pub struct NativeTransport {
	agent: ureq::Agent,
}

impl NativeTransport {
	pub fn new() -> Self {
		Self {
			agent: ureq::AgentBuilder::new().build(),
		}
	}
}

impl Default for NativeTransport {
	fn default() -> Self {
		Self::new()
	}
}

impl HttpTransport for NativeTransport {
	fn persistent(&self) -> bool {
		true
	}

	fn request(
		&self,
		method: &str,
		url: &str,
		body: Option<&[u8]>,
		timeout: Duration,
	) -> Result<HttpResponse, TransportError> {
		let mut request = self.agent.request(method, url).timeout(timeout);
		if body.is_some() {
			request = request.set("Content-Type", "application/json");
		}
		let response = match body {
			Some(body) => request.send_bytes(body),
			None => request.call(),
		};
		let response = match response {
			Ok(response) => response,
			Err(ureq::Error::Status(_, response)) => response,
			Err(ureq::Error::Transport(error)) => {
				return Err(TransportError::connection(error.to_string()));
			}
		};
		let status = response.status();
		let mut bytes = Vec::new();
		response
			.into_reader()
			.read_to_end(&mut bytes)
			.map_err(|error| TransportError::connection(error.to_string()))?;
		Ok(HttpResponse {
			status,
			body: bytes,
		})
	}
}

pub struct CurlTransport;

impl CurlTransport {
	pub fn available() -> bool {
		Command::new("curl")
			.arg("--version")
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.status()
			.map(|status| status.success())
			.unwrap_or(false)
	}
}

impl HttpTransport for CurlTransport {
	fn request(
		&self,
		method: &str,
		url: &str,
		body: Option<&[u8]>,
		timeout: Duration,
	) -> Result<HttpResponse, TransportError> {
		let mut command = Command::new("curl");
		command.args([
			"--silent",
			"--show-error",
			"--max-time",
			&format!("{:.3}", timeout.as_secs_f64()),
			"--request",
			method,
			"--header",
			"Content-Type: application/json",
			"--write-out",
			"\n%{http_code}",
			url,
		]);
		if body.is_some() {
			command.args(["--data-binary", "@-"]);
		}
		command
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped());
		let mut child = command
			.spawn()
			.map_err(|error| TransportError::connection(error.to_string()))?;
		if let Some(body) = body {
			let mut stdin = child
				.stdin
				.take()
				.ok_or_else(|| TransportError::connection("curl stdin is unavailable"))?;
			stdin
				.write_all(body)
				.map_err(|error| TransportError::connection(error.to_string()))?;
		}
		let output = child
			.wait_with_output()
			.map_err(|error| TransportError::connection(error.to_string()))?;
		if !output.status.success() {
			let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
			return Err(TransportError::connection(if message.is_empty() {
				format!("curl exited with status {}", output.status)
			} else {
				message
			}));
		}
		let index = output
			.stdout
			.iter()
			.rposition(|byte| *byte == b'\n')
			.ok_or_else(|| TransportError::protocol("curl returned an invalid response"))?;
		let (head, tail) = output.stdout.split_at(index);
		let status = std::str::from_utf8(&tail[1..])
			.ok()
			.and_then(|value| value.trim().parse::<u16>().ok())
			.ok_or_else(|| TransportError::protocol("curl returned an invalid status"))?;
		Ok(HttpResponse {
			status,
			body: head.to_vec(),
		})
	}
}

pub struct AutoTransport {
	native: NativeTransport,
	curl: Option<CurlTransport>,
	fallback_until: Mutex<Option<Instant>>,
}

impl AutoTransport {
	pub fn new() -> Self {
		Self {
			native: NativeTransport::new(),
			curl: CurlTransport::available().then_some(CurlTransport),
			fallback_until: Mutex::new(None),
		}
	}

	fn fallback_active(&self) -> bool {
		self.fallback_until
			.lock()
			.map(|until| until.is_some_and(|until| Instant::now() < until))
			.unwrap_or(false)
	}

	fn begin_fallback(&self) {
		if let Ok(mut until) = self.fallback_until.lock() {
			*until = Some(Instant::now() + Duration::from_secs(2));
		}
	}
}

impl Default for AutoTransport {
	fn default() -> Self {
		Self::new()
	}
}

impl HttpTransport for AutoTransport {
	fn persistent(&self) -> bool {
		!self.fallback_active()
	}

	fn request(
		&self,
		method: &str,
		url: &str,
		body: Option<&[u8]>,
		timeout: Duration,
	) -> Result<HttpResponse, TransportError> {
		if self.fallback_active() {
			if let Some(curl) = &self.curl {
				return curl.request(method, url, body, timeout);
			}
		}
		match self.native.request(method, url, body, timeout) {
			Ok(response) => Ok(response),
			Err(error) if error.is_connection() => match &self.curl {
				Some(curl) => {
					self.begin_fallback();
					curl.request(method, url, body, timeout)
				}
				None => Err(error),
			},
			Err(error) => Err(error),
		}
	}
}

pub fn build_transport(mode: TransportMode) -> Box<dyn HttpTransport + Send + Sync> {
	match mode {
		TransportMode::Auto => Box::new(AutoTransport::new()),
		TransportMode::Native => Box::new(NativeTransport::new()),
		TransportMode::Curl => Box::new(CurlTransport),
	}
}
