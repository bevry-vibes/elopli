#![cfg(feature = "linux")]

mod app;
mod autostart;
mod sheets;
mod slider;
mod theme;
mod tray;
mod worker;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use elopli_core::{Config, ElopliClient, TransportMode, platform_config_path};
use gtk::prelude::*;
use gtk::{gio, glib};

fn main() {
	let arguments = std::env::args().skip(1).collect::<Vec<_>>();
	if arguments.iter().any(|argument| argument == "--smoke-test") {
		std::process::exit(smoke_test());
	}
	if arguments.iter().any(|argument| argument == "--version") {
		println!("Elopli {}", env!("CARGO_PKG_VERSION"));
		return;
	}

	glib::set_application_name("Elopli");
	let application = gtk::Application::builder()
		.application_id("com.bevry.elopli")
		.flags(gio::ApplicationFlags::empty())
		.build();

	let autostart = arguments.iter().any(|argument| argument == "--autostart");
	let first_activation = Rc::new(Cell::new(true));
	application.connect_activate(move |application| {
		let start_hidden = autostart && first_activation.replace(false);
		app::activate(application, start_hidden);
	});
	let _ = application.run_with_args(&["elopli-app"]);
}

/// Headless hardware check for CI and release validation.
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
