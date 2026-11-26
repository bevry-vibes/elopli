# Linux

The Linux app is a native GTK4 tray panel.
It uses the shared Rust core and `elopli-session` crate, the `ksni` StatusNotifierItem service, and GTK4 painting.
It has no cloud service, account, or telemetry.

## Status

Implemented and hardware-read-tested on Fedora 45 Workstation with GNOME Shell 51 on Wayland against one Key Light Air and two Key Lights, firmware 1.0.3 (222).
The full interaction smoke test, packaging install, and Ubuntu validation are still open.

## Requirements

- Fedora 45 or Ubuntu 24.04, x86_64 or aarch64.
- GTK 4.14 or later. Fedora needs `gtk4-devel`; Ubuntu needs `libgtk-4-dev`.
- Rust 1.100, pinned by `rust-toolchain.toml`.
- A StatusNotifierItem host. GNOME needs the AppIndicator extension; Fedora enables it by default.

Fedora:

```sh
sudo dnf install gtk4-devel
```

Ubuntu:

```sh
sudo apt install libgtk-4-dev
```

GNOME:

```sh
gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com
```

## Build

```sh
cargo build --locked -p elopli-app-linux --features linux --release
cargo build --locked -p elopli --release
```

The app is `target/release/elopli-app`.
The CLI is `target/release/elopli`.

Build a portable tarball, checksums, and, when `appimagetool` is on `PATH` or `APPIMAGETOOL` is set, an AppImage:

```sh
./scripts/build-linux.sh
```

The script writes `dist/Elopli-<version>-linux-<arch>/`, a `.tar.gz`, and `SHA256SUMS`.

## Run

Launch `elopli-app`.
The StatusNotifierItem appears in the desktop's tray.
Its menu has Show Elopli, Refresh, All On, All Off, Discover Lights, Launch at Login, Reveal Config, and Quit Elopli.

The panel has one card per light:

- Power switch.
- Brightness slider from 2 to 100.
- Warmth slider from 0 (cool) to 100 (warm), warm on the left.
- WiFi icon with SSID, frequency, RSSI, quality, and the saved address on hover.
- Menu for Rename, Power On Behaviour, Pulse, Sweep, group actions, and Identify.
- Hover the model name for the light's serial number; it is hidden by default and never written to disk or logs.
- Error text with the address and a power-cycle hint when a light is offline.

Hold Ctrl and click a card, switch, or slider to link lights.
Pending selections use yellow; committed links use magenta.
Release Ctrl to apply the links.
Drag a card by its grip to swap it with another card.

Pulse and Sweep open a sheet with a count, a human duration (`1.0s`, `500ms`, `1.5`, `10m`, `1h`), and the animation controls.
Refresh polling pauses for the lights that are animating.
Closing a sheet stops and restores a running animation.

## Wayland and the panel

GNOME on Wayland does not let an application place its own window, and GNOME does not support layer-shell.
The panel is therefore a regular window with a header bar; it cannot pop up attached to the tray icon.
The tray menu is the primary entry point.
Open the panel with **Show Elopli** in that menu.

GNOME's AppIndicator extension controls tray click behaviour:

- A single left click opens the tray menu.
- A right click opens the same menu.
- A double left click calls the item's Activate method and opens the panel.
- A middle click calls SecondaryActivate and also opens the panel.

Elopli cannot override these mappings; they live in the extension.
Hosts that call Activate on a single left click, such as KDE, open the panel directly.
The panel cannot be anchored next to the tray icon on GNOME Wayland because clients cannot position top-level windows.
It opens where the compositor places it, and **Show Elopli** remains in the menu for a single-click path.

## Names

The light's own `displayName` is the authoritative name.
Rename writes it to the light with `PUT /elgato/accessory-info`, so it follows the light to macOS, Windows, and other Linux machines.

`devices.json` still stores a `name` key.
It is the internal identity for link groups, registry matching, and CLI targeting, and it is kept equal to the light's display name:

- Rename updates the registry key and migrates link groups.
- A refresh adopts a display name changed by another interface, and migrates link groups.
- Discovery uses the display name when the light has one.
- A light with no display name falls back to its mDNS instance name until it is renamed.

The registry key is not a separate nickname.
Renaming a light changes the name used by the CLI and the link graph as well as the card title.

If your lights already have local names from an older setup but report no display name, rename each one once in any Elopli interface.
That writes the name to the light, and every interface then adopts it automatically.

## Autostart and Single Instance

Launch at Login writes `~/.config/autostart/com.bevry.elopli.desktop` with the current executable and `--autostart`.
Clear the menu item to remove the file.

The app owns the `com.bevry.elopli` D-Bus name.
A second launch forwards activation to the running instance and exits; it does not create a second tray icon.

## Discovery and Firewall

Discovery browses mDNS `_elg._tcp` on UDP 5353 and stores the chosen IPv4 address.
It needs multicast DNS on the local network; a firewall that blocks UDP 5353 returns an empty list.

If discovery finds nothing:

- Allow mDNS through the firewall on the light's network.
- Disable or split-tunnel VPN and virtual bridges (docker0, virbr, tun) that can shadow the light subnet.
- Prefer one IPv4 address per light with a DHCP reservation.
- Use a dedicated 2.4 GHz SSID if the router steers bands aggressively.
- Power-cycle a light that has fallen off the network.

The light API is plain HTTP on TCP 9123 with no authentication or encryption.
Use Elopli only on a trusted local network.

## Smoke Test and Screenshots

The headless hardware check reads the registry and prints one line per light:

```sh
./target/release/elopli-app --smoke-test
```

It exits 0 when every configured light answered and 1 otherwise.

`ELOPLI_SCREENSHOT` renders the panel to a PNG and quits.
It uses the GTK render node, so it works on Wayland without a screenshot portal:

```sh
ELOPLI_SCREENSHOT=/tmp/elopli-panel.png ./target/debug/elopli-app
```

## Development

```sh
cargo fmt --all -- --check
cargo clippy -p elopli-app-linux --features linux --all-targets --locked -- -D warnings
cargo test -p elopli-app-linux --features linux --locked
```

The Linux app uses safe Rust.
The core, session, CLI, and Linux app keep the workspace's `unsafe_code = "forbid"`.
