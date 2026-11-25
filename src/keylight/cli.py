"""Command-line interface for Elgato Key Lights."""

from __future__ import annotations

import argparse
import json
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from typing import Any, Callable

from . import __version__
from .api import (
	MIRED_MAX,
	MIRED_MIN,
	POWER_ON_RESTORE_DEFAULTS,
	POWER_ON_RESTORE_LAST,
	KeyLightApiError,
	KeyLightClient,
	KeyLightConnectionError,
	KeyLightError,
	kelvin_to_mired,
	mired_to_kelvin,
	mired_to_warmth,
	validate_brightness,
	validate_display_name,
	validate_temperature,
	validate_warmth,
)
from .config import Config, ConfigError, Device, config_path
from .discovery import DiscoveryError, discover


class CommandError(Exception):
	"""The command line is not valid for the current configuration."""


def _add_common(parser: argparse.ArgumentParser) -> None:
	parser.add_argument("--config", metavar="PATH", default=argparse.SUPPRESS, help="path to the device registry")
	parser.add_argument("--json", action="store_true", default=argparse.SUPPRESS, help="print machine-readable JSON")
	parser.add_argument(
		"--timeout",
		type=float,
		default=argparse.SUPPRESS,
		help="request timeout in seconds (default 3)",
	)
	parser.add_argument(
		"--retries",
		type=int,
		default=argparse.SUPPRESS,
		help="retries per request (default 2)",
	)
	parser.add_argument(
		"--transport",
		choices=["auto", "urllib", "curl"],
		default=argparse.SUPPRESS,
		help="HTTP transport (default auto)",
	)


def _add_targets(parser: argparse.ArgumentParser) -> None:
	parser.add_argument("devices", nargs="*", metavar="DEVICE", help="device name from the registry")
	parser.add_argument("--all", action="store_true", help="use every device in the registry")
	parser.add_argument(
		"--host",
		action="append",
		default=[],
		metavar="HOST",
		help="use a host directly; repeat for more",
	)


def _add_wait(parser: argparse.ArgumentParser) -> None:
	parser.add_argument(
		"--wait",
		type=float,
		default=0.0,
		metavar="SECONDS",
		help="keep retrying unreachable devices for this long",
	)


