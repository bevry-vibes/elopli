use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::control::{Control, ControlGuard};
use crate::error::{ElopliError, Result};
use crate::model::{
	AccessoryInfo, LightState, PowerOnBehavior, PowerOnSettings, Snapshot, Update,
	parse_accessory_info, parse_error_messages, parse_power_on_settings, parse_state,
};
use crate::pulse::{FRAME_INTERVAL_MS, MAX_FADE_MS, PulseCancellation, PulseConfig, PulseOutcome};
use crate::sweep::SweepConfig;
use crate::transport::{HttpTransport, TransportMode, build_transport};
use crate::validate::{
	validate_brightness, validate_display_name, validate_temperature, warmth_to_mired,
};

/// How often a running animation checks the light for an external change.
/// The pulse checks every frame for short animations; long pulses and sweeps
/// check at this interval to keep the frame rate high.
const EXTERNAL_CHECK_INTERVAL: Duration = Duration::from_millis(250);

/// Animations send a few keyframes and rely on the light's own fade. That
/// only works over one reused connection; a new connection per keyframe
/// eventually stops the light's HTTP server.
const ANIMATION_TRANSPORT_ERROR: &str = "pulse and sweep need a persistent HTTP connection; the light drops repeated short-lived connections. Use the native transport and grant local network access if the platform asks for it.";

