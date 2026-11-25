use std::collections::HashMap;
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent};

use crate::error::{ElopliError, Result};
use crate::model::DiscoveredDevice;

const SERVICE: &str = "_elg._tcp.local.";

fn product_name(board_type: Option<u16>, model: &str) -> String {
	match board_type {
		Some(53) => "Elgato Key Light".to_string(),
		Some(200) => "Elgato Key Light Air".to_string(),
		_ => {
			let mut parts = model.split_whitespace().collect::<Vec<_>>();
			if parts.last().is_some_and(|part| {
				part.len() >= 6
					&& part
						.chars()
						.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
			}) {
				parts.pop();
			}
			parts.join(" ")
		}
	}
}

pub fn discover(timeout: Duration) -> Result<Vec<DiscoveredDevice>> {
	let daemon = ServiceDaemon::new().map_err(|error| ElopliError::Discovery(error.to_string()))?;
	let receiver = daemon
		.browse(SERVICE)
		.map_err(|error| ElopliError::Discovery(error.to_string()))?;
	let deadline = Instant::now() + timeout;
	let mut found: HashMap<String, DiscoveredDevice> = HashMap::new();
	loop {
		let remaining = deadline.saturating_duration_since(Instant::now());
		if remaining.is_zero() {
			break;
		}
		match receiver.recv_timeout(remaining) {
			Ok(ServiceEvent::ServiceResolved(info)) => {
				let fullname = info.get_fullname().to_string();
				let instance = fullname
					.split("._elg._tcp.local.")
					.next()
					.unwrap_or(&fullname)
					.to_string();
				let board_type = info
					.get_property_val_str("dt")
					.and_then(|value| value.parse::<u16>().ok());
				let model = info
					.get_property_val_str("md")
					.map(str::to_string)
					.unwrap_or_default();
				let address = info.get_addresses_v4().into_iter().next();
				found.insert(
					fullname,
					DiscoveredDevice {
						instance,
						hostname: info.get_hostname().trim_end_matches('.').to_string(),
						address,
						port: info.get_port(),
						product: product_name(board_type, &model),
						board_type,
						model: model
							.split_whitespace()
							.last()
							.map(str::to_string)
							.filter(|value| !value.is_empty()),
					},
				);
			}
			Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
				found.remove(&fullname);
			}
			Ok(_) => {}
			Err(_) => break,
		}
	}
	let _ = daemon.shutdown();
	let mut devices = found.into_values().collect::<Vec<_>>();
	devices.sort_by(|left, right| left.instance.cmp(&right.instance));
	Ok(devices)
}
