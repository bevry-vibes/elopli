Assisted-by: OpenCode · DeepSeek V4.1 Flash <opencode-opencodego-deepseekv41flash@local>

Companion: [1795599587000-official-client-language.prompts.md](./1795599587000-official-client-language.prompts.md)

# Official client: native per-platform UI with a Rust core

## status

Decisions:

- Shared core and CLI: Rust.
- macOS UI: SwiftUI.
- Windows UI: Rust `windows-reactor` first.
- Linux UI: Rust `gtk4-rs`.
- Fallbacks, shelved: C# WinUI 3, Vala, AppKit.
- Packaging: AppImage via Soar now. Flatpak later.
- Code signing: self-signing only. All paid and OSS signing programmes are shelved.
- Implementation has not started.

## context

Elgato Control Center removed its Apple Shortcuts actions.
The three lights still answer a local HTTP API on port 9123.
The API is cracked and verified. See [docs/api.md](../docs/api.md).
A Python CLI proves the interface and controls all three lights now.

The official client must run on macOS, Linux, and Windows.
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
| Core and CLI | `keylight-core` and `keylight` | Rust | Primary |
| macOS | SwiftUI `MenuBarExtra` + App Intents | Swift | Primary |
| Windows | WinUI 3 through `windows-reactor` | Rust | Primary, spike first |
| Fedora | GTK 4 + libadwaita through `gtk4-rs` | Rust | Primary |
| Windows fallback | WinUI 3 | C# | Shelved |
| Fedora fallback | GTK 4 + libadwaita | Vala | Shelved |
| macOS fallback | AppKit `NSStatusItem` + `NSPopover` | Swift | Shelved |

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
The official app must use the light's `wifi-info`.

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

- Use blocking HTTP for the core. It is simpler to expose over FFI.
  - `ureq` for HTTP.
  - `serde` and `serde_json` for JSON.
  - `mdns-sd` for discovery.
  - `thiserror` for typed errors.
- Keep one `snapshot()` call that fetches the light state and accessory info together.
- Validate brightness, mired, display name, and power-on values in the core.
- Keep the core free of UI and platform code.
- Expose a C ABI through `keylight-ffi`, or generate Swift bindings with UniFFI.
- Use `tracing` or `log` with a file sink for diagnostics.

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

For this project, use Rust with `gtk4-rs` and `libadwaita`.

Useful pieces:

- `gtk::Window::set_decorated(false)` for no title bar.
- `gtk::Window::set_resizable(false)` for no resize.
- `gtk::Window::set_skip_taskbar_hint(true)` for no taskbar entry.
- `gtk::Window::set_keep_above(true)` for a popover-like window.
- `ksni` for a StatusNotifierItem tray icon.
- `ksni` activation gives the click position for window placement.
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

Use `MenuBarExtra` with `.menuBarExtraStyle(.window)`.
Set `LSUIElement=true` to hide the Dock icon.
The menu-bar window is a popover, not a draggable window.
Add App Intents in the same app target.
Link the Rust core as a static library.

Useful pieces:

- `MenuBarExtra` and `.window` style for the popover.
- `LSUIElement` for no Dock icon.
- `AppIntent` and `AppShortcutsProvider` for Shortcuts actions.
- `SMAppService.mainApp.register()` for launch at login.
- `NSLocalNetworkUsageDescription` for the local network prompt.
- `NSBonjourServices` with `_elg._tcp` for mDNS.
- UniFFI or a manual C ABI for the Rust core.
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
- swift-bridge: <https://chinedufn.github.io/swift-bridge/>
- cbindgen: <https://github.com/mozilla/cbindgen>

### Testing and CI

- Keep the Python CLI as the protocol oracle.
- Keep the fake HTTP device in `tests/fake_device.py`.
- Add Rust integration tests against the same fake device.
- Add UI smoke tests per platform.
- Use GitHub Actions with `macos-latest`, `windows-latest`, and `ubuntu-22.04`.
- Pin the Rust toolchain in `rust-toolchain.toml`.
- Build the Swift app on macOS only.
- Build the AppImage on Ubuntu or Fedora.
- Publish checksums with every artifact.

## Windows packaging and Inno Setup

Inno Setup is a free Windows installer generator.
It creates a single `setup.exe` with a wizard, Start Menu shortcuts, an uninstaller, and an Add/Remove Programs entry.
It is not a package format and not a runtime.
It can install an unpackaged, self-contained WinUI 3 app.
It can be signed later if a certificate appears.

`aural-keyboard` does not use Inno Setup.
Its Windows release is a ZIP plus a SHA-256 file.
The `aural system install` command copies the binary, registers autostart, and provides uninstall.
The README documents this pattern.

Options for this project:

1. ZIP plus `keylight install` and `keylight uninstall`. Follows aural-keyboard. No installer tooling.
2. Inno Setup installer. Familiar wizard and Add/Remove Programs entry. More work.
3. MSIX. Avoid, because unsigned MSIX needs sideloading or developer mode.

Recommendation: start with option 1.
Add Inno Setup later if a GUI installer is needed.
The app is a tray app, so the ZIP plus self-install is acceptable for the first release.

Resources:

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
For C# WinUI 3, set `WindowsPackageType=None` and self-contained deployment.
Distribute a ZIP with `keylight install`, or an Inno Setup installer later.

### macOS

Build an `.app` bundle.
Ad-hoc sign it with `codesign -s -`.
For a stable local identity, create a self-signed "KeyLight Code Signing" certificate in the login keychain and sign with it.
Distribute a DMG or ZIP.
Gatekeeper warns on downloaded builds.
Document `xattr -d com.apple.quarantine`.

## code signing: self-signing only

Active:

| Platform | Method | Cost |
| --- | --- | --- |
| macOS | Ad-hoc `codesign -s -` | Free |
| macOS | Self-signed "KeyLight Code Signing" identity for stable TCC | Free |
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
    keylight-core/
    keylight/
    keylight-ffi/
  apps/
    macos/
      KeyLight/
    windows/
      keylight-winui/       Rust windows-reactor
    linux/
      keylight-gtk/         Rust gtk4-rs
  packaging/
    appimage/
    windows/
  tests/
```

`keylight-core` holds:

- `LightState`, `AccessoryInfo`, `WifiInfo`, `DeviceSettings`, and `Device`.
- `KeyLightClient` with retry and timeout.
- `DeviceStore` for the JSON registry.
- `Discovery` with `mdns-sd`.
- A `snapshot()` call that fetches state and accessory info together.

`apps/linux/keylight-gtk` depends on `keylight-core` directly.
`apps/windows/keylight-winui` depends on `keylight-core` directly.
`apps/macos/KeyLight` links `keylight-ffi`.

## phases

1. Finish the protocol reference. The Python CLI and [docs/api.md](../docs/api.md) are the source of truth.
2. Build `keylight-core` and the Rust CLI.
3. Spike `windows-reactor` against the checklist.
4. Build the macOS SwiftUI app and App Intents.
5. Build the Windows app, or the C# fallback.
6. Build the Linux GTK 4 app.
7. Build the AppImage and publish it through Soar.
8. Add self-signing and release automation.
9. Keep the Python CLI as the protocol oracle.

## open questions

1. Windows: does the Reactor spike pass the borderless and no-taskbar checklist?
2. Windows packaging: ZIP plus `keylight install`, or Inno Setup from the start?
3. macOS: SwiftUI `MenuBarExtra`, or AppKit `NSStatusItem` plus `NSPopover` if the popover needs more control?
4. Linux: `ksni`, or libayatana-appindicator directly?
5. Rust FFI: UniFFI, swift-bridge, or a manual C ABI with cbindgen?
