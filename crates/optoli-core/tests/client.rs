use std::sync::{Arc, Mutex};
use std::time::Duration;

use optoli_core::{
	OptoliClient, PowerOnBehavior, PulseConfig, PulseEase, PulseTarget, TransportMode, Update,
	validate_brightness, validate_display_name, validate_temperature, validate_warmth,
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

fn client(port: u16) -> OptoliClient {
	OptoliClient::new(
		"127.0.0.1",
		port,
		Duration::from_secs(2),
		0,
		TransportMode::Native,
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
			duration_ms: 100,
			ease: PulseEase::Linear,
			target: PulseTarget::Brightness,
			value: 80,
		})
		.unwrap();
	assert_eq!(result, original);

	let updates = state.lock().unwrap().updates.clone();
	assert!(
		updates
			.iter()
			.any(|update| { update["lights"][0]["brightness"].as_u64() == Some(80) })
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
