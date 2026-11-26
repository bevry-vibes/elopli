use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use elopli_core::{
	ElopliClient, PowerOnBehavior, PulseCancellation, PulseConfig, SweepConfig, TransportMode,
	Update, validate_brightness, validate_display_name, validate_temperature, validate_warmth,
};
use serde_json::{Value, json};
use tiny_http::{Header, Method, Response, Server};

const SERIAL: &str = "TESTSERIAL0001";
const MAC: &str = "AA:BB:CC:DD:EE:FF";

struct FakeState {
	on: u8,
	brightness: u8,
	temperature: u16,
	display_name: String,
	behavior: u8,
	power_on_brightness: u8,
	power_on_temperature: u16,
	last_update: Option<Value>,
	updates: Vec<Value>,
	update_times: Vec<Instant>,
	last_settings: Option<Value>,
}

impl FakeState {
	fn lights(&self) -> Value {
		json!({
			"numberOfLights": 1,
			"lights": [{
				"on": self.on,
				"brightness": self.brightness,
				"temperature": self.temperature,
			}],
		})
	}

	fn info(&self) -> Value {
		json!({
			"productName": "Elgato Key Light",
			"hardwareBoardType": 53,
			"hardwareRevision": "1",
			"macAddress": MAC,
			"firmwareBuildNumber": 222,
			"firmwareVersion": "1.0.3",
			"serialNumber": SERIAL,
			"displayName": self.display_name,
			"features": ["lights"],
			"wifi-info": {
				"ssid": "test",
				"frequencyMHz": 2400,
				"rssi": -40,
			},
		})
	}

	fn settings(&self) -> Value {
		json!({
			"powerOnBehavior": self.behavior,
			"powerOnBrightness": self.power_on_brightness,
			"powerOnTemperature": self.power_on_temperature,
			"switchOnDurationMs": 100,
			"switchOffDurationMs": 300,
			"colorChangeDurationMs": 100,
		})
	}
}

fn json_response(value: Value) -> Response<std::io::Cursor<Vec<u8>>> {
	Response::from_data(serde_json::to_vec(&value).unwrap())
		.with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
}

fn empty_response(status: u16) -> Response<std::io::Cursor<Vec<u8>>> {
	Response::from_data(Vec::new())
		.with_status_code(status)
		.with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
}

fn start_server() -> (u16, Arc<Mutex<FakeState>>) {
	let server = Server::http("127.0.0.1:0").unwrap();
	let port = server.server_addr().to_ip().unwrap().port();
	let state = Arc::new(Mutex::new(FakeState {
		on: 1,
		brightness: 50,
		temperature: 267,
		display_name: "Test".to_string(),
		behavior: 1,
		power_on_brightness: 20,
		power_on_temperature: 213,
		last_update: None,
		updates: Vec::new(),
		update_times: Vec::new(),
		last_settings: None,
	}));
	let shared = Arc::clone(&state);
	std::thread::spawn(move || {
		for mut request in server.incoming_requests() {
			let method = request.method().clone();
			let path = request.url().to_string();
			let mut body = String::new();
			request.as_reader().read_to_string(&mut body).ok();
			let mut state = shared.lock().unwrap();
			let response = match (method, path.as_str()) {
				(Method::Get, "/elgato/lights") => json_response(state.lights()),
				(Method::Put, "/elgato/lights") => {
					let payload: Value = serde_json::from_str(&body).unwrap();
					state.last_update = Some(payload.clone());
					state.updates.push(payload.clone());
					state.update_times.push(Instant::now());
					if let Some(light) = payload
						.get("lights")
						.and_then(Value::as_array)
						.and_then(|lights| lights.first())
					{
						if let Some(on) = light.get("on").and_then(Value::as_u64) {
							state.on = on as u8;
						}
						if let Some(brightness) = light.get("brightness").and_then(Value::as_u64) {
							state.brightness = brightness as u8;
							if state.brightness == 0 {
								state.on = 0;
							}
						}
						if let Some(temperature) = light.get("temperature").and_then(Value::as_u64)
						{
							state.temperature = temperature as u16;
						}
					}
					json_response(state.lights())
				}
				(Method::Get, "/elgato/accessory-info") => json_response(state.info()),
				(Method::Put, "/elgato/accessory-info") => {
					let payload: Value = serde_json::from_str(&body).unwrap();
					if let Some(name) = payload.get("displayName").and_then(Value::as_str) {
						state.display_name = name.to_string();
						empty_response(200)
					} else {
						empty_response(400)
					}
				}
				(Method::Get, "/elgato/lights/settings") => json_response(state.settings()),
				(Method::Put, "/elgato/lights/settings") => {
					let payload: Value = serde_json::from_str(&body).unwrap();
					let required = [
						"powerOnBehavior",
						"powerOnBrightness",
						"powerOnTemperature",
						"switchOnDurationMs",
						"switchOffDurationMs",
						"colorChangeDurationMs",
					];
					if required.iter().all(|key| payload.get(*key).is_some()) {
						state.last_settings = Some(payload.clone());
						state.behavior = payload["powerOnBehavior"].as_u64().unwrap() as u8;
						state.power_on_brightness =
							payload["powerOnBrightness"].as_u64().unwrap() as u8;
						state.power_on_temperature =
							payload["powerOnTemperature"].as_u64().unwrap() as u16;
						empty_response(200)
					} else {
						json_response(json!({"errors": [{"message": "Fail to parse JSON data"}]}))
							.with_status_code(400)
					}
				}
				(Method::Post, "/elgato/identify") => empty_response(200),
				_ => empty_response(404),
			};
			request.respond(response).ok();
		}
	});
	(port, state)
}

