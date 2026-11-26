use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand, ValueEnum};
use elopli_core::pulse::MAX_FADE_MS;
use elopli_core::{
	Config, DEFAULT_DURATION, Device, ElopliClient, ElopliError, LightState, PowerOnBehavior,
	PulseConfig, Result, SweepConfig, TransportMode, Update, config_path, discover,
	kelvin_to_mired, parse_duration, validate_brightness, validate_display_name,
	validate_temperature, validate_warmth,
};
use elopli_session::LinkGraph;
use rayon::prelude::*;
use serde_json::{Map, Value, json};

macro_rules! out {
	($($arg:tt)*) => {{
		let mut stdout = std::io::stdout().lock();
		if let Err(error) = writeln!(stdout, $($arg)*)
			&& error.kind() == std::io::ErrorKind::BrokenPipe
		{
			std::process::exit(0);
		}
	}};
}

fn absolute_path(path: &Path) -> PathBuf {
	path.canonicalize()
		.or_else(|_| std::path::absolute(path))
		.unwrap_or_else(|_| path.to_path_buf())
}

fn duration_ms(input: &str, minimum: u32, maximum: u32, label: &str) -> Result<u32> {
	let value = parse_duration(input)?;
	if !(minimum..=maximum).contains(&value) {
		return Err(ElopliError::InvalidValue(format!(
			"{label} duration must be between {minimum} and {maximum} ms"
		)));
	}
	Ok(value)
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TransportArg {
	Auto,
	Native,
	Curl,
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

fn parse_wait_seconds(value: &str) -> std::result::Result<f64, String> {
	let value = value
		.parse::<f64>()
		.map_err(|_| "wait must be a number of seconds".to_string())?;
	if !value.is_finite() || !(0.0..=3600.0).contains(&value) {
		return Err("wait must be between 0 and 3600 seconds".into());
	}
	Ok(value)
}

fn parse_timeout_seconds(value: &str) -> std::result::Result<f64, String> {
	let value = value
		.parse::<f64>()
		.map_err(|_| "timeout must be a number of seconds".to_string())?;
	if !value.is_finite() || !(0.001..=60.0).contains(&value) {
		return Err("timeout must be between 0.001 and 60 seconds".into());
	}
	Ok(value)
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
		value_parser = parse_timeout_seconds,
		value_name = "SECONDS",
		help = "request timeout in seconds, 0.001 to 60"
	)]
	timeout: f64,
	#[arg(
		long,
		global = true,
		default_value_t = 2,
		value_parser = clap::value_parser!(u32).range(0..=10),
		help = "retries per request"
	)]
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
	#[command(about = "set brightness from 2 to 100")]
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
	#[command(about = "pulse brightness and restore the original values")]
	Pulse(PulseArgs),
	#[command(about = "sweep temperature across the full range and restore the original values")]
	Sweep(SweepArgs),
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
		value_parser = parse_wait_seconds,
		value_name = "SECONDS",
		help = "keep retrying unreachable devices for this long, up to 3600"
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
	#[arg(long, value_name = "PERCENT", help = "brightness from 2 to 100")]
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
	#[arg(
		long,
		value_name = "DURATION",
		help = "fade time, for example 500ms; values above 900ms snap on this firmware"
	)]
	duration: Option<String>,
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
	#[command(flatten)]
	wait: WaitArgs,
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
		value_parser = parse_wait_seconds,
		value_name = "SECONDS",
		help = "browse time in seconds, up to 3600"
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
		default_value = DEFAULT_DURATION,
		value_name = "DURATION",
		help = "target duration, for example 1.0s, 500ms, or 10m (default 1.0s, up to 1 hour)"
	)]
	duration: String,
	#[arg(
		long,
		default_value_t = 100,
		value_parser = clap::value_parser!(u8).range(2..=100),
		value_name = "PERCENT",
		help = "peak value in percent (default 100)"
	)]
	value: u8,
	#[arg(
		long,
		default_value_t = 0,
		value_parser = clap::value_parser!(u8).range(0..=100),
		value_name = "PERCENT",
		help = "low value in percent; 0 or 1 turns the light off during the pulse (default 0)"
	)]
	low: u8,
}

