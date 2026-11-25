Assisted-by: OpenCode · DeepSeek V4.1 Flash <opencode-opencodego-deepseekv41flash@local>

Companion: [1795599587000-official-client-language.prompts.md](./1795599587000-official-client-language.prompts.md)

# Optoli: native per-platform client with a Rust core

## status

Optoli means "open elgato light".

Decisions:

- Codename and product name: Optoli.
- CLI and executable: `optoli`.
- Installer and app name: Optoli.
- Shared core and CLI: Rust.
- macOS UI: SwiftUI `MenuBarExtra`.
- Windows UI: Rust `windows-reactor`.
- Linux UI: Rust `gtk4-rs`.
- Linux tray: `ksni`, which implements the accepted AppIndicator protocol.
- Rust to Swift bridge: UniFFI.
- Packaging: AppImage via Soar now. Flatpak later.
- Windows distribution: Scoop bucket plus a portable ZIP. Inno Setup later.
- Code signing: Apple Development team signing for App Intents. Ad-hoc builds cannot register App Intents. Paid notarisation is shelved.
- Fallbacks, shelved: C# WinUI 3, Vala, AppKit.
- Implementation has started.

## implementation status

Done and verified on macOS 27.2.1:

- `optoli-core`: HTTP, config, mDNS discovery, validation, power-on settings.
- `optoli` CLI: status, power, brightness, warmth, Kelvin, mired, set, rename, power-on, identify, info, discover, list, config.
- `optoli-ffi`: UniFFI records and objects.
- Fake-device integration tests: 6 passing.
- macOS app: SwiftUI `MenuBarExtra` with cards, WiFi tooltips, gradient sliders, rename, power-on, and App Intents.
- App build script and headless smoke test.
- App Intents registered with an Apple Development team signature.
- Both sliders reflect the active temperature:
  - The temperature slider runs from cool blue to warm amber.
  - The brightness slider runs from near-black to the active temperature colour.

Pending:

- Windows Reactor spike.
- Windows WinUI 3 app.
- Linux GTK 4 app.
- AppImage via Soar.
- Scoop manifest.

Important finding:

- An ad-hoc signed app is rejected by `linkd` for App Intents with `Unable to get teamId`.
- Native Shortcuts need a team-signed bundle.
- An Apple Development team signature resolves this.
- The team-signed build was verified: `linkd` indexed `com.bevry.optoli`, interpolated the AppShortcuts, and accepted the process registration.
- Without a team, the CLI through **Run Shell Script** remains the Shortcuts path.

## context

Elgato Control Center removed its Apple Shortcuts actions.
The three lights still answer a local HTTP API on port 9123.
The API is cracked and verified. See [docs/api.md](../docs/api.md).
The Rust CLI and the macOS app now use that API.

Optoli must run on macOS, Linux, and Windows.
It must be native, with no webview.
It must be pinned to the menu bar or system tray.
It must have no Dock icon and no draggable window.
It must show a card for each light.
It must provide native Apple Shortcuts actions on macOS.

The UI is simple: a card grid, a settings sheet, and a tray menu.
Three native UIs are acceptable.
The project is also a learning exercise for native cross-platform work.

## decision

| Platform | UI | Language | Status |
| --- | --- | --- | --- |
| Core and CLI | `optoli-core` and `optoli` | Rust | Primary |
| macOS | SwiftUI `MenuBarExtra` + App Intents | Swift | Primary |
| Windows | WinUI 3 through `windows-reactor` | Rust | Primary, spike first |
| Fedora | GTK 4 + libadwaita through `gtk4-rs` | Rust | Primary |
| Windows fallback | WinUI 3 | C# | Shelved |
| Fedora fallback | GTK 4 + libadwaita | Vala | Shelved |
| macOS fallback | AppKit `NSStatusItem` + `NSPopover` | Swift | Shelved |

`MenuBarExtra` is chosen. The Linux tray uses AppIndicator through `ksni`. The Swift bridge uses UniFFI.

## requirements

All items are must, except Apple notarisation.