def build_parser() -> argparse.ArgumentParser:
	common = argparse.ArgumentParser(add_help=False)
	_add_common(common)
	parser = argparse.ArgumentParser(
		prog="keylight",
		description="Control Elgato Key Lights over the local network.",
		parents=[common],
	)
	parser.add_argument("--version", action="version", version=f"%(prog)s {__version__}")
	subparsers = parser.add_subparsers(dest="command", required=True)

	status = subparsers.add_parser("status", parents=[common], help="read the state of one or more lights")
	_add_targets(status)

	listing = subparsers.add_parser("list", parents=[common], help="list the configured devices")
	listing.set_defaults(command="list")

	for name, help_text in (
		("on", "turn one or more lights on"),
		("off", "turn one or more lights off"),
		("toggle", "toggle one or more lights"),
	):
		command = subparsers.add_parser(name, parents=[common], help=help_text)
		_add_targets(command)
		_add_wait(command)

	for name, metavar, help_text in (
		("brightness", "PERCENT", "set brightness from 0 to 100"),
		("warmth", "PERCENT", "set warmth from 0 (cool) to 100 (warm)"),
		("kelvin", "KELVIN", "set colour temperature from 2900 to 7000 K"),
		("temperature", "MIRED", "set colour temperature in mired from 143 to 344"),
	):
		command = subparsers.add_parser(name, parents=[common], help=help_text)
		command.add_argument("value", metavar=metavar, type=float)
		_add_targets(command)
		_add_wait(command)

	setter = subparsers.add_parser("set", parents=[common], help="set several properties in one request")
	setter.add_argument("devices", nargs="*", metavar="DEVICE", help="device name from the registry")
	setter.add_argument("--all", action="store_true", help="use every device in the registry")
	setter.add_argument(
		"--host",
		action="append",
		default=[],
		metavar="HOST",
		help="use a host directly; repeat for more",
	)
	power = setter.add_mutually_exclusive_group()
	power.add_argument("--on", dest="power", action="store_const", const=True, help="turn the light on")
	power.add_argument("--off", dest="power", action="store_const", const=False, help="turn the light off")
	setter.add_argument("--brightness", type=float, metavar="PERCENT", help="brightness from 0 to 100")
	colour = setter.add_mutually_exclusive_group()
	colour.add_argument("--warmth", type=float, metavar="PERCENT", help="warmth from 0 (cool) to 100 (warm)")
	colour.add_argument("--kelvin", type=float, metavar="KELVIN", help="colour temperature from 2900 to 7000 K")
	colour.add_argument("--temperature", type=float, metavar="MIRED", help="colour temperature in mired")
	setter.add_argument("--duration-ms", type=int, metavar="MS", help="transition time")
	_add_wait(setter)

	identify = subparsers.add_parser("identify", parents=[common], help="flash one or more lights")
	_add_targets(identify)

	rename = subparsers.add_parser("rename", parents=[common], help="change the device display name")
	rename.add_argument("device", metavar="DEVICE", help="device name from the registry or a host")
	rename.add_argument("new_name", metavar="NEW_NAME", help="new display name (1 to 63 bytes)")

	power_on = subparsers.add_parser("power-on", parents=[common], help="read or set the power-on behaviour")
	power_on.add_argument("devices", nargs="*", metavar="DEVICE", help="device name from the registry")
	power_on.add_argument("--all", action="store_true", help="use every device in the registry")
	power_on.add_argument(
		"--host",
		action="append",
		default=[],
		metavar="HOST",
		help="use a host directly; repeat for more",
	)
	power_on_mode = power_on.add_mutually_exclusive_group()
	power_on_mode.add_argument(
		"--last",
		dest="power_on_mode",
		action="store_const",
		const=POWER_ON_RESTORE_LAST,
		help="restore the last used setting",
	)
	power_on_mode.add_argument(
		"--defaults",
		dest="power_on_mode",
		action="store_const",
		const=POWER_ON_RESTORE_DEFAULTS,
		help="restore the defaults",
	)
	power_on.add_argument("--brightness", type=float, metavar="PERCENT", help="default brightness for --defaults")
	power_on_colour = power_on.add_mutually_exclusive_group()
	power_on_colour.add_argument("--warmth", type=float, metavar="PERCENT", help="default warmth for --defaults")
	power_on_colour.add_argument("--kelvin", type=float, metavar="KELVIN", help="default colour temperature for --defaults")
	power_on_colour.add_argument("--temperature", type=float, metavar="MIRED", help="default colour temperature in mired")
	_add_wait(power_on)

	info = subparsers.add_parser("info", parents=[common], help="read product and network details")
	_add_targets(info)

	discovery = subparsers.add_parser("discover", parents=[common], help="find lights through mDNS")
	discovery.add_argument(
		"--wait",
		type=float,
		default=3.0,
		metavar="SECONDS",
		help="browse time in seconds (default 3)",
	)
	discovery.add_argument("--write", action="store_true", help="add the found devices to the registry")

	config = subparsers.add_parser("config", parents=[common], help="inspect or create the device registry")
	config_commands = config.add_subparsers(dest="config_command", required=True)
	config_commands.add_parser("path", parents=[common], help="print the registry path")
	config_commands.add_parser("show", parents=[common], help="print the registry as JSON")
	initialise = config_commands.add_parser("init", parents=[common], help="create an empty registry")
	initialise.add_argument("--force", action="store_true", help="replace an existing registry")

	return parser


def _settings(args: argparse.Namespace) -> dict[str, Any]:
	timeout = getattr(args, "timeout", 3.0)
	if timeout <= 0:
		raise CommandError("--timeout must be greater than zero")
	retries = getattr(args, "retries", 2)
	if retries < 0:
		raise CommandError("--retries must be zero or more")
	return {
		"config": getattr(args, "config", None),
		"json": getattr(args, "json", False),
		"timeout": timeout,
		"retries": retries,
		"transport": getattr(args, "transport", "auto"),
	}


def _client(device: Device, settings: dict[str, Any]) -> KeyLightClient:
	return KeyLightClient(
		device.host,
		port=device.port,
		timeout=settings["timeout"],
		retries=settings["retries"],
		transport=settings["transport"],
	)


