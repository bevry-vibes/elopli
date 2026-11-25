"""Tests for mDNS parsing."""

from __future__ import annotations

from keylight.discovery import parse_avahi, parse_browse, parse_lookup, product_name, unescape_dns

BROWSE = """
Browsing for _elg._tcp.local.
DATE: ---Wed 25 Nov 2026---
17:16:10.838  ...STARTING...
Timestamp     A/R    Flags  if Domain               Service Type         Instance Name
17:16:10.838  Add        3  10 local.               _elg._tcp.           Elgato Key Light EF56
17:16:10.838  Add        3  12 local.               _elg._tcp.           Elgato Key Light EF56
17:16:10.838  Add        3  10 local.               _elg._tcp.           Elgato Key Light Air AB12
"""

LOOKUP = """
Lookup Elgato Key Light CD34._elg._tcp.local.
DATE: ---Wed 25 Nov 2026---
17:16:33.793  ...STARTING...
17:16:33.793  Elgato\\032Key\\032Light\\032CD34._elg._tcp.local. can be reached at elgato-key-light-cd34.local.:9123 (interface 12) Flags: 1
 mf=Elgato dt=53 id=AA:BB:CC:DD:EE:FF md=Elgato\\ Key\\ Light\\ 20GAK9901 pv=1.0
"""


def test_unescape() -> None:
	assert unescape_dns("Elgato\\032Key\\ Light") == "Elgato Key Light"


def test_parse_browse() -> None:
	assert parse_browse(BROWSE) == ["Elgato Key Light EF56", "Elgato Key Light Air AB12"]


def test_parse_lookup() -> None:
	lookup = parse_lookup(LOOKUP)
	assert lookup is not None
	assert lookup.host == "elgato-key-light-cd34.local"
	assert lookup.port == 9123
	assert lookup.txt["dt"] == "53"
	assert lookup.txt["md"] == "Elgato Key Light 20GAK9901"


def test_product_name() -> None:
	assert product_name(53, "Elgato Key Light 20GAK9901") == "Elgato Key Light"
	assert product_name(200, "Elgato Key Light Air 20LAB9901") == "Elgato Key Light Air"
	assert product_name(None, "Elgato Key Light Mini 20GAM9901") == "Elgato Key Light Mini"


def test_parse_avahi() -> None:
	line = '=;en0;IPv4;Elgato Key Light CD34;_elg._tcp;local;elgato-key-light-cd34.local;192.0.2.11;9123;"dt=53" "md=Elgato Key Light 20GAK9901"'
	devices = parse_avahi(line)
	assert len(devices) == 1
	assert devices[0].address == "192.0.2.11"
	assert devices[0].product == "Elgato Key Light"
