# Elgato Key Light CLI

Control Elgato Key Light and Key Light Air devices over the local network.

This project exists because Elgato Control Center removed its Apple Shortcuts actions.
The lights still expose a small HTTP API on port 9123.
This CLI uses that API directly.

## Status

The local API is reverse engineered and verified on three devices:

- Key Light Air, firmware 1.0.3 (222)
- Key Light, firmware 1.0.3 (222)

The client controls power, brightness, and colour temperature.
It also reads and writes the device display name and the power-on behaviour.
It does not store serial numbers or MAC addresses.

## Requirements

- Python 3.11 or later
- `curl`, used as a fallback when the host blocks Python sockets

## Install

```sh
uv tool install .
```

Or run it from the repository:

```sh
uv run keylight --help
```

## Configure

Create `devices.json` in the current directory, or at `~/.config/keylight/devices.json`:

```json
{
  "devices": [
    { "name": "Back", "host": "192.0.2.10" },
    { "name": "Left", "host": "192.0.2.11" },
    { "name": "Right", "host": "192.0.2.12" }
  ]
}
```

The host can be an IP address or an mDNS name.
Run `keylight discover` to find devices.
Run `keylight discover --write` to add them to the registry.

Set `KEYLIGHT_CONFIG` or pass `--config` to use another file.

## Use

```sh
keylight status
keylight on
keylight off Right
keylight toggle --all
keylight brightness 50
keylight warmth 70
keylight kelvin 4000
keylight set --on --brightness 60 --warmth 40 --all
keylight identify Back
keylight rename Right "Desk Right"
keylight power-on
keylight power-on --defaults --brightness 20 --warmth 35 --all
keylight power-on --last --all
keylight info
```

Add `--json` for machine-readable output:

```sh
keylight status --json
```

### Wake a sleeping light

A light that stays off can drop off the network.
Use `--wait` to retry until the light answers:

```sh
keylight on --wait 180
```

## How it works

The light serves plain HTTP on port 9123.
It announces the `_elg._tcp` service through mDNS.
A write sends a partial JSON object and returns the new state.

Read [docs/api.md](docs/api.md) for the full protocol notes.
Read [docs/shortcuts.md](docs/shortcuts.md) for the Apple Shortcuts bridge.

## Security

The API has no authentication and no transport encryption.
Use it only on a trusted local network.
A request that carries an `Origin` header gets HTTP 403.

## Prior art

- [schlarpc/elgato-key-light-mini-firmware-re](https://github.com/schlarpc/elgato-key-light-mini-firmware-re)
- [frenck/python-elgato](https://github.com/frenck/python-elgato)
- [adamesch/elgato-key-light-api](https://github.com/adamesch/elgato-key-light-api)
- [p-conrad/klc](https://github.com/p-conrad/klc)

## Development

```sh
uv sync
uv run pytest
```

<!-- LICENSE/ -->

## License

Unless stated otherwise all works are:

- Copyright &copy; [Benjamin Lupton](https://balupton.com)

and licensed under:

- [Reciprocal Public License 1.5](http://spdx.org/licenses/RPL-1.5.html)

<!-- /LICENSE -->
