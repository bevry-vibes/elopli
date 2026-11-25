use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand, ValueEnum};
use elopli_core::{
	Config, Device, ElopliClient, ElopliError, LightState, PowerOnBehavior, PulseConfig, PulseEase,
	PulseTarget, Result, TransportMode, Update, config_path, discover, kelvin_to_mired,
	validate_brightness, validate_display_name, validate_temperature, validate_warmth,
};
use rayon::prelude::*;
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TransportArg {
	Auto,
	Native,
	Curl,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PulseEaseArg {
	Linear,
	EaseIn,
	EaseOut,
	EaseInOut,
}

impl From<PulseEaseArg> for PulseEase {
	fn from(value: PulseEaseArg) -> Self {
		match value {
			PulseEaseArg::Linear => Self::Linear,
			PulseEaseArg::EaseIn => Self::EaseIn,
			PulseEaseArg::EaseOut => Self::EaseOut,
			PulseEaseArg::EaseInOut => Self::EaseInOut,
		}
	}
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PulseTargetArg {
	Brightness,
	Temperature,
}

impl From<PulseTargetArg> for PulseTarget {
	fn from(value: PulseTargetArg) -> Self {
		match value {
			PulseTargetArg::Brightness => Self::Brightness,
			PulseTargetArg::Temperature => Self::Temperature,
		}
	}
}

impl From<TransportArg> for TransportMode {
	fn from(value: TransportArg) -> Self {
		match value {
			TransportArg::Auto => TransportMode::Auto,
			TransportArg::Native => TransportMode::Native,
			TransportArg::Curl => TransportMode::Curl,
		}
	}
}

#[derive(Debug, Parser)]
#[command(
	name = "elopli",
	version,
	about = "Control Elgato Key Lights from the command line."
)]
struct Cli {
	#[arg(
		long,
		global = true,
		value_name = "PATH",
		help = "path to the device registry"
	)]
	config: Option<PathBuf>,
	#[arg(long, global = true, help = "print machine-readable JSON")]
	json: bool,
	#[arg(
		long,
		global = true,
		default_value_t = 3.0,
		value_name = "SECONDS",
		help = "request timeout in seconds"
	)]
	timeout: f64,
	#[arg(long, global = true, default_value_t = 2, help = "retries per request")]
	retries: u32,
	#[arg(
		long,
		global = true,
		value_enum,
		default_value_t = TransportArg::Auto,
		help = "HTTP transport"
	)]
	transport: TransportArg,
	#[command(subcommand)]
	command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
	#[command(about = "read the state of one or more lights")]
	Status(TargetArgs),
	#[command(about = "list the configured devices")]
	List,
	#[command(about = "turn one or more lights on")]
	On(TargetWaitArgs),
	#[command(about = "turn one or more lights off")]
	Off(TargetWaitArgs),
	#[command(about = "toggle one or more lights")]
	Toggle(TargetWaitArgs),
	#[command(about = "set brightness from 0 to 100")]
	Brightness(ValueArgs),
	#[command(about = "set warmth from 0 (cool) to 100 (warm)")]
	Warmth(ValueArgs),
	#[command(about = "set colour temperature from 2900 to 7000 K")]
	Kelvin(ValueArgs),
	#[command(about = "set colour temperature in mired from 143 to 344")]
	Temperature(ValueArgs),
	#[command(about = "set several properties in one request")]
	Set(SetArgs),
	#[command(about = "change the device display name")]
	Rename(RenameArgs),
	#[command(about = "read or set the power-on behaviour")]
	PowerOn(PowerOnArgs),
	#[command(about = "pulse brightness or temperature and restore the original values")]
	Pulse(PulseArgs),
	#[command(about = "flash one or more lights")]
	Identify(TargetArgs),
	#[command(about = "read product and network details")]
	Info(TargetArgs),
	#[command(about = "find lights through mDNS")]
	Discover(DiscoverArgs),
	#[command(about = "inspect or create the device registry")]
	Config(ConfigArgs),
}

#[derive(Debug, Args)]
struct TargetArgs {
	#[arg(value_name = "DEVICE", help = "device name from the registry")]
	devices: Vec<String>,
	#[arg(long, help = "use every device in the registry")]
	all: bool,
	#[arg(
		long,
		value_name = "HOST",
		help = "use a host directly; repeat for more"
	)]
	host: Vec<String>,
}

#[derive(Debug, Args)]
struct WaitArgs {
	#[arg(
		long,
		default_value_t = 0.0,
		value_name = "SECONDS",
		help = "keep retrying unreachable devices for this long"
	)]
	wait: f64,
}

