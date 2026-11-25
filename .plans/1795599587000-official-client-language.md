Assisted-by: OpenCode · DeepSeek V4.1 Flash <opencode-opencodego-deepseekv41flash@local>

Companion: [1795599587000-official-client-language.prompts.md](./1795599587000-official-client-language.prompts.md)

# Official client: native per-platform UI with a Rust core

## context

Elgato Control Center removed its Apple Shortcuts actions.
The three lights still answer a local HTTP API on port 9123.
The API is cracked and verified. See [docs/api.md](../docs/api.md).
A Python CLI proves the interface and controls all three lights now.

The official client must run on macOS, Linux, and Windows.
It must be a native app, not a webview.
It must be pinned to the menu bar or system tray.
It must have no Dock icon and no draggable window.
It must have a card interface for each light.
It must provide native Apple Shortcuts actions on macOS.

## decision

Use a shared Rust core with native UIs on each platform:

| Platform | UI | Language |
| --- | --- | --- |
| Core and CLI | `keylight-core` and `keylight` | Rust |
| macOS | SwiftUI menu-bar app with App Intents | Swift |
| Windows | WinUI 3 app | Rust `windows-reactor` (spike), or C# WinUI 3 (fallback) |
| Fedora | GTK 4 + libadwaita app | Rust `gtk4-rs` |

The UI is simple: a card grid, a settings sheet, and a tray menu.
Three native UIs are acceptable, and the project is a learning exercise for native cross-platform work.

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
| Ad-hoc macOS signing | Must | Free |
| Apple notarisation | Could | Paid, so not required |
| Paid Windows signing | Could | Free OSS option exists |

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

## Windows choices

### C# WinUI 3

This is the most mature and best-supported native Windows UI.

Strengths:

- Stable WinUI 3 and Windows App SDK.
- Best tooling in Visual Studio.
- Native controls, accessibility, and MSIX or unpackaged deployment.
- Large ecosystem.

Weaknesses:

- Adds C# and .NET to the project.
- WinUI 3 has no built-in tray icon. Use `H.NotifyIcon` or Win32 `Shell_NotifyIcon`.
- A borderless popover near the tray needs `Shell_NotifyIconGetRect`.

This is the safe choice.

### Rust `windows-reactor`

`windows-reactor` is a Microsoft-backed declarative WinUI 3 library for Rust.
It is part of `windows-rs`.
The current version is 0.101, and it is a preview.

Strengths:

- Native WinUI 3 controls.
- Declarative component and view model, similar to React.
- Typed messages and state.
- Built-in `ApplicationView::notify_icon` tray icon.
- `Tooltip` support for the WiFi hover.
- Self-contained deployment with `windows-reactor-setup`.
- Keeps the Windows UI in Rust, so the core and UI share one language.
- Good learning value for native Rust on Windows.

Weaknesses:

- Preview. Public APIs may change.
- New ecosystem and fewer examples.
- Some rough edges are still open, such as GDI handle leaks in certain window cases.
- Requires the Windows App Runtime for framework-dependent builds.

This is the most interesting choice for a Rust learning repo.
Spike it before committing:

1. Build a window with a card and a tray icon.
2. Show a tooltip on hover.
3. Test unpackaged self-contained deployment.
4. Test Windows 10 and Windows 11.

If the spike passes, use it.
If it fails, fall back to C# WinUI 3.

### C++/WinRT

Stable, native, and no .NET.
It is the best choice if C# is unacceptable and Rust is not ready.
It is more verbose than C#.
The tray still needs Win32 `Shell_NotifyIcon`.

### Rust Win32 or native-windows-gui

The `windows` crate gives direct Win32 access.
`native-windows-gui` wraps Win32 controls.
Both are stable and native, but they are low-level.
They are not WinUI 3.

### Windows recommendation

Spike `windows-reactor` first.
It keeps the Windows UI in Rust and has a built-in tray.
Fall back to C# WinUI 3 if the spike is unstable.

## Fedora choices

GNOME's documentation says the platform libraries are written in C and use GObject-Introspection.
C is the default platform language.
Applications can use any language with GNOME bindings.

| Language | Project | Notes |
| --- | --- | --- |
| C | GTK, GLib | Reference language. Recommended for libraries. Verbose for apps. |
| Vala | Vala | GNOME's own language. GObject-friendly. Compiles to C. Used by Calculator, Boxes, Clocks, Gitg. |
| Rust | gtk-rs | Supported binding. Memory-safe. Used by Authenticator, Shortwave, Video Trimmer. |
| Python | PyGObject | Fast development. Runtime dependency. Used by Music, Lollypop, Pitivi. |
| JavaScript | GJS | Used by Maps, Polari, Sound Recorder. |
| C++ | gtkmm | Mature. Used by Gnote, GParted, Inkscape. |
| C# | Gir.core | Used by Denaro, Parabolic, Tagger. |
| Java | Java-GI | Works with OpenJDK 25 or later. |
| Perl | Glib::Object::Introspection | Supported binding. |

There is no single recommended application language.
GNOME recommends C for libraries, because it gives the widest binding support.

For this project, use **Rust with gtk4-rs and libadwaita**.

Why:

- It links `keylight-core` directly. No FFI or sidecar.
- It is memory-safe and compiled.
- It is a supported GNOME binding with real GNOME apps.
- It keeps the Linux UI and the core in one language.
- The UI is simple, so Rust's verbosity is acceptable.

Vala would be the most idiomatic GNOME choice, but it adds a language and an FFI layer.
Python would be the fastest to write, but it adds a runtime and FFI.
C would be the reference choice, but it is more work and less safe.

## macOS choices

Use SwiftUI with `MenuBarExtra` and `.menuBarExtraStyle(.window)`.