def _resolve_devices(args: argparse.Namespace, config: Config, default_all: bool = False) -> list[Device]:
	hosts = getattr(args, "host", [])
	if hosts:
		return [Device(name=host, host=host) for host in hosts]
	if getattr(args, "all", False):
		if not config.devices:
			raise CommandError(f"no devices in {config.path}")
		return list(config.devices)
	names = getattr(args, "devices", [])
	if names:
		return [config.find(name) for name in names]
	if default_all:
		if not config.devices:
			raise CommandError(f"no devices in {config.path}")
		return list(config.devices)
	if len(config.devices) == 1:
		return [config.devices[0]]
	if not config.devices:
		raise CommandError(f"no devices in {config.path}; run keylight config init")
	names = ", ".join(device.name for device in config.devices)
	raise CommandError(f"specify a device name or --all; configured devices: {names}")


def _parallel(devices: list[Device], operation: Callable[[Device], dict[str, Any]]) -> list[dict[str, Any]]:
	if len(devices) == 1:
		return [operation(devices[0])]
	with ThreadPoolExecutor(max_workers=min(8, len(devices))) as pool:
		return list(pool.map(operation, devices))


def _base(device: Device) -> dict[str, Any]:
	return {"name": device.name, "host": device.host, "port": device.port}


def _state_record(device: Device, settings: dict[str, Any]) -> dict[str, Any]:
	client = _client(device, settings)
	try:
		state = client.state()
	except KeyLightApiError as error:
		return {**_base(device), "reachable": True, "state": "error", "error": str(error)}
	except KeyLightError as error:
		return {**_base(device), "reachable": False, "state": "offline", "error": str(error)}
	return {**_base(device), "reachable": True, "state": "on" if state.on else "off", **state.to_dict()}


def _with_deadline(operation: Callable[[], Any], seconds: float) -> Any:
	deadline = time.monotonic() + max(0.0, seconds)
	while True:
		try:
			return operation()
		except KeyLightConnectionError:
			if time.monotonic() >= deadline:
				raise
			time.sleep(min(1.0, max(0.05, deadline - time.monotonic())))


def _apply_record(
	device: Device,
	settings: dict[str, Any],
	operation: Callable[[KeyLightClient], Any],
	wait: float,
) -> dict[str, Any]:
	client = _client(device, settings)
	try:
		state = _with_deadline(lambda: operation(client), wait)
	except KeyLightApiError as error:
		return {**_base(device), "reachable": True, "state": "error", "error": str(error)}
	except KeyLightError as error:
		return {**_base(device), "reachable": False, "state": "offline", "error": str(error)}
	return {**_base(device), "reachable": True, "state": "on" if state.on else "off", **state.to_dict()}


def _identify_record(device: Device, settings: dict[str, Any]) -> dict[str, Any]:
	client = _client(device, settings)
	try:
		client.identify()
	except KeyLightApiError as error:
		return {**_base(device), "reachable": True, "error": str(error)}
	except KeyLightError as error:
		return {**_base(device), "reachable": False, "error": str(error)}
	return {**_base(device), "reachable": True, "identified": True}


def _info_record(device: Device, settings: dict[str, Any]) -> dict[str, Any]:
	client = _client(device, settings)
	try:
		info = client.accessory_info()
		state = client.state()
	except KeyLightApiError as error:
		return {**_base(device), "reachable": True, "error": str(error)}
	except KeyLightError as error:
		return {**_base(device), "reachable": False, "error": str(error)}
	return {
		**_base(device),
		"reachable": True,
		**info.to_dict(),
		"state": "on" if state.on else "off",
		**state.to_dict(),
	}


POWER_ON_LABELS = {
	POWER_ON_RESTORE_LAST: "restore last used",
	POWER_ON_RESTORE_DEFAULTS: "restore defaults",
}


def _rename_record(device: Device, settings: dict[str, Any], name: str) -> dict[str, Any]:
	client = _client(device, settings)
	try:
		client.set_display_name(name)
		info = client.accessory_info()
	except KeyLightApiError as error:
		return {**_base(device), "reachable": True, "error": str(error)}
	except KeyLightError as error:
		return {**_base(device), "reachable": False, "error": str(error)}
	return {**_base(device), "reachable": True, "display_name": info.display_name}