pub struct ElopliClient {
	host: String,
	port: u16,
	timeout: Duration,
	retries: u32,
	retry_delay: Duration,
	transport: Arc<dyn HttpTransport + Send + Sync>,
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
		Self::new_with_state_dir(host, port, timeout, retries, mode, None)
	}

	/// Construct a client with an explicit control/state directory. Tests use
	/// this to avoid writing into the user's real application state.
	pub fn new_with_state_dir(
		host: impl Into<String>,
		port: u16,
		timeout: Duration,
		retries: u32,
		mode: TransportMode,
		state_dir: Option<PathBuf>,
	) -> Self {
		let host = host.into();
		let control = match state_dir {
			Some(directory) => Control::new_in(&host, port, directory),
			None => Control::new(&host, port),
		};
		Self {
			control,
			host,
			port,
			timeout,
			retries,
			retry_delay: Duration::from_millis(500),
			transport: build_transport(mode),
		}
	}

	fn url(&self, path: &str) -> String {
		let host = if self.host.contains(':') && !self.host.starts_with('[') {
			format!("[{}]", self.host)
		} else {
			self.host.clone()
		};
		format!("http://{host}:{}{path}", self.port)
	}

	fn request(&self, method: &str, path: &str, payload: Option<&Value>) -> Result<Value> {
		self.request_with(method, path, payload, false)
	}

	fn request_persistent(
		&self,
		method: &str,
		path: &str,
		payload: Option<&Value>,
	) -> Result<Value> {
		self.request_with(method, path, payload, true)
	}

	fn request_with(
		&self,
		method: &str,
		path: &str,
		payload: Option<&Value>,
		persistent: bool,
	) -> Result<Value> {
		let body = payload
			.map(serde_json::to_vec)
			.transpose()
			.map_err(|error| ElopliError::InvalidValue(error.to_string()))?;
		let url = self.url(path);
		let mut last_error: Option<ElopliError> = None;
		for attempt in 0..=self.retries {
			let response = if persistent {
				self.transport
					.request_persistent(method, &url, body.as_deref(), self.timeout)
			} else {
				self.transport
					.request(method, &url, body.as_deref(), self.timeout)
			};
			match response {
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

	fn state_persistent(&self) -> Result<LightState> {
		parse_state(&self.request_persistent("GET", "/elgato/lights", None)?)
	}

	pub fn update(&self, mut update: Update) -> Result<LightState> {
		if update.is_empty() {
			return Err(ElopliError::InvalidValue(
				"no properties were provided".into(),
			));
		}
		if let Some(brightness) = update.brightness {
			validate_brightness(f64::from(brightness))?;
		}
		if let Some(temperature) = update.temperature {
			validate_temperature(f64::from(temperature))?;
		}
		if let Some(duration_ms) = update.duration_ms
			&& duration_ms > MAX_FADE_MS
		{
			return Err(ElopliError::InvalidValue(format!(
				"transition duration must be {MAX_FADE_MS} ms or less because the firmware snaps at 1000 ms"
			)));
		}
		let _guard = self.begin_action()?;
		if update.on == Some(true) && update.brightness.is_none() {
			let current = self.state()?;
			if current.brightness < 2 {
				update.brightness = Some(2);
			}
		}
		self.write_update(update)
	}

	fn write_update(&self, update: Update) -> Result<LightState> {
		self.send_update(update, false, false)
	}

	/// Pulse frames may reach 0% brightness, which switches the light off.
	fn write_pulse_update(&self, update: Update) -> Result<LightState> {
		self.send_update(update, true, true)
	}

	fn send_update(
		&self,
		update: Update,
		allow_zero_brightness: bool,
		persistent: bool,
	) -> Result<LightState> {
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
			if duration_ms > MAX_FADE_MS {
				return Err(ElopliError::InvalidValue(format!(
					"transition duration must be {MAX_FADE_MS} ms or less because the firmware snaps at 1000 ms"
				)));
			}
			light.insert("durationMs".into(), json!(duration_ms));
		}
		let payload = json!({ "lights": [Value::Object(light)] });
		let response = if persistent {
			self.request_persistent("PUT", "/elgato/lights", Some(&payload))
		} else {
			self.request("PUT", "/elgato/lights", Some(&payload))
		};
		parse_state(&response?)
	}

	pub fn set_on(&self, on: bool) -> Result<LightState> {
		self.update(Update {
			on: Some(on),
			..Update::default()
		})
	}

	pub fn toggle(&self) -> Result<LightState> {
		let _guard = self.begin_action()?;
		let current = self.state()?;
		let visible = current.on && current.brightness >= 2;
		let mut update = Update {
			on: Some(!visible),
			..Update::default()
		};
		if !visible && current.brightness < 2 {
			update.brightness = Some(2);
		}
		self.write_update(update)
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
		let brightness = brightness
			.map(|value| validate_brightness(f64::from(value)))
			.transpose()?;
		let temperature = temperature
			.map(|value| validate_temperature(f64::from(value)))
			.transpose()?;
		let _guard = self.begin_action()?;
		let settings = self.request("GET", "/elgato/lights/settings", None)?;
		let mut settings = settings.as_object().cloned().ok_or_else(|| {
			ElopliError::InvalidResponse("settings response is not an object".into())
		})?;
		settings.insert("powerOnBehavior".into(), json!(behavior as u8));
		if let Some(brightness) = brightness {
			settings.insert("powerOnBrightness".into(), json!(brightness));
		}
		if let Some(temperature) = temperature {
			settings.insert("powerOnTemperature".into(), json!(temperature));
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
		self.require_persistent_transport()?;
		let (_guard, token) = match self.control.acquire() {
			Ok(value) => value,
			Err(ElopliError::Superseded) => {
				return Ok(PulseOutcome::Cancelled {
					state: self.state()?,
					reason: PulseCancellation::Superseded,
				});
			}
			Err(error) => return Err(error),
		};
		let original = self.state_persistent()?;
		self.require_persistent_transport()?;
		let result = self.run_pulse(config, original, &token);
		match &result {
			Err(_) => {
				let _ = self.restore_state(original);
			}
			Ok(PulseOutcome::Cancelled {
				reason: PulseCancellation::Superseded,
				..
			}) => {
				let _ = self.restore_state(original);
				return Ok(PulseOutcome::Cancelled {
					state: original,
					reason: PulseCancellation::Superseded,
				});
			}
			_ => {}
		}
		result
	}

	/// Sweep the temperature across the full cool-to-warm range.
	pub fn sweep(&self, config: SweepConfig) -> Result<PulseOutcome> {
		config.validate()?;
		self.require_persistent_transport()?;
		let (_guard, token) = match self.control.acquire() {
			Ok(value) => value,
			Err(ElopliError::Superseded) => {
				return Ok(PulseOutcome::Cancelled {
					state: self.state()?,
					reason: PulseCancellation::Superseded,
				});
			}
			Err(error) => return Err(error),
		};
		let original = self.state_persistent()?;
		self.require_persistent_transport()?;
		let result = self.run_sweep(config, original, &token);
		match &result {
			Err(_) => {
				let _ = self.restore_state(original);
			}
			Ok(PulseOutcome::Cancelled {
				reason: PulseCancellation::Superseded,
				..
			}) => {
				let _ = self.restore_state(original);
				return Ok(PulseOutcome::Cancelled {
					state: original,
					reason: PulseCancellation::Superseded,
				});
			}
			_ => {}
		}
		result
	}

	/// Stop a running pulse or sweep. The animation restores the original
	/// state; an external change is preserved instead.
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
		let originally_on = original.on && original.brightness >= 2;
		let start_brightness = config.brightness.unwrap_or(if originally_on {
			original.brightness
		} else {
			original.brightness.max(2)
		});
		let final_brightness = if originally_on {
			original.brightness
		} else {
			0
		};
		let final_on = if originally_on { Some(true) } else { None };
		let mut expected = original;
		let mut last_external_check = Instant::now();
		let mut durations =
			config.segment_durations(original.temperature, original.brightness, originally_on)?;

		if !originally_on {
			// Start an off light at the warm end of the sweep. Bring it up to
			// its working brightness first, then keep the rest of the initial
			// budget for the first warm-to-cool fade.
			let turn_on_ms = durations[0].saturating_sub(100).min(1_000);
			let initial_brightness = if turn_on_ms > 0 { 2 } else { start_brightness };
			// Stage the visible start while the light is still off. This
			// discards the stored temperature and prevents a power-on flash at
			// the stored brightness.
			self.write_pulse_update(Update {
				on: Some(false),
				brightness: Some(initial_brightness),
				temperature: Some(warm),
				..Update::default()
			})?;
			expected = self.write_pulse_update(Update {
				on: Some(true),
				..Update::default()
			})?;
			if turn_on_ms > 0
				&& start_brightness > 2
				&& let Some(outcome) = self.ramp_to(
					token,
					expected,
					start_brightness,
					warm,
					Some(true),
					turn_on_ms,
					0,
					false,
					true,
					false,
					&mut expected,
					Some(&mut last_external_check),
				)? {
				return Ok(outcome);
			}
			let leftover = durations[0].saturating_sub(turn_on_ms);
			durations[1] = durations[1].saturating_add(leftover);
		} else if config.brightness.is_some() {
			expected = self.write_pulse_update(Update {
				on: Some(true),
				brightness: Some(start_brightness),
				..Update::default()
			})?;
		}

		// An on light moves from its current temperature to the warm end first.
		// An off light was already placed at the warm end above.
		if originally_on
			&& let Some(outcome) = self.ramp_to(
				token,
				expected,
				start_brightness,
				warm,
				Some(true),
				durations[0],
				0,
				false,
				false,
				true,
				&mut expected,
				Some(&mut last_external_check),
			)? {
			return Ok(outcome);
		}

		for cycle in 0..config.count {
			// Each sweep is a warm-to-cool transition.
			if let Some(outcome) = self.ramp_to(
				token,
				expected,
				start_brightness,
				cool,
				Some(true),
				durations[(1 + cycle * 2) as usize],
				0,
				false,
				false,
				true,
				&mut expected,
				Some(&mut last_external_check),
			)? {
				return Ok(outcome);
			}
			if cycle + 1 < config.count
				&& let Some(outcome) = self.ramp_to(
					token,
					expected,
					start_brightness,
					warm,
					Some(true),
					durations[(2 + cycle * 2) as usize],
					0,
					false,
					false,
					true,
					&mut expected,
					Some(&mut last_external_check),
				)? {
				return Ok(outcome);
			}
		}

		// Return to the original values for an on light. An off light stays at
		// the cool end while its brightness fades out.
		if let Some(outcome) = self.ramp_to(
			token,
			expected,
			final_brightness,
			original.temperature,
			final_on,
			durations[(segments - 1) as usize],
			0,
			false,
			true,
			originally_on,
			&mut expected,
			Some(&mut last_external_check),
		)? {
			return Ok(outcome);
		}

		if let Some(outcome) = self.cancelled(token, &expected)? {
			return Ok(outcome);
		}
		// The final ramp already targeted the original values. Restoring
		// immediately would snap the last part of that fade. Restore the power
		// state when the light was not visibly on before the sweep.
		if !originally_on {
			self.restore_state(original)?;
		}
		Ok(PulseOutcome::Completed(original))
	}

	fn begin_action(&self) -> Result<ControlGuard> {
		let (guard, _) = self.control.acquire()?;
		Ok(guard)
	}

	fn require_persistent_transport(&self) -> Result<()> {
		if self.transport.persistent() {
			Ok(())
		} else {
			Err(ElopliError::Unsupported(ANIMATION_TRANSPORT_ERROR.into()))
		}
	}

	fn run_pulse(
		&self,
		config: PulseConfig,
		original: LightState,
		token: &str,
	) -> Result<PulseOutcome> {
		let originally_on = original.on && original.brightness >= 2;
		let mut plan = config.plan(originally_on, original.brightness);
		let check_every_frame = config.duration_ms <= 2_000;
		let mut expected = original;
		let mut from = LightState {
			brightness: if originally_on {
				original.brightness
			} else {
				0
			},
			..original
		};
		let mut last_external_check = Instant::now();

		if !originally_on {
			// Turn on at the 2% floor first, then rise. This avoids jumping on
			// at the stored brightness.
			let turn_on_ms = plan
				.first()
				.map(|segment| segment.duration_ms.min(120))
				.unwrap_or(0);
			if turn_on_ms > 0 {
				let state = self.write_pulse_update(Update {
					on: Some(true),
					brightness: Some(2),
					..Update::default()
				})?;
				std::thread::sleep(Duration::from_millis(u64::from(turn_on_ms)));
				expected = state;
				from = LightState {
					brightness: 2,
					..state
				};
				plan[0].duration_ms = plan[0].duration_ms.saturating_sub(turn_on_ms).max(1);
			}
		}

		for segment in plan {
			if let Some(outcome) = self.ramp_to(
				token,
				from,
				segment.target.brightness,
				original.temperature,
				segment.target.on,
				segment.duration_ms,
				segment.hold_ms,
				check_every_frame,
				true,
				false,
				&mut expected,
				Some(&mut last_external_check),
			)? {
				return Ok(outcome);
			}
			from = expected;
		}

		if let Some(outcome) = self.cancelled(token, &expected)? {
			return Ok(outcome);
		}
		// A visible light already ended at its original value. An off light
		// ended at 0%, so confirm the exact original power state.
		if !originally_on {
			self.restore_state(original)?;
		}
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
		hold_ms: u32,
		check_every_frame: bool,
		includes_brightness: bool,
		includes_temperature: bool,
		expected: &mut LightState,
		mut last_external_check: Option<&mut Instant>,
	) -> Result<Option<PulseOutcome>> {
		// Send immediate keyframes at a fixed interval. The light applies each
		// step at once and its stored colorChangeDurationMs smooths the change.
		// Use one frame per percent of brightness, or per mired of temperature,
		// when the duration allows it. Otherwise cap the rate at 10 Hz.
		let hold_ms = hold_ms.min(ramp_ms.saturating_sub(1));
		let motion_ms = ramp_ms.saturating_sub(hold_ms).max(1);
		let distance = match (includes_brightness, includes_temperature) {
			(true, true) => u32::from(to_brightness.abs_diff(from.brightness))
				.max(u32::from(to_temperature.abs_diff(from.temperature))),
			(true, false) => u32::from(to_brightness.abs_diff(from.brightness)),
			(false, true) => u32::from(to_temperature.abs_diff(from.temperature)),
			(false, false) => 1,
		};
		let rate_steps = (motion_ms / FRAME_INTERVAL_MS).max(1);
		let steps = distance.max(1).min(rate_steps).max(1);
		let step_ms = motion_ms.div_ceil(steps).max(1);
		let started = Instant::now();

		for step in 1..=steps {
			self.require_persistent_transport()?;
			if self.control.token()? != token {
				return Ok(Some(PulseOutcome::Cancelled {
					state: self.state_persistent()?,
					reason: PulseCancellation::Superseded,
				}));
			}
			let external_change = if check_every_frame {
				let current = self.state_persistent()?;
				(current != *expected).then_some(current)
			} else {
				match last_external_check.as_deref_mut() {
					Some(last_check) if last_check.elapsed() >= EXTERNAL_CHECK_INTERVAL => {
						let current = self.state_persistent()?;
						*last_check = Instant::now();
						(current != *expected).then_some(current)
					}
					_ => None,
				}
			};
			if let Some(state) = external_change {
				return Ok(Some(PulseOutcome::Cancelled {
					state,
					reason: PulseCancellation::ExternalChange,
				}));
			}
			let progress = f64::from(step) / f64::from(steps);
			let mut update = Update {
				on,
				..Update::default()
			};
			if includes_brightness {
				update.brightness = Some(lerp_u8(from.brightness, to_brightness, progress));
			}
			if includes_temperature {
				update.temperature = Some(lerp_u16(from.temperature, to_temperature, progress));
			}
			*expected = self.write_pulse_update(update)?;
			let deadline = started + Duration::from_millis(u64::from(step_ms) * u64::from(step));
			if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
				std::thread::sleep(remaining);
			}
		}
		// Let the final step reach the endpoint, then hold it for the rest of
		// the segment. This is what makes the 2% minimum visible.
		let deadline = started + Duration::from_millis(u64::from(ramp_ms));
		if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
			std::thread::sleep(remaining);
		}
		Ok(None)
	}

	/// Return an outcome when a pulse should stop, or keep going.
	fn cancelled(&self, token: &str, expected: &LightState) -> Result<Option<PulseOutcome>> {
		if self.control.token()? != token {
			return Ok(Some(PulseOutcome::Cancelled {
				state: self.state_persistent()?,
				reason: PulseCancellation::Superseded,
			}));
		}
		let current = self.state_persistent()?;
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
