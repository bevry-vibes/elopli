"""Device discovery through mDNS."""

from __future__ import annotations

import re
import shutil
import socket
import subprocess
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from typing import Any

SERVICE = "_elg._tcp"
BOARD_TYPES = {53: "Elgato Key Light", 200: "Elgato Key Light Air"}


class DiscoveryError(Exception):
	"""No mDNS browser is available."""


@dataclass(frozen=True)
class Lookup:
	host: str
	port: int
	txt: dict[str, str]


@dataclass(frozen=True)
class DiscoveredDevice:
	instance: str
	host: str
	address: str | None
	port: int
	product: str
	board_type: int | None
	model: str | None

	def to_dict(self) -> dict[str, Any]:
		return {
			"name": self.instance,
			"host": self.host,
			"address": self.address,
			"port": self.port,
			"product": self.product,
			"board_type": self.board_type,
			"model": self.model,
		}


def unescape_dns(value: str) -> str:
	def replace(match: re.Match[str]) -> str:
		token = match.group(1)
		if token.isdigit():
			return chr(int(token, 10))
		return token

	return re.sub(r"\\([0-9]{3}|.)", replace, value)


def parse_browse(output: str) -> list[str]:
	instances: list[str] = []
	seen: set[str] = set()
	for line in output.splitlines():
		if SERVICE + "." not in line:
			continue
		if not re.search(r"\s(Add|Rmv|Remove)\s", line):
			continue
		instance = unescape_dns(line.split(SERVICE + ".", 1)[1].strip())
		if instance and instance not in seen:
			seen.add(instance)
			instances.append(instance)
	return instances


def parse_lookup(output: str) -> Lookup | None:
	match = re.search(r"can be reached at (\S+?):(\d+)", output)
	if match is None:
		return None
	txt: dict[str, str] = {}
	for line in output.splitlines():
		stripped = line.strip()
		if not stripped or "=" not in stripped:
			continue
		for token in re.finditer(r"([A-Za-z][A-Za-z0-9_-]*)=((?:\\.|[^\s])*)", stripped):
			txt[token.group(1)] = unescape_dns(token.group(2))
	return Lookup(host=match.group(1).rstrip("."), port=int(match.group(2)), txt=txt)


def product_name(board_type: int | None, model: str) -> str:
	if board_type in BOARD_TYPES:
		return BOARD_TYPES[board_type]
	parts = model.split()
	if parts and re.fullmatch(r"[0-9A-Z]{6,}", parts[-1]):
		parts = parts[:-1]
	return " ".join(parts)


def _run(command: list[str], seconds: float) -> str:
	process = subprocess.Popen(
		command,
		stdout=subprocess.PIPE,
		stderr=subprocess.STDOUT,
		text=True,
	)
	try:
		output, _ = process.communicate(timeout=seconds)
	except subprocess.TimeoutExpired:
		process.terminate()
		try:
			output, _ = process.communicate(timeout=1)
		except subprocess.TimeoutExpired:
			process.kill()
			output, _ = process.communicate()
	return output


def _resolve_dns_sd(dns_sd: str, instance: str, lookup_timeout: float) -> DiscoveredDevice | None:
	lookup = parse_lookup(_run([dns_sd, "-L", instance, SERVICE + ".", "local."], lookup_timeout))
	if lookup is None:
		return None
	try:
		address = socket.gethostbyname(lookup.host)
	except OSError:
		address = None
	board_type = int(lookup.txt["dt"]) if lookup.txt.get("dt", "").isdigit() else None
	model = lookup.txt.get("md", "")
	return DiscoveredDevice(
		instance=instance,
		host=lookup.host,
		address=address,
		port=lookup.port,
		product=product_name(board_type, model),
		board_type=board_type,
		model=model.split()[-1] if model else None,
	)


def _discover_dns_sd(dns_sd: str, timeout: float, lookup_timeout: float) -> list[DiscoveredDevice]:
	instances = parse_browse(_run([dns_sd, "-B", SERVICE + ".", "local."], timeout))
	if not instances:
		return []
	with ThreadPoolExecutor(max_workers=min(8, len(instances))) as pool:
		results = list(pool.map(lambda instance: _resolve_dns_sd(dns_sd, instance, lookup_timeout), instances))
	return [result for result in results if result is not None]


def parse_avahi(output: str) -> list[DiscoveredDevice]:
	devices: list[DiscoveredDevice] = []
	for line in output.splitlines():
		if not line.startswith("="):
			continue
		fields = line.split(";")
		if len(fields) < 10:
			continue
		host = fields[6].rstrip(".")
		address = fields[7] or None
		try:
			port = int(fields[8])
		except ValueError:
			port = 9123
		txt: dict[str, str] = {}
		for token in re.finditer(r'"([A-Za-z][A-Za-z0-9_-]*)=([^"]*)"', fields[9]):
			txt[token.group(1)] = unescape_dns(token.group(2))
		board_type = int(txt["dt"]) if txt.get("dt", "").isdigit() else None
		model = txt.get("md", "")
		devices.append(
			DiscoveredDevice(
				instance=unescape_dns(fields[3]),
				host=host,
				address=address,
				port=port,
				product=product_name(board_type, model),
				board_type=board_type,
				model=model.split()[-1] if model else None,
			)
		)
	return devices


def discover(timeout: float = 3.0, lookup_timeout: float = 2.0) -> list[DiscoveredDevice]:
	dns_sd = shutil.which("dns-sd")
	if dns_sd:
		return _discover_dns_sd(dns_sd, timeout, lookup_timeout)
	avahi = shutil.which("avahi-browse")
	if avahi:
		return parse_avahi(_run([avahi, "-rpt", SERVICE], timeout))
	raise DiscoveryError("install dns-sd or avahi-browse to use discovery")