#[derive(Debug, Args)]
struct TargetWaitArgs {
	#[command(flatten)]
	targets: TargetArgs,
	#[command(flatten)]
	wait: WaitArgs,
}

#[derive(Debug, Args)]
struct ValueArgs {
	#[arg(value_name = "VALUE")]
	value: f64,
	#[command(flatten)]
	targets: TargetArgs,
	#[command(flatten)]
	wait: WaitArgs,
}

#[derive(Debug, Args)]
struct SetArgs {
	#[command(flatten)]
	targets: TargetArgs,
	#[arg(long, help = "turn the light on")]
	on: bool,
	#[arg(long, help = "turn the light off")]
	off: bool,
	#[arg(long, value_name = "PERCENT", help = "brightness from 0 to 100")]
	brightness: Option<f64>,
	#[arg(
		long,
		value_name = "PERCENT",
		help = "warmth from 0 (cool) to 100 (warm)"
	)]
	warmth: Option<f64>,
	#[arg(
		long,
		value_name = "KELVIN",
		help = "colour temperature from 2900 to 7000 K"
	)]
	kelvin: Option<f64>,
	#[arg(long, value_name = "MIRED", help = "colour temperature in mired")]
	temperature: Option<f64>,
	#[arg(long, value_name = "MS", help = "transition time")]
	duration_ms: Option<u32>,
	#[command(flatten)]
	wait: WaitArgs,
}

#[derive(Debug, Args)]
struct RenameArgs {
	#[arg(
		value_name = "DEVICE",
		help = "device name from the registry or a host"
	)]
	device: String,
	#[arg(value_name = "NEW_NAME", help = "new display name (1 to 63 bytes)")]
	new_name: String,
}

#[derive(Debug, Args)]
struct PowerOnArgs {
	#[command(flatten)]
	targets: TargetArgs,
	#[arg(long, help = "restore the last used setting")]
	last: bool,
	#[arg(long, help = "restore the defaults")]
	defaults: bool,
	#[arg(
		long,
		value_name = "PERCENT",
		help = "default brightness for --defaults"
	)]
	brightness: Option<f64>,
	#[arg(long, value_name = "PERCENT", help = "default warmth for --defaults")]
	warmth: Option<f64>,
	#[arg(
		long,
		value_name = "KELVIN",
		help = "default colour temperature for --defaults"
	)]
	kelvin: Option<f64>,
	#[arg(
		long,
		value_name = "MIRED",
		help = "default colour temperature in mired"
	)]
	temperature: Option<f64>,
	#[command(flatten)]
	wait: WaitArgs,
}

#[derive(Debug, Args)]
struct DiscoverArgs {
	#[arg(
		long,
		default_value_t = 3.0,
		value_name = "SECONDS",
		help = "browse time in seconds"
	)]
	wait: f64,
	#[arg(long, help = "add the found devices to the registry")]
	write: bool,
}

#[derive(Debug, Args)]
struct PulseArgs {
	#[command(flatten)]
	targets: TargetArgs,
	#[arg(
		long,
		default_value_t = 3,
		value_parser = clap::value_parser!(u32).range(1..=20),
		help = "number of peak-to-minimum pulses (default 3)"
	)]
	count: u32,
	#[arg(
		long,
		default_value_t = 600,
		value_parser = clap::value_parser!(u32).range(50..=10000),
		value_name = "MS",
		help = "duration of the entire pulse animation in milliseconds (default 600)"
	)]
	duration: u32,
	#[arg(
		long,
		value_enum,
		default_value_t = PulseEaseArg::Linear,
		help = "easing curve (default linear)"
	)]
	ease: PulseEaseArg,
	#[arg(
		long,
		value_enum,
		default_value_t = PulseTargetArg::Brightness,
		help = "property to pulse (default brightness)"
	)]
	target: PulseTargetArg,
	#[arg(
		long,
		default_value_t = 100,
		value_parser = clap::value_parser!(u8).range(0..=100),
		value_name = "PERCENT",
		help = "peak value in percent (default 100)"
	)]
	value: u8,
}

#[derive(Debug, Args)]
struct ConfigArgs {
	#[command(subcommand)]
	command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
	#[command(about = "print the registry path")]
	Path,
	#[command(about = "print the registry as JSON")]
	Show,
	#[command(about = "create an empty registry")]
	Init {
		#[arg(long, help = "replace an existing registry")]
		force: bool,
	},
}

#[derive(Debug, Clone)]
struct Settings {
	config_path: PathBuf,
	json: bool,
	timeout: Duration,
	retries: u32,
	transport: TransportMode,
}