| Requirement | Priority | Note |
| --- | --- | --- |
| macOS, Linux, and Windows | Must | The user uses all three |
| Shared core across platforms | Must | One protocol implementation |
| Native UI, no webview | Must | No Tauri, Electron, or Compose |
| Card interface for each light | Must | Not a menu-bar combo box |
| Pinned to menu bar or tray | Must | No draggable window |
| No Dock icon on macOS | Must | `LSUIElement` |
| System tray on each platform | Must | Opens the card window |
| Native Apple Shortcuts actions | Must | App Intents on macOS |
| Control power, brightness, warmth | Must | The three requested properties |
| Rename | Must | Device display name |
| Power-on behaviour | Must | Restore last, or restore defaults |
| WiFi status and hover details | Must | See the data source below |
| Non-interactive CLI | Must | Shortcuts and scripts call it |
| JSON output | Must | Shortcuts parses the result |
| mDNS discovery | Must | Avoid a manual IP list |
| Self-signing only | Must | Free |
| Apple notarisation | Shelved | Paid |
| Paid Windows signing | Shelved | Not free |

## the WiFi data source

The light provides the data.
`GET /elgato/accessory-info` returns:

```json
"wifi-info": {
  "ssid": "example",
  "frequencyMHz": 2400,
  "rssi": -54
}
```

| Field | Meaning |
| --- | --- |
| `ssid` | The network that the light joined |
| `frequencyMHz` | 2400 or 5000 |
| `rssi` | Signal strength in dBm |

macOS CoreWLAN reports the Mac's own WiFi.
It cannot report each light's signal strength.
Optoli must use the light's `wifi-info`.

Hover text:

```text
example · 2.4 GHz · -54 dBm (good)
```

| RSSI | Label |
| --- | --- |
| -50 or better | excellent |
| -60 or better | good |
| -70 or better | fair |
| below -70 | weak |

If the accessory request fails, show a disconnected icon and the last known values.
The API does not return the IP address.
The client knows it from the registry or mDNS.

## research and best practices

### Rust core

- Use blocking HTTP. It is simpler to expose over FFI.
  - `ureq` for HTTP.
  - `serde` and `serde_json` for JSON.
  - `mdns-sd` for discovery.
  - `thiserror` for typed errors.
- Keep one `snapshot()` call that fetches the light state and accessory info together.
- Validate brightness, mired, display name, and power-on values in the core.
- Keep the core free of UI and platform code.
- Expose the core to Swift through UniFFI.

Resources:

- `ureq`: <https://docs.rs/ureq>
- `mdns-sd`: <https://docs.rs/mdns-sd>
- `thiserror`: <https://docs.rs/thiserror>
- Rust API guidelines: <https://rust-lang.github.io/api-guidelines/>

### Windows Reactor

Reactor is a Microsoft-backed declarative WinUI 3 library for Rust.
It is a preview. The current version is 0.101.

Useful pieces:

- `ApplicationView` declares windows and notification icons.
- `NotifyIcon::on_activate` gives a `ScreenPoint` with the tray click position.
- `WindowVisuals` sets client size, initial position, always-on-top, and min/max.
- `Tooltip` and `TooltipExt` cover the WiFi hover.
- `windows-reactor-setup` stages the Windows App Runtime for a self-contained app.
- The `notifyicon` sample is the starting point for the tray.
- The `self_contained` sample is the starting point for distribution.

Known risk:

- `WindowVisuals` does not expose borderless mode, resizability, taskbar visibility, or title-bar visibility.
- Reactor does not expose the window handle in its public API.
- A no-title-bar, no-taskbar, no-drag popover may need a Reactor change or Win32 interop.

Spike checklist before the Windows UI:

1. Create a tray icon with a tooltip and a menu.
2. Receive the tray click position from `on_activate`.
3. Open a window near that position.
4. Remove the title bar and the resize frame.
5. Disable dragging.
6. Hide the window from the taskbar and Alt-Tab.
7. Dismiss the window when it loses focus.
8. Show three cards with brightness, warmth, and a WiFi tooltip.
9. Build a self-contained unpackaged app.
10. Test on Windows 10 and Windows 11.
11. Check for GDI or handle leaks across repeated open and close.

If items 3–7 fail, fall back to C# WinUI 3.
C# exposes `AppWindow`, `OverlappedPresenter`, `ShowInSwitchers`, `ExtendsContentIntoTitleBar`, and the window handle.

Resources:

