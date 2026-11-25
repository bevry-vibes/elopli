"""Tests for the command-line interface."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from fake_device import MAC, SERIAL
from keylight.cli import main


def test_status_json_has_no_identity(config_path: Path, capsys: Any) -> None:
	assert main(["status", "--json", "--config", str(config_path)]) == 0
	output = capsys.readouterr().out
	payload = json.loads(output)
	assert payload[0]["name"] == "Test"
	assert payload[0]["state"] == "on"
	assert SERIAL not in output
	assert MAC not in output
	assert "serial" not in output.casefold()
	assert "mac_address" not in output


def test_status_with_a_name_selects_one_device(config_path: Path, capsys: Any) -> None:
	path = config_path.with_name("two.json")
	payload = json.loads(config_path.read_text(encoding="utf-8"))
	second = dict(payload["devices"][0], name="Other")
	payload["devices"].append(second)
	path.write_text(json.dumps(payload), encoding="utf-8")
	assert main(["status", "Test", "--json", "--config", str(path)]) == 0
	output = json.loads(capsys.readouterr().out)
	assert [record["name"] for record in output] == ["Test"]


def test_off_and_on(config_path: Path, server: Any, capsys: Any) -> None:
	assert main(["off", "--config", str(config_path)]) == 0
	assert server.state["lights"][0]["on"] == 0
	assert main(["on", "--config", str(config_path)]) == 0
	assert server.state["lights"][0]["on"] == 1


def test_brightness(config_path: Path, server: Any, capsys: Any) -> None:
	assert main(["brightness", "75", "--config", str(config_path)]) == 0
	assert server.state["lights"][0]["brightness"] == 75


def test_warmth_and_kelvin(config_path: Path, server: Any, capsys: Any) -> None:
	assert main(["warmth", "100", "--config", str(config_path)]) == 0
	assert server.state["lights"][0]["temperature"] == 344
	assert main(["kelvin", "5000", "--config", str(config_path)]) == 0
	assert server.state["lights"][0]["temperature"] == 200


def test_set(config_path: Path, server: Any, capsys: Any) -> None:
	assert main(["set", "Test", "--off", "--brightness", "40", "--warmth", "50", "--config", str(config_path)]) == 0
	assert server.state["lights"] == [{"on": 0, "brightness": 40, "temperature": 244}]


def test_rejects_out_of_range_brightness(config_path: Path, capsys: Any) -> None:
	assert main(["brightness", "101", "--config", str(config_path)]) == 2
	assert "brightness must be between 0 and 100" in capsys.readouterr().err


def test_rejects_out_of_range_kelvin(config_path: Path, capsys: Any) -> None:
	assert main(["kelvin", "100", "--config", str(config_path)]) == 2
	assert "kelvin must be between" in capsys.readouterr().err


def test_set_needs_a_property(config_path: Path, capsys: Any) -> None:
	assert main(["set", "Test", "--config", str(config_path)]) == 2
	assert "set needs at least one property" in capsys.readouterr().err


def test_unknown_device(config_path: Path, capsys: Any) -> None:
	assert main(["on", "Nope", "--config", str(config_path)]) == 2
	assert "unknown device: Nope" in capsys.readouterr().err


def test_rename(config_path: Path, server: Any, capsys: Any) -> None:
	assert main(["rename", "Test", "Desk Lamp", "--config", str(config_path)]) == 0
	assert server.info["displayName"] == "Desk Lamp"
	assert "renamed to Desk Lamp" in capsys.readouterr().out


def test_rename_rejects_empty(config_path: Path, capsys: Any) -> None:
	assert main(["rename", "Test", "", "--config", str(config_path)]) == 2
	assert "must not be empty" in capsys.readouterr().err


def test_rename_rejects_long_name(config_path: Path, capsys: Any) -> None:
	assert main(["rename", "Test", "x" * 64, "--config", str(config_path)]) == 2
	assert "63 bytes or fewer" in capsys.readouterr().err


def test_rename_rejects_multibyte_over_limit(config_path: Path, capsys: Any) -> None:
	assert main(["rename", "Test", "é" * 32, "--config", str(config_path)]) == 2
	assert "63 bytes or fewer" in capsys.readouterr().err


def test_power_on_show(config_path: Path, capsys: Any) -> None:
	assert main(["power-on", "--json", "--config", str(config_path)]) == 0
	output = json.loads(capsys.readouterr().out)
	assert output[0]["power_on_behavior"] == 1
	assert output[0]["power_on_behavior_label"] == "restore last used"


def test_power_on_defaults(config_path: Path, server: Any, capsys: Any) -> None:
	assert main(
		[
			"power-on",
			"--defaults",
			"--brightness",
			"25",
			"--warmth",
			"50",
			"--config",
			str(config_path),
		]
	) == 0
	assert server.settings["powerOnBehavior"] == 2
	assert server.settings["powerOnBrightness"] == 25
	assert server.settings["powerOnTemperature"] == 244
	assert "restore defaults" in capsys.readouterr().out


def test_power_on_last(config_path: Path, server: Any, capsys: Any) -> None:
	server.settings["powerOnBehavior"] = 2
	assert main(["power-on", "--last", "--config", str(config_path)]) == 0
	assert server.settings["powerOnBehavior"] == 1


def test_power_on_options_need_defaults(config_path: Path, capsys: Any) -> None:
	assert main(["power-on", "--brightness", "25", "--config", str(config_path)]) == 2
	assert "need --defaults" in capsys.readouterr().err


def test_info_hides_identity(config_path: Path, capsys: Any) -> None:
	assert main(["info", "--config", str(config_path)]) == 0
	output = capsys.readouterr().out
	assert "Elgato Key Light" in output
	assert "1.0.3 (222)" in output
	assert SERIAL not in output
	assert MAC not in output


def test_list_and_config_path(config_path: Path, capsys: Any) -> None:
	assert main(["list", "--config", str(config_path)]) == 0
	assert "Test" in capsys.readouterr().out
	assert main(["config", "path", "--config", str(config_path)]) == 0
	assert str(config_path) in capsys.readouterr().out


def test_unreachable_device_returns_one(config_path: Path, capsys: Any) -> None:
	path = config_path.with_name("offline.json")
	path.write_text(json.dumps({"devices": [{"name": "Offline", "host": "127.0.0.1", "port": 1}]}), encoding="utf-8")
	assert main(["status", "--config", str(path), "--retries", "0", "--timeout", "1"]) == 1
	output = capsys.readouterr().out
	assert "offline" in output