fn settings(cli: &Cli) -> Result<Settings> {
	if cli.timeout <= 0.0 || !cli.timeout.is_finite() {
		return Err(ElopliError::InvalidValue(
			"--timeout must be greater than zero".into(),
		));
	}
	Ok(Settings {
		config_path: config_path(cli.config.as_deref()),
		json: cli.json,
		timeout: Duration::from_secs_f64(cli.timeout),
		retries: cli.retries,
		transport: cli.transport.into(),
	})
}

fn client(device: &Device, settings: &Settings) -> ElopliClient {
	ElopliClient::new(
		device.host.clone(),
		device.port,
		settings.timeout,
		settings.retries,
		settings.transport,
	)
}

fn base(device: &Device) -> Map<String, Value> {
	let mut record = Map::new();
	record.insert("name".into(), json!(device.name));
	record.insert("host".into(), json!(device.host));
	record.insert("port".into(), json!(device.port));
	record
}

fn merge(record: &mut Map<String, Value>, value: Value) {
	if let Value::Object(value) = value {
		record.extend(value);
	}
}

fn error_record(device: &Device, error: &ElopliError) -> Value {
	let (reachable, state) = match error {
		ElopliError::Api { .. } => (true, "error"),
		_ => (false, "offline"),
	};
	let mut record = base(device);
	record.insert("reachable".into(), json!(reachable));
	record.insert("state".into(), json!(state));
	record.insert("error".into(), json!(error.to_string()));
	Value::Object(record)
}

fn state_record(device: &Device, settings: &Settings) -> Value {
	let client = client(device, settings);
	match client.state() {
		Ok(state) => {
			let mut record = base(device);
			record.insert("reachable".into(), json!(true));
			record.insert("state".into(), json!(if state.on { "on" } else { "off" }));
			merge(&mut record, state.to_json());
			Value::Object(record)
		}
		Err(error) => error_record(device, &error),
	}
}

fn with_wait<T>(mut operation: impl FnMut() -> Result<T>, wait: f64) -> Result<T> {
	let deadline = Instant::now() + Duration::from_secs_f64(wait.max(0.0));
	loop {
		match operation() {
			Err(ElopliError::Connection(_)) if Instant::now() < deadline => {
				std::thread::sleep(Duration::from_secs(1));
			}
			result => return result,
		}
	}
}

fn apply_record(
	device: &Device,
	settings: &Settings,
	operation: impl Fn(&ElopliClient) -> Result<LightState>,
	wait: f64,
) -> Value {
	let client = client(device, settings);
	match with_wait(|| operation(&client), wait) {
		Ok(state) => {
			let mut record = base(device);
			record.insert("reachable".into(), json!(true));
			record.insert("state".into(), json!(if state.on { "on" } else { "off" }));
			merge(&mut record, state.to_json());
			Value::Object(record)
		}
		Err(error) => error_record(device, &error),
	}
}

fn identify_record(device: &Device, settings: &Settings) -> Value {
	let client = client(device, settings);
	match client.identify() {
		Ok(()) => {
			let mut record = base(device);
			record.insert("reachable".into(), json!(true));
			record.insert("identified".into(), json!(true));
			Value::Object(record)
		}
		Err(error) => error_record(device, &error),
	}
}

fn info_record(device: &Device, settings: &Settings) -> Value {
	let client = client(device, settings);
	match client.snapshot() {
		Ok(snapshot) => {
			let mut record = base(device);
			record.insert("reachable".into(), json!(true));
			merge(&mut record, snapshot.info.to_json());
			record.insert(
				"state".into(),
				json!(if snapshot.state.on { "on" } else { "off" }),
			);
			merge(&mut record, snapshot.state.to_json());
			Value::Object(record)
		}
		Err(error) => error_record(device, &error),
	}
}

fn power_on_record(
	device: &Device,
	settings: &Settings,
	update: Option<(PowerOnBehavior, Option<u8>, Option<u16>)>,
	wait: f64,
) -> Value {
	let client = client(device, settings);
	let result = match update {
		Some((behavior, brightness, temperature)) => with_wait(
			|| client.set_power_on(behavior, brightness, temperature),
			wait,
		),
		None => client.power_on_settings(),
	};
	match result {
		Ok(power_on) => {
			let mut record = base(device);
			record.insert("reachable".into(), json!(true));
			merge(&mut record, power_on.to_json());
			Value::Object(record)
		}
		Err(error) => error_record(device, &error),
	}
}

fn rename_record(device: &Device, settings: &Settings, name: &str) -> Value {
	let client = client(device, settings);
	match client.rename(name) {
		Ok(display_name) => {
			let mut record = base(device);
			record.insert("reachable".into(), json!(true));
			record.insert("display_name".into(), json!(display_name));
			Value::Object(record)
		}
		Err(error) => error_record(device, &error),
	}
}