#[derive(Debug, Args)]
struct SweepArgs {
	#[command(flatten)]
	targets: TargetArgs,
	#[arg(
		long,
		default_value_t = 1,
		value_parser = clap::value_parser!(u32).range(1..=20),
		help = "number of warm-to-cool sweeps (default 1)"
	)]
	count: u32,
	#[arg(
		long,
		default_value = DEFAULT_DURATION,
		value_name = "DURATION",
		help = "duration of the entire sweep, for example 1.0s, 500ms, or 10m (default 1.0s, up to 1 hour)"
	)]
	duration: String,
	#[arg(
		long,
		default_value_t = 0,
		value_parser = clap::value_parser!(u8).range(0..=100),
		value_name = "PERCENT",
		help = "brightness to hold for the sweep; 0 or 1 keeps the current brightness"
	)]
	brightness: u8,
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
	if cli.timeout <= 0.0 || !cli.timeout.is_finite() || cli.timeout > 60.0 {
		return Err(ElopliError::InvalidValue(
			"--timeout must be between 0 and 60 seconds".into(),
		));
	}
	let timeout = Duration::try_from_secs_f64(cli.timeout)
		.map_err(|_| ElopliError::InvalidValue("--timeout is out of range".into()))?;
	Ok(Settings {
		config_path: config_path(cli.config.as_deref()),
		json: cli.json,
		timeout,
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
		ElopliError::Connection(_) | ElopliError::Discovery(_) => (false, "offline"),
		_ => (true, "error"),
	};
	let mut record = base(device);
	record.insert("reachable".into(), json!(reachable));
	record.insert("state".into(), json!(state));
	record.insert("error_kind".into(), json!(error.kind()));
	record.insert("error".into(), json!(error.to_string()));
	Value::Object(record)
}

fn success_record(device: &Device, add: impl FnOnce(&mut Map<String, Value>)) -> Value {
	let mut record = base(device);
	record.insert("reachable".into(), json!(true));
	add(&mut record);
	Value::Object(record)
}

fn add_state(record: &mut Map<String, Value>, state: LightState) {
	record.insert("state".into(), json!(if state.on { "on" } else { "off" }));
	merge(record, state.to_json());
}

fn state_record(device: &Device, settings: &Settings) -> Value {
	let client = client(device, settings);
	match client.state() {
		Ok(state) => success_record(device, |record| add_state(record, state)),
		Err(error) => error_record(device, &error),
	}
}

