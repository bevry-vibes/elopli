# Optoli

**open elgato light**

Optoli controls Elgato Key Light and Key Light Air devices over the local network.
It has a native macOS menu-bar app, a Rust CLI, and a shared Rust core.

The macOS interface is a card for each light.
Each card has power, brightness, warmth, WiFi details, rename, and power-on behaviour.
The app has no Dock icon and no draggable window.
It appears in the menu bar.

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
- Pulse brightness, warmth, or both, then restore the original values.
- Drag anywhere on a card to reorder. Dropping on another card swaps their positions.
- Live refresh while the app is open.
- Native app icon.
- JSON output from the CLI.
- Retry and wake handling for a light that drops off the network.

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
open apps/macos/Optoli/build/Build/Products/Debug/Optoli.app
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
cp -R apps/macos/Optoli/build/Build/Products/Debug/Optoli.app ~/Applications/
open ~/Applications/Optoli.app
```

The team-signed build registers the App Intents with Shortcuts.
A team ID is shown in Xcode under Settings > Accounts.
Without a team-signed build, use the CLI through **Run Shell Script** in Shortcuts.

## Build the CLI

```sh
cargo build --release -p optoli
./target/release/optoli --help
```

Or install it:

```sh
cargo install --path crates/optoli
```

## Configure

Create `devices.json` in the current directory, or at the platform config path:

- macOS: `~/Library/Application Support/optoli/devices.json`
- Linux: `~/.config/optoli/devices.json`
- Windows: `%APPDATA%\optoli\devices.json`

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
Run `optoli discover` to find devices.
Run `optoli discover --write` to add them to the registry.

Set `OPTOLI_CONFIG` or pass `--config` to use another file.

## CLI

```sh
optoli status
optoli on
optoli off Right
optoli toggle --all
optoli brightness 50
optoli warmth 70
optoli kelvin 4000
optoli set --on --brightness 60 --warmth 40 --all
optoli identify Back
optoli rename Right "Desk Right"
optoli power-on
optoli power-on --defaults --brightness 20 --warmth 35 --all
optoli power-on --last --all
optoli pulse --count 3 --duration 600 --ease ease-in-out --target both --value 100
optoli pulse Left --count 1 --duration 300 --ease linear --target brightness --value 80
optoli info
optoli discover
```

Add `--json` for machine-readable output.

### Wake a sleeping light

A light that stays off can drop off the network.
Use `--wait` to retry until the light answers:

```sh
optoli on --wait 180
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

The macOS app is in `apps/macos/Optoli`.
The shared core is in `crates/optoli-core`.
The UniFFI bridge is in `crates/optoli-ffi`.
The CLI is in `crates/optoli`.

<!-- LICENSE/ -->

## License

Unless stated otherwise all works are:

- Copyright &copy; [Benjamin Lupton](https://balupton.com)

and licensed under:

- [Reciprocal Public License 1.5](http://spdx.org/licenses/RPL-1.5.html)

<!-- /LICENSE -->
