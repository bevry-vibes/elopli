use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
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

	/// A request that must reuse the persistent native connection. Animations
	/// use this path so a transport fallback can never open a short-lived
	/// connection in the middle of a keyframe sequence.
	fn request_persistent(
		&self,
		method: &str,
		url: &str,
		body: Option<&[u8]>,
		timeout: Duration,
	) -> Result<HttpResponse, TransportError> {
		if self.persistent() {
			self.request(method, url, body, timeout)
		} else {
			Err(TransportError::connection(
				"a persistent native HTTP connection is required for animations",
			))
		}
	}

	/// Whether the transport reuses one connection for every request.
	fn persistent(&self) -> bool {
		false
	}
}

static NATIVE_AGENT: OnceLock<ureq::Agent> = OnceLock::new();

fn native_agent() -> &'static ureq::Agent {
	NATIVE_AGENT.get_or_init(|| ureq::AgentBuilder::new().build())
}

pub struct NativeTransport;

impl NativeTransport {
	pub fn new() -> Self {
		Self
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
		let mut request = native_agent().request(method, url).timeout(timeout);
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

static CURL_AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Build a curl command that never opens a console window on Windows. Both
/// the availability probe and the request path use this so a tray app does
/// not flash a console on its first network call.
fn curl_command() -> Command {
	#[cfg(windows)]
	let mut command = Command::new("curl");
	#[cfg(not(windows))]
	let command = Command::new("curl");
	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		const CREATE_NO_WINDOW: u32 = 0x0800_0000;
		command.creation_flags(CREATE_NO_WINDOW);
	}
	command
}

fn parse_curl_response(stdout: &[u8]) -> Result<HttpResponse, TransportError> {
	let index = stdout
		.iter()
		.rposition(|byte| *byte == b'\n')
		.ok_or_else(|| TransportError::protocol("curl returned an invalid response"))?;
	let (head, tail) = stdout.split_at(index);
	let status = std::str::from_utf8(&tail[1..])
		.ok()
		.and_then(|value| value.trim().parse::<u16>().ok())
		.ok_or_else(|| TransportError::protocol("curl returned an invalid status"))?;
	Ok(HttpResponse {
		status,
		body: head.to_vec(),
	})
}

pub struct CurlTransport;

impl CurlTransport {
	pub fn available() -> bool {
		*CURL_AVAILABLE.get_or_init(|| {
			curl_command()
				.arg("--version")
				.stdout(Stdio::null())
				.stderr(Stdio::null())
				.status()
				.map(|status| status.success())
				.unwrap_or(false)
		})
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
		let max_time = timeout.as_secs_f64().max(0.001);
		let mut command = curl_command();
		command.args([
			"-q",
			"--noproxy",
			"*",
			"--silent",
			"--show-error",
			"--max-time",
			&format!("{max_time:.3}"),
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
			if let Err(error) = stdin.write_all(body) {
				let _ = child.kill();
				let _ = child.wait();
				return Err(TransportError::connection(error.to_string()));
			}
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
		parse_curl_response(&output.stdout)
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

	fn request_persistent(
		&self,
		method: &str,
		url: &str,
		body: Option<&[u8]>,
		timeout: Duration,
	) -> Result<HttpResponse, TransportError> {
		if self.fallback_active() {
			return Err(TransportError::connection(
				"a persistent native HTTP connection is required for animations",
			));
		}
		self.native.request(method, url, body, timeout)
	}

	fn request(
		&self,
		method: &str,
		url: &str,
		body: Option<&[u8]>,
		timeout: Duration,
	) -> Result<HttpResponse, TransportError> {
		if self.fallback_active()
			&& let Some(curl) = &self.curl
		{
			return curl.request(method, url, body, timeout);
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

static AUTO_TRANSPORT: OnceLock<Arc<AutoTransport>> = OnceLock::new();
static NATIVE_TRANSPORT: OnceLock<Arc<NativeTransport>> = OnceLock::new();
static CURL_TRANSPORT: OnceLock<Arc<CurlTransport>> = OnceLock::new();

pub fn build_transport(mode: TransportMode) -> Arc<dyn HttpTransport + Send + Sync> {
	match mode {
		TransportMode::Auto => AUTO_TRANSPORT
			.get_or_init(|| Arc::new(AutoTransport::new()))
			.clone(),
		TransportMode::Native => NATIVE_TRANSPORT
			.get_or_init(|| Arc::new(NativeTransport::new()))
			.clone(),
		TransportMode::Curl => CURL_TRANSPORT
			.get_or_init(|| Arc::new(CurlTransport))
			.clone(),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parses_a_curl_response_and_status() {
		let response = parse_curl_response(b"{\"ok\":true}\n200").unwrap();
		assert_eq!(response.status, 200);
		assert_eq!(response.body, b"{\"ok\":true}");
	}

	#[test]
	fn parses_an_empty_curl_body() {
		let response = parse_curl_response(b"\n204").unwrap();
		assert_eq!(response.status, 204);
		assert!(response.body.is_empty());
	}

	#[test]
	fn rejects_a_curl_response_without_a_status() {
		assert!(parse_curl_response(b"no separator").is_err());
		assert!(parse_curl_response(b"body\nnot-a-status").is_err());
	}

	#[test]
	fn connection_errors_are_marked_for_fallback() {
		assert!(TransportError::connection("down").is_connection());
		assert!(!TransportError::protocol("bad json").is_connection());
	}
}