fn wait_for_reachable(client: &ElopliClient, wait: f64) -> Result<()> {
	if wait <= 0.0 {
		return Ok(());
	}
	let timeout = Duration::try_from_secs_f64(wait).map_err(|_| {
		ElopliError::InvalidValue("--wait must be a positive finite number of seconds".into())
	})?;
	let deadline = Instant::now() + timeout;
	loop {
		match client.state() {
			Ok(_) => return Ok(()),
			Err(ElopliError::Connection(_)) if Instant::now() < deadline => {
				std::thread::sleep(Duration::from_secs(1));
			}
			Err(error) => return Err(error),
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
	let result = wait_for_reachable(&client, wait).and_then(|()| operation(&client));
	match result {
		Ok(state) => success_record(device, |record| add_state(record, state)),
		Err(error) => error_record(device, &error),
	}
}

fn identify_record(device: &Device, settings: &Settings) -> Value {
	let client = client(device, settings);
	match client.identify() {
		Ok(()) => success_record(device, |record| {
			record.insert("identified".into(), json!(true));
		}),
		Err(error) => error_record(device, &error),
	}
}

fn info_record(device: &Device, settings: &Settings) -> Value {
	let client = client(device, settings);
	match client.snapshot() {
		Ok(snapshot) => success_record(device, |record| {
			merge(record, snapshot.info.to_json());
			add_state(record, snapshot.state);
		}),
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
	let result = wait_for_reachable(&client, wait).and_then(|()| match update {
		Some((behavior, brightness, temperature)) => {
			client.set_power_on(behavior, brightness, temperature)
		}
		None => client.power_on_settings(),
	});
	match result {
		Ok(power_on) => success_record(device, |record| merge(record, power_on.to_json())),
		Err(error) => error_record(device, &error),
	}
}

fn rename_record(device: &Device, settings: &Settings, name: &str, wait: f64) -> Value {
	let client = client(device, settings);
	match wait_for_reachable(&client, wait).and_then(|()| client.rename(name)) {
		Ok(display_name) => success_record(device, |record| {
			record.insert("display_name".into(), json!(display_name));
		}),
		Err(error) => error_record(device, &error),
	}
}

fn pulse_record(device: &Device, settings: &Settings, config: PulseConfig) -> Value {
	let client = client(device, settings);
	match client.pulse(config) {
		Ok(outcome) => success_record(device, |record| {
			add_state(record, *outcome.state());
			record.insert("pulse_completed".into(), json!(outcome.completed()));
			if let Some(reason) = outcome.cancellation() {
				record.insert("pulse_cancelled".into(), json!(reason.label()));
			}
			record.insert("pulse_count".into(), json!(config.count));
			record.insert("pulse_duration_ms".into(), json!(config.duration_ms));
			record.insert("pulse_value".into(), json!(config.value));
			record.insert("pulse_low".into(), json!(config.low));
		}),
		Err(error) => error_record(device, &error),
	}
}

fn sweep_record(device: &Device, settings: &Settings, config: SweepConfig) -> Value {
	let client = client(device, settings);
	match client.sweep(config) {
		Ok(outcome) => success_record(device, |record| {
			add_state(record, *outcome.state());
			record.insert("sweep_completed".into(), json!(outcome.completed()));
			if let Some(reason) = outcome.cancellation() {
				record.insert("sweep_cancelled".into(), json!(reason.label()));
			}
			record.insert("sweep_count".into(), json!(config.count));
			record.insert("sweep_duration_ms".into(), json!(config.duration_ms));
			record.insert("sweep_brightness".into(), json!(config.brightness));
		}),
		Err(error) => error_record(device, &error),
	}
}

fn resolve_devices(args: &TargetArgs, config: &Config, default_all: bool) -> Result<Vec<Device>> {
	if args.all && !args.devices.is_empty() {
		return Err(ElopliError::InvalidValue(
			"--all cannot be combined with device names".into(),
		));
	}
	if !args.host.is_empty() && (args.all || !args.devices.is_empty()) {
		return Err(ElopliError::InvalidValue(
			"--host cannot be combined with --all or device names".into(),
		));
	}
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
	let threads = devices.len().clamp(1, 8);
	match rayon::ThreadPoolBuilder::new().num_threads(threads).build() {
		Ok(pool) => pool.install(|| devices.par_iter().map(operation).collect()),
		Err(_) => devices.iter().map(operation).collect(),
	}
}

fn exit_code(records: &[Value]) -> u8 {
	if records.iter().any(|record| {
		record.get("reachable").and_then(Value::as_bool) != Some(true)
			|| record.get("error").is_some()
			|| record.get("pulse_completed").and_then(Value::as_bool) == Some(false)
			|| record.get("sweep_completed").and_then(Value::as_bool) == Some(false)
	}) {
		1
	} else {
		0
	}
}

fn print_json(value: &Value) {
	out!(
		"{}",
		serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".into())
	);
}

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
	let width = |value: &str| value.chars().count();
	let mut widths = headers
		.iter()
		.map(|header| width(header))
		.collect::<Vec<_>>();
	for row in rows {
		for (index, cell) in row.iter().enumerate() {
			if let Some(column) = widths.get_mut(index) {
				*column = (*column).max(width(cell));
			}
		}
	}
	let header = headers
		.iter()
		.enumerate()
		.map(|(index, header)| format!("{header:<width$}", width = widths[index]))
		.collect::<Vec<_>>()
		.join("  ");
	out!("{}", header.trim_end());
	for row in rows {
		let line = row
			.iter()
			.enumerate()
			.map(|(index, cell)| {
				format!(
					"{cell:<width$}",
					width = widths.get(index).copied().unwrap_or(0)
				)
			})
			.collect::<Vec<_>>()
			.join("  ");
		out!("{}", line.trim_end());
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
		out!("No devices found.");
		return;
	}
	for record in records {
		out!("{}", format_state(record));
	}
}

fn print_info(records: &[Value]) {
	for (index, record) in records.iter().enumerate() {
		if index > 0 {
			out!();
		}
		let name = record.get("name").and_then(Value::as_str).unwrap_or("?");
		if record.get("reachable").and_then(Value::as_bool) != Some(true) {
			out!(
				"{name}: offline ({})",
				record
					.get("error")
					.and_then(Value::as_str)
					.unwrap_or("unreachable")
			);
			continue;
		}
		if let Some(error) = record.get("error").and_then(Value::as_str) {
			out!("{name}: failed ({error})");
			continue;
		}
		out!(
			"{name} ({})",
			record.get("host").and_then(Value::as_str).unwrap_or("?")
		);
		out!(
			"  Product:     {}",
			record.get("product").and_then(Value::as_str).unwrap_or("-")
		);
		out!(
			"  Firmware:    {}",
			record
				.get("firmware")
				.and_then(Value::as_str)
				.unwrap_or("-")
		);
		out!(
			"  Device name: {}",
			record
				.get("display_name")
				.and_then(Value::as_str)
				.filter(|name| !name.is_empty())
				.unwrap_or("(none)")
		);
		if let Some(wifi) = record.get("wifi").and_then(Value::as_object) {
			out!(
				"  Wi-Fi:       {}, {} MHz, {} dBm ({})",
				wifi.get("ssid").and_then(Value::as_str).unwrap_or("-"),
				wifi.get("frequency_mhz")
					.and_then(Value::as_u64)
					.unwrap_or(0),
				wifi.get("rssi").and_then(Value::as_i64).unwrap_or(0),
				wifi.get("quality").and_then(Value::as_str).unwrap_or("-")
			);
		}
		out!(
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
			out!(
				"{name}: offline ({})",
				record
					.get("error")
					.and_then(Value::as_str)
					.unwrap_or("unreachable")
			);
			continue;
		}
		if let Some(error) = record.get("error").and_then(Value::as_str) {
			out!("{name}: failed ({error})");
			continue;
		}
		out!(
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
			out!(
				"{name}: offline ({})",
				record
					.get("error")
					.and_then(Value::as_str)
					.unwrap_or("unreachable")
			);
		} else if let Some(error) = record.get("error").and_then(Value::as_str) {
			out!("{name}: failed ({error})");
		} else {
			out!(
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
			out!(
				"{name}: offline ({})",
				record
					.get("error")
					.and_then(Value::as_str)
					.unwrap_or("unreachable")
			);
			continue;
		}
		if let Some(error) = record.get("error").and_then(Value::as_str) {
			out!("{name}: failed ({error})");
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
		let duration = record
			.get("pulse_duration_ms")
			.and_then(Value::as_u64)
			.unwrap_or(0);
		out!(
			"{name}: {verb} {}× over {} ms (peak {}%, low {}%){}; final {}, brightness {}%, warmth {}%",
			record
				.get("pulse_count")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			duration,
			record
				.get("pulse_value")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			record.get("pulse_low").and_then(Value::as_u64).unwrap_or(0),
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

fn print_sweeps(records: &[Value]) {
	for record in records {
		let name = record.get("name").and_then(Value::as_str).unwrap_or("?");
		if record.get("reachable").and_then(Value::as_bool) != Some(true) {
			out!(
				"{name}: offline ({})",
				record
					.get("error")
					.and_then(Value::as_str)
					.unwrap_or("unreachable")
			);
			continue;
		}
		if let Some(error) = record.get("error").and_then(Value::as_str) {
			out!("{name}: failed ({error})");
			continue;
		}
		let completed = record
			.get("sweep_completed")
			.and_then(Value::as_bool)
			.unwrap_or(true);
		let verb = if completed { "swept" } else { "sweep stopped" };
		let reason = record
			.get("sweep_cancelled")
			.and_then(Value::as_str)
			.unwrap_or("");
		let brightness = match record.get("sweep_brightness").and_then(Value::as_u64) {
			Some(value) if value > 0 => format!("brightness {value}%"),
			_ => "brightness unchanged".into(),
		};
		out!(
			"{name}: {verb} {}× over {} ms ({}){}; final {}, brightness {}%, warmth {}%",
			record
				.get("sweep_count")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			record
				.get("sweep_duration_ms")
				.and_then(Value::as_u64)
				.unwrap_or(0),
			brightness,
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
	let path = absolute_path(&settings.config_path);
	match &args.command {
		ConfigCommand::Path => {
			if settings.json {
				print_json(&json!({ "path": path.display().to_string() }));
			} else {
				out!("{}", path.display());
			}
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
			let mut config = Config::new(settings.config_path.clone());
			config.save()?;
			if settings.json {
				print_json(&json!({
					"path": path.display().to_string(),
					"created": true,
				}));
			} else {
				out!("{}", path.display());
			}
		}
	}
	Ok(0)
}

fn run_discover(args: &DiscoverArgs, settings: &Settings) -> Result<u8> {
	if args.wait <= 0.0 || !args.wait.is_finite() || args.wait > 3600.0 {
		return Err(ElopliError::InvalidValue(
			"--wait must be between 0 and 3600 seconds".into(),
		));
	}
	let timeout = Duration::try_from_secs_f64(args.wait)
		.map_err(|_| ElopliError::InvalidValue("--wait is out of range".into()))?;
	let found = discover(timeout)?;
	if args.write {
		let mut config = Config::load(settings.config_path.clone())?;
		let mut added = 0;
		for device in &found {
			let Some(address) = device.address else {
				eprintln!(
					"Skipped {}: no IPv4 address was advertised",
					device.instance
				);
				continue;
			};
			let host = address.to_string();
			let client = ElopliClient::new(
				host.clone(),
				device.port,
				Duration::from_secs(3),
				2,
				TransportMode::Native,
			);
			let remote = client
				.info()
				.ok()
				.map(|info| info.display_name.trim().to_string())
				.filter(|name| !name.is_empty());
			let mut name = remote.unwrap_or_else(|| device.instance.clone());
			if config.devices.iter().any(|existing| {
				existing.name.to_lowercase() == name.to_lowercase() && existing.host != host
			}) {
				name = device.instance.clone();
			}
			if config.add(Device {
				name,
				host,
				port: device.port,
			})? {
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
	let mut config = Config::load(settings.config_path.clone())?;
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
			let device = config
				.find(&args.device)
				.unwrap_or_else(|_| Device::new(args.device.clone(), args.device.clone()));
			let name = validate_display_name(&args.new_name)?;
			let record = rename_record(&device, &settings, &name, args.wait.wait);
			if let Some(display_name) = record
				.get("display_name")
				.and_then(Value::as_str)
				.filter(|name| !name.is_empty())
			{
				match config.rename(&device.name, display_name) {
					Ok(true) => {
						if let Err(error) = config.save() {
							eprintln!(
								"renamed on the device, but could not update {}: {error}",
								config.path.display()
							);
						} else {
							let links_path = config.path.with_file_name("links.json");
							if let Ok(mut links) = LinkGraph::load(&links_path)
								&& links.rename(&device.name, display_name)
								&& let Err(error) = links.save(&links_path)
							{
								eprintln!(
									"renamed on the device, but could not update {}: {error}",
									links_path.display()
								);
							}
						}
					}
					Ok(false) => {}
					Err(error) => eprintln!(
						"renamed on the device, but could not update {}: {error}",
						config.path.display()
					),
				}
			}
			if settings.json {
				print_json(&Value::Array(vec![record.clone()]));
			} else {
				print_renames(std::slice::from_ref(&record));
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
			let duration_ms = args
				.duration
				.as_deref()
				.map(|value| duration_ms(value, 1, MAX_FADE_MS, "transition"))
				.transpose()?;
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
				duration_ms,
			};
			if update.on.is_none() && update.brightness.is_none() && update.temperature.is_none() {
				return Err(ElopliError::InvalidValue(
					"set needs at least one property besides --duration".into(),
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
				duration_ms: duration_ms(&args.duration, 50, 3_600_000, "pulse")?,
				value: args.value,
				low: args.low,
			};
			prompt_config.validate()?;
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
		Command::Sweep(args) => {
			let sweep_config = SweepConfig {
				count: args.count,
				duration_ms: duration_ms(&args.duration, 100, 3_600_000, "sweep")?,
				brightness: (args.brightness >= 2).then_some(args.brightness),
			};
			sweep_config.validate()?;
			let devices = resolve_devices(&args.targets, &config, false)?;
			let records = parallel(&devices, |device| {
				sweep_record(device, &settings, sweep_config)
			});
			if settings.json {
				print_json(&Value::Array(records.clone()));
			} else {
				print_sweeps(&records);
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
					if let Some(error) = record.get("error").and_then(Value::as_str) {
						out!("{name}: failed ({error})");
					} else if record.get("reachable").and_then(Value::as_bool) == Some(true) {
						out!("{name}: identified");
					} else {
						out!("{name}: offline");
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
		Command::Config(_) | Command::Discover(_) => Err(ElopliError::Unsupported(
			"this command is handled before the device registry is loaded".into(),
		)),
	}
}

fn main() -> ExitCode {
	let cli = Cli::parse();
	let json = cli.json;
	match run(cli) {
		Ok(code) => ExitCode::from(code),
		Err(error) => {
			let code = match error {
				ElopliError::Config(_) | ElopliError::InvalidValue(_) => 2,
				_ => 1,
			};
			if json {
				print_json(&json!({
					"error": {
						"kind": error.kind(),
						"message": error.to_string(),
					}
				}));
			} else {
				eprintln!("elopli: {error}");
			}
			ExitCode::from(code)
		}
	}
}
