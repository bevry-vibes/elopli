# Elopli

Elopli controls Elgato Key Light and Key Light Air devices over the local network.
It has a native macOS menu-bar app, a Rust CLI, and a shared Rust core.

The macOS interface is a card for each light.
Each card has power, brightness, warmth, WiFi details, rename, and power-on behaviour.
The app has no Dock icon and no draggable window.
It appears in the menu bar.

Evergreen design decisions are in [DESIGN.md](DESIGN.md).

## Status

macOS is implemented.
Windows and Fedora are planned with native UIs on the same Rust core.

Verified on:

- Key Light Air, firmware 1.0.3 (222)
- Key Light, firmware 1.0.3 (222)

Serial numbers and MAC addresses are discarded by the client.

## Features

- Power on and off.
- Brightness from 2 to 100. The light switches off below 2.
- Warmth from 0 (cool) to 100 (warm), plus Kelvin and mired.
- Rename the light.
- Power-on behaviour: restore the last setting, or restore defaults.
- WiFi icon with SSID, frequency, RSSI, and quality on hover.
- mDNS discovery.
- Native Apple Shortcuts actions with a multi-select light picker and per-action icons.
- Link lights or individual controls with Command-click.
- Pulse brightness from the current value to a peak, then through peak-to-minimum cycles, then back to the original values.
  A count of 3 means three peak-to-minimum fades.
  The low defaults to 0% (off).
  Set the low to 2% or higher to keep the light on during each pulse.
  An off light starts and ends off: count 1 runs off → peak → off.
  The duration is split by distance across the initial rise, each fade, and the final restore.
  The app sends its own immediate keyframes at up to 10 Hz, or one per percent when the duration allows, so the duration stays exact.
  A new action stops a running pulse, and a pulse stops itself when the light changes elsewhere.
  The client uses one persistent connection and never sends animation frames over `curl`.
- Sweep temperature from warmest to coolest and back to warmest for each count, then restore the original values.
  A sweep can hold a fixed brightness for the whole animation.
- Pulse and sweep editors in each light's menu.
  Refresh polling pauses for the light that is animating.
- Drag anywhere on a card to reorder. Dropping on another card swaps their positions.
- Live refresh while the app is open.
- Native app icon.
- JSON output from the CLI.
- Retry and wake handling for a light that drops off the network.

## Pulse reliability

