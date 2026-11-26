use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use mdns_sd::{RecvTimeoutError, ServiceDaemon, ServiceEvent};

use crate::error::{ElopliError, Result};
use crate::model::DiscoveredDevice;

const SERVICE: &str = "_elg._tcp.local.";

/// Rank candidate addresses so discovery is deterministic when a light
/// advertises more than one IPv4 address. Private LAN addresses win, then
/// link-local, then any other unicast address, with loopback last.
fn address_rank(address: Ipv4Addr) -> Option<(u8, u32)> {
	if address.is_unspecified()
		|| address.is_broadcast()
		|| address.is_multicast()
		|| address == Ipv4Addr::new(255, 255, 255, 255)
	{
		return None;
	}
	let rank = if address.is_private() {
		0
	} else if address.is_link_local() {
		1
	} else if address.is_loopback() {
		3
	} else {
		2
	};
	Some((rank, u32::from(address)))
}

fn select_address(addresses: impl IntoIterator<Item = Ipv4Addr>) -> Option<Ipv4Addr> {
	addresses
		.into_iter()
		.filter_map(|address| address_rank(address).map(|rank| (rank, address)))
		.min_by_key(|(rank, _)| *rank)
		.map(|(_, address)| address)
}

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
				let address = select_address(info.get_addresses_v4());
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
			Err(RecvTimeoutError::Timeout) => break,
			Err(RecvTimeoutError::Disconnected) => {
				return Err(ElopliError::Discovery(
					"the mDNS service daemon stopped before the browse finished".into(),
				));
			}
		}
	}
	let shutdown = daemon.shutdown();
	let mut devices = found.into_values().collect::<Vec<_>>();
	devices.sort_by(|left, right| left.instance.cmp(&right.instance));
	if devices.is_empty()
		&& let Err(error) = shutdown
	{
		return Err(ElopliError::Discovery(format!(
			"mDNS browse returned no devices and the service daemon did not shut down cleanly: {error}"
		)));
	}
	Ok(devices)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn prefers_private_addresses_in_a_stable_order() {
		let addresses = [
			Ipv4Addr::new(169, 254, 10, 20),
			Ipv4Addr::new(192, 0, 2, 30),
			Ipv4Addr::new(192, 168, 1, 20),
			Ipv4Addr::new(10, 0, 0, 5),
		];
		assert_eq!(select_address(addresses), Some(Ipv4Addr::new(10, 0, 0, 5)));
	}

	#[test]
	fn ignores_unusable_addresses() {
		let addresses = [
			Ipv4Addr::UNSPECIFIED,
			Ipv4Addr::BROADCAST,
			Ipv4Addr::new(224, 0, 0, 1),
		];
		assert_eq!(select_address(addresses), None);
	}

	#[test]
	fn uses_loopback_only_as_a_last_resort() {
		let addresses = [Ipv4Addr::LOCALHOST, Ipv4Addr::new(198, 51, 100, 7)];
		assert_eq!(
			select_address(addresses),
			Some(Ipv4Addr::new(198, 51, 100, 7))
		);
		assert_eq!(
			select_address([Ipv4Addr::LOCALHOST]),
			Some(Ipv4Addr::LOCALHOST)
		);
	}
}