fn test_state_dir() -> PathBuf {
	static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
	DIRECTORY
		.get_or_init(|| {
			let directory = std::env::temp_dir().join(format!(
				"elopli-tests-{}-{}",
				std::process::id(),
				std::time::SystemTime::now()
					.duration_since(std::time::UNIX_EPOCH)
					.map(|duration| duration.as_nanos())
					.unwrap_or_default()
			));
			std::fs::create_dir_all(&directory).unwrap();
			directory
		})
		.clone()
}

fn client(port: u16) -> ElopliClient {
	ElopliClient::new_with_state_dir(
		"127.0.0.1",
		port,
		Duration::from_secs(2),
		0,
		TransportMode::Native,
		Some(test_state_dir()),
	)
}

#[test]
fn reads_state_and_sends_only_named_fields() {
	let (port, state) = start_server();
	let client = client(port);

	let initial = client.state().unwrap();
	assert!(initial.on);
	assert_eq!(initial.brightness, 50);
	assert_eq!(initial.temperature, 267);
	assert_eq!(initial.warmth(), 62);
	assert_eq!(initial.kelvin(), 3745);

	let updated = client
		.update(Update {
			brightness: Some(80),
			..Update::default()
		})
		.unwrap();
	assert_eq!(updated.brightness, 80);
	assert_eq!(updated.temperature, 267);

	let last = state.lock().unwrap().last_update.clone().unwrap();
	assert_eq!(last, json!({"lights": [{"brightness": 80}]}));
}

#[test]
fn toggles_power() {
	let (port, state) = start_server();
	let client = client(port);
	assert!(!client.toggle().unwrap().on);
	assert_eq!(state.lock().unwrap().on, 0);
	assert!(client.toggle().unwrap().on);
	assert_eq!(state.lock().unwrap().on, 1);
}

#[test]
fn on_and_toggle_floor_brightness_to_visible() {
	let (port, state) = start_server();
	let client = client(port);
	{
		let mut state = state.lock().unwrap();
		state.on = 0;
		state.brightness = 0;
	}

	let on = client.set_on(true).unwrap();
	assert!(on.on);
	assert_eq!(on.brightness, 2);

	{
		let mut state = state.lock().unwrap();
		state.on = 1;
		state.brightness = 0;
	}
	let on = client.toggle().unwrap();
	assert!(on.on);
	assert_eq!(on.brightness, 2);
}

#[test]
fn discards_identity_fields() {
	let (port, _) = start_server();
	let info = client(port).info().unwrap();
	let payload = info.to_json().to_string();
	assert!(!payload.contains(SERIAL));
	assert!(!payload.contains(MAC));
	assert_eq!(info.firmware(), "1.0.3 (222)");
	assert_eq!(info.wifi.unwrap().quality(), "excellent");
}

#[test]
fn renames_and_reads_back() {
	let (port, _) = start_server();
	let client = client(port);
	assert_eq!(client.rename("Desk Lamp").unwrap(), "Desk Lamp");
	assert_eq!(client.info().unwrap().display_name, "Desk Lamp");
}

#[test]
fn round_trips_power_on_settings() {
	let (port, state) = start_server();
	let client = client(port);

	let settings = client
		.set_power_on(PowerOnBehavior::RestoreDefaults, Some(25), Some(244))
		.unwrap();
	assert_eq!(settings.behavior, PowerOnBehavior::RestoreDefaults);
	assert_eq!(settings.brightness, 25);
	assert_eq!(settings.temperature, 244);
	assert_eq!(state.lock().unwrap().behavior, 2);

	let last = client
		.set_power_on(PowerOnBehavior::RestoreLast, None, None)
		.unwrap();
	assert_eq!(last.behavior, PowerOnBehavior::RestoreLast);
	assert_eq!(last.brightness, 25);
	assert_eq!(last.temperature, 244);
}