fn pulse_record(device: &Device, settings: &Settings, config: PulseConfig) -> Value {
	let client = client(device, settings);
	match client.pulse(config) {
		Ok(outcome) => {
			let state = outcome.state().clone();
			let mut record = base(device);
			record.insert("reachable".into(), json!(true));
			record.insert("state".into(), json!(if state.on { "on" } else { "off" }));
			merge(&mut record, state.to_json());
			record.insert("pulse_completed".into(), json!(outcome.completed()));
			if let Some(reason) = outcome.cancellation() {
				record.insert("pulse_cancelled".into(), json!(reason.label()));
			}
			record.insert("pulse_count".into(), json!(config.count));
			record.insert("pulse_duration_ms".into(), json!(config.duration_ms));
			record.insert("pulse_ease".into(), json!(config.ease.label()));
			record.insert("pulse_target".into(), json!(config.target.label()));
			record.insert("pulse_value".into(), json!(config.value));
			Value::Object(record)
		}
		Err(error) => error_record(device, &error),
	}
}

fn resolve_devices(args: &TargetArgs, config: &Config, default_all: bool) -> Result<Vec<Device>> {
	if !args.host.is_empty() {
		return Ok(args
			.host
			.iter()
			.map(|host| Device::new(host.clone(), host.clone()))
			.collect());
	}
	if args.all {
		if config.devices.is_empty() {
			return Err(ElopliError::Config(format!(
				"no devices in {}",
				config.path.display()
			)));
		}
		return Ok(config.devices.clone());
	}
	if !args.devices.is_empty() {
		return args.devices.iter().map(|name| config.find(name)).collect();
	}
	if default_all {
		if config.devices.is_empty() {
			return Err(ElopliError::Config(format!(
				"no devices in {}",
				config.path.display()
			)));
		}
		return Ok(config.devices.clone());
	}
	if config.devices.len() == 1 {
		return Ok(config.devices.clone());
	}
	if config.devices.is_empty() {
		return Err(ElopliError::Config(format!(
			"no devices in {}; run elopli config init",
			config.path.display()
		)));
	}
	let names = config
		.devices
		.iter()
		.map(|device| device.name.as_str())
		.collect::<Vec<_>>()
		.join(", ");
	Err(ElopliError::Config(format!(
		"specify a device name or --all; configured devices: {names}"
	)))
}

fn parallel(devices: &[Device], operation: impl Fn(&Device) -> Value + Sync + Send) -> Vec<Value> {
	devices.par_iter().map(operation).collect()
}

fn exit_code(records: &[Value]) -> u8 {
	if records.iter().any(|record| {
		record.get("reachable").and_then(Value::as_bool) != Some(true)
			|| record.get("error").is_some()
	}) {
		1
	} else {
		0
	}
}

fn print_json(value: &Value) {
	println!(
		"{}",
		serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".into())
	);
}

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
	let mut widths = headers
		.iter()
		.map(|header| header.len())
		.collect::<Vec<_>>();
	for row in rows {
		for (index, cell) in row.iter().enumerate() {
			widths[index] = widths[index].max(cell.len());
		}
	}
	let header = headers
		.iter()
		.enumerate()
		.map(|(index, header)| format!("{header:<width$}", width = widths[index]))
		.collect::<Vec<_>>()
		.join("  ");
	println!("{}", header.trim_end());
	for row in rows {
		let line = row
			.iter()
			.enumerate()
			.map(|(index, cell)| format!("{cell:<width$}", width = widths[index]))
			.collect::<Vec<_>>()
			.join("  ");
		println!("{}", line.trim_end());
	}
}

fn format_state(record: &Value) -> String {
	let name = record.get("name").and_then(Value::as_str).unwrap_or("?");
	if record.get("reachable").and_then(Value::as_bool) != Some(true) {
		return format!(
			"{name}: offline ({})",
			record
				.get("error")
				.and_then(Value::as_str)
				.unwrap_or("unreachable")
		);
	}
	if let Some(error) = record.get("error").and_then(Value::as_str) {
		return format!("{name}: failed ({error})");
	}
	format!(
		"{name}: {}, brightness {}%, warmth {}% ({} mired, {} K)",
		record.get("state").and_then(Value::as_str).unwrap_or("?"),
		record
			.get("brightness")
			.and_then(Value::as_u64)
			.unwrap_or(0),
		record.get("warmth").and_then(Value::as_u64).unwrap_or(0),
		record
			.get("temperature_mired")
			.and_then(Value::as_u64)
			.unwrap_or(0),
		record.get("kelvin").and_then(Value::as_u64).unwrap_or(0)
	)
}