def _apply_settings(client: KeyLightClient, update: dict[str, Any]) -> dict[str, Any]:
	current = client.settings()
	current.update(update)
	client.update_settings(current)
	# A settings PUT returns an empty body, so read the stored values again.
	return client.settings()


def _power_on_record(
	device: Device,
	settings: dict[str, Any],
	update: dict[str, Any],
	wait: float,
) -> dict[str, Any]:
	client = _client(device, settings)
	try:
		if update:
			state = _with_deadline(lambda: _apply_settings(client, update), wait)
		else:
			state = client.settings()
	except KeyLightApiError as error:
		return {**_base(device), "reachable": True, "error": str(error)}
	except KeyLightError as error:
		return {**_base(device), "reachable": False, "error": str(error)}
	behavior = int(state.get("powerOnBehavior", 0))
	temperature = int(state.get("powerOnTemperature", MIRED_MIN))
	temperature = max(MIRED_MIN, min(MIRED_MAX, temperature))
	return {
		**_base(device),
		"reachable": True,
		"power_on_behavior": behavior,
		"power_on_behavior_label": POWER_ON_LABELS.get(behavior, f"unknown ({behavior})"),
		"power_on_brightness": int(state.get("powerOnBrightness", 0)),
		"power_on_temperature_mired": temperature,
		"power_on_warmth": mired_to_warmth(temperature),
		"power_on_kelvin": mired_to_kelvin(temperature),
	}


def _format_state(record: dict[str, Any]) -> str:
	if not record.get("reachable", True):
		return f"{record['name']}: offline ({record.get('error', 'unreachable')})"
	if "error" in record:
		return f"{record['name']}: failed ({record['error']})"
	return (
		f"{record['name']}: {record['state']}, "
		f"brightness {record['brightness']}%, "
		f"warmth {record['warmth']}% "
		f"({record['temperature_mired']} mired, {record['kelvin']} K)"
	)


def _print_table(headers: list[str], rows: list[list[str]]) -> None:
	widths = [len(header) for header in headers]
	for row in rows:
		for index, cell in enumerate(row):
			widths[index] = max(widths[index], len(cell))
	print("  ".join(header.ljust(widths[index]) for index, header in enumerate(headers)).rstrip())
	for row in rows:
		print("  ".join(cell.ljust(widths[index]) for index, cell in enumerate(row)).rstrip())


def _print_status(records: list[dict[str, Any]]) -> None:
	rows: list[list[str]] = []
	for record in records:
		if not record.get("reachable", True):
			rows.append([record["name"], "offline", "-", "-", "-", "-"])
			continue
		if "error" in record:
			rows.append([record["name"], "error", "-", "-", "-", "-"])
			continue
		rows.append(
			[
				record["name"],
				record["state"],
				f"{record['brightness']}%",
				f"{record['warmth']}%",
				str(record["temperature_mired"]),
				f"{record['kelvin']} K",
			]
		)
	_print_table(["NAME", "STATE", "BRIGHTNESS", "WARMTH", "MIRED", "KELVIN"], rows)


def _print_info(records: list[dict[str, Any]]) -> None:
	for index, record in enumerate(records):
		if index:
			print()
		if not record.get("reachable", True):
			print(f"{record['name']}: offline ({record.get('error', 'unreachable')})")
			continue
		if "error" in record:
			print(f"{record['name']}: failed ({record['error']})")
			continue
		print(f"{record['name']} ({record['host']})")
		print(f"  Product:     {record.get('product') or '-'}")
		print(f"  Firmware:    {record.get('firmware') or '-'}")
		print(f"  Device name: {record.get('display_name') or '(none)'}")
		wifi = record.get("wifi")
		if wifi:
			print(f"  Wi-Fi:       {wifi['ssid']}, {wifi['frequency_mhz']} MHz, {wifi['rssi']} dBm")
		print(
			f"  State:       {record.get('state', '-')}, "
			f"brightness {record.get('brightness', '-')}%, "
			f"warmth {record.get('warmth', '-')}%"
		)


def _exit_code(records: list[dict[str, Any]]) -> int:
	failed = any(not record.get("reachable", True) or "error" in record for record in records)
	return 1 if failed else 0


def _emit_states(records: list[dict[str, Any]], settings: dict[str, Any]) -> None:
	if settings["json"]:
		print(json.dumps(records, indent=2))
		return
	for record in records:
		print(_format_state(record))


