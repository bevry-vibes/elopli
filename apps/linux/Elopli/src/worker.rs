use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use elopli_core::{
	DiscoveredDevice, PowerOnBehavior, PulseConfig, PulseOutcome, Result, SweepConfig, discover,
};
use elopli_session::{Fetched, LinkKind, Session, SessionSnapshot, run_pulse, run_sweep};

const SCAN_TIMEOUT: Duration = Duration::from_secs(3);

pub enum Command {
	Refresh,
	SetOn {
		name: String,
		on: bool,
	},
	SetAllOn(bool),
	QueueBrightness {
		name: String,
		value: u8,
	},
	QueueWarmth {
		name: String,
		value: u8,
	},
	Rename {
		name: String,
		new_name: String,
	},
	SetPowerOn {
		name: String,
		behavior: PowerOnBehavior,
		brightness: Option<u8>,
		temperature: Option<u16>,
	},
	Identify(String),
	Pulse {
		names: Vec<String>,
		config: PulseConfig,
	},
	Sweep {
		names: Vec<String>,
		config: SweepConfig,
	},
	CancelAnimations(Vec<String>),
	Scan,
	AddDiscovered(DiscoveredDevice),
	UpdateDiscovered(DiscoveredDevice),
	Move {
		name: String,
		onto: String,
	},
	ToggleLink {
		name: String,
		kind: LinkKind,
	},
	CommitLinks,
	Shutdown,
}

enum Message {
	Command(Command),
	AnimationDone {
		name: String,
		result: Result<PulseOutcome>,
	},
	ScanDone(Result<Vec<DiscoveredDevice>>),
	RefreshDone {
		revisions: HashMap<String, u64>,
		fetched: Vec<Fetched>,
	},
}

/// Owns the shared session on a worker thread. The GTK thread reads snapshots
/// and sends commands; no network call ever runs on the main loop.
pub struct Worker {
	sender: Sender<Message>,
	snapshot: Arc<Mutex<SessionSnapshot>>,
	revision: Arc<AtomicU64>,
	join: Mutex<Option<JoinHandle<()>>>,
}

impl Worker {
	pub fn start() -> std::result::Result<Self, String> {
		let session = Session::load(None).map_err(|error| error.to_string())?;
		let snapshot = Arc::new(Mutex::new(session.snapshot()));
		let revision = Arc::new(AtomicU64::new(1));
		let (sender, receiver) = mpsc::channel();
		let shared = Arc::clone(&snapshot);
		let counter = Arc::clone(&revision);
		let worker_sender = sender.clone();
		let join = thread::spawn(move || {
			run(session, receiver, worker_sender, shared, counter);
		});
		Ok(Self {
			sender,
			snapshot,
			revision,
			join: Mutex::new(Some(join)),
		})
	}

	pub fn send(&self, command: Command) {
		let _ = self.sender.send(Message::Command(command));
	}

	pub fn snapshot(&self) -> SessionSnapshot {
		self.snapshot
			.lock()
			.map(|snapshot| snapshot.clone())
			.unwrap_or_else(|_| fallback_snapshot())
	}

	pub fn revision(&self) -> u64 {
		self.revision.load(Ordering::Relaxed)
	}

	pub fn shutdown(&self) {
		let _ = self.sender.send(Message::Command(Command::Shutdown));
		let join = self.join.lock().ok().and_then(|mut join| join.take());
		if let Some(join) = join {
			let _ = join.join();
		}
	}
}

impl Drop for Worker {
	fn drop(&mut self) {
		self.shutdown();
	}
}

fn fallback_snapshot() -> SessionSnapshot {
	SessionSnapshot {
		config_path: PathBuf::new(),
		devices: Vec::new(),
		groups: Vec::new(),
		pending_links: Vec::new(),
		pulsing: HashSet::new(),
		live_brightness: HashMap::new(),
		live_warmth: HashMap::new(),
		discovered: Vec::new(),
		discovery_rows: Vec::new(),
		error: Some("the session snapshot could not be read".into()),
		refreshing: false,
		scanning: false,
	}
}

