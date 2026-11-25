use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::control::{Control, ControlGuard};
use crate::error::{ElopliError, Result};
use crate::model::{
	AccessoryInfo, LightState, PowerOnBehavior, PowerOnSettings, Snapshot, Update,
	parse_accessory_info, parse_error_messages, parse_power_on_settings, parse_state,
};
use crate::pulse::{PulseCancellation, PulseConfig, PulseEase, PulseOutcome};
use crate::sweep::SweepConfig;
use crate::transport::{HttpTransport, TransportMode, build_transport};
use crate::validate::{
	validate_brightness, validate_display_name, validate_temperature, warmth_to_mired,
};

pub struct ElopliClient {
	host: String,
	port: u16,
	timeout: Duration,
	retries: u32,
	retry_delay: Duration,
	transport: Box<dyn HttpTransport + Send + Sync>,
	control: Control,
}

impl ElopliClient {
	pub fn new(
		host: impl Into<String>,
		port: u16,
		timeout: Duration,
		retries: u32,
		mode: TransportMode,
	) -> Self {
		let host = host.into();
		Self {
			control: Control::new(&host, port),
			host,
			port,
			timeout,
			retries,
			retry_delay: Duration::from_millis(500),
			transport: build_transport(mode),
		}
	}

	fn url(&self, path: &str) -> String {
		format!("http://{}:{}{path}", self.host, self.port)
	}

	fn request(&self, method: &str, path: &str, payload: Option<&Value>) -> Result<Value> {
		let body = payload
			.map(serde_json::to_vec)
			.transpose()
			.map_err(|error| ElopliError::InvalidValue(error.to_string()))?;
		let url = self.url(path);
		let mut last_error: Option<ElopliError> = None;
		for attempt in 0..=self.retries {
			match self
				.transport
				.request(method, &url, body.as_deref(), self.timeout)
			{
				Ok(response) => {
					if response.status >= 400 {
						let payload =
							serde_json::from_slice::<Value>(&response.body).unwrap_or(Value::Null);
						let mut messages = parse_error_messages(&payload);
						if messages.is_empty() {
							let text = String::from_utf8_lossy(&response.body).trim().to_string();
							if !text.is_empty() {
								messages.push(text);
							}
						}
						let error = ElopliError::api(response.status, &messages);
						if response.status >= 500 && attempt < self.retries {
							last_error = Some(error);
						} else {
							return Err(error);
						}
					} else if response.body.is_empty() {
						return Ok(Value::Null);
					} else {
						return serde_json::from_slice(&response.body)
							.map_err(|error| ElopliError::InvalidResponse(error.to_string()));
					}
				}
				Err(error) => {
					last_error = Some(ElopliError::Connection(error.message));
				}
			}
			if attempt < self.retries {
				std::thread::sleep(self.retry_delay * (attempt + 1));
			}
		}
		Err(last_error.unwrap_or_else(|| ElopliError::Connection("request failed".into())))
	}

	pub fn state(&self) -> Result<LightState> {
		parse_state(&self.request("GET", "/elgato/lights", None)?)
	}

	pub fn update(&self, update: Update) -> Result<LightState> {
		let _guard = self.begin_action()?;
		self.write_update(update)
	}

	fn write_update(&self, update: Update) -> Result<LightState> {
		self.send_update(update, false)
	}

	/// Pulse frames may reach 0% brightness, which switches the light off.
	fn write_pulse_update(&self, update: Update) -> Result<LightState> {
		self.send_update(update, true)
	}

