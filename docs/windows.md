# Windows

The Windows app is a native tray panel for Windows 10 1809 or later on x64.
It uses the shared Rust core and session layer, the Win32 shell, and a GDI+ panel renderer.
It has no runtime dependency beyond the Windows system libraries.

## Requirements

- Windows 10 1809 or later, x64.
- Rust 1.100 via [rustup](https://rustup.rs).
- Visual Studio Build Tools 2022 with the Desktop development with C++ workload for an MSVC build.
- For a local GNU build: `rustup toolchain install 1.100.0-x86_64-pc-windows-gnu` and a MinGW-w64 GCC toolchain on `PATH`.

## Build

```powershell
cargo build --locked -p elopli-app --features windows --release
cargo build --locked -p elopli --release
```

The app is `target\release\ElopliApp.exe`.
The CLI is `target\release\elopli.exe`.
The different names avoid a case-insensitive filesystem collision between `Elopli.exe` and `elopli.exe`.

Build a portable ZIP and Scoop manifest:

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1
```

The script writes `dist\Elopli-<version>-windows-x64\`, a ZIP, `SHA256SUMS`, and `dist\elopli.json`.
Use `-Toolchain 1.100.0-x86_64-pc-windows-gnu` for a local GNU build, and `-SkipBuild` to package existing binaries.

## Run

Launch `ElopliApp.exe`.
It adds a lightbulb to the notification area and starts hidden when launched with `--autostart`.
Left-click the tray icon to open or hide the panel.
Right-click it for Show/Hide, Refresh, All On, All Off, Discover Lights, Launch at Login, Reveal Config, and Quit.

The panel has one card per light:

- Power switch.
- Brightness slider from 2 to 100.
- Warmth slider from 0 (cool) to 100 (warm), with warm on the left.
- WiFi icon with SSID, band, RSSI, quality, and the saved address on hover.
- Menu for Rename, Power On Behaviour, Pulse, Sweep, group actions, and Identify.
- Hover the model name for the light's serial number; it is hidden by default and never written to disk or logs.
- Error text with the address and a power-cycle hint when a light is offline.

Hold Ctrl and click a card, switch, or slider to link lights. Pending selections use yellow; committed links use magenta.
A card link links all three fields. A field link links only that field.
Link groups are stored in `links.json` beside the device registry.
Drag a card onto another card to swap their positions.

Pulse and Sweep open a sheet with a count, a human duration (`1.0s`, `500ms`, `1.5`, `10m`, `1h`), and the animation controls.
Refresh polling pauses for the lights that are animating.
Closing the panel or quitting the app stops and restores a running animation.

Press Esc to close a sheet or hide the panel.
Clicking another window hides the panel when no sheet is open.

## Configuration

The app always uses `%APPDATA%\elopli\devices.json`.
The registry has this shape:

```json
{
  "devices": [
    { "name": "Back", "host": "192.0.2.10" },
    { "name": "Left", "host": "192.0.2.11" }
  ]
}
```

The loader accepts a UTF-8 BOM, validates duplicate names and hosts, writes atomically, and uses a file lock.
Run `elopli config path` to print the CLI path; the CLI prefers `.\devices.json` when present, while the app uses the `%APPDATA%` path.

## Discovery and Firewall

Discovery browses mDNS `_elg._tcp` on UDP 5353 and prefers a private IPv4 address when a light advertises more than one.
The app stores the chosen IPv4 address, not the hostname.

If discovery finds nothing:

- Allow `ElopliApp.exe` on private networks in Windows Defender Firewall.
- Make sure the network profile is Private rather than Public.
- Disable or split-tunnel VPN and Hyper-V/WSL virtual adapters that can shadow the light's subnet.
- Use a dedicated 2.4 GHz SSID or a DHCP reservation for each light.
- Power-cycle a light that has fallen off the network.

The light API is plain HTTP on TCP 9123 with no authentication or encryption.
Use Elopli only on a trusted local network.

## Autostart and Single Instance

Launch at Login writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Elopli` with `"<path>\ElopliApp.exe" --autostart`.
Clear the menu item to remove the value.
A second launch signals the existing process to show the panel and exits; it does not start another tray icon.

## Updates and Signing

The app does not self-update.
Download a new ZIP, replace the executable, and verify `SHA256SUMS`.
Releases can be signed with Authenticode and a timestamp:

```powershell
pwsh -NoProfile -File scripts/build-windows.ps1 -CertificatePath .\certificate.pfx -CertificatePassword $env:CERT_PASSWORD
```

The script signs `ElopliApp.exe` and `elopli.exe` when `signtool.exe` is available before it creates the ZIP and hashes.
Signing is optional for local use and required for a trusted public release.

## Smoke Test

The debug build keeps the console subsystem and supports a headless hardware check:

```powershell
cargo build --locked -p elopli-app --features windows
.\target\debug\ElopliApp.exe --smoke-test
```

It prints the registry path and one line per light, then exits 0 when every light answered and 1 otherwise.
The release build is a GUI-subsystem executable, so it has no console for the smoke test.

## Development

`ELOPLI_NO_HIDE=1` keeps the panel visible when it loses focus, which is useful for screenshots and UI work.
The Win32 boundary is the only crate that allows `unsafe_code`; the core, session, CLI, and FFI keep the workspace's `forbid`.
