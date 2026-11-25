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