- Reactor readme: <https://github.com/microsoft/windows-rs/tree/master/crates/libs/reactor>
- Reactor crate: <https://docs.rs/windows-reactor>
- Reactor samples: <https://github.com/microsoft/windows-rs/tree/master/crates/samples/reactor>
- Notify icon sample: <https://github.com/microsoft/windows-rs/tree/master/crates/samples/reactor/notifyicon>
- Self-contained sample: <https://github.com/microsoft/windows-rs/tree/master/crates/samples/reactor/self_contained>
- Window placement sample: <https://github.com/microsoft/windows-rs/tree/master/crates/samples/reactor/window-placement-registry>
- Runtime setup: <https://docs.rs/windows-reactor-setup>
- Unpackaged WinUI 3: <https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/unpackage-winui-app>
- `Shell_NotifyIcon`: <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw>
- `Shell_NotifyIconGetRect`: <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyicongetrect>
- Extended window styles: <https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles>
- Open reactor leak report: <https://github.com/microsoft/windows-rs/issues/5287>

### Linux GTK 4

GNOME's documentation says C is the default platform language.
Applications can use C, C++, Java, JavaScript, Perl, Python, Rust, Vala, or C#.

Use Rust with `gtk4-rs` and `libadwaita`.

Use `ksni` to implement the tray.
`ksni` is a Rust crate for the StatusNotifierItem protocol, which is the same AppIndicator protocol that GNOME exposes through its extension.
It is not a different standard. It is the implementation choice inside the AppIndicator approach.
`ksni` activation gives the click position for window placement.

Useful pieces:

- `gtk::Window::set_decorated(false)` for no title bar.
- `gtk::Window::set_resizable(false)` for no resize.
- `gtk::Window::set_skip_taskbar_hint(true)` for no taskbar entry.
- `gtk::Window::set_keep_above(true)` for a popover-like window.
- `ksni` for the StatusNotifierItem tray.
- `libadwaita` for GNOME-native cards and settings.
- GNOME HIG for layout and spacing.

AppIndicator on GNOME is accepted.
`aural-keyboard` already requires the extension:

```sh
sudo dnf install gnome-shell-extension-appindicator
gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com
```

Resources:

- gtk4-rs book: <https://gtk-rs.org/gtk4-rs/stable/latest/book/>
- gtk4 docs: <https://docs.rs/gtk4>
- libadwaita docs: <https://docs.rs/libadwaita>
- GNOME HIG: <https://developer.gnome.org/hig/>
- GNOME languages: <https://developer.gnome.org/documentation/introduction/languages.html>
- `ksni`: <https://docs.rs/ksni>
- AppIndicator extension: <https://extensions.gnome.org/extension/615/appindicator-support/>
- linuxdeploy GTK plugin: <https://github.com/linuxdeploy/linuxdeploy-plugin-gtk>
- AppImage tooling: <https://github.com/AppImage/appimagetool>
- AppImage docs: <https://docs.appimage.org/>
- Soar: <https://github.com/pkgforge/soar>
- Soar packages: <https://docs.pkgforge.dev/repositories/soarpkgs>

### macOS SwiftUI

Implemented in `apps/macos/Optoli`.

Use `MenuBarExtra` with `.menuBarExtraStyle(.window)`.
Set `LSUIElement=true` to hide the Dock icon.
The menu-bar window is a popover, not a draggable window.
Add App Intents in the same app target.
Link the Rust core through UniFFI.

Build:

```sh
./scripts/build-macos.sh
open apps/macos/Optoli/build/Build/Products/Debug/Optoli.app
```

The app is ad-hoc signed by default.
That is enough for the card interface and the CLI.

App Intents need a team-signed bundle.
`linkd` rejects ad-hoc apps with `Unable to get teamId`.
Use an Apple Development team in Xcode:

```sh
DEVELOPMENT_TEAM=XXXXXXXXXX ./scripts/build-macos.sh
```

The team-signed build registers the App Intents.
Without a team, use the CLI through **Run Shell Script**.

Useful pieces:

- `MenuBarExtra` and `.window` style for the popover.
- `LSUIElement` for no Dock icon.
- `AppIntent` and `AppShortcutsProvider` for Shortcuts actions.
- `SMAppService.mainApp.register()` for launch at login.
- `NSLocalNetworkUsageDescription` for the local network prompt.
- `NSBonjourServices` with `_elg._tcp` for mDNS.
- UniFFI-generated Swift bindings and the Rust static library.
- An Xcode app target, not a bare SwiftPM executable, for App Intents and the Info.plist.

