"""A fake Elgato Key Light for the test suite."""

from __future__ import annotations

import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

SERIAL = "TESTSERIAL0001"
MAC = "AA:BB:CC:DD:EE:FF"

INFO = {
	"productName": "Elgato Key Light",
	"hardwareBoardType": 53,
	"hardwareRevision": "1",
	"macAddress": MAC,
	"firmwareBuildNumber": 222,
	"firmwareVersion": "1.0.3",
	"serialNumber": SERIAL,
	"displayName": "Test",
	"features": ["lights"],
	"wifi-info": {"ssid": "test", "frequencyMHz": 2400, "rssi": -40},
}


class Handler(BaseHTTPRequestHandler):
	def log_message(self, *args: Any) -> None:
		return

	def send_json(self, status: int, payload: Any) -> None:
		body = json.dumps(payload).encode("utf-8")
		self.send_response(status)
		self.send_header("Content-Type", "application/json")
		self.send_header("Content-Length", str(len(body)))
		self.end_headers()
		self.wfile.write(body)

	def do_GET(self) -> None:
		self.server.requests.append(("GET", self.path, None))  # type: ignore[attr-defined]
		if self.path == "/elgato/lights":
			self.send_json(200, self.server.state)  # type: ignore[attr-defined]
		elif self.path == "/elgato/accessory-info":
			self.send_json(200, INFO)
		elif self.path == "/elgato/lights/settings":
			self.send_json(200, {"powerOnBehavior": 1})
		else:
			self.send_json(404, {"errors": [{"message": "not found"}]})

	def do_PUT(self) -> None:
		length = int(self.headers.get("Content-Length", 0))
		raw = self.rfile.read(length)
		self.server.requests.append(("PUT", self.path, raw))  # type: ignore[attr-defined]
		try:
			payload = json.loads(raw or b"{}")
		except json.JSONDecodeError:
			self.send_json(400, {"errors": [{"message": "Fail to parse JSON data"}]})
			return
		if self.path == "/elgato/lights":
			self.server.state["lights"][0].update(payload["lights"][0])  # type: ignore[attr-defined]
			self.send_json(200, self.server.state)  # type: ignore[attr-defined]
		elif self.path == "/elgato/accessory-info":
			if not payload.get("displayName"):
				self.send_json(400, {"errors": [{"message": "Fail to parse JSON data"}]})
				return
			self.send_json(200, {})
		else:
			self.send_json(404, {"errors": [{"message": "not found"}]})

	def do_POST(self) -> None:
		self.server.requests.append(("POST", self.path, None))  # type: ignore[attr-defined]
		if self.path == "/elgato/identify":
			self.send_json(200, {})
		else:
			self.send_json(404, {"errors": [{"message": "not found"}]})


def make_server() -> ThreadingHTTPServer:
	httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
	httpd.state = {"numberOfLights": 1, "lights": [{"on": 1, "brightness": 50, "temperature": 267}]}  # type: ignore[attr-defined]
	httpd.requests = []  # type: ignore[attr-defined]
	return httpd
