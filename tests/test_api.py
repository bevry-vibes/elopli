"""Tests for the HTTP client."""

from __future__ import annotations

import json
from typing import Any

import pytest

from fake_device import MAC, SERIAL
from keylight.api import (
	KeyLightApiError,
	KeyLightClient,
	KeyLightConnectionError,
	kelvin_to_mired,
	mired_to_kelvin,
	mired_to_warmth,
	validate_brightness,
	validate_temperature,
	validate_warmth,
)


def test_reads_state(client: KeyLightClient) -> None:
	state = client.state()
	assert state.on is True
	assert state.brightness == 50
	assert state.temperature == 267
	assert state.kelvin == 3745
	assert state.warmth == 62


def test_writes_only_named_fields(client: KeyLightClient, server: Any) -> None:
	state = client.update(brightness=80)
	assert state.brightness == 80
	assert state.temperature == 267
	method, path, body = server.requests[-1]
	assert method == "PUT"
	assert path == "/elgato/lights"
	assert json.loads(body) == {"lights": [{"brightness": 80}]}


def test_power_and_toggle(client: KeyLightClient, server: Any) -> None:
	assert client.set_on(False).on is False
	assert server.state["lights"][0]["on"] == 0
	assert client.toggle().on is True
	assert server.state["lights"][0]["on"] == 1


def test_rejects_empty_update(client: KeyLightClient) -> None:
	with pytest.raises(ValueError):
		client.update()


def test_reports_api_errors(client: KeyLightClient) -> None:
	with pytest.raises(KeyLightApiError) as error:
		client.update_settings({"powerOnBehavior": 1})
	assert "Fail to parse JSON data" in str(error.value)


def test_rename(client: KeyLightClient, server: Any) -> None:
	assert client.set_display_name("  Desk  ") == "Desk"
	assert server.info["displayName"] == "Desk"
	with pytest.raises(ValueError):
		client.set_display_name("")
	with pytest.raises(ValueError):
		client.set_display_name("x" * 64)


def test_update_settings(client: KeyLightClient, server: Any) -> None:
	settings = client.settings()
	settings.update({"powerOnBehavior": 2, "powerOnBrightness": 25, "powerOnTemperature": 244})
	client.update_settings(settings)
	assert server.settings["powerOnBehavior"] == 2
	assert server.settings["powerOnBrightness"] == 25
	assert server.settings["powerOnTemperature"] == 244


def test_reports_connection_errors() -> None:
	client = KeyLightClient("127.0.0.1", port=1, retries=0, transport="urllib")
	with pytest.raises(KeyLightConnectionError):
		client.state()


def test_identify(client: KeyLightClient, server: Any) -> None:
	client.identify()
	assert server.requests[-1] == ("POST", "/elgato/identify", None)


def test_accessory_info_discards_identity(client: KeyLightClient) -> None:
	info = client.accessory_info()
	assert info.product_name == "Elgato Key Light"
	assert info.firmware == "1.0.3 (222)"
	payload = json.dumps(info.to_dict())
	assert SERIAL not in payload
	assert MAC not in payload
	assert not hasattr(info, "serial_number")


def test_validation() -> None:
	assert validate_brightness(50.4) == 50
	with pytest.raises(ValueError):
		validate_brightness(101)
	with pytest.raises(ValueError):
		validate_brightness(-1)
	with pytest.raises(ValueError):
		validate_brightness("50")
	with pytest.raises(ValueError):
		validate_temperature(100)
	assert validate_warmth(0) == 143
	assert validate_warmth(50) == 244
	assert validate_warmth(100) == 344
	assert kelvin_to_mired(5000) == 200
	assert kelvin_to_mired(7000) == 143
	assert kelvin_to_mired(2900) == 344
	assert mired_to_kelvin(143) == 6993
	assert mired_to_warmth(143) == 0
	assert mired_to_warmth(344) == 100