	fn send_update(&self, update: Update, allow_zero_brightness: bool) -> Result<LightState> {
		if update.is_empty() {
			return Err(ElopliError::InvalidValue(
				"no properties were provided".into(),
			));
		}
		let mut light = Map::new();
		if let Some(on) = update.on {
			light.insert("on".into(), json!(u8::from(on)));
		}
		if let Some(brightness) = update.brightness {
			let brightness = if allow_zero_brightness {
				brightness.min(100)
			} else {
				validate_brightness(f64::from(brightness))?
			};
			light.insert("brightness".into(), json!(brightness));
		}
		if let Some(temperature) = update.temperature {
			light.insert(
				"temperature".into(),
				json!(validate_temperature(f64::from(temperature))?),
			);
		}
		if let Some(duration_ms) = update.duration_ms {
			light.insert("durationMs".into(), json!(duration_ms));
		}
		let payload = json!({ "lights": [Value::Object(light)] });
		parse_state(&self.request("PUT", "/elgato/lights", Some(&payload))?)
	}

	pub fn set_on(&self, on: bool) -> Result<LightState> {
		self.update(Update {
			on: Some(on),
			..Update::default()
		})
	}

	pub fn toggle(&self) -> Result<LightState> {
		self.update(Update {
			on: Some(!self.state()?.on),
			..Update::default()
		})
	}

	pub fn info(&self) -> Result<AccessoryInfo> {
		parse_accessory_info(&self.request("GET", "/elgato/accessory-info", None)?)
	}

	pub fn snapshot(&self) -> Result<Snapshot> {
		Ok(Snapshot {
			state: self.state()?,
			info: self.info()?,
		})
	}

	pub fn identify(&self) -> Result<()> {
		let _guard = self.begin_action()?;
		self.request("POST", "/elgato/identify", None)?;
		Ok(())
	}

	pub fn power_on_settings(&self) -> Result<PowerOnSettings> {
		parse_power_on_settings(&self.request("GET", "/elgato/lights/settings", None)?)
	}

	pub fn set_power_on(
		&self,
		behavior: PowerOnBehavior,
		brightness: Option<u8>,
		temperature: Option<u16>,
	) -> Result<PowerOnSettings> {
		let _guard = self.begin_action()?;
		let settings = self.request("GET", "/elgato/lights/settings", None)?;
		let mut settings = settings.as_object().cloned().ok_or_else(|| {
			ElopliError::InvalidResponse("settings response is not an object".into())
		})?;
		settings.insert("powerOnBehavior".into(), json!(behavior as u8));
		if let Some(brightness) = brightness {
			settings.insert(
				"powerOnBrightness".into(),
				json!(validate_brightness(f64::from(brightness))?),
			);
		}
		if let Some(temperature) = temperature {
			settings.insert(
				"powerOnTemperature".into(),
				json!(validate_temperature(f64::from(temperature))?),
			);
		}
		self.request(
			"PUT",
			"/elgato/lights/settings",
			Some(&Value::Object(settings)),
		)?;
		self.power_on_settings()
	}

	pub fn rename(&self, name: &str) -> Result<String> {
		let name = validate_display_name(name)?;
		let _guard = self.begin_action()?;
		self.request(
			"PUT",
			"/elgato/accessory-info",
			Some(&json!({ "displayName": name })),
		)?;
		Ok(self.info()?.display_name)
	}

	pub fn pulse(&self, config: PulseConfig) -> Result<PulseOutcome> {
		config.validate()?;
		// Cancel any pulse that is already running before queueing for the lock.
		self.control.cancel()?;
		let _guard = self.control.lock()?;
		let token = self.control.token()?;
		let original = self.state()?;
		let result = self.run_pulse(config, original, &token);
		if result.is_err() {
			let _ = self.restore_state(original);
		}
		result
	}

	/// Sweep the temperature across the full cool-to-warm range.
	pub fn sweep(&self, config: SweepConfig) -> Result<PulseOutcome> {
		config.validate()?;
		self.control.cancel()?;
		let _guard = self.control.lock()?;
		let token = self.control.token()?;
		let original = self.state()?;
		let result = self.run_sweep(config, original, &token);
		if result.is_err() {
			let _ = self.restore_state(original);
		}
		result
	}

	/// Stop a running pulse without changing the light. The pulse leaves the
	/// light at its last frame, or at the external value it detected.
	pub fn cancel_pulse(&self) -> Result<()> {
		self.control.cancel()
	}

