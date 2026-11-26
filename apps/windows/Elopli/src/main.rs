#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod icons;
mod render;
mod session_worker;
mod theme;
mod tray;
mod win32;

use std::time::Duration;

use elopli_core::{Config, ElopliClient, TransportMode, platform_config_path};

fn main() {
	let arguments = std::env::args().skip(1).collect::<Vec<_>>();
	if arguments.iter().any(|argument| argument == "--smoke-test") {
		std::process::exit(smoke_test());
	}

	let autostart = arguments.iter().any(|argument| argument == "--autostart");
	win32::init_process();
	let Some(_instance) = win32::single_instance("Local\\com.bevry.elopli") else {
		win32::show_existing_window(app::WINDOW_CLASS);
		return;
	};
	if let Err(error) = app::run(autostart) {
		win32::message_box("Elopli", &error.to_string());
	}
}

/// Headless hardware check for CI and release validation. Build the debug
/// binary to keep the console subsystem; release builds are GUI-only.
fn smoke_test() -> i32 {
	let path = platform_config_path();
	let config = match Config::load(path) {
		Ok(config) => config,
		Err(error) => {
			eprintln!("config error: {error}");
			return 1;
		}
	};
	println!("config: {}", config.path.display());
	let mut failed = false;
	for device in &config.devices {
		let client = ElopliClient::new(
			device.host.clone(),
			device.port,
			Duration::from_secs(3),
			2,
			TransportMode::Native,
		);
		match client.snapshot() {
			Ok(snapshot) => {
				let wifi = snapshot
					.info
					.wifi
					.as_ref()
					.map_or("-", |wifi| wifi.ssid.as_str());
				println!(
					"{}: {} {}% {}% wifi={wifi}",
					device.name,
					if snapshot.state.on { "on" } else { "off" },
					snapshot.state.brightness,
					snapshot.state.warmth()
				);
			}
			Err(error) => {
				eprintln!("{}: {error}", device.name);
				failed = true;
			}
		}
	}
	if failed { 1 } else { 0 }
}