The Key Light and Key Light Air run firmware 1.0.3 (222), the final release for both.
Build 220 fixed a bug where Key Light Air became unresponsive after an undetermined amount of time; build 222 added security fixes.
There is no later firmware, and no older release is more reliable.
See the [Key Light Air firmware history](https://help.elgato.com/hc/en-us/articles/30938140888973-Elgato-Key-Light-Air-Firmware-Release-History).

There is no open firmware for Key Light Air.
[Key Right](https://github.com/micthiesen/key-right) replaces the controller in the full-size Key Light with an ESP32-C3, but that is a hardware modification, not a patch for the Air.
The MK.2 generation uses a WebSocket protocol and is more robust, but it is different hardware.

The small HTTP server is sensitive to bursts of short-lived connections.
Clients that keep one connection open do better than clients that open a connection per request.
Elopli therefore:

- keeps a native HTTP connection open and refuses to animate when only `curl` is available;
- sends native keyframes at up to 10 Hz, or one per percent when the duration allows;
- pauses refresh polling for the light that is pulsing.

Practical steps:

- Quit Elgato Control Center while using Elopli. It polls and writes the same lights.
- Prefer a native or terminal run that has Local Network permission.
- Give each light a DHCP reservation, and use a dedicated 2.4 GHz SSID if the router steers bands aggressively.
- Power-cycle a light if port 9123 stops accepting connections after a burst.

Prior art that informed this:

- [ioBroker's adapter](https://www.npmjs.com/package/iobroker.elgato-key-light) debounces writes by 200 ms.
- [python-elgato](https://github.com/frenck/python-elgato) shares one HTTP session.
- [Harness Automation](https://docs.harnessautomation.com/gem/admin/drivers/elgato_key_light) documents the occasional HTTP server lockup.

## Requirements

macOS app:

- macOS 14 or later
- Xcode 27
- Rust 1.100
- `xcodegen` (`brew install xcodegen`)

CLI:

- Rust 1.100

## Build the macOS app

```sh
DEVELOPMENT_TEAM=XXXXXXXXXX ./scripts/build-macos.sh
open apps/macos/Elopli/build/Build/Products/Debug/Elopli.app
```

For a release build:

```sh
DEVELOPMENT_TEAM=XXXXXXXXXX ./scripts/build-macos.sh release
```

Use `DEVELOPMENT_TEAM` to sign with your Apple Development team.
That is required for native App Intents.
Without it, the build is ad-hoc and App Intents do not register.

The script builds the Rust FFI library, generates the UniFFI Swift bindings, generates the Xcode project, and builds the app.
The app has no Dock icon.
Click the lightbulb in the menu bar.

Hold Command and click a card, toggle, or slider to link lights.
Linked items change together.
A card link is shorthand for linking all three fields on that card.
Command-clicking a field on an unlinked card joins that field to the matching group without linking its power or other fields.
Pending selections use the yellow highlight; committed links use magenta.
Drag anywhere on a card to reorder it.
Dropping a card on another card swaps their positions.
The app refreshes every five seconds while it is open.

## Install for Shortcuts

Native App Intents require a team-signed app.
An ad-hoc build is rejected by the system `linkd` service because it has no team ID.
Use a free Apple ID Personal Team or your Apple Development team in Xcode.
The paid Apple Developer Program is not required for local App Intents.

```sh
DEVELOPMENT_TEAM=XXXXXXXXXX ./scripts/build-macos.sh
cp -R apps/macos/Elopli/build/Build/Products/Debug/Elopli.app ~/Applications/
open ~/Applications/Elopli.app
```

The team-signed build registers the App Intents with Shortcuts.
A team ID is shown in Xcode under Settings > Accounts.
Without a team-signed build, use the CLI through **Run Shell Script** in Shortcuts.

## Build the CLI

```sh
cargo build --release -p elopli
./target/release/elopli --help
```

Or install it:

```sh
cargo install --path crates/elopli
```

## Configure

Create `devices.json` in the current directory, or at the platform config path:

- macOS: `~/Library/Application Support/elopli/devices.json`
- Linux: `~/.config/elopli/devices.json`
- Windows: `%APPDATA%\elopli\devices.json`

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
Run `elopli discover` to find devices.
Run `elopli discover --write` to add them to the registry.

Set `ELOPLI_CONFIG` or pass `--config` to use another file.

## CLI

```sh
elopli status
elopli on
elopli off Right
elopli toggle --all
elopli brightness 50
elopli warmth 70
elopli kelvin 4000
elopli set --on --brightness 60 --warmth 40 --all
elopli identify Back
elopli rename Right "Desk Right"
elopli power-on
elopli power-on --defaults --brightness 20 --warmth 35 --all
elopli power-on --last --all
elopli pulse --count 3 --duration 3000 --value 100
elopli pulse Left --count 1 --duration 300 --value 80
elopli sweep --count 1 --duration 3000
elopli sweep Left --count 2 --duration 4000 --brightness 40
elopli info
elopli discover
```

Add `--json` for machine-readable output.

### Wake a sleeping light

A light that stays off can drop off the network.
Use `--wait` to retry until the light answers:

```sh
elopli on --wait 180
```

## How it works

The light serves plain HTTP on port 9123.
It announces the `_elg._tcp` service through mDNS.
A write sends a partial JSON object and returns the new state.

Read [docs/api.md](docs/api.md) for the protocol notes.
Read [docs/shortcuts.md](docs/shortcuts.md) for the Shortcuts actions.

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
cargo test --workspace
cargo fmt --all -- --check
./scripts/build-macos.sh
```

The macOS app is in `apps/macos/Elopli`.
The shared core is in `crates/elopli-core`.
The UniFFI bridge is in `crates/elopli-ffi`.
The CLI is in `crates/elopli`.

<!-- LICENSE/ -->

## License

Unless stated otherwise all works are:

- Copyright &copy; [Benjamin Lupton](https://balupton.com)

and licensed under:

- [Reciprocal Public License 1.5](http://spdx.org/licenses/RPL-1.5.html)

<!-- /LICENSE -->