	fn run_sweep(
		&self,
		config: SweepConfig,
		original: LightState,
		token: &str,
	) -> Result<PulseOutcome> {
		let cool = warmth_to_mired(0.0)?;
		let warm = warmth_to_mired(100.0)?;
		let segments = config.count.saturating_mul(2).saturating_add(1);
		let mut expected = original;
		if let Some(brightness) = config.brightness {
			expected = self.write_pulse_update(Update {
				on: Some(true),
				brightness: Some(brightness),
				..Update::default()
			})?;
		} else if !original.on {
			expected = self.write_pulse_update(Update {
				on: Some(true),
				brightness: Some(original.brightness.max(2)),
				..Update::default()
			})?;
		}

		// Move from the current temperature to the warm end first.
		if let Some(outcome) = self.ramp_to(
			token,
			expected,
			original.brightness,
			warm,
			Some(true),
			segment_ms(config.duration_ms, segments, 0),
			config.ease,
			false,
			true,
			&mut expected,
		)? {
			return Ok(outcome);
		}

		for cycle in 0..config.count {
			// Each sweep is a warm-to-cool transition.
			if let Some(outcome) = self.ramp_to(
				token,
				expected,
				original.brightness,
				cool,
				Some(true),
				segment_ms(config.duration_ms, segments, 1 + cycle * 2),
				config.ease,
				false,
				true,
				&mut expected,
			)? {
				return Ok(outcome);
			}
			if cycle + 1 < config.count {
				if let Some(outcome) = self.ramp_to(
					token,
					expected,
					original.brightness,
					warm,
					Some(true),
					segment_ms(config.duration_ms, segments, 2 + cycle * 2),
					config.ease,
					false,
					true,
					&mut expected,
				)? {
					return Ok(outcome);
				}
			}
		}

		// Return from the cool end to the original temperature.
		if let Some(outcome) = self.ramp_to(
			token,
			expected,
			original.brightness,
			original.temperature,
			Some(true),
			segment_ms(config.duration_ms, segments, segments - 1),
			config.ease,
			false,
			true,
			&mut expected,
		)? {
			return Ok(outcome);
		}

		if let Some(outcome) = self.cancelled(token, &expected)? {
			return Ok(outcome);
		}
		self.restore_state(original)?;
		Ok(PulseOutcome::Completed(original))
	}

	fn begin_action(&self) -> Result<ControlGuard> {
		self.control.cancel()?;
		self.control.lock()
	}

	fn run_pulse(
		&self,
		config: PulseConfig,
		original: LightState,
		token: &str,
	) -> Result<PulseOutcome> {
		let peak_brightness = config.value.clamp(2, 100);
		// The light treats anything below 2% as off, so the pulse keeps the
		// light on at 2%.
		let low_brightness = 2;
		let segments = config.count.saturating_mul(2).saturating_add(1);

		let mut expected = original;

		// Current state to the first peak.
		if let Some(outcome) = self.ramp_to(
			token,
			expected,
			peak_brightness,
			original.temperature,
			Some(true),
			segment_ms(config.duration_ms, segments, 0),
			config.ease,
			true,
			false,
			&mut expected,
		)? {
			return Ok(outcome);
		}

		for cycle in 0..config.count {
			// Every pulse is a peak-to-minimum transition. The light stays
			// on at its 2% minimum.
			if let Some(outcome) = self.ramp_to(
				token,
				expected,
				low_brightness,
				original.temperature,
				None,
				segment_ms(config.duration_ms, segments, 1 + cycle * 2),
				config.ease,
				true,
				false,
				&mut expected,
			)? {
				return Ok(outcome);
			}

			if cycle + 1 < config.count {
				if let Some(outcome) = self.ramp_to(
					token,
					expected,
					peak_brightness,
					original.temperature,
					Some(true),
					segment_ms(config.duration_ms, segments, 2 + cycle * 2),
					config.ease,
					true,
					false,
					&mut expected,
				)? {
					return Ok(outcome);
				}
			}
		}

		// Return to the value the light had before the pulse. If an action
		// interrupted the pulse, that action has already applied its value
		// and the pulse stopped without restoring.
		if original.on {
			if let Some(outcome) = self.ramp_to(
				token,
				expected,
				original.brightness,
				original.temperature,
				Some(true),
				segment_ms(config.duration_ms, segments, segments - 1),
				config.ease,
				true,
				false,
				&mut expected,
			)? {
				return Ok(outcome);
			}
		}

		if let Some(outcome) = self.cancelled(token, &expected)? {
			return Ok(outcome);
		}
		self.restore_state(original)?;
		Ok(PulseOutcome::Completed(original))
	}