fn print_status(records: &[Value]) {
	let rows = records
		.iter()
		.map(|record| {
			let name = record.get("name").and_then(Value::as_str).unwrap_or("?");
			if record.get("reachable").and_then(Value::as_bool) != Some(true) {
				vec![
					name.to_string(),
					"offline".into(),
					"-".into(),
					"-".into(),
					"-".into(),
					"-".into(),
				]
			} else if record.get("error").is_some() {
				vec![
					name.to_string(),
					"error".into(),
					"-".into(),
					"-".into(),
					"-".into(),
					"-".into(),
				]
			} else {
				vec![
					name.to_string(),
					record
						.get("state")
						.and_then(Value::as_str)
						.unwrap_or("?")
						.to_string(),
					format!(
						"{}%",
						record
							.get("brightness")
							.and_then(Value::as_u64)
							.unwrap_or(0)
					),
					format!(
						"{}%",
						record.get("warmth").and_then(Value::as_u64).unwrap_or(0)
					),
					record
						.get("temperature_mired")
						.and_then(Value::as_u64)
						.unwrap_or(0)
						.to_string(),
					format!(
						"{} K",
						record.get("kelvin").and_then(Value::as_u64).unwrap_or(0)
					),
				]
			}
		})
		.collect::<Vec<_>>();
	print_table(
		&["NAME", "STATE", "BRIGHTNESS", "WARMTH", "MIRED", "KELVIN"],
		&rows,
	);
}

fn print_states(records: &[Value]) {
	if records.is_empty() {
		println!("No devices found.");
		return;
	}
	for record in records {
		println!("{}", format_state(record));
	}
}

fn print_info(records: &[Value]) {
	for (index, record) in records.iter().enumerate() {
		if index > 0 {
			println!();
		}
		let name = record.get("name").and_then(Value::as_str).unwrap_or("?");
		if record.get("reachable").and_then(Value::as_bool) != Some(true) {
			println!(
				"{name}: offline ({})",
				record
					.get("error")
					.and_then(Value::as_str)
					.unwrap_or("unreachable")
			);
			continue;
		}
		if let Some(error) = record.get("error").and_then(Value::as_str) {
			println!("{name}: failed ({error})");
			continue;
		}
		println!(
			"{name} ({})",
			record.get("host").and_then(Value::as_str).unwrap_or("?")
		);
		println!(
			"  Product:     {}",
			record.get("product").and_then(Value::as_str).unwrap_or("-")
		);
		println!(
			"  Firmware:    {}",
			record
				.get("firmware")
				.and_then(Value::as_str)
				.unwrap_or("-")
		);
		println!(
			"  Device name: {}",
			record
				.get("display_name")
				.and_then(Value::as_str)
				.filter(|name| !name.is_empty())
				.unwrap_or("(none)")
		);
		if let Some(wifi) = record.get("wifi").and_then(Value::as_object) {
			println!(
				"  Wi-Fi:       {}, {} MHz, {} dBm ({})",
				wifi.get("ssid").and_then(Value::as_str).unwrap_or("-"),
				wifi.get("frequency_mhz")
					.and_then(Value::as_u64)
					.unwrap_or(0),
				wifi.get("rssi").and_then(Value::as_i64).unwrap_or(0),
				wifi.get("quality").and_then(Value::as_str).unwrap_or("-")
			);
		}
		println!(
			"  State:       {}, brightness {}%, warmth {}%",
			record.get("state").and_then(Value::as_str).unwrap_or("-"),
			record
				.get("brightness")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			record.get("warmth").and_then(Value::as_u64).unwrap_or(0)
		);
	}
}

fn print_power_on(records: &[Value]) {
	for record in records {
		let name = record.get("name").and_then(Value::as_str).unwrap_or("?");
		if record.get("reachable").and_then(Value::as_bool) != Some(true) {
			println!(
				"{name}: offline ({})",
				record
					.get("error")
					.and_then(Value::as_str)
					.unwrap_or("unreachable")
			);
			continue;
		}
		if let Some(error) = record.get("error").and_then(Value::as_str) {
			println!("{name}: failed ({error})");
			continue;
		}
		println!(
			"{name}: {} ({}), default brightness {}%, default warmth {}% ({} mired, {} K)",
			record
				.get("power_on_behavior_label")
				.and_then(Value::as_str)
				.unwrap_or("?"),
			record
				.get("power_on_behavior")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			record
				.get("power_on_brightness")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			record
				.get("power_on_warmth")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			record
				.get("power_on_temperature_mired")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			record
				.get("power_on_kelvin")
				.and_then(Value::as_u64)
				.unwrap_or(0)
		);
	}
}

