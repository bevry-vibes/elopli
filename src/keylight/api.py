"""HTTP client for the Elgato Key Light local interface."""

from __future__ import annotations

import json
import math
import shutil
import subprocess
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Any

DEFAULT_PORT = 9123
BRIGHTNESS_MIN = 0
BRIGHTNESS_MAX = 100
MIRED_MIN = 143
MIRED_MAX = 344
KELVIN_MIN = 2900
KELVIN_MAX = 7000
DISPLAY_NAME_MAX_BYTES = 63
POWER_ON_RESTORE_LAST = 1
POWER_ON_RESTORE_DEFAULTS = 2


class KeyLightError(Exception):
	"""Base error for the client."""


class KeyLightConnectionError(KeyLightError):
	"""The device did not answer."""


class KeyLightApiError(KeyLightError):
	"""The device answered with an error."""

	def __init__(self, status: int, messages: list[str], body: str = "") -> None:
		self.status = status
		self.messages = messages
		self.body = body
		detail = "; ".join(message for message in messages if message)
		if not detail:
			detail = body.strip() or "unknown error"
		super().__init__(f"HTTP {status}: {detail}")


def _coerce_number(name: str, value: Any) -> float:
	if isinstance(value, bool) or not isinstance(value, (int, float)):
		raise ValueError(f"{name} must be a number")
	number = float(value)
	if not math.isfinite(number):
		raise ValueError(f"{name} must be a finite number")
	return number


def validate_brightness(value: Any) -> int:
	number = round(_coerce_number("brightness", value))
	if not BRIGHTNESS_MIN <= number <= BRIGHTNESS_MAX:
		raise ValueError(f"brightness must be between {BRIGHTNESS_MIN} and {BRIGHTNESS_MAX}")
	return number


def validate_temperature(value: Any) -> int:
	number = round(_coerce_number("temperature", value))
	if not MIRED_MIN <= number <= MIRED_MAX:
		raise ValueError(f"temperature must be between {MIRED_MIN} and {MIRED_MAX} mired")
	return number


def validate_warmth(value: Any) -> int:
	number = _coerce_number("warmth", value)
	if not 0 <= number <= 100:
		raise ValueError("warmth must be between 0 and 100")
	return round(MIRED_MIN + (number / 100) * (MIRED_MAX - MIRED_MIN))


def kelvin_to_mired(value: Any) -> int:
	number = _coerce_number("kelvin", value)
	if not KELVIN_MIN <= number <= KELVIN_MAX:
		raise ValueError(f"kelvin must be between {KELVIN_MIN} and {KELVIN_MAX}")
	# Clamp because the exact conversion of 2900 K is 345 mired, above the device limit.
	return max(MIRED_MIN, min(MIRED_MAX, round(1_000_000 / number)))


def mired_to_kelvin(value: int) -> int:
	return round(1_000_000 / value)


def mired_to_warmth(value: int) -> int:
	return round(((value - MIRED_MIN) / (MIRED_MAX - MIRED_MIN)) * 100)


def validate_display_name(value: str) -> str:
	if not isinstance(value, str):
		raise ValueError("display name must be text")
	name = value.strip()
	if not name:
		raise ValueError("display name must not be empty")
	if len(name.encode("utf-8")) > DISPLAY_NAME_MAX_BYTES:
		raise ValueError(f"display name must be {DISPLAY_NAME_MAX_BYTES} bytes or fewer")
	return name


class Transport:
	"""A minimal HTTP transport."""

	def request(self, method: str, url: str, body: bytes | None, timeout: float) -> tuple[int, bytes]:
		raise NotImplementedError


class UrllibTransport(Transport):
	"""HTTP transport based on the Python standard library."""

	def request(self, method: str, url: str, body: bytes | None, timeout: float) -> tuple[int, bytes]:
		request = urllib.request.Request(url, data=body, method=method)
		if body is not None:
			request.add_header("Content-Type", "application/json")
		try:
			with urllib.request.urlopen(request, timeout=timeout) as response:
				return response.status, response.read()
		except urllib.error.HTTPError as error:
			return error.code, error.read()
		except (urllib.error.URLError, OSError) as error:
			reason = getattr(error, "reason", error)
			raise KeyLightConnectionError(str(reason)) from error