def _print_renames(records: list[dict[str, Any]]) -> None:
	for record in records:
		if not record.get("reachable", True):
			print(f"{record['name']}: offline ({record.get('error', 'unreachable')})")
		elif "error" in record:
			print(f"{record['name']}: failed ({record['error']})")
		else:
			print(f"{record['name']}: renamed to {record['display_name']}")


def _print_power_on(records: list[dict[str, Any]]) -> None:
	for record in records:
		if not record.get("reachable", True):
			print(f"{record['name']}: offline ({record.get('error', 'unreachable')})")
		elif "error" in record:
			print(f"{record['name']}: failed ({record['error']})")
		else:
			print(
				f"{record['name']}: {record['power_on_behavior_label']} "
				f"({record['power_on_behavior']}), default brightness "
				f"{record['power_on_brightness']}%, default warmth "
				f"{record['power_on_warmth']}% "
				f"({record['power_on_temperature_mired']} mired, "
				f"{record['power_on_kelvin']} K)"
			)


def _run_list(config: Config, settings: dict[str, Any]) -> int:
	if settings["json"]:
		print(json.dumps([device.to_dict() for device in config.devices], indent=2))
		return 0
	if not config.devices:
		print(f"No devices in {config.path}", file=sys.stderr)
		return 0
	_print_table(
		["NAME", "HOST", "PORT"],
		[[device.name, device.host, str(device.port)] for device in config.devices],
	)
	return 0


def _run_discover(args: argparse.Namespace, settings: dict[str, Any]) -> int:
	try:
		found = discover(timeout=args.wait)
	except DiscoveryError as error:
		print(f"keylight: {error}", file=sys.stderr)
		return 1
	if args.write:
		config = Config.load(config_path(settings["config"]))
		added = 0
		for device in found:
			if config.add(Device(name=device.instance, host=device.address or device.host, port=device.port)):
				added += 1
		config.save()
		print(f"Added {added} device(s) to {config.path}", file=sys.stderr)
	if settings["json"]:
		print(json.dumps([device.to_dict() for device in found], indent=2))
		return 0
	if not found:
		print("No Elgato lights found.", file=sys.stderr)
		return 0
	_print_table(
		["NAME", "ADDRESS", "HOST", "PRODUCT"],
		[[device.instance, device.address or "-", device.host, device.product] for device in found],
	)
	return 0


def _run_config(args: argparse.Namespace) -> int:
	path = config_path(getattr(args, "config", None))
	if args.config_command == "path":
		print(path)
		return 0
	if args.config_command == "show":
		config = Config.load(path)
		print(json.dumps(config.to_dict(), indent=2))
		return 0
	if args.config_command == "init":
		if path.exists() and not args.force:
			raise CommandError(f"{path} already exists; use --force to replace it")
		config = Config.load(path)
		config.devices = []
		config.save()
		print(path)
		return 0
	raise CommandError("unknown config command")