fn print_renames(records: &[Value]) {
	for record in records {
		let name = record.get("name").and_then(Value::as_str).unwrap_or("?");
		if record.get("reachable").and_then(Value::as_bool) != Some(true) {
			println!(
				"{name}: offline ({})",
				record
					.get("error")
					.and_then(Value::as_str)
					.unwrap_or("unreachable")
			);
		} else if let Some(error) = record.get("error").and_then(Value::as_str) {
			println!("{name}: failed ({error})");
		} else {
			println!(
				"{name}: renamed to {}",
				record
					.get("display_name")
					.and_then(Value::as_str)
					.unwrap_or("?")
			);
		}
	}
}

fn print_pulses(records: &[Value]) {
	for record in records {
		let name = record.get("name").and_then(Value::as_str).unwrap_or("?");
		if record.get("reachable").and_then(Value::as_bool) != Some(true) {
			println!(
				"{name}: offline ({})",
				record
					.get("error")
					.and_then(Value::as_str)
					.unwrap_or("unreachable")
			);
			continue;
		}
		if let Some(error) = record.get("error").and_then(Value::as_str) {
			println!("{name}: failed ({error})");
			continue;
		}
		let completed = record
			.get("pulse_completed")
			.and_then(Value::as_bool)
			.unwrap_or(true);
		let verb = if completed { "pulsed" } else { "pulse stopped" };
		let reason = record
			.get("pulse_cancelled")
			.and_then(Value::as_str)
			.unwrap_or("");
		println!(
			"{name}: {verb} {}× over {} ms ({} {}, peak {}%){}; final {}, brightness {}%, warmth {}%",
			record
				.get("pulse_count")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			record
				.get("pulse_duration_ms")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			record
				.get("pulse_ease")
				.and_then(Value::as_str)
				.unwrap_or("?"),
			record
				.get("pulse_target")
				.and_then(Value::as_str)
				.unwrap_or("?"),
			record
				.get("pulse_value")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			if reason.is_empty() {
				String::new()
			} else {
				format!(", {reason}")
			},
			record.get("state").and_then(Value::as_str).unwrap_or("?"),
			record
				.get("brightness")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			record.get("warmth").and_then(Value::as_u64).unwrap_or(0)
		);
	}
}

fn print_discovered(devices: &[elopli_core::DiscoveredDevice]) {
	let rows = devices
		.iter()
		.map(|device| {
			vec![
				device.instance.clone(),
				device
					.address
					.map(|address| address.to_string())
					.unwrap_or_else(|| "-".into()),
				device.hostname.clone(),
				device.product.clone(),
			]
		})
		.collect::<Vec<_>>();
	print_table(&["NAME", "ADDRESS", "HOST", "PRODUCT"], &rows);
}

fn run_config(args: &ConfigArgs, settings: &Settings) -> Result<u8> {
	match &args.command {
		ConfigCommand::Path => {
			println!("{}", settings.config_path.display());
		}
		ConfigCommand::Show => {
			let config = Config::load(settings.config_path.clone())?;
			print_json(&config.to_json());
		}
		ConfigCommand::Init { force } => {
			if settings.config_path.exists() && !force {
				return Err(ElopliError::Config(format!(
					"{} already exists; use --force to replace it",
					settings.config_path.display()
				)));
			}
			let config = Config {
				path: settings.config_path.clone(),
				devices: Vec::new(),
			};
			config.save()?;
			println!("{}", settings.config_path.display());
		}
	}
	Ok(0)
}

fn run_discover(args: &DiscoverArgs, settings: &Settings) -> Result<u8> {
	if args.wait <= 0.0 || !args.wait.is_finite() {
		return Err(ElopliError::InvalidValue(
			"--wait must be greater than zero".into(),
		));
	}
	let found = discover(Duration::from_secs_f64(args.wait))?;
	if args.write {
		let mut config = Config::load(settings.config_path.clone())?;
		let mut added = 0;
		for device in &found {
			let host = device
				.address
				.map(|address| address.to_string())
				.unwrap_or_else(|| device.hostname.clone());
			if config.add(Device {
				name: device.instance.clone(),
				host,
				port: device.port,
			}) {
				added += 1;
			}
		}
		config.save()?;
		eprintln!("Added {added} device(s) to {}", config.path.display());
	}
	if settings.json {
		print_json(&Value::Array(
			found.iter().map(|device| device.to_json()).collect(),
		));
	} else {
		print_discovered(&found);
	}
	Ok(0)
}

