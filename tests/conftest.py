"""Shared fixtures for the test suite."""

from __future__ import annotations

import json
import threading
from pathlib import Path
from typing import Any, Iterator

import pytest

from fake_device import make_server
from keylight.api import KeyLightClient


@pytest.fixture
def server() -> Iterator[Any]:
	httpd = make_server()
	thread = threading.Thread(target=httpd.serve_forever, daemon=True)
	thread.start()
	yield httpd
	httpd.shutdown()
	httpd.server_close()


@pytest.fixture
def client(server: Any) -> KeyLightClient:
	return KeyLightClient("127.0.0.1", port=server.server_port, transport="urllib")


@pytest.fixture
def config_path(tmp_path: Path, server: Any) -> Path:
	path = tmp_path / "devices.json"
	path.write_text(
		json.dumps({"devices": [{"name": "Test", "host": "127.0.0.1", "port": server.server_port}]}),
		encoding="utf-8",
	)
	return path