#[test]
fn pulse_restores_the_original_state() {
	let (port, state) = start_server();
	let client = client(port);
	let original = client.state().unwrap();

	let result = client
		.pulse(PulseConfig {
			count: 1,
			duration_ms: 300,
			value: 80,
			low: 2,
		})
		.unwrap();
	assert!(result.completed());
	assert_eq!(result.state(), &original);

	let updates = state.lock().unwrap().updates.clone();
	assert!(
		updates
			.iter()
			.any(|update| { update["lights"][0]["brightness"].as_u64() == Some(80) })
	);
	assert_eq!(client.state().unwrap(), original);
}

#[test]
fn actions_cancel_a_running_pulse_and_win() {
	let (port, state) = start_server();
	let pulse_client = Arc::new(client(port));
	let handle = std::thread::spawn(move || {
		pulse_client
			.pulse(PulseConfig {
				count: 3,
				duration_ms: 1_000,
				value: 100,
				low: 2,
			})
			.unwrap()
	});

	std::thread::sleep(Duration::from_millis(150));
	let action = client(port)
		.update(Update {
			brightness: Some(23),
			..Update::default()
		})
		.unwrap();
	assert_eq!(action.brightness, 23);

	let outcome = handle.join().unwrap();
	assert!(!outcome.completed());
	assert_eq!(outcome.cancellation(), Some(PulseCancellation::Superseded));
	assert_eq!(state.lock().unwrap().brightness, 23);
}

#[test]
fn frames_respect_the_ten_hertz_floor() {
	let (port, state) = start_server();
	let client = client(port);
	client
		.pulse(PulseConfig {
			count: 3,
			duration_ms: 700,
			value: 100,
			low: 2,
		})
		.unwrap();
	let times = state.lock().unwrap().update_times.clone();
	assert!(times.len() >= 7);
	for pair in times.windows(2) {
		let gap = pair[1].duration_since(pair[0]);
		assert!(
			gap >= Duration::from_millis(80),
			"frames only {gap:?} apart"
		);
	}
}

#[test]
fn cancel_pulse_restores_the_original_state() {
	let (port, state) = start_server();
	let pulse_client = Arc::new(client(port));
	let original = pulse_client.state().unwrap();
	let handle = std::thread::spawn(move || {
		pulse_client
			.pulse(PulseConfig {
				count: 3,
				duration_ms: 2_000,
				value: 100,
				low: 2,
			})
			.unwrap()
	});

	std::thread::sleep(Duration::from_millis(150));
	client(port).cancel_pulse().unwrap();

	let outcome = handle.join().unwrap();
	assert!(!outcome.completed());
	assert_eq!(outcome.cancellation(), Some(PulseCancellation::Superseded));
	assert_eq!(outcome.state(), &original);
	assert_eq!(state.lock().unwrap().brightness, original.brightness);
}

#[test]
fn pulse_stops_and_preserves_an_external_change() {
	let (port, state) = start_server();
	let pulse_client = Arc::new(client(port));
	let handle = std::thread::spawn(move || {
		pulse_client
			.pulse(PulseConfig {
				count: 3,
				duration_ms: 1_000,
				value: 100,
				low: 2,
			})
			.unwrap()
	});

	for _ in 0..100 {
		if state.lock().unwrap().updates.len() >= 2 {
			break;
		}
		std::thread::sleep(Duration::from_millis(20));
	}
	{
		let mut state = state.lock().unwrap();
		state.on = 1;
		state.brightness = 33;
	}

	let outcome = handle.join().unwrap();
	assert!(!outcome.completed());
	assert_eq!(
		outcome.cancellation(),
		Some(PulseCancellation::ExternalChange)
	);
	assert_eq!(outcome.state().brightness, 33);
	assert_eq!(state.lock().unwrap().brightness, 33);
}

#[test]
fn pulse_runs_peak_to_minimum_once_per_count() {
	let (port, state) = start_server();
	let client = client(port);
	let original = client.state().unwrap();

	let result = client
		.pulse(PulseConfig {
			count: 3,
			duration_ms: 700,
			value: 80,
			low: 2,
		})
		.unwrap();
	assert!(result.completed());
	assert_eq!(result.state(), &original);

	let updates = state.lock().unwrap().updates.clone();
	let minima = updates
		.iter()
		.filter(|update| update["lights"][0]["brightness"].as_u64() == Some(2))
		.count();
	assert_eq!(minima, 3);
	assert_eq!(client.state().unwrap(), original);
}