	#[allow(clippy::too_many_arguments)]
	fn ramp_to(
		&self,
		token: &str,
		from: LightState,
		to_brightness: u8,
		to_temperature: u16,
		on: Option<bool>,
		ramp_ms: u32,
		ease: PulseEase,
		includes_brightness: bool,
		includes_temperature: bool,
		expected: &mut LightState,
	) -> Result<Option<PulseOutcome>> {
		let frame_ms = self.transport.pulse_frame_ms().max(120);
		// Hold the endpoint briefly so peaks and minima are visible even when
		// the whole animation is short.
		let hold_ms = (ramp_ms / 5).max(1).min(ramp_ms / 2).max(1);
		let motion_ms = ramp_ms.saturating_sub(hold_ms).max(1);
		let steps = (motion_ms / frame_ms).max(1);
		let step_ms = (motion_ms / steps).max(10);

		for step in 1..=steps {
			if let Some(outcome) = self.cancelled(token, expected)? {
				return Ok(Some(outcome));
			}
			let progress = ease.apply(f64::from(step) / f64::from(steps));
			let mut update = Update {
				on,
				duration_ms: Some(step_ms),
				..Update::default()
			};
			if includes_brightness {
				update.brightness = Some(lerp_u8(from.brightness, to_brightness, progress));
			}
			if includes_temperature {
				update.temperature = Some(lerp_u16(from.temperature, to_temperature, progress));
			}
			*expected = self.write_pulse_update(update)?;
			std::thread::sleep(std::time::Duration::from_millis(u64::from(step_ms)));
		}
		std::thread::sleep(std::time::Duration::from_millis(u64::from(hold_ms)));
		Ok(None)
	}

	/// Return an outcome when a pulse should stop, or keep going.
	fn cancelled(&self, token: &str, expected: &LightState) -> Result<Option<PulseOutcome>> {
		if self.control.token()? != token {
			return Ok(Some(PulseOutcome::Cancelled {
				state: self.state()?,
				reason: PulseCancellation::Superseded,
			}));
		}
		let current = self.state()?;
		if current != *expected {
			return Ok(Some(PulseOutcome::Cancelled {
				state: current,
				reason: PulseCancellation::ExternalChange,
			}));
		}
		Ok(None)
	}

	fn restore_state(&self, state: LightState) -> Result<()> {
		let payload = json!({
			"lights": [{
				"on": u8::from(state.on),
				"brightness": state.brightness,
				"temperature": state.temperature,
			}],
		});
		self.request("PUT", "/elgato/lights", Some(&payload))?;
		Ok(())
	}
}

fn lerp_u8(from: u8, to: u8, progress: f64) -> u8 {
	(f64::from(from) + (f64::from(to) - f64::from(from)) * progress).round() as u8
}

fn lerp_u16(from: u16, to: u16, progress: f64) -> u16 {
	(f64::from(from) + (f64::from(to) - f64::from(from)) * progress).round() as u16
}

/// Split the total pulse duration across every segment. Using the boundaries
/// avoids losing milliseconds to integer division and keeps the whole
/// animation at the requested duration.
fn segment_ms(total_ms: u32, segments: u32, index: u32) -> u32 {
	let segments = u64::from(segments.max(1));
	let total = u64::from(total_ms);
	let start = total * u64::from(index) / segments;
	let end = total * u64::from(index + 1) / segments;
	(end.saturating_sub(start)).max(1) as u32
}
