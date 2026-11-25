"""Tests for the device registry."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from keylight.config import Config, ConfigError, Device


def test_round_trip(tmp_path: Path) -> None:
	path = tmp_path / "devices.json"
	config = Config(path=path)
	config.add(Device(name="Back", host="192.0.2.10"))
	config.add(Device(name="Left", host="192.0.2.11", port=9123))
	config.save()
	loaded = Config.load(path)
	assert [device.name for device in loaded.devices] == ["Back", "Left"]
	assert loaded.find("back").host == "192.0.2.10"
	assert loaded.find("192.0.2.11").name == "Left"


def test_missing_file_is_empty(tmp_path: Path) -> None:
	config = Config.load(tmp_path / "devices.json")
	assert config.devices == []


def test_rejects_duplicate_names(tmp_path: Path) -> None:
	path = tmp_path / "devices.json"
	path.write_text(
		json.dumps({"devices": [{"name": "Back", "host": "192.0.2.10"}, {"name": "back", "host": "192.0.2.11"}]}),
		encoding="utf-8",
	)
	with pytest.raises(ConfigError):
		Config.load(path)


def test_rejects_bad_port(tmp_path: Path) -> None:
	path = tmp_path / "devices.json"
	path.write_text(json.dumps({"devices": [{"name": "Back", "host": "192.0.2.10", "port": 0}]}), encoding="utf-8")
	with pytest.raises(ConfigError):
		Config.load(path)


def test_unknown_device(tmp_path: Path) -> None:
	config = Config.load(tmp_path / "devices.json")
	with pytest.raises(ConfigError):
		config.find("Back")


def test_does_not_add_the_same_host_twice(tmp_path: Path) -> None:
	config = Config.load(tmp_path / "devices.json")
	assert config.add(Device(name="Back", host="192.0.2.10")) is True
	assert config.add(Device(name="Other", host="192.0.2.10")) is False
	assert len(config.devices) == 1