class CurlTransport(Transport):
	"""HTTP transport for hosts where the local network blocks Python sockets."""

	@staticmethod
	def available() -> bool:
		return shutil.which("curl") is not None

	def request(self, method: str, url: str, body: bytes | None, timeout: float) -> tuple[int, bytes]:
		curl = shutil.which("curl")
		if curl is None:
			raise KeyLightConnectionError("curl is not installed")
		command = [
			curl,
			"--silent",
			"--show-error",
			"--max-time",
			f"{timeout:g}",
			"--request",
			method,
			"--header",
			"Content-Type: application/json",
		]
		if body is not None:
			command.extend(["--data-binary", "@-"])
		command.extend(["--write-out", "\n%{http_code}", url])
		try:
			process = subprocess.run(
				command,
				input=body if body is not None else b"",
				capture_output=True,
				timeout=timeout + 2,
			)
		except subprocess.TimeoutExpired as error:
			raise KeyLightConnectionError("request timed out") from error
		if process.returncode != 0:
			message = process.stderr.decode("utf-8", "replace").strip()
			raise KeyLightConnectionError(message or f"curl exited with status {process.returncode}")
		head, separator, tail = process.stdout.rpartition(b"\n")
		if not separator:
			raise KeyLightConnectionError("curl returned an invalid response")
		try:
			status = int(tail)
		except ValueError as error:
			raise KeyLightConnectionError("curl returned an invalid status") from error
		return status, head


class AutoTransport(Transport):
	"""Use Python sockets, then fall back to curl when the host blocks them."""

	def __init__(self, preference: str = "auto") -> None:
		if preference not in {"auto", "urllib", "curl"}:
			raise ValueError("transport must be one of auto, urllib, curl")
		if preference == "curl":
			self._transport: Transport = CurlTransport()
		else:
			self._transport = UrllibTransport()
		self._may_fallback = preference == "auto"

	def request(self, method: str, url: str, body: bytes | None, timeout: float) -> tuple[int, bytes]:
		try:
			return self._transport.request(method, url, body, timeout)
		except KeyLightConnectionError:
			if not self._may_fallback or not CurlTransport.available():
				raise
			self._transport = CurlTransport()
			self._may_fallback = False
			return self._transport.request(method, url, body, timeout)


@dataclass(frozen=True)
class LightState:
	on: bool
	brightness: int
	temperature: int

	@classmethod
	def from_payload(cls, payload: dict[str, Any]) -> LightState:
		lights = payload.get("lights")
		if not isinstance(lights, list) or not lights:
			raise KeyLightApiError(200, ["response contains no light state"])
		light = lights[0]
		if not isinstance(light, dict):
			raise KeyLightApiError(200, ["response contains an invalid light state"])
		return cls(
			on=bool(light.get("on", 0)),
			brightness=int(light.get("brightness", 0)),
			temperature=int(light.get("temperature", MIRED_MIN)),
		)

	@property
	def kelvin(self) -> int:
		return mired_to_kelvin(self.temperature)

	@property
	def warmth(self) -> int:
		return mired_to_warmth(self.temperature)

	def to_dict(self) -> dict[str, Any]:
		return {
			"on": self.on,
			"brightness": self.brightness,
			"warmth": self.warmth,
			"kelvin": self.kelvin,
			"temperature_mired": self.temperature,
		}


@dataclass(frozen=True)
class WifiInfo:
	ssid: str
	frequency_mhz: int
	rssi: int

	@classmethod
	def from_payload(cls, payload: Any) -> WifiInfo | None:
		if not isinstance(payload, dict):
			return None
		return cls(
			ssid=str(payload.get("ssid", "")),
			frequency_mhz=int(payload.get("frequencyMHz", 0)),
			rssi=int(payload.get("rssi", 0)),
		)

	def to_dict(self) -> dict[str, Any]:
		return {
			"ssid": self.ssid,
			"frequency_mhz": self.frequency_mhz,
			"rssi": self.rssi,
		}


