"""Device registry for the command-line interface."""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .api import DEFAULT_PORT


class ConfigError(Exception):
	"""The device registry is missing or invalid."""


def config_path(explicit: str | None = None) -> Path:
	if explicit:
		return Path(explicit).expanduser()
	from_env = os.environ.get("KEYLIGHT_CONFIG")
	if from_env:
		return Path(from_env).expanduser()
	local = Path("devices.json")
	if local.exists():
		return local
	return Path("~/.config/keylight/devices.json").expanduser()


@dataclass(frozen=True)
class Device:
	name: str
	host: str
	port: int = DEFAULT_PORT

	def to_dict(self) -> dict[str, Any]:
		return {"name": self.name, "host": self.host, "port": self.port}


@dataclass
class Config:
	path: Path
	devices: list[Device] = field(default_factory=list)

	@classmethod
	def load(cls, path: Path) -> Config:
		if not path.exists():
			return cls(path=path)
		try:
			payload = json.loads(path.read_text(encoding="utf-8"))
		except (OSError, json.JSONDecodeError) as error:
			raise ConfigError(f"cannot read {path}: {error}") from error
		entries = payload.get("devices") if isinstance(payload, dict) else None
		if not isinstance(entries, list):
			raise ConfigError(f"{path} must contain a devices list")
		devices: list[Device] = []
		seen: set[str] = set()
		for entry in entries:
			if not isinstance(entry, dict):
				raise ConfigError(f"{path} contains an invalid device entry")
			name = entry.get("name")
			host = entry.get("host")
			port = entry.get("port", DEFAULT_PORT)
			if not isinstance(name, str) or not name.strip():
				raise ConfigError(f"{path} contains a device without a name")
			if not isinstance(host, str) or not host.strip():
				raise ConfigError(f"{path} contains a device without a host")
			if not isinstance(port, int) or not 1 <= port <= 65535:
				raise ConfigError(f"{path} contains an invalid port for {name}")
			key = name.casefold()
			if key in seen:
				raise ConfigError(f"{path} contains the device name {name} more than once")
			seen.add(key)
			devices.append(Device(name=name, host=host, port=port))
		return cls(path=path, devices=devices)

	def save(self) -> None:
		self.path.parent.mkdir(parents=True, exist_ok=True)
		payload = {"devices": [device.to_dict() for device in self.devices]}
		self.path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

	def find(self, name: str) -> Device:
		needle = name.casefold()
		for device in self.devices:
			if device.name.casefold() == needle or device.host.casefold() == needle:
				return device
		raise ConfigError(f"unknown device: {name}")

	def add(self, device: Device) -> bool:
		if any(existing.host == device.host or existing.name == device.name for existing in self.devices):
			return False
		self.devices.append(device)
		return True

	def to_dict(self) -> dict[str, Any]:
		return {"path": str(self.path), "devices": [device.to_dict() for device in self.devices]}
