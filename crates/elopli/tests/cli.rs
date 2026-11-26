use std::path::Path;
use std::process::Command;

fn binary() -> Command {
	Command::new(env!("CARGO_BIN_EXE_elopli"))
}

#[test]
fn huge_timeout_is_a_usage_error_not_a_panic() {
	let output = binary()
		.args(["--timeout", "1e300", "list"])
		.output()
		.unwrap();
	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("timeout must be between"));
}

#[test]
fn infinite_wait_is_a_usage_error_not_a_panic() {
	let output = binary()
		.args(["on", "--wait", "inf", "--host", "127.0.0.1"])
		.output()
		.unwrap();
	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("wait must be between"));
}

#[test]
fn json_errors_are_json() {
	let missing = std::env::temp_dir().join(format!(
		"elopli-no-such-{}-{}.json",
		std::process::id(),
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.map(|duration| duration.as_nanos())
			.unwrap_or_default()
	));
	let output = binary()
		.args(["--json", "--config", missing.to_str().unwrap(), "status"])
		.output()
		.unwrap();
	assert_eq!(output.status.code(), Some(2));
	let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
	assert_eq!(payload["error"]["kind"], "config");
}

#[test]
fn config_path_json_is_machine_readable() {
	let output = binary()
		.args(["--json", "config", "path"])
		.output()
		.unwrap();
	assert!(output.status.success());
	let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
	assert!(
		payload["path"]
			.as_str()
			.is_some_and(|path| Path::new(path).is_absolute())
	);
}