fn run(
	mut session: Session,
	receiver: Receiver<Message>,
	sender: Sender<Message>,
	snapshot: Arc<Mutex<SessionSnapshot>>,
	revision: Arc<AtomicU64>,
) {
	let mut animations = 0usize;
	let mut dirty = false;
	spawn_refresh(&mut session, &sender);

	loop {
		let timeout = if session.has_pending_slider_writes() {
			Duration::from_millis(25)
		} else {
			Duration::from_millis(100)
		};
		match receiver.recv_timeout(timeout) {
			Ok(Message::Command(Command::Shutdown)) => {
				session.cancel_all_animations();
				let deadline = Instant::now() + Duration::from_secs(5);
				while animations > 0 && Instant::now() < deadline {
					if let Ok(Message::AnimationDone { name, result }) =
						receiver.recv_timeout(Duration::from_millis(100))
					{
						session.finish_animation(&name, result);
						animations = animations.saturating_sub(1);
					}
				}
				publish(&session, &snapshot, &revision);
				break;
			}
			Ok(Message::Command(command)) => {
				handle(command, &mut session, &sender, &mut animations);
				dirty = true;
			}
			Ok(Message::AnimationDone { name, result }) => {
				session.finish_animation(&name, result);
				animations = animations.saturating_sub(1);
				dirty = true;
			}
			Ok(Message::ScanDone(result)) => {
				session.finish_scan(&result);
				dirty = true;
			}
			Ok(Message::RefreshDone { revisions, fetched }) => {
				session.finish_refresh(&revisions, fetched);
				dirty = true;
			}
			Err(RecvTimeoutError::Timeout) => {}
			Err(RecvTimeoutError::Disconnected) => break,
		}

		let mut wrote_slider = false;
		while let Some((key, value)) = session.next_slider_write(Instant::now()) {
			session.write_slider(&key, value);
			wrote_slider = true;
		}
		session.clear_stale_live(Instant::now());
		if wrote_slider {
			dirty = true;
		}
		if dirty {
			publish(&session, &snapshot, &revision);
			dirty = false;
		}
	}
}

fn handle(
	command: Command,
	session: &mut Session,
	sender: &Sender<Message>,
	animations: &mut usize,
) {
	match command {
		Command::Refresh => spawn_refresh(session, sender),
		Command::Shutdown => {}
		Command::SetOn { name, on } => {
			let _ = session.set_on(&name, on);
		}
		Command::SetAllOn(on) => {
			let _ = session.set_all_on(on);
		}
		Command::QueueBrightness { name, value } => session.queue_brightness(&name, value),
		Command::QueueWarmth { name, value } => session.queue_warmth(&name, value),
		Command::Rename { name, new_name } => {
			if let Err(error) = session.rename(&name, &new_name) {
				session.set_error(error.to_string());
			}
		}
		Command::SetPowerOn {
			name,
			behavior,
			brightness,
			temperature,
		} => {
			if let Err(error) = session.set_power_on(&name, behavior, brightness, temperature) {
				session.set_error(error.to_string());
			}
		}
		Command::Identify(name) => {
			let _ = session.identify(&name);
		}
		Command::Pulse { names, config } => {
			let targets = session.begin_animation(&names);
			let options = session.options();
			for target in targets {
				let sender = sender.clone();
				*animations += 1;
				thread::spawn(move || {
					let result = run_pulse(&target, config, options);
					let _ = sender.send(Message::AnimationDone {
						name: target.name,
						result,
					});
				});
			}
		}
		Command::Sweep { names, config } => {
			let targets = session.begin_animation(&names);
			let options = session.options();
			for target in targets {
				let sender = sender.clone();
				*animations += 1;
				thread::spawn(move || {
					let result = run_sweep(&target, config, options);
					let _ = sender.send(Message::AnimationDone {
						name: target.name,
						result,
					});
				});
			}
		}
		Command::CancelAnimations(names) => session.cancel_animations(&names),
		Command::Scan => {
			if session.begin_scan() {
				let sender = sender.clone();
				thread::spawn(move || {
					let result = discover(SCAN_TIMEOUT);
					let _ = sender.send(Message::ScanDone(result));
				});
			}
		}
		Command::AddDiscovered(device) => {
			if let Err(error) = session.add_discovered(&device) {
				session.set_error(error.to_string());
			}
			spawn_refresh(session, sender);
		}
		Command::UpdateDiscovered(device) => {
			if let Err(error) = session.update_discovered(&device) {
				session.set_error(error.to_string());
			}
			spawn_refresh(session, sender);
		}
		Command::Move { name, onto } => {
			let _ = session.move_device(&name, &onto);
		}
		Command::ToggleLink { name, kind } => {
			if let Err(error) = session.toggle_link(&name, kind) {
				session.set_error(error.to_string());
			}
		}
		Command::CommitLinks => {
			if let Err(error) = session.commit_links() {
				session.set_error(error.to_string());
			}
		}
	}
}

fn spawn_refresh(session: &mut Session, sender: &Sender<Message>) {
	let Some(plan) = session.begin_refresh() else {
		return;
	};
	let sender = sender.clone();
	thread::spawn(move || {
		let revisions = plan.revisions().clone();
		let fetched = plan.execute();
		let _ = sender.send(Message::RefreshDone { revisions, fetched });
	});
}

fn publish(session: &Session, snapshot: &Arc<Mutex<SessionSnapshot>>, revision: &Arc<AtomicU64>) {
	if let Ok(mut current) = snapshot.lock() {
		*current = session.snapshot();
	}
	revision.fetch_add(1, Ordering::Relaxed);
}
