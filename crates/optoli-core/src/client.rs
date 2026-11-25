use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::error::{OptoliError, Result};
use crate::model::{
	AccessoryInfo, LightState, PowerOnBehavior, PowerOnSettings, Snapshot, Update,
	parse_accessory_info, parse_error_messages, parse_power_on_settings, parse_state,
};
use crate::pulse::PulseConfig;
use crate::transport::{HttpTransport, TransportMode, build_transport};
use crate::validate::{
	validate_brightness, validate_display_name, validate_temperature, warmth_to_mired,
};

pub struct OptoliClient {
	host: String,
	port: u16,
	timeout: Duration,
	retries: u32,
	retry_delay: Duration,
	transport: Box<dyn HttpTransport + Send + Sync>,
}

impl OptoliClient {
	pub fn new(
		host: impl Into<String>,
		port: u16,
		timeout: Duration,
		retries: u32,
		mode: TransportMode,
	) -> Self {
		Self {
			host: host.into(),
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
			.map_err(|error| OptoliError::InvalidValue(error.to_string()))?;
		let url = self.url(path);
		let mut last_error: Option<OptoliError> = None;
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
						let error = OptoliError::api(response.status, &messages);
						if response.status >= 500 && attempt < self.retries {
							last_error = Some(error);
						} else {
							return Err(error);
						}
					} else if response.body.is_empty() {
						return Ok(Value::Null);
					} else {
						return serde_json::from_slice(&response.body)
							.map_err(|error| OptoliError::InvalidResponse(error.to_string()));
					}
				}
				Err(error) => {
					last_error = Some(OptoliError::Connection(error.message));
				}
			}
			if attempt < self.retries {
				std::thread::sleep(self.retry_delay * (attempt + 1));
			}
		}
		Err(last_error.unwrap_or_else(|| OptoliError::Connection("request failed".into())))
	}

	pub fn state(&self) -> Result<LightState> {
		parse_state(&self.request("GET", "/elgato/lights", None)?)
	}

	pub fn update(&self, update: Update) -> Result<LightState> {
		if update.is_empty() {
			return Err(OptoliError::InvalidValue(
				"no properties were provided".into(),
			));
		}
		let mut light = Map::new();
		if let Some(on) = update.on {
			light.insert("on".into(), json!(u8::from(on)));
		}
		if let Some(brightness) = update.brightness {
			light.insert(
				"brightness".into(),
				json!(validate_brightness(f64::from(brightness))?),
			);
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
		let settings = self.request("GET", "/elgato/lights/settings", None)?;
		let mut settings = settings.as_object().cloned().ok_or_else(|| {
			OptoliError::InvalidResponse("settings response is not an object".into())
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
		self.request(
			"PUT",
			"/elgato/accessory-info",
			Some(&json!({ "displayName": name })),
		)?;
		Ok(self.info()?.display_name)
	}

	pub fn pulse(&self, config: PulseConfig) -> Result<LightState> {
		config.validate()?;
		let original = self.state()?;
		let result = self.run_pulse(config, original);
		let restore = self.restore_state(original);
		match (result, restore) {
			(Err(error), _) => Err(error),
			(Ok(()), Err(error)) => Err(error),
			(Ok(()), Ok(())) => Ok(original),
		}
	}

	fn run_pulse(&self, config: PulseConfig, original: LightState) -> Result<()> {
		let base_brightness = original.brightness.max(2);
		let base_temperature = original.temperature;
		let peak_brightness = if config.target.includes_brightness() {
			config.value.clamp(2, 100)
		} else {
			base_brightness
		};
		let peak_temperature = if config.target.includes_temperature() {
			warmth_to_mired(f64::from(config.value))?
		} else {
			base_temperature
		};

		if !original.on {
			self.update(Update {
				on: Some(true),
				brightness: Some(base_brightness),
				temperature: Some(base_temperature),
				..Update::default()
			})?;
		}

		let ramp_ms = config.duration_ms / 2;
		let steps = (ramp_ms / 50).max(2);
		let step_ms = (ramp_ms / steps).max(10);

		for _ in 0..config.count {
			for step in 1..=steps {
				let progress = config.ease.apply(f64::from(step) / f64::from(steps));
				let brightness = lerp_u8(base_brightness, peak_brightness, progress);
				let temperature = lerp_u16(base_temperature, peak_temperature, progress);
				self.update(Update {
					brightness: Some(brightness),
					temperature: Some(temperature),
					duration_ms: Some(step_ms),
					..Update::default()
				})?;
				std::thread::sleep(std::time::Duration::from_millis(u64::from(step_ms)));
			}
			for step in 1..=steps {
				let progress = config.ease.apply(f64::from(step) / f64::from(steps));
				let brightness = lerp_u8(peak_brightness, base_brightness, progress);
				let temperature = lerp_u16(peak_temperature, base_temperature, progress);
				self.update(Update {
					brightness: Some(brightness),
					temperature: Some(temperature),
					duration_ms: Some(step_ms),
					..Update::default()
				})?;
				std::thread::sleep(std::time::Duration::from_millis(u64::from(step_ms)));
			}
		}
		Ok(())
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