def run(args: argparse.Namespace) -> int:
	settings = _settings(args)
	if args.command == "discover":
		return _run_discover(args, settings)
	if args.command == "config":
		return _run_config(args)
	config = Config.load(config_path(settings["config"]))
	if args.command == "list":
		return _run_list(config, settings)
	if args.command == "rename":
		device = config.find(args.device)
		name = validate_display_name(args.new_name)
		record = _rename_record(device, settings, name)
		if settings["json"]:
			print(json.dumps([record], indent=2))
		else:
			_print_renames([record])
		return _exit_code([record])
	default_all = args.command in {"status", "info"} or (
		args.command == "power-on" and args.power_on_mode is None
	)
	devices = _resolve_devices(args, config, default_all=default_all)

	if args.command == "status":
		records = _parallel(devices, lambda device: _state_record(device, settings))
		if settings["json"]:
			print(json.dumps(records, indent=2))
		else:
			_print_status(records)
		return _exit_code(records)

	if args.command in {"on", "off", "toggle"}:
		value = {"on": True, "off": False, "toggle": None}[args.command]
		if value is None:
			operation = lambda client: client.toggle()
		else:
			operation = lambda client: client.set_on(bool(value))
		records = _parallel(
			devices,
			lambda device: _apply_record(device, settings, operation, args.wait),
		)
		_emit_states(records, settings)
		return _exit_code(records)

	if args.command == "brightness":
		brightness = validate_brightness(args.value)
		records = _parallel(
			devices,
			lambda device: _apply_record(device, settings, lambda client: client.update(brightness=brightness), args.wait),
		)
		_emit_states(records, settings)
		return _exit_code(records)

	if args.command == "warmth":
		temperature = validate_warmth(args.value)
		records = _parallel(
			devices,
			lambda device: _apply_record(device, settings, lambda client: client.update(temperature=temperature), args.wait),
		)
		_emit_states(records, settings)
		return _exit_code(records)

	if args.command == "kelvin":
		temperature = kelvin_to_mired(args.value)
		records = _parallel(
			devices,
			lambda device: _apply_record(device, settings, lambda client: client.update(temperature=temperature), args.wait),
		)
		_emit_states(records, settings)
		return _exit_code(records)

	if args.command == "temperature":
		temperature = validate_temperature(args.value)
		records = _parallel(
			devices,
			lambda device: _apply_record(device, settings, lambda client: client.update(temperature=temperature), args.wait),
		)
		_emit_states(records, settings)
		return _exit_code(records)

	if args.command == "set":
		update: dict[str, Any] = {}
		if args.power is not None:
			update["on"] = args.power
		if args.brightness is not None:
			update["brightness"] = validate_brightness(args.brightness)
		if args.warmth is not None:
			update["temperature"] = validate_warmth(args.warmth)
		if args.kelvin is not None:
			update["temperature"] = kelvin_to_mired(args.kelvin)
		if args.temperature is not None:
			update["temperature"] = validate_temperature(args.temperature)
		if args.duration_ms is not None:
			update["duration_ms"] = args.duration_ms
		if not update:
			raise CommandError("set needs at least one property")
		records = _parallel(
			devices,
			lambda device: _apply_record(device, settings, lambda client: client.update(**update), args.wait),
		)
		_emit_states(records, settings)
		return _exit_code(records)

	if args.command == "power-on":
		if args.power_on_mode != POWER_ON_RESTORE_DEFAULTS and any(
			value is not None for value in (args.brightness, args.warmth, args.kelvin, args.temperature)
		):
			raise CommandError("--brightness, --warmth, --kelvin, and --temperature need --defaults")
		update: dict[str, Any] = {}
		if args.power_on_mode is not None:
			update["powerOnBehavior"] = args.power_on_mode
		if args.brightness is not None:
			update["powerOnBrightness"] = validate_brightness(args.brightness)
		if args.warmth is not None:
			update["powerOnTemperature"] = validate_warmth(args.warmth)
		if args.kelvin is not None:
			update["powerOnTemperature"] = kelvin_to_mired(args.kelvin)
		if args.temperature is not None:
			update["powerOnTemperature"] = validate_temperature(args.temperature)
		records = _parallel(
			devices,
			lambda device: _power_on_record(device, settings, update, args.wait),
		)
		if settings["json"]:
			print(json.dumps(records, indent=2))
		else:
			_print_power_on(records)
		return _exit_code(records)

	if args.command == "identify":
		records = _parallel(devices, lambda device: _identify_record(device, settings))
		if settings["json"]:
			print(json.dumps(records, indent=2))
		else:
			for record in records:
				if record.get("reachable", True):
					print(f"{record['name']}: identified")
				else:
					print(f"{record['name']}: offline ({record.get('error', 'unreachable')})")
		return _exit_code(records)

	if args.command == "info":
		records = _parallel(devices, lambda device: _info_record(device, settings))
		if settings["json"]:
			print(json.dumps(records, indent=2))
		else:
			_print_info(records)
		return _exit_code(records)

	raise CommandError(f"unknown command: {args.command}")


def main(argv: list[str] | None = None) -> int:
	parser = build_parser()
	args = parser.parse_args(argv)
	try:
		return run(args)
	except (ConfigError, CommandError, ValueError) as error:
		print(f"keylight: {error}", file=sys.stderr)
		return 2
	except (KeyLightError, DiscoveryError, OSError) as error:
		print(f"keylight: {error}", file=sys.stderr)
		return 1
	except KeyboardInterrupt:
		return 130


if __name__ == "__main__":
	raise SystemExit(main())