fn run(cli: Cli) -> Result<u8> {
	let settings = settings(&cli)?;
	if let Command::Config(args) = &cli.command {
		return run_config(args, &settings);
	}
	if let Command::Discover(args) = &cli.command {
		return run_discover(args, &settings);
	}
	let config = Config::load(settings.config_path.clone())?;
	match &cli.command {
		Command::List => {
			if settings.json {
				print_json(&Value::Array(
					config.devices.iter().map(Device::to_json).collect(),
				));
			} else if config.devices.is_empty() {
				eprintln!("No devices in {}", config.path.display());
			} else {
				let rows = config
					.devices
					.iter()
					.map(|device| {
						vec![
							device.name.clone(),
							device.host.clone(),
							device.port.to_string(),
						]
					})
					.collect::<Vec<_>>();
				print_table(&["NAME", "HOST", "PORT"], &rows);
			}
			Ok(0)
		}
		Command::Rename(args) => {
			let device = config.find(&args.device)?;
			let name = validate_display_name(&args.new_name)?;
			let record = rename_record(&device, &settings, &name);
			if settings.json {
				print_json(&Value::Array(vec![record.clone()]));
			} else {
				print_renames(&[record.clone()]);
			}
			Ok(exit_code(&[record]))
		}
		Command::Status(args) => {
			let devices = resolve_devices(args, &config, true)?;
			let records = parallel(&devices, |device| state_record(device, &settings));
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_status(&records);
			}
			Ok(exit_code(&records))
		}
		Command::On(args) => {
			let devices = resolve_devices(&args.targets, &config, false)?;
			let records = parallel(&devices, |device| {
				apply_record(
					device,
					&settings,
					|client| client.set_on(true),
					args.wait.wait,
				)
			});
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_states(&records);
			}
			Ok(exit_code(&records))
		}
		Command::Off(args) => {
			let devices = resolve_devices(&args.targets, &config, false)?;
			let records = parallel(&devices, |device| {
				apply_record(
					device,
					&settings,
					|client| client.set_on(false),
					args.wait.wait,
				)
			});
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_states(&records);
			}
			Ok(exit_code(&records))
		}
		Command::Toggle(args) => {
			let devices = resolve_devices(&args.targets, &config, false)?;
			let records = parallel(&devices, |device| {
				apply_record(device, &settings, |client| client.toggle(), args.wait.wait)
			});
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_states(&records);
			}
			Ok(exit_code(&records))
		}
		Command::Brightness(args) => {
			let value = validate_brightness(args.value)?;
			let devices = resolve_devices(&args.targets, &config, false)?;
			let records = parallel(&devices, |device| {
				apply_record(
					device,
					&settings,
					|client| {
						client.update(Update {
							brightness: Some(value),
							..Update::default()
						})
					},
					args.wait.wait,
				)
			});
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_states(&records);
			}
			Ok(exit_code(&records))
		}
		Command::Warmth(args) => {
			let value = validate_warmth(args.value)?;
			let devices = resolve_devices(&args.targets, &config, false)?;
			let records = parallel(&devices, |device| {
				apply_record(
					device,
					&settings,
					|client| {
						client.update(Update {
							temperature: Some(value),
							..Update::default()
						})
					},
					args.wait.wait,
				)
			});
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_states(&records);
			}
			Ok(exit_code(&records))
		}
		Command::Kelvin(args) => {
			let value = kelvin_to_mired(args.value)?;
			let devices = resolve_devices(&args.targets, &config, false)?;
			let records = parallel(&devices, |device| {
				apply_record(
					device,
					&settings,
					|client| {
						client.update(Update {
							temperature: Some(value),
							..Update::default()
						})
					},
					args.wait.wait,
				)
			});
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_states(&records);
			}
			Ok(exit_code(&records))
		}
		Command::Temperature(args) => {
			let value = validate_temperature(args.value)?;
			let devices = resolve_devices(&args.targets, &config, false)?;
			let records = parallel(&devices, |device| {
				apply_record(
					device,
					&settings,
					|client| {
						client.update(Update {
							temperature: Some(value),
							..Update::default()
						})
					},
					args.wait.wait,
				)
			});
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_states(&records);
			}
			Ok(exit_code(&records))
		}
		Command::Set(args) => {
			if args.on && args.off {
				return Err(ElopliError::InvalidValue(
					"--on and --off cannot be used together".into(),
				));
			}
			let colour_count = [
				args.warmth.is_some(),
				args.kelvin.is_some(),
				args.temperature.is_some(),
			]
			.into_iter()
			.filter(|value| *value)
			.count();
			if colour_count > 1 {
				return Err(ElopliError::InvalidValue(
					"--warmth, --kelvin, and --temperature cannot be used together".into(),
				));
			}
			let update = Update {
				on: if args.on {
					Some(true)
				} else if args.off {
					Some(false)
				} else {
					None
				},
				brightness: args.brightness.map(validate_brightness).transpose()?,
				temperature: if let Some(warmth) = args.warmth {
					Some(validate_warmth(warmth)?)
				} else if let Some(kelvin) = args.kelvin {
					Some(kelvin_to_mired(kelvin)?)
				} else if let Some(temperature) = args.temperature {
					Some(validate_temperature(temperature)?)
				} else {
					None
				},
				duration_ms: args.duration_ms,
			};
			if update.is_empty() {
				return Err(ElopliError::InvalidValue(
					"set needs at least one property".into(),
				));
			}
			let devices = resolve_devices(&args.targets, &config, false)?;
			let records = parallel(&devices, |device| {
				apply_record(
					device,
					&settings,
					|client| client.update(update),
					args.wait.wait,
				)
			});
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_states(&records);
			}
			Ok(exit_code(&records))
		}
		Command::PowerOn(args) => {
			if args.last && args.defaults {
				return Err(ElopliError::InvalidValue(
					"--last and --defaults cannot be used together".into(),
				));
			}
			let colour_count = [
				args.warmth.is_some(),
				args.kelvin.is_some(),
				args.temperature.is_some(),
			]
			.into_iter()
			.filter(|value| *value)
			.count();
			if colour_count > 1 {
				return Err(ElopliError::InvalidValue(
					"--warmth, --kelvin, and --temperature cannot be used together".into(),
				));
			}
			let has_values = args.brightness.is_some() || colour_count > 0;
			if has_values && !args.defaults {
				return Err(ElopliError::InvalidValue(
					"--brightness, --warmth, --kelvin, and --temperature need --defaults".into(),
				));
			}
			let behavior = if args.last {
				Some(PowerOnBehavior::RestoreLast)
			} else if args.defaults {
				Some(PowerOnBehavior::RestoreDefaults)
			} else {
				None
			};
			let brightness = args.brightness.map(validate_brightness).transpose()?;
			let temperature = if let Some(warmth) = args.warmth {
				Some(validate_warmth(warmth)?)
			} else if let Some(kelvin) = args.kelvin {
				Some(kelvin_to_mired(kelvin)?)
			} else if let Some(temperature) = args.temperature {
				Some(validate_temperature(temperature)?)
			} else {
				None
			};
			let update = behavior.map(|behavior| (behavior, brightness, temperature));
			let devices = resolve_devices(&args.targets, &config, update.is_none())?;
			let records = parallel(&devices, |device| {
				power_on_record(device, &settings, update, args.wait.wait)
			});
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_power_on(&records);
			}
			Ok(exit_code(&records))
		}
		Command::Pulse(args) => {
			let prompt_config = PulseConfig {
				count: args.count,
				duration_ms: args.duration,
				ease: args.ease.into(),
				target: args.target.into(),
				value: args.value,
			};
			let devices = resolve_devices(&args.targets, &config, false)?;
			let records = parallel(&devices, |device| {
				pulse_record(device, &settings, prompt_config)
			});
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_pulses(&records);
			}
			Ok(exit_code(&records))
		}
		Command::Identify(args) => {
			let devices = resolve_devices(args, &config, false)?;
			let records = parallel(&devices, |device| identify_record(device, &settings));
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				for record in &records {
					let name = record.get("name").and_then(Value::as_str).unwrap_or("?");
					if record.get("reachable").and_then(Value::as_bool) == Some(true) {
						println!("{name}: identified");
					} else {
						println!(
							"{name}: offline ({})",
							record
								.get("error")
								.and_then(Value::as_str)
								.unwrap_or("unreachable")
						);
					}
				}
			}
			Ok(exit_code(&records))
		}
		Command::Info(args) => {
			let devices = resolve_devices(args, &config, true)?;
			let records = parallel(&devices, |device| info_record(device, &settings));
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_info(&records);
			}
			Ok(exit_code(&records))
		}
		_ => unreachable!(),
	}
}

fn main() -> ExitCode {
	let cli = Cli::parse();
	match run(cli) {
		Ok(code) => ExitCode::from(code),
		Err(error @ (ElopliError::Config(_) | ElopliError::InvalidValue(_))) => {
			eprintln!("elopli: {error}");
			ExitCode::from(2)
		}
		Err(error) => {
			eprintln!("elopli: {error}");
			ExitCode::from(1)
		}
	}
}