#[test]
fn sweep_covers_cool_and_warm_and_restores() {
	let (port, state) = start_server();
	let client = client(port);
	let original = client.state().unwrap();

	let result = client
		.sweep(SweepConfig {
			count: 1,
			duration_ms: 300,
			brightness: None,
		})
		.unwrap();
	assert!(result.completed());
	assert_eq!(result.state(), &original);

	let updates = state.lock().unwrap().updates.clone();
	assert!(
		updates
			.iter()
			.any(|update| update["lights"][0]["temperature"].as_u64() == Some(143))
	);
	assert!(
		updates
			.iter()
			.any(|update| update["lights"][0]["temperature"].as_u64() == Some(344))
	);
	assert_eq!(client.state().unwrap(), original);
}

#[test]
fn off_light_sweep_ramps_brightness_before_temperature() {
	let (port, state) = start_server();
	let client = client(port);
	client.set_on(false).unwrap();
	let original = client.state().unwrap();
	let before = state.lock().unwrap().updates.len();

	client
		.sweep(SweepConfig {
			count: 1,
			duration_ms: 1_000,
			brightness: None,
		})
		.unwrap();

	let updates = state.lock().unwrap().updates[before..].to_vec();
	let first = &updates[0]["lights"][0];
	assert!(first["brightness"].as_u64().is_some_and(|value| value <= 2));
	assert_eq!(first["temperature"].as_u64(), Some(344));
	let first_cool = updates.iter().position(|update| {
		update["lights"][0]["temperature"]
			.as_u64()
			.is_some_and(|temperature| temperature < 344)
	});
	if let Some(index) = first_cool {
		assert!(updates[..index].iter().any(|update| {
			update["lights"][0]["brightness"]
				.as_u64()
				.is_some_and(|brightness| brightness > 2)
		}));
	}
	assert_eq!(client.state().unwrap(), original);
}

#[test]
fn sweep_holds_brightness_and_restores_it() {
	let (port, state) = start_server();
	let client = client(port);
	let original = client.state().unwrap();

	client
		.sweep(SweepConfig {
			count: 1,
			duration_ms: 300,
			brightness: Some(30),
		})
		.unwrap();

	let updates = state.lock().unwrap().updates.clone();
	assert!(
		updates
			.iter()
			.any(|update| update["lights"][0]["brightness"].as_u64() == Some(30))
	);
	assert_eq!(client.state().unwrap(), original);
}

#[test]
fn long_sweep_sends_immediate_frames() {
	let (port, state) = start_server();
	let client = client(port);
	let original = client.state().unwrap();

	client
		.sweep(SweepConfig {
			count: 1,
			duration_ms: 6_000,
			brightness: None,
		})
		.unwrap();

	let updates = state.lock().unwrap().updates.clone();
	let durations: Vec<u64> = updates
		.iter()
		.filter_map(|update| update["lights"][0]["durationMs"].as_u64())
		.collect();
	assert!(durations.is_empty());
	assert!(updates.len() >= 6);
	assert_eq!(client.state().unwrap(), original);
}

#[test]
fn pulse_can_turn_the_light_off_at_the_low() {
	let (port, state) = start_server();
	let client = client(port);
	let original = client.state().unwrap();

	let result = client
		.pulse(PulseConfig {
			count: 1,
			duration_ms: 300,
			value: 100,
			low: 0,
		})
		.unwrap();
	assert!(result.completed());
	assert_eq!(result.state(), &original);

	let updates = state.lock().unwrap().updates.clone();
	assert!(
		updates
			.iter()
			.any(|update| update["lights"][0]["brightness"].as_u64() == Some(0))
	);
	assert_eq!(client.state().unwrap(), original);
}

#[test]
fn off_light_pulse_starts_and_ends_off() {
	let (port, state) = start_server();
	let client = client(port);
	client.set_on(false).unwrap();
	let original = client.state().unwrap();
	assert!(!original.on);

	let result = client
		.pulse(PulseConfig {
			count: 1,
			duration_ms: 500,
			value: 100,
			low: 0,
		})
		.unwrap();
	assert!(result.completed());
	assert_eq!(result.state(), &original);

	let updates = state.lock().unwrap().updates.clone();
	let first_brightness = updates
		.iter()
		.filter_map(|update| update["lights"][0]["brightness"].as_u64())
		.next()
		.unwrap();
	assert!(
		first_brightness <= 2,
		"off pulse turned on at {first_brightness}%"
	);
	assert_eq!(client.state().unwrap(), original);
}

#[test]
fn validation_rejects_out_of_range_values() {
	assert!(validate_brightness(1.0).is_err());
	assert!(validate_brightness(2.0).is_ok());
	assert!(validate_brightness(101.0).is_err());
	assert!(validate_temperature(100.0).is_err());
	assert!(validate_warmth(101.0).is_err());
	assert!(validate_display_name("").is_err());
	assert!(validate_display_name(&"x".repeat(64)).is_err());
	assert_eq!(validate_display_name("  Desk  ").unwrap(), "Desk");
}