- `LSUIElement=true` hides the Dock icon.
- `MenuBarExtra` pins the interface to the menu bar.
- The window is a popover, not a draggable window.
- App Intents are part of the app. No helper is needed.
- The app links `keylight-ffi` as a static library.

AppKit `NSStatusItem` plus `NSPopover` is the fallback if `MenuBarExtra` cannot express the popover.

## why not a shared UI

### Kotlin Multiplatform

Compose Multiplatform is a native binary, but it is not native widgets.
It renders with Skia and Material.
It cannot register App Intents, so macOS still needs a Swift helper.
Hiding the Dock icon and anchoring the popover need JVM/AWT workarounds.
It adds a JVM runtime.
It does not meet the native requirement.

### SwiftCrossUI

SwiftCrossUI is active and capable.
It has hover, tooltips, and card layouts.
But it has no tray API, and it is pre-1.0.
Its LLM policy prevents an upstream tray contribution.
A local tray shim costs 1–2 days on macOS, 3–5 days on Windows, and 5–10 days on Fedora.
That is a project, not a patch.
It does not meet the tray requirement without significant extra work.

### Tauri and web

Rejected by the user.
No webview, no web UI.

## the menu-bar-pinned interface

### macOS

SwiftUI `MenuBarExtra` with a window style.
`LSUIElement` for no Dock icon.
No title bar, no resize, no drag.
App Intents for Shortcuts.

### Windows

`windows-reactor` `ApplicationView::notify_icon` for the tray.
A borderless window positioned with `Shell_NotifyIconGetRect` if Reactor does not expose the anchor.
Hide the window from the taskbar and Alt-Tab.
No title bar, no resize, no drag.

If C# is used, `H.NotifyIcon` plus a borderless `AppWindow`.

### Fedora

GTK 4 window with `skip_taskbar_hint`, `decorated=false`, `resizable=false`.
A status notifier item through `ksni` or libayatana-appindicator.
The menu opens the card window.
GNOME needs the AppIndicator extension.
`aural-keyboard` already uses this extension, so it is an accepted dependency.

The exact tray icon anchor is not available through the status notifier protocol.
Position the window near the top-right of the screen.
This is the one platform where the pin is approximate.

## packaging

### AppImage via Soar, now

Build a GTK 4 AppImage with the required libraries.
Publish it through Soar.
Soar supports AppImage and delta updates through zsync.
This is the fastest path and avoids Flatpak sandbox work.

### Flatpak, later

Flatpak is shelved for now.
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
Distribute a ZIP or an Inno Setup installer.
Avoid MSIX, because unsigned MSIX needs sideloading or developer mode.

### macOS

Build a `.app` bundle.
Ad-hoc sign it with `codesign -s -`.
Distribute a DMG or ZIP.
Gatekeeper warns on downloaded builds.
Document `xattr -d com.apple.quarantine`.

## code signing costs

| Platform | Option | Cost | Notes |
| --- | --- | --- | --- |
| macOS | Ad-hoc signature | Free | No notarisation. Gatekeeper warns. |
| macOS | Apple Developer Program | $99/year | Needed for Developer ID and notarisation. |
| Windows | Unsigned | Free | SmartScreen warning. |
| Windows | Self-signed certificate | Free | Not trusted by other machines. |
| Windows | OV certificate | $150–$300/year | SmartScreen reputation builds over time. |
| Windows | EV certificate | $300–$800/year | Hardware token or cloud HSM. |
| Windows | Azure Trusted Signing | $9.99/month Basic | 5,000 signatures. Eligibility rules apply. |
| Windows | SignPath Foundation | Free for OSS | OV-level Authenticode. Conditions apply. |
| Linux | None | Free | Soar uses checksums. Flathub signs its repo. |

### SignPath Foundation

SignPath Foundation provides free code signing for open-source projects.
The project must have:

- An OSI-approved license.
- No commercial dual-licensing.
- No proprietary components.
- Active maintenance.
- A public release.
- A code signing policy on the project home page.
- MFA and defined author, reviewer, and approver roles.
- Reproducible builds from source.

RPL-1.5 is OSI-approved.
The project meets the license condition.
Acceptance is discretionary and depends on project reputation.
Apply after the first public release.

If accepted, Windows builds are signed for free.
Until then, ship unsigned.

Do not buy a certificate.
The paid options are listed only for completeness.

## proposed structure

```text
elgato-key-light/
  Cargo.toml
  crates/
    keylight-core/
    keylight/
    keylight-ffi/
  apps/
    macos/
      KeyLight/
    windows/
      keylight-winui/       Rust windows-reactor spike
      keylight-winui-cs/    C# WinUI 3 fallback
    linux/
      keylight-gtk/         Rust gtk4-rs
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
The C# fallback uses P/Invoke to `keylight-ffi`.

## phases

1. Port the Python CLI to `keylight-core` and the Rust CLI.
2. Build the macOS SwiftUI app and App Intents.
3. Spike `windows-reactor`.
4. Build the Windows WinUI 3 app, or the C# fallback.
5. Build the Linux GTK 4 app.
6. Build the AppImage and publish it through Soar.
7. Apply to SignPath Foundation after the first release.
8. Keep the Python CLI as the protocol oracle.

## open questions

1. Windows: spike `windows-reactor` first, or go straight to C# WinUI 3?
2. Linux: Rust gtk4-rs, or Vala for the most idiomatic GNOME app?
3. macOS: SwiftUI `MenuBarExtra`, or AppKit `NSStatusItem` plus `NSPopover`?
4. Windows packaging: ZIP, or Inno Setup installer?
5. Apply to SignPath Foundation now, or after the first public release?