Resources:

- `MenuBarExtra`: <https://developer.apple.com/documentation/swiftui/menubarextra>
- Window style: <https://developer.apple.com/documentation/swiftui/menubarextrastyle/window>
- `LSUIElement`: <https://developer.apple.com/documentation/bundleresources/information-property-list/lsuielement>
- App Intents: <https://developer.apple.com/documentation/appintents>
- `AppShortcutsProvider`: <https://developer.apple.com/documentation/appintents/appshortcutsprovider>
- Local network privacy: <https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy>
- `NSBonjourServices`: <https://developer.apple.com/documentation/bundleresources/information-property-list/nsbonjureservices>
- `SMAppService`: <https://developer.apple.com/documentation/servicemanagement/smappservice>
- UniFFI: <https://mozilla.github.io/uniffi-rs/>

### UniFFI bridge

UniFFI generates the Swift bindings from the Rust interface.

- Define the API in the `optoli-ffi` crate with UniFFI records, enums, and errors.
- Build the crate as a static library.
- Run `uniffi-bindgen` to generate Swift and the module map.
- Add the generated files and the static library to the Xcode target.
- Call the generated Swift API from the SwiftUI views and App Intents.

Why UniFFI:

- It handles strings, records, enums, options, errors, and async.
- It removes most unsafe glue.
- The Swift side stays readable as the API grows.
- Manual C ABI is the fallback if UniFFI adds too much build friction.

### Testing and CI

- Use the Python CLI only as a temporary oracle during the port.
- Keep the fake HTTP device in `tests/fake_device.py` until the Rust tests have their own fake server.
- Add Rust integration tests against a fake device.
- Add UI smoke tests per platform.
- Use GitHub Actions with `macos-latest`, `windows-latest`, and `ubuntu-22.04`.
- Pin the Rust toolchain in `rust-toolchain.toml`.
- Build the Swift app on macOS only.
- Build the AppImage on Ubuntu or Fedora.
- Publish checksums with every artifact.

## Windows packaging, Scoop, and Inno Setup

### What the ZIP contains

The release ZIP contains only the built app:

- `Optoli.exe` — the WinUI 3 tray app.
- `optoli-cli.exe` — the CLI, because Windows file names are case-insensitive.
- Windows App Runtime files from `windows-reactor-setup`.
- App resources and icons.
- License and README files.

The Scoop manifest is not inside the ZIP.
The manifest is a separate JSON file.

### How CI and Scoop fit together

1. CI builds the Windows app.
2. CI stages the self-contained runtime.
3. CI creates `optoli-windows-x64.zip`.
4. CI computes the SHA-256 of the ZIP.
5. CI uploads the ZIP and its `.sha256` file to the GitHub release.
6. CI generates `optoli.json` with:
   - `version`
   - `url` pointing to the release ZIP
   - `hash` with the SHA-256
   - `extract_dir`, if the ZIP has a top-level folder
   - `bin` for `optoli-cli.exe`, with an alias shim named `optoli`
   - `shortcuts` for `Optoli.exe`
   - `post_install` to run `optoli install --autostart`
   - `uninstaller` to run `optoli uninstall`
7. CI uploads `optoli.json` as a release asset.
8. CI can also commit `optoli.json` to the Scoop bucket repository.

A user can install from the manifest URL directly:

```powershell
scoop install https://github.com/<owner>/<repo>/releases/download/v0.1.0/optoli.json
```

Scoop supports a manifest URL. This is a one-off install.
For updates, use a bucket:

```powershell
scoop bucket add bevry https://github.com/bevry-vibes/scoop-bucket
scoop install optoli
```

The bucket is better for `scoop update` and version tracking.
The direct manifest URL is a convenience for the first run.

Example manifest shape:

