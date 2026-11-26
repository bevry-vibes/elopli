use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use elopli_core::{
	Config, Device, DiscoveredDevice, ElopliClient, ElopliError, LightState, PowerOnBehavior,
	PowerOnSettings, PulseConfig, PulseOutcome, Result, Snapshot, SweepConfig, Update, discover,
	expand_tilde, kelvin_to_mired, platform_config_path, warmth_to_mired,
};

use crate::link::{LinkGraph, LinkGroup, LinkKind, LinkTarget};
use crate::slider::{SliderKey, SliderQueue};
use crate::{ClientOptions, LIVE_CLEAR_DELAY, LightDevice};

/// A light's address, copied out of the session for a background animation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceTarget {
	pub name: String,
	pub host: String,
	pub port: u16,
}

impl From<&LightDevice> for DeviceTarget {
	fn from(device: &LightDevice) -> Self {
		Self {
			name: device.name.clone(),
			host: device.host.clone(),
			port: device.port,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryState {
	Added,
	Missing,
	New,
}

/// One row in the discovery sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryRow {
	pub id: String,
	pub name: String,
	pub detail: String,
	pub state: DiscoveryState,
	pub needs_update: bool,
	pub discovered: Option<DiscoveredDevice>,
}

/// Everything a render thread needs for one frame.
#[derive(Debug, Clone)]
pub struct SessionSnapshot {
	pub config_path: PathBuf,
	pub devices: Vec<LightDevice>,
	pub groups: Vec<LinkGroup>,
	pub pending_links: Vec<LinkTarget>,
	pub pulsing: HashSet<String>,
	pub live_brightness: HashMap<String, u8>,
	pub live_warmth: HashMap<String, u8>,
	pub discovered: Vec<DiscoveredDevice>,
	pub discovery_rows: Vec<DiscoveryRow>,
	pub error: Option<String>,
	pub refreshing: bool,
	pub scanning: bool,
}

/// The shared UI session. Keep one on a worker thread.
pub struct Session {
	config: Config,
	devices: Vec<LightDevice>,
	revisions: HashMap<String, u64>,
	links: LinkGraph,
	links_path: PathBuf,
	options: ClientOptions,
	pulsing: HashSet<String>,
	live_brightness: HashMap<String, u8>,
	live_warmth: HashMap<String, u8>,
	live_at: HashMap<(String, LinkKind), Instant>,
	sliders: SliderQueue,
	discovered: Vec<DiscoveredDevice>,
	error: Option<String>,
	refreshing: bool,
	scanning: bool,
}

impl Session {
	pub fn load(path: Option<PathBuf>) -> Result<Self> {
		let path = path.map(expand_tilde).unwrap_or_else(platform_config_path);
		let config = Config::load(path)?;
		let links_path = links_path(&config.path);
		let (links, error) = match LinkGraph::load(&links_path) {
			Ok(links) => (links, None),
			Err(error) => (LinkGraph::default(), Some(error.to_string())),
		};
		let mut session = Self {
			config,
			devices: Vec::new(),
			revisions: HashMap::new(),
			links,
			links_path,
			options: ClientOptions::default(),
			pulsing: HashSet::new(),
			live_brightness: HashMap::new(),
			live_warmth: HashMap::new(),
			live_at: HashMap::new(),
			sliders: SliderQueue::default(),
			discovered: Vec::new(),
			error,
			refreshing: false,
			scanning: false,
		};
		session.rebuild_devices();
		Ok(session)
	}

	pub fn with_options(mut self, options: ClientOptions) -> Self {
		self.options = options;
		self
	}

	pub fn options(&self) -> ClientOptions {
		self.options
	}

	pub fn config_path(&self) -> &Path {
		&self.config.path
	}

	pub fn links_path(&self) -> &Path {
		&self.links_path
	}

	pub fn devices(&self) -> &[LightDevice] {
		&self.devices
	}

	pub fn links(&self) -> &LinkGraph {
		&self.links
	}

	pub fn error(&self) -> Option<&str> {
		self.error.as_deref()
	}

	pub fn refreshing(&self) -> bool {
		self.refreshing
	}

	pub fn scanning(&self) -> bool {
		self.scanning
	}

	pub fn pulsing(&self) -> &HashSet<String> {
		&self.pulsing
	}

	pub fn discovered(&self) -> &[DiscoveredDevice] {
		&self.discovered
	}

	pub fn live_brightness(&self, name: &str) -> Option<u8> {
		self.live_brightness.get(name).copied()
	}

	pub fn live_warmth(&self, name: &str) -> Option<u8> {
		self.live_warmth.get(name).copied()
	}

	pub fn clear_error(&mut self) {
		self.error = None;
	}

	pub fn set_error(&mut self, message: impl Into<String>) {
		self.error = Some(message.into());
	}

	pub fn snapshot(&self) -> SessionSnapshot {
		SessionSnapshot {
			config_path: self.config.path.clone(),
			devices: self.devices.clone(),
			groups: self.links.groups.clone(),
			pending_links: self.links.pending.iter().cloned().collect(),
			pulsing: self.pulsing.clone(),
			live_brightness: self.live_brightness.clone(),
			live_warmth: self.live_warmth.clone(),
			discovered: self.discovered.clone(),
			discovery_rows: self.discovery_rows(),
			error: self.error.clone(),
			refreshing: self.refreshing,
			scanning: self.scanning,
		}
	}

	fn rebuild_devices(&mut self) {
		let existing = std::mem::take(&mut self.devices)
			.into_iter()
			.map(|device| (device.name.clone(), device))
			.collect::<HashMap<_, _>>();
		self.devices = self
			.config
			.devices
			.iter()
			.map(|device| match existing.get(&device.name) {
				Some(current) if current.host == device.host && current.port == device.port => {
					current.clone()
				}
				_ => LightDevice::new(&device.name, &device.host, device.port),
			})
			.collect();
		let names = self
			.devices
			.iter()
			.map(|device| device.name.clone())
			.collect::<HashSet<_>>();
		self.revisions.retain(|name, _| names.contains(name));
		self.pulsing.retain(|name| names.contains(name));
		self.live_brightness.retain(|name, _| names.contains(name));
		self.live_warmth.retain(|name, _| names.contains(name));
		self.live_at.retain(|(name, _), _| names.contains(name));
		self.sliders.retain_names(&names);
		if self.links.prune(&names) {
			let _ = self.links.save(&self.links_path);
		}
	}

	fn reload_config(&mut self) {
		match self.config.reload() {
			Ok(()) => self.rebuild_devices(),
			Err(error) => self.error = Some(error.to_string()),
		}
	}

	/// Claim a refresh for the idle lights. The caller runs
	/// [`RefreshPlan::execute`] on a worker thread and passes the result to
	/// [`Session::finish_refresh`]. Devices that are animating or have a
	/// pending slider write are skipped so polling cannot fight a local action.
	pub fn begin_refresh(&mut self) -> Option<RefreshPlan> {
		if self.refreshing {
			return None;
		}
		self.refreshing = true;
		self.reload_config();
		let targets = self
			.devices
			.iter()
			.filter(|device| {
				!self.pulsing.contains(&device.name) && !self.has_pending_slider(&device.name)
			})
			.cloned()
			.collect::<Vec<_>>();
		let revisions = targets
			.iter()
			.map(|device| (device.name.clone(), self.revision(&device.name)))
			.collect::<HashMap<_, _>>();
		Some(RefreshPlan {
			targets,
			revisions,
			options: self.options,
		})
	}

	pub fn finish_refresh(&mut self, revisions: &HashMap<String, u64>, fetched: Vec<Fetched>) {
		for fetched in fetched {
			if self.pulsing.contains(&fetched.name) {
				continue;
			}
			if self.revision(&fetched.name) != revisions.get(&fetched.name).copied().unwrap_or(0) {
				continue;
			}
			let Some(index) = self
				.devices
				.iter()
				.position(|device| device.name == fetched.name)
			else {
				continue;
			};
			match fetched.snapshot {
				Ok(snapshot) => {
					self.devices[index].state = Some(snapshot.state);
					self.devices[index].info = Some(snapshot.info);
					self.devices[index].error = None;
					if let Some(power_on) = fetched.power_on {
						self.devices[index].power_on = Some(power_on);
					}
				}
				Err(error) => self.devices[index].error = Some(error.to_string()),
			}
		}
		self.refreshing = false;
	}

	/// Blocking convenience for tests and synchronous callers.
	pub fn refresh(&mut self) {
		if let Some(plan) = self.begin_refresh() {
			let revisions = plan.revisions().clone();
			let fetched = plan.execute();
			self.finish_refresh(&revisions, fetched);
		}
	}

	fn revision(&self, name: &str) -> u64 {
		self.revisions.get(name).copied().unwrap_or(0)
	}

	fn touch(&mut self, names: &[String]) {
		for name in names {
			*self.revisions.entry(name.clone()).or_default() += 1;
		}
	}

	fn has_pending_slider(&self, name: &str) -> bool {
		self.sliders.has_pending_for(name)
	}

	fn update_names(
		&mut self,
		names: &[String],
		action: impl Fn(&LightDevice, &ElopliClient) -> Result<LightState> + Sync,
	) -> Result<()> {
		let targets = self
			.devices
			.iter()
			.filter(|device| names.contains(&device.name))
			.cloned()
			.collect::<Vec<_>>();
		if targets.is_empty() {
			return Ok(());
		}
		let target_names = targets
			.iter()
			.map(|device| device.name.clone())
			.collect::<Vec<_>>();
		self.touch(&target_names);
		let results = parallel_map(&targets, |device| {
			let client = client_for_target(&DeviceTarget::from(device), self.options);
			action(device, &client).map_err(|error| error.to_string())
		});
		for (device, result) in targets.iter().zip(results) {
			let Some(index) = self
				.devices
				.iter()
				.position(|current| current.name == device.name)
			else {
				continue;
			};
			match result {
				Ok(state) => {
					self.devices[index].state = Some(state);
					self.devices[index].error = None;
				}
				Err(error) => self.devices[index].error = Some(error),
			}
		}
		self.touch(&target_names);
		Ok(())
	}

	pub fn set_on(&mut self, name: &str, on: bool) -> Result<()> {
		let names = self.links.linked_device_names(name, LinkKind::Power);
		self.update_names(&names, |device, client| {
			let brightness = if on && device.state.is_some_and(|state| state.brightness < 2) {
				Some(2)
			} else {
				None
			};
			client.update(Update {
				on: Some(on),
				brightness,
				..Update::default()
			})
		})
	}

	pub fn set_all_on(&mut self, on: bool) -> Result<()> {
		let names = self
			.devices
			.iter()
			.map(|device| device.name.clone())
			.collect::<Vec<_>>();
		self.update_names(&names, |device, client| {
			let brightness = if on && device.state.is_some_and(|state| state.brightness < 2) {
				Some(2)
			} else {
				None
			};
			client.update(Update {
				on: Some(on),
				brightness,
				..Update::default()
			})
		})
	}

	pub fn toggle(&mut self, name: &str) -> Result<()> {
		let visible = self
			.devices
			.iter()
			.find(|device| device.name == name)
			.is_some_and(LightDevice::is_effectively_on);
		self.set_on(name, !visible)
	}

	pub fn set_brightness(&mut self, name: &str, value: u8) -> Result<()> {
		let value = value.clamp(2, 100);
		let names = self.links.linked_device_names(name, LinkKind::Brightness);
		self.update_names(&names, |_, client| {
			client.update(Update {
				brightness: Some(value),
				..Update::default()
			})
		})
	}

	pub fn set_warmth(&mut self, name: &str, value: u8) -> Result<()> {
		let temperature = warmth_to_mired(f64::from(value.min(100)))?;
		let names = self.links.linked_device_names(name, LinkKind::Warmth);
		self.update_names(&names, |_, client| {
			client.update(Update {
				temperature: Some(temperature),
				..Update::default()
			})
		})
	}

	pub fn set_kelvin(&mut self, name: &str, value: u16) -> Result<()> {
		let temperature = kelvin_to_mired(f64::from(value))?;
		let names = self.links.linked_device_names(name, LinkKind::Warmth);
		self.update_names(&names, |_, client| {
			client.update(Update {
				temperature: Some(temperature),
				..Update::default()
			})
		})
	}

	pub fn rename(&mut self, name: &str, new_name: &str) -> Result<()> {
		let Some(device) = self
			.devices
			.iter()
			.find(|device| device.name == name)
			.cloned()
		else {
			return Err(ElopliError::Config(format!("unknown device: {name}")));
		};
		self.touch(std::slice::from_ref(&device.name));
		let client = client_for_target(&DeviceTarget::from(&device), self.options);
		match client.rename(new_name) {
			Ok(_) => {
				self.touch(std::slice::from_ref(&device.name));
				self.refresh();
				Ok(())
			}
			Err(error) => {
				self.set_device_error(name, error.to_string());
				Err(error)
			}
		}
	}

	pub fn set_power_on(
		&mut self,
		name: &str,
		behavior: PowerOnBehavior,
		brightness: Option<u8>,
		temperature: Option<u16>,
	) -> Result<()> {
		let Some(device) = self
			.devices
			.iter()
			.find(|device| device.name == name)
			.cloned()
		else {
			return Err(ElopliError::Config(format!("unknown device: {name}")));
		};
		self.touch(std::slice::from_ref(&device.name));
		let client = client_for_target(&DeviceTarget::from(&device), self.options);
		match client.set_power_on(behavior, brightness, temperature) {
			Ok(settings) => {
				if let Some(index) = self.devices.iter().position(|device| device.name == name) {
					self.devices[index].power_on = Some(settings);
					self.devices[index].error = None;
				}
				self.touch(std::slice::from_ref(&device.name));
				Ok(())
			}
			Err(error) => {
				self.set_device_error(name, error.to_string());
				Err(error)
			}
		}
	}

	pub fn identify(&mut self, name: &str) -> Result<()> {
		let Some(device) = self
			.devices
			.iter()
			.find(|device| device.name == name)
			.cloned()
		else {
			return Err(ElopliError::Config(format!("unknown device: {name}")));
		};
		self.touch(std::slice::from_ref(&device.name));
		let client = client_for_target(&DeviceTarget::from(&device), self.options);
		let result = client.identify();
		if let Err(error) = &result {
			self.set_device_error(name, error.to_string());
		}
		self.touch(std::slice::from_ref(&device.name));
		result
	}

	pub fn queue_brightness(&mut self, name: &str, value: u8) {
		let value = value.clamp(2, 100);
		let names = self.links.linked_device_names(name, LinkKind::Brightness);
		self.queue_slider(names, LinkKind::Brightness, value);
	}

	pub fn queue_warmth(&mut self, name: &str, value: u8) {
		let value = value.min(100);
		let names = self.links.linked_device_names(name, LinkKind::Warmth);
		self.queue_slider(names, LinkKind::Warmth, value);
	}

	fn queue_slider(&mut self, names: Vec<String>, kind: LinkKind, value: u8) {
		let key = SliderKey::new(names.clone(), kind);
		let now = Instant::now();
		for name in &names {
			self.set_live(name, kind, value, now);
		}
		self.sliders.queue(key, value);
	}

	fn set_live(&mut self, name: &str, kind: LinkKind, value: u8, now: Instant) {
		match kind {
			LinkKind::Brightness => {
				self.live_brightness.insert(name.to_string(), value);
			}
			LinkKind::Warmth => {
				self.live_warmth.insert(name.to_string(), value);
			}
			LinkKind::Power | LinkKind::Card => return,
		}
		self.live_at.insert((name.to_string(), kind), now);
	}

	/// Remove live values after the clear delay, unless the name still has a
	/// pending slider write.
	pub fn clear_stale_live(&mut self, now: Instant) {
		let stale = self
			.live_at
			.iter()
			.filter(|(_, at)| now.saturating_duration_since(**at) >= LIVE_CLEAR_DELAY)
			.map(|((name, kind), _)| (name.clone(), *kind))
			.collect::<Vec<_>>();
		for (name, kind) in stale {
			if self.sliders.has_pending_for(&name) {
				continue;
			}
			match kind {
				LinkKind::Brightness => {
					self.live_brightness.remove(&name);
				}
				LinkKind::Warmth => {
					self.live_warmth.remove(&name);
				}
				LinkKind::Power | LinkKind::Card => {}
			}
			self.live_at.remove(&(name, kind));
		}
	}

	pub fn has_pending_slider_writes(&self) -> bool {
		self.sliders.pending() > 0
	}

	/// Take the next slider value that is due. The caller must pass it to
	/// [`Session::write_slider`].
	pub fn next_slider_write(&mut self, now: Instant) -> Option<(SliderKey, u8)> {
		let (key, value) = self.sliders.due(now)?;
		self.sliders.take(&key);
		Some((key, value))
	}

	pub fn write_slider(&mut self, key: &SliderKey, value: u8) {
		let result = match key.kind {
			LinkKind::Brightness => self.update_names(&key.names, |_, client| {
				client.update(Update {
					brightness: Some(value.clamp(2, 100)),
					..Update::default()
				})
			}),
			LinkKind::Warmth => {
				warmth_to_mired(f64::from(value.min(100))).and_then(|temperature| {
					self.update_names(&key.names, |_, client| {
						client.update(Update {
							temperature: Some(temperature),
							..Update::default()
						})
					})
				})
			}
			LinkKind::Power | LinkKind::Card => Ok(()),
		};
		if let Err(error) = result {
			self.error = Some(error.to_string());
		}
		self.sliders.complete(key, Instant::now());
	}

	pub fn toggle_link(&mut self, name: &str, kind: LinkKind) -> Result<()> {
		let changed = self.links.toggle_selection(LinkTarget::new(name, kind));
		if changed {
			self.links.save(&self.links_path)?;
		}
		Ok(())
	}

	pub fn commit_links(&mut self) -> Result<()> {
		let sync = self.links.commit();
		self.links.save(&self.links_path)?;
		for group in sync {
			self.sync_group(&group)?;
		}
		Ok(())
	}

	fn sync_group(&mut self, group: &LinkGroup) -> Result<()> {
		let members = self
			.devices
			.iter()
			.filter(|device| group.device_names.contains(&device.name))
			.cloned()
			.collect::<Vec<_>>();
		let Some(source) = members.first().and_then(|device| device.state) else {
			return Ok(());
		};
		let targets = members
			.iter()
			.skip(1)
			.map(|device| device.name.clone())
			.collect::<Vec<_>>();
		if targets.is_empty() {
			return Ok(());
		}
		let brightness = source.brightness.max(2);
		match group.kind {
			LinkKind::Power => self.update_names(&targets, |_, client| {
				client.update(Update {
					on: Some(source.on),
					..Update::default()
				})
			}),
			LinkKind::Brightness => self.update_names(&targets, |_, client| {
				client.update(Update {
					brightness: Some(brightness),
					..Update::default()
				})
			}),
			LinkKind::Warmth => self.update_names(&targets, |_, client| {
				client.update(Update {
					temperature: Some(source.temperature),
					..Update::default()
				})
			}),
			LinkKind::Card => self.update_names(&targets, |_, client| {
				client.update(Update {
					on: Some(source.on),
					brightness: source.on.then_some(brightness),
					temperature: Some(source.temperature),
					..Update::default()
				})
			}),
		}
	}

	pub fn move_device(&mut self, name: &str, onto: &str) -> Result<bool> {
		let Some(from) = self.devices.iter().position(|device| device.name == name) else {
			return Ok(false);
		};
		let Some(to) = self.devices.iter().position(|device| device.name == onto) else {
			return Ok(false);
		};
		if from == to {
			return Ok(false);
		}
		self.devices.swap(from, to);
		let names = self
			.devices
			.iter()
			.map(|device| device.name.clone())
			.collect::<Vec<_>>();
		if let Err(error) = self
			.config
			.reorder(&names)
			.and_then(|()| self.config.save())
		{
			self.error = Some(error.to_string());
			return Err(error);
		}
		Ok(true)
	}

	/// Start a scan. Returns false when one is already running.
	pub fn begin_scan(&mut self) -> bool {
		if self.scanning {
			return false;
		}
		self.scanning = true;
		true
	}

	/// Apply the result of a background scan.
	pub fn finish_scan(&mut self, result: &Result<Vec<DiscoveredDevice>>) {
		self.scanning = false;
		match result {
			Ok(found) => {
				self.discovered = found.clone();
				self.error = None;
			}
			Err(error) => self.error = Some(error.to_string()),
		}
	}

	pub fn scan(&mut self, timeout: Duration) -> Result<Vec<DiscoveredDevice>> {
		if !self.begin_scan() {
			return Ok(self.discovered.clone());
		}
		let result = discover(timeout);
		self.finish_scan(&result);
		result
	}

	pub fn matches(known: &LightDevice, discovered: &DiscoveredDevice) -> bool {
		if known.port != discovered.port {
			return false;
		}
		if known.name.eq_ignore_ascii_case(&discovered.instance) {
			return true;
		}
		if known.host.eq_ignore_ascii_case(&discovered.hostname) {
			return true;
		}
		discovered
			.address
			.is_some_and(|address| known.host.eq_ignore_ascii_case(&address.to_string()))
	}

	pub fn discovery_rows(&self) -> Vec<DiscoveryRow> {
		let mut rows = Vec::new();
		for known in &self.devices {
			let discovered = self
				.discovered
				.iter()
				.find(|discovered| Self::matches(known, discovered));
			let detail = match discovered {
				Some(discovered) => format!(
					"{} · {}",
					discovered.product,
					discovered.address.map_or_else(
						|| discovered.hostname.clone(),
						|address| address.to_string()
					)
				),
				None => format!(
					"{} · {} · not found",
					known
						.info
						.as_ref()
						.map_or(known.host.as_str(), |info| info.product_name.as_str()),
					known.host
				),
			};
			let needs_update = discovered
				.and_then(|discovered| discovered.address)
				.is_some_and(|address| !known.host.eq_ignore_ascii_case(&address.to_string()));
			rows.push(DiscoveryRow {
				id: format!("known-{}", known.name),
				name: known.name.clone(),
				detail,
				state: if discovered.is_some() {
					DiscoveryState::Added
				} else {
					DiscoveryState::Missing
				},
				needs_update,
				discovered: discovered.cloned(),
			});
		}
		for discovered in &self.discovered {
			if self
				.devices
				.iter()
				.any(|known| Self::matches(known, discovered))
			{
				continue;
			}
			rows.push(DiscoveryRow {
				id: format!("new-{}", discovered.instance),
				name: discovered.instance.clone(),
				detail: format!(
					"{} · {}",
					discovered.product,
					discovered.address.map_or_else(
						|| discovered.hostname.clone(),
						|address| address.to_string()
					)
				),
				state: DiscoveryState::New,
				needs_update: false,
				discovered: Some(discovered.clone()),
			});
		}
		rows
	}

	fn address_for(discovered: &DiscoveredDevice) -> Result<String> {
		discovered
			.address
			.map(|address| address.to_string())
			.ok_or_else(|| {
				ElopliError::Config(format!(
					"{} did not advertise an IPv4 address; use a network with IPv4 mDNS",
					discovered.instance
				))
			})
	}

	pub fn add_discovered(&mut self, discovered: &DiscoveredDevice) -> Result<bool> {
		if self
			.devices
			.iter()
			.any(|known| Self::matches(known, discovered))
		{
			return Ok(false);
		}
		let address = Self::address_for(discovered)?;
		let changed = self.config.add(Device {
			name: discovered.instance.clone(),
			host: address,
			port: discovered.port,
		})?;
		if changed {
			self.config.save()?;
			self.rebuild_devices();
		}
		Ok(changed)
	}

	pub fn update_discovered(&mut self, discovered: &DiscoveredDevice) -> Result<bool> {
		let address = Self::address_for(discovered)?;
		let Some(known) = self
			.devices
			.iter()
			.find(|known| Self::matches(known, discovered))
			.cloned()
		else {
			return Err(ElopliError::Config(format!(
				"{} is not in the device registry",
				discovered.instance
			)));
		};
		let changed = self.config.add(Device {
			name: known.name,
			host: address,
			port: discovered.port,
		})?;
		if changed {
			self.config.save()?;
			self.rebuild_devices();
		}
		Ok(changed)
	}

	/// Mark devices as animating and return their addresses for child threads.
	pub fn begin_animation(&mut self, names: &[String]) -> Vec<DeviceTarget> {
		let targets = self
			.devices
			.iter()
			.filter(|device| names.contains(&device.name))
			.map(DeviceTarget::from)
			.collect::<Vec<_>>();
		for target in &targets {
			self.pulsing.insert(target.name.clone());
		}
		let target_names = targets
			.iter()
			.map(|target| target.name.clone())
			.collect::<Vec<_>>();
		self.touch(&target_names);
		targets
	}

	pub fn finish_animation(&mut self, name: &str, result: Result<PulseOutcome>) {
		self.pulsing.remove(name);
		if let Some(index) = self.devices.iter().position(|device| device.name == name) {
			match result {
				Ok(outcome) => {
					self.devices[index].state = Some(*outcome.state());
					self.devices[index].error = None;
				}
				Err(error) => self.devices[index].error = Some(error.to_string()),
			}
		}
		self.touch(&[name.to_string()]);
	}

	pub fn pulse(&mut self, names: &[String], config: PulseConfig) -> Result<()> {
		let targets = self.begin_animation(names);
		let results = parallel_map(&targets, |target| run_pulse(target, config, self.options));
		for (target, result) in targets.iter().zip(results) {
			self.finish_animation(&target.name, result);
		}
		Ok(())
	}

	pub fn sweep(&mut self, names: &[String], config: SweepConfig) -> Result<()> {
		let targets = self.begin_animation(names);
		let results = parallel_map(&targets, |target| run_sweep(target, config, self.options));
		for (target, result) in targets.iter().zip(results) {
			self.finish_animation(&target.name, result);
		}
		Ok(())
	}

	pub fn cancel_animations(&mut self, names: &[String]) {
		let targets = self
			.devices
			.iter()
			.filter(|device| names.contains(&device.name) && self.pulsing.contains(&device.name))
			.map(DeviceTarget::from)
			.collect::<Vec<_>>();
		for target in &targets {
			if let Err(error) = cancel_target(target, self.options) {
				self.set_device_error(&target.name, error.to_string());
			}
		}
	}

	pub fn cancel_all_animations(&mut self) {
		let names = self.pulsing.iter().cloned().collect::<Vec<_>>();
		self.cancel_animations(&names);
	}

	fn set_device_error(&mut self, name: &str, message: String) {
		if let Some(index) = self.devices.iter().position(|device| device.name == name) {
			self.devices[index].error = Some(message);
		}
	}
}

pub fn links_path(config_path: &Path) -> PathBuf {
	config_path.with_file_name("links.json")
}

fn client_for_target(target: &DeviceTarget, options: ClientOptions) -> ElopliClient {
	ElopliClient::new(
		target.host.clone(),
		target.port,
		options.timeout,
		options.retries,
		options.transport,
	)
}

/// Run a pulse for one light. Intended for a UI animation thread.
pub fn run_pulse(
	target: &DeviceTarget,
	config: PulseConfig,
	options: ClientOptions,
) -> Result<PulseOutcome> {
	client_for_target(target, options).pulse(config)
}

/// Run a sweep for one light. Intended for a UI animation thread.
pub fn run_sweep(
	target: &DeviceTarget,
	config: SweepConfig,
	options: ClientOptions,
) -> Result<PulseOutcome> {
	client_for_target(target, options).sweep(config)
}

/// Ask a light to stop its animation. The running animation restores its
/// original state before it exits.
pub fn cancel_target(target: &DeviceTarget, options: ClientOptions) -> Result<()> {
	client_for_target(target, options).cancel_pulse()
}

/// A snapshot claimed for a background refresh.
pub struct RefreshPlan {
	targets: Vec<LightDevice>,
	revisions: HashMap<String, u64>,
	options: ClientOptions,
}

impl RefreshPlan {
	pub fn revisions(&self) -> &HashMap<String, u64> {
		&self.revisions
	}

	/// Fetch every target in parallel. Run this off the command thread.
	pub fn execute(&self) -> Vec<Fetched> {
		parallel_map(&self.targets, |device| fetch(device, self.options))
	}
}

pub struct Fetched {
	pub name: String,
	pub snapshot: Result<Snapshot>,
	pub power_on: Option<PowerOnSettings>,
}

fn fetch(device: &LightDevice, options: ClientOptions) -> Fetched {
	let client = client_for_target(&DeviceTarget::from(device), options);
	match client.snapshot() {
		Ok(snapshot) => {
			let power_on = client.power_on_settings().ok();
			Fetched {
				name: device.name.clone(),
				snapshot: Ok(snapshot),
				power_on,
			}
		}
		Err(error) => Fetched {
			name: device.name.clone(),
			snapshot: Err(error),
			power_on: None,
		},
	}
}

fn parallel_map<T, U>(items: &[T], operation: impl Fn(&T) -> U + Sync) -> Vec<U>
where
	T: Sync,
	U: Send,
{
	std::thread::scope(|scope| {
		items
			.iter()
			.map(|item| scope.spawn(|| operation(item)))
			.collect::<Vec<_>>()
			.into_iter()
			.map(|handle| handle.join().expect("session worker panicked"))
			.collect()
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use elopli_core::{AccessoryInfo, LightState, Snapshot, WifiInfo};
	use std::net::Ipv4Addr;

	fn device(name: &str, host: &str, port: u16) -> LightDevice {
		LightDevice::new(name, host, port)
	}

	fn discovered(
		name: &str,
		host: &str,
		address: Option<Ipv4Addr>,
		port: u16,
	) -> DiscoveredDevice {
		DiscoveredDevice {
			instance: name.to_string(),
			hostname: host.to_string(),
			address,
			port,
			product: "Elgato Key Light".to_string(),
			board_type: Some(53),
			model: None,
		}
	}

	#[test]
	fn discovery_matches_name_host_address_and_port() {
		let known = device("Left", "192.0.2.10", 9123);
		assert!(Session::matches(
			&known,
			&discovered("Left", "other.local", None, 9123)
		));
		assert!(Session::matches(
			&known,
			&discovered("Other", "192.0.2.10", None, 9123)
		));
		assert!(Session::matches(
			&known,
			&discovered(
				"Other",
				"other.local",
				Some(Ipv4Addr::new(192, 0, 2, 10)),
				9123
			)
		));
		assert!(!Session::matches(
			&known,
			&discovered("Left", "192.0.2.10", None, 9999)
		));
	}

	#[test]
	fn discovery_rows_show_added_missing_update_and_new() {
		let mut session = Session {
			config: Config::new(PathBuf::from("devices.json")),
			devices: vec![
				device("Left", "192.0.2.10", 9123),
				device("Right", "192.0.2.11", 9123),
				device("Back", "192.0.2.12", 9123),
			],
			revisions: HashMap::new(),
			links: LinkGraph::default(),
			links_path: PathBuf::from("links.json"),
			options: ClientOptions::default(),
			pulsing: HashSet::new(),
			live_brightness: HashMap::new(),
			live_warmth: HashMap::new(),
			live_at: HashMap::new(),
			sliders: SliderQueue::default(),
			discovered: vec![
				discovered(
					"Left",
					"left.local",
					Some(Ipv4Addr::new(192, 0, 2, 10)),
					9123,
				),
				discovered(
					"Right",
					"right.local",
					Some(Ipv4Addr::new(192, 0, 2, 99)),
					9123,
				),
				discovered("New", "new.local", Some(Ipv4Addr::new(192, 0, 2, 20)), 9123),
			],
			error: None,
			refreshing: false,
			scanning: false,
		};
		let rows = session.discovery_rows();
		assert_eq!(rows.len(), 4);
		assert_eq!(rows[0].state, DiscoveryState::Added);
		assert!(!rows[0].needs_update);
		assert_eq!(rows[1].state, DiscoveryState::Added);
		assert!(rows[1].needs_update);
		assert_eq!(rows[2].state, DiscoveryState::Missing);
		assert_eq!(rows[3].state, DiscoveryState::New);
		assert_eq!(rows[3].name, "New");

		session.error = None;
	}

	fn accessory_info() -> AccessoryInfo {
		AccessoryInfo {
			product_name: "Elgato Key Light".to_string(),
			display_name: "Left".to_string(),
			firmware_version: "1.0.3".to_string(),
			firmware_build_number: 222,
			hardware_board_type: 53,
			wifi: Some(WifiInfo {
				ssid: "test".to_string(),
				frequency_mhz: 2400,
				rssi: -40,
			}),
		}
	}

	fn refresh_session() -> Session {
		let path = std::env::temp_dir().join(format!(
			"elopli-refresh-{}-{}.json",
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.map(|duration| duration.as_nanos())
				.unwrap_or_default()
		));
		let mut config = Config::new(path.clone());
		config
			.add(Device {
				name: "Left".to_string(),
				host: "192.0.2.10".to_string(),
				port: 9123,
			})
			.unwrap();
		config.save().unwrap();
		Session {
			config,
			devices: vec![device("Left", "192.0.2.10", 9123)],
			revisions: HashMap::new(),
			links: LinkGraph::default(),
			links_path: path.with_file_name("links.json"),
			options: ClientOptions::default(),
			pulsing: HashSet::new(),
			live_brightness: HashMap::new(),
			live_warmth: HashMap::new(),
			live_at: HashMap::new(),
			sliders: SliderQueue::default(),
			discovered: Vec::new(),
			error: None,
			refreshing: false,
			scanning: false,
		}
	}

	#[test]
	fn refresh_plan_applies_results_and_clears_the_flag() {
		let mut session = refresh_session();
		let plan = session.begin_refresh().unwrap();
		assert!(session.refreshing());
		let fetched = vec![Fetched {
			name: "Left".to_string(),
			snapshot: Ok(Snapshot {
				state: LightState {
					on: true,
					brightness: 42,
					temperature: 300,
				},
				info: accessory_info(),
			}),
			power_on: None,
		}];
		let revisions = plan.revisions().clone();
		session.finish_refresh(&revisions, fetched);
		assert!(!session.refreshing());
		assert_eq!(session.devices()[0].state.unwrap().brightness, 42);
	}

	#[test]
	fn a_refresh_result_is_ignored_after_a_local_write() {
		let mut session = refresh_session();
		let plan = session.begin_refresh().unwrap();
		let revisions = plan.revisions().clone();
		session.touch(&["Left".to_string()]);
		session.finish_refresh(
			&revisions,
			vec![Fetched {
				name: "Left".to_string(),
				snapshot: Ok(Snapshot {
					state: LightState {
						on: true,
						brightness: 42,
						temperature: 300,
					},
					info: accessory_info(),
				}),
				power_on: None,
			}],
		);
		assert!(session.devices()[0].state.is_none());
		assert!(!session.refreshing());
	}

	#[test]
	fn only_one_refresh_can_be_claimed() {
		let mut session = refresh_session();
		assert!(session.begin_refresh().is_some());
		assert!(session.begin_refresh().is_none());
		session.finish_refresh(&HashMap::new(), Vec::new());
		assert!(session.begin_refresh().is_some());
	}

	#[test]
	fn subtitle_prefers_a_different_hardware_name() {
		let mut device = device("Desk", "192.0.2.10", 9123);
		device.info = Some(AccessoryInfo {
			product_name: "Elgato Key Light Air".to_string(),
			display_name: "Right".to_string(),
			firmware_version: "1.0.3".to_string(),
			firmware_build_number: 222,
			hardware_board_type: 200,
			wifi: Some(WifiInfo {
				ssid: "test".to_string(),
				frequency_mhz: 2400,
				rssi: -40,
			}),
		});
		assert_eq!(device.subtitle(), "Right · Elgato Key Light Air");
	}
}