@dataclass(frozen=True)
class AccessoryInfo:
	product_name: str
	display_name: str
	firmware_version: str
	firmware_build_number: int
	hardware_board_type: int
	wifi: WifiInfo | None

	@classmethod
	def from_payload(cls, payload: dict[str, Any]) -> AccessoryInfo:
		# The payload carries serialNumber and macAddress. Do not store them,
		# because callers must never print or log device identity data.
		return cls(
			product_name=str(payload.get("productName", "")),
			display_name=str(payload.get("displayName", "")),
			firmware_version=str(payload.get("firmwareVersion", "")),
			firmware_build_number=int(payload.get("firmwareBuildNumber", 0)),
			hardware_board_type=int(payload.get("hardwareBoardType", 0)),
			wifi=WifiInfo.from_payload(payload.get("wifi-info")),
		)

	@property
	def firmware(self) -> str:
		return f"{self.firmware_version} ({self.firmware_build_number})"

	def to_dict(self) -> dict[str, Any]:
		return {
			"product": self.product_name,
			"display_name": self.display_name,
			"firmware": self.firmware,
			"board_type": self.hardware_board_type,
			"wifi": None if self.wifi is None else self.wifi.to_dict(),
		}


def _error_messages(body: bytes) -> list[str]:
	try:
		payload = json.loads(body)
	except json.JSONDecodeError:
		return []
	errors = payload.get("errors") if isinstance(payload, dict) else None
	if not isinstance(errors, list):
		return []
	messages = []
	for error in errors:
		if isinstance(error, dict):
			message = error.get("message")
			if isinstance(message, str):
				messages.append(message)
	return messages


class KeyLightClient:
	"""A client for one Elgato Key Light."""

	def __init__(
		self,
		host: str,
		port: int = DEFAULT_PORT,
		timeout: float = 3.0,
		retries: int = 2,
		retry_delay: float = 0.5,
		transport: str = "auto",
	) -> None:
		self.host = host
		self.port = port
		self.timeout = timeout
		self.retries = max(0, retries)
		self.retry_delay = max(0.0, retry_delay)
		self._transport = AutoTransport(transport)

	@property
	def address(self) -> str:
		return f"{self.host}:{self.port}"

	def _request(self, method: str, path: str, payload: dict[str, Any] | None = None) -> Any:
		body = None if payload is None else json.dumps(payload).encode("utf-8")
		url = f"http://{self.host}:{self.port}{path}"
		last_error: KeyLightError | None = None
		for attempt in range(self.retries + 1):
			try:
				status, response = self._transport.request(method, url, body, self.timeout)
			except KeyLightConnectionError as error:
				last_error = error
			else:
				if status < 400:
					if not response:
						return {}
					try:
						return json.loads(response)
					except json.JSONDecodeError as error:
						raise KeyLightApiError(
							status,
							["response is not valid JSON"],
							response.decode("utf-8", "replace"),
						) from error
				error = KeyLightApiError(
					status,
					_error_messages(response),
					response.decode("utf-8", "replace"),
				)
				if status < 500:
					raise error
				last_error = error
			if attempt < self.retries:
				time.sleep(self.retry_delay * (attempt + 1))
		if last_error is not None:
			raise last_error
		raise KeyLightConnectionError("request failed")

	def state(self) -> LightState:
		return LightState.from_payload(self._request("GET", "/elgato/lights"))

	def update(
		self,
		*,
		on: bool | None = None,
		brightness: Any | None = None,
		temperature: Any | None = None,
		duration_ms: int | None = None,
	) -> LightState:
		light: dict[str, Any] = {}
		if on is not None:
			light["on"] = 1 if on else 0
		if brightness is not None:
			light["brightness"] = validate_brightness(brightness)
		if temperature is not None:
			light["temperature"] = validate_temperature(temperature)
		if duration_ms is not None:
			if duration_ms < 0:
				raise ValueError("duration_ms must be zero or more")
			light["durationMs"] = int(duration_ms)
		if not light:
			raise ValueError("no fields to update")
		payload = self._request("PUT", "/elgato/lights", {"lights": [light]})
		return LightState.from_payload(payload)

	def set_on(self, value: bool) -> LightState:
		return self.update(on=bool(value))

	def toggle(self) -> LightState:
		return self.update(on=not self.state().on)

	def accessory_info(self) -> AccessoryInfo:
		return AccessoryInfo.from_payload(self._request("GET", "/elgato/accessory-info"))

	def identify(self) -> None:
		self._request("POST", "/elgato/identify")

	def settings(self) -> dict[str, Any]:
		return self._request("GET", "/elgato/lights/settings")

	def update_settings(self, settings: dict[str, Any]) -> dict[str, Any]:
		return self._request("PUT", "/elgato/lights/settings", settings)

	def set_display_name(self, name: str) -> str:
		clean = validate_display_name(name)
		self._request("PUT", "/elgato/accessory-info", {"displayName": clean})
		return clean