```json
{
  "version": "0.1.0",
  "description": "Control Elgato Key Lights from the menu bar or system tray.",
  "homepage": "https://github.com/bevry-vibes/optoli",
  "license": "RPL-1.5",
  "url": "https://github.com/bevry-vibes/optoli/releases/download/v0.1.0/optoli-windows-x64.zip",
  "hash": "<sha256>",
  "extract_dir": "optoli-windows-x64",
  "bin": [["optoli-cli.exe", "optoli"]],
  "shortcuts": [["Optoli.exe", "Optoli"]],
  "post_install": ["optoli install --autostart"],
  "uninstaller": { "script": "optoli uninstall" }
}
```

### Inno Setup

Inno Setup is a free Windows installer generator.
It creates a single `setup.exe` with a wizard, Start Menu shortcuts, an uninstaller, and an Add/Remove Programs entry.
It is not a package format or a runtime.
It can install an unpackaged, self-contained WinUI 3 app.
An unsigned installer still triggers SmartScreen.

`aural-keyboard` does not use Inno Setup.
Its Windows release is a ZIP plus a SHA-256 file.
The `aural system install` command copies the binary, registers autostart, and provides uninstall.

Recommendation: use Scoop plus the portable ZIP first.
Add Inno Setup later only if a GUI installer is needed.
Winget can wait.

Resources:

- Scoop buckets: <https://github.com/ScoopInstaller/Scoop/wiki/Buckets>
- Scoop app manifests: <https://github.com/ScoopInstaller/Scoop/wiki/App-Manifests>
- Scoop pre/post install scripts: <https://github.com/ScoopInstaller/Scoop/wiki/Pre-Post-(un)install-scripts>
- Inno Setup: <https://jrsoftware.org/isinfo.php>
- Inno Setup help: <https://jrsoftware.org/ishelp/>
- aural-keyboard release workflow: <https://github.com/bevry-vibes/aural-keyboard/blob/main/.github/workflows/release.yml>
- aural-keyboard macOS packaging: <https://github.com/bevry-vibes/aural-keyboard/blob/main/scripts/package-app.sh>

## the menu-bar-pinned interface

### macOS

SwiftUI `MenuBarExtra` with a window style.
`LSUIElement` for no Dock icon.
No title bar, no resize, no drag.
App Intents for Shortcuts.

### Windows

`windows-reactor` `ApplicationView::notify_icon` for the tray.
`NotifyIcon::on_activate` gives the click position.
`WindowVisuals::initial_position` places the window near the tray.
The spike must confirm borderless mode and taskbar hiding.

If Reactor cannot do it, fall back to C# WinUI 3.
Use `AppWindow`, `OverlappedPresenter`, and `ShowInSwitchers=false`.
Position with `Shell_NotifyIconGetRect`.
Dismiss on focus loss.

### Fedora

GTK 4 window with `skip_taskbar_hint`, `decorated=false`, `resizable=false`.
A StatusNotifierItem through `ksni`.
`ksni` activation gives the click position.
Position the window near the top-right of the screen.
GNOME needs the AppIndicator extension.
`aural-keyboard` already uses it, so it is an accepted dependency.

## packaging

### AppImage via Soar, now

Build a GTK 4 AppImage with the required libraries.
Use `linuxdeploy` with the GTK plugin and `appimagetool`.
Bundle GTK 4, libadwaita, GSettings schemas, icons, and fonts.
Publish through Soar.
Soar supports AppImage and zsync delta updates.

### Flatpak, later

Flatpak is shelved.
When added:

- Use the GNOME runtime.
- Grant network access for local HTTP and mDNS.
- Grant StatusNotifier/AppIndicator bus access for the tray.
- GNOME still needs the AppIndicator extension.
- Publish through Flathub.

### Windows

Use an unpackaged, self-contained build.
`windows-reactor-setup` stages the Windows App Runtime.
Distribute a ZIP with `optoli install`, and publish a Scoop manifest.
Add Inno Setup later if a GUI installer is needed.

### macOS

Build an `Optoli.app` bundle.
Ad-hoc sign it with `codesign -s -`.
For a stable local identity, create a self-signed "Optoli Code Signing" certificate in the login keychain and sign with it.
Distribute a DMG or ZIP.
Gatekeeper warns on downloaded builds.
Document `xattr -d com.apple.quarantine`.

## code signing: self-signing only

Active:

| Platform | Method | Cost |
| --- | --- | --- |
| macOS | Ad-hoc `codesign -s -` | Free |
| macOS | Self-signed "Optoli Code Signing" identity for stable TCC | Free |
| Windows | Self-signed certificate for local testing only | Free |
| Windows | Unsigned public release, SmartScreen warning | Free |
| Linux | Unsigned AppImage with checksums | Free |

Shelved:

- SignPath Foundation.
- Apple Developer Program and notarisation.
- OV and EV Windows certificates.
- Azure Trusted Signing.
- Any paid certificate.

SignPath requires a released, maintained project with reputation.
It is not a good fit for a new hobby project.
Shelf it for later.

Self-signed Windows certificates are not trusted on other machines.
They do not remove SmartScreen warnings.
Do not use them for public releases.

Costs for reference only:

| Platform | Option | Cost |
| --- | --- | --- |
| macOS | Apple Developer Program | $99/year |
| Windows | OV certificate | $150–$300/year |
| Windows | EV certificate | $300–$800/year |
| Windows | Azure Trusted Signing | $9.99/month Basic |
| Windows | SignPath Foundation | Free for OSS, conditions apply |

## proposed structure

```text
elgato-key-light/
  Cargo.toml
  rust-toolchain.toml
  crates/
    optoli-core/
    optoli/
    optoli-ffi/
  apps/
    macos/
      Optoli/
    windows/
      optoli-winui/       Rust windows-reactor
    linux/
      optoli-gtk/         Rust gtk4-rs
  packaging/
    appimage/
    scoop/
    windows/
  tests/
```

`optoli-core` holds:

- `LightState`, `AccessoryInfo`, `WifiInfo`, `DeviceSettings`, and `Device`.
- `OptoliClient` with retry and timeout.
- `DeviceStore` for the JSON registry.
- `Discovery` with `mdns-sd`.
- A `snapshot()` call that fetches state and accessory info together.

`apps/linux/optoli-gtk` depends on `optoli-core` directly.
`apps/windows/optoli-winui` depends on `optoli-core` directly.
`apps/macos/Optoli` links `optoli-ffi` through UniFFI.

## retiring the Python reference CLI

The Python CLI was the exploration tool.
It is not part of the official client.

Once the Rust CLI reaches parity, there is no strong reason to keep it as a shipped CLI:

- It creates two command surfaces and two sets of documentation.
- It creates two packaging paths, `uv` and Cargo or Homebrew.
- It creates two bug surfaces and two release cadences.
- It has no capability that the Rust CLI lacks.
- The Rust CLI is the supported binary for users, scripts, and Shortcuts.

Keep it only during the port:

- Use it as a temporary oracle against the fake device and the real lights.
- Compare the JSON output and exit codes.
- Freeze it when parity is reached.

Parity checklist:

- `status`, `list`, `on`, `off`, `toggle`.
- `brightness`, `warmth`, `kelvin`, `temperature`, `set`.
- `rename`, `power-on`.
- `identify`, `info`, `discover`.
- JSON output, exit codes, device selection, and config file.
- Retry and `--wait`.

Retirement steps:

1. Run both CLIs against the same fake device.
2. Run both CLIs against the real lights.
3. Compare the JSON records and exit codes.
4. Remove the Python CLI from the README and install docs.
5. Delete the Python package, its tests, and the `uv` project files.
6. Port the fake device to a Rust test server, or keep a small Python test fixture if it is useful.
7. Remove Python from CI.
8. Keep [docs/api.md](../docs/api.md) and the git history.

If a quick protocol probe is still useful, keep a small script under `tests/`.
Do not keep a second user-facing CLI.

## phases

1. Done: use the Python CLI as the temporary oracle. It is retired.
2. Done: build `optoli-core` and the `optoli` Rust CLI.
3. Done: build the macOS SwiftUI app and App Intents with UniFFI.
4. Next: spike `windows-reactor` against the checklist.
5. Next: build the Windows app, or the C# fallback.
6. Next: build the Linux GTK 4 app with `ksni`.
7. Next: build the AppImage and publish it through Soar.
8. Next: build the Scoop manifest and CI release flow.
9. Next: add self-signing and release automation.

## open questions

1. Windows: does the Reactor spike pass the borderless and no-taskbar checklist?
2. Windows GUI and CLI naming: is `Optoli.exe` plus `optoli-cli.exe` with an `optoli` shim acceptable?
