Assisted-by: OpenCode · DeepSeek V4.1 Flash <opencode-opencodego-deepseekv41flash@local>

Companion: [1795599587000-official-client-language.prompts.md](./1795599587000-official-client-language.prompts.md)

# Official client: language, UI, and architecture

## context

Elgato Control Center removed its Apple Shortcuts actions.
The three lights still answer a local HTTP API on port 9123.
The API is cracked and verified. See [docs/api.md](../docs/api.md).
A Python CLI proves the interface and controls all three lights now.

The official client must run on macOS, Linux, and Windows.
It needs a proper card interface, not a menu-bar list.
It needs system tray integration on each platform.
It needs native Apple Shortcuts actions on macOS.

## requirements

All items are must, except Apple notarisation.

| Requirement | Priority | Note |
| --- | --- | --- |
| macOS, Linux, and Windows | Must | The user uses all three |
| Shared core across platforms | Must | One protocol implementation |
| Card interface for each light | Must | Not a menu-bar combo box |
| System tray on each platform | Must | Opens or controls the app |
| Native Apple Shortcuts actions | Must | App Intents on macOS |
| Control power, brightness, warmth | Must | The three requested properties |
| Rename | Must | Device display name |
| Power-on behaviour | Must | Restore last, or restore defaults |
| WiFi status and hover details | Must | See the data source below |
| Non-interactive CLI | Must | Shortcuts and scripts call it |
| JSON output | Must | Shortcuts parses the result |
| mDNS discovery | Must | Avoid a manual IP list |
| Signed macOS app | Must | Signing is separate from notarisation |
| Apple notarisation | Could | Not required by the user |
| Windows code signing | Could | Reduces SmartScreen warnings |

## where the WiFi data comes from

The light provides the data.
`GET /elgato/accessory-info` returns a `wifi-info` object:

```json
{
  "wifi-info": {
    "ssid": "example",
    "frequencyMHz": 2400,
    "rssi": -54
  }
}
```

| Field | Meaning |
| --- | --- |
| `ssid` | The network that the light joined |
| `frequencyMHz` | 2400 or 5000 |
| `rssi` | Signal strength in dBm |

macOS can report the Mac's own WiFi through CoreWLAN.
It cannot report each light's signal strength.
The official app must use the light's `wifi-info` for a per-light icon.

The client can show this hover text:

```text
example · 2.4 GHz · -54 dBm (good)
```

Use this mapping for the quality label:

| RSSI | Label |
| --- | --- |
| -50 or better | excellent |
| -60 or better | good |
| -70 or better | fair |
| below -70 | weak |

If the accessory request fails, the light is offline.
Show a disconnected icon and the last known values, if any.

The IP address is not in the API response.
The client knows it from the registry or from mDNS.
The client can show it as extra hover text.

## UI options

### Swift open-source

#### SwiftCrossUI

SwiftCrossUI is the main open-source Swift UI framework.
It is a SwiftUI-like declarative framework.
It has these backends:

- AppKit on macOS.
- WinUI 3 on Windows.
- GTK 4 on Linux.
- UIKit on iOS and tvOS.
- Android.

Facts from the repository at 2026-11-25:

- Latest release: v0.11.0, 2026-11-19.
- 1806 stars, 214 open issues.
- MIT license.
- Active development, with commits in the last week.
- Hover gestures and tooltips exist on the AppKit, GTK, and WinUI backends.
- Background and corner-radius modifiers exist, so cards are possible.
- The framework is pre-1.0.
- There is no system tray API.
- Swift Bundler builds macOS, Linux, and Windows apps.

Assessment:

- It can produce a proper card interface.
- It can produce the WiFi hover tooltip.
- It cannot produce a tray icon without platform-specific Swift code.
- The WinUI and GTK backends support "most" features, not all.
- App Intents still need a standard macOS app target or a helper app.
- It is the best Swift-only shared UI option, but it does not remove platform work.

#### SwiftUI on macOS plus SwiftCrossUI on Linux and Windows

This uses SwiftUI for the native macOS app and App Intents.
It uses SwiftCrossUI for the Linux and Windows app.
It gives a native macOS surface and a shared Swift UI elsewhere.

The cost is two UI code paths and the SwiftCrossUI risks above.
The macOS app can include App Intents directly.

#### SwiftGtk and SwiftWinUI

SwiftGtk gives direct GTK 4 bindings.
SwiftWinUI gives direct WinUI 3 bindings.
Both are lower-level than SwiftCrossUI.
They require separate UI code for Linux and Windows.
They are useful only for a native per-platform Swift UI.

### Rust

#### Tauri v2

Tauri v2 is a stable framework for a Rust backend and a web frontend.
It uses the system webview:

- WebView2 on Windows.
- WebKitGTK on Fedora.
- WKWebView on macOS.

It has these strengths:

- Built-in system tray.
- Built-in bundler for `.msi`, `.exe`, `.deb`, `.rpm`, `.AppImage`, `.app`, and `.dmg`.
- Mature ecosystem and large community.
- Excellent card layouts, hover states, tooltips, and animation through HTML and CSS.
- One UI codebase for all three platforms.
- The Rust backend can use `keylight-core` directly.

There is one critical constraint.
The light rejects any request with an `Origin` header with HTTP 403.
A webview request to `http://<light>:9123` carries an `Origin` header.
A JSON PUT also triggers a CORS preflight, which the light does not support.
The frontend must never call the light directly.
All calls must go through Tauri commands in the Rust backend.
The Rust HTTP client sends no `Origin` header.

Tauri is proven in the Bevry project `command-centre`.
It is the lowest-risk path to a proper cross-platform card UI.

#### Iced

Iced is a cross-platform Rust GUI library with an Elm-style model.
It has these strengths:

- One Rust codebase.
- Native rendering with `wgpu` or `tiny-skia`.
- Cards and custom widgets are possible.

It has these weaknesses:

- The README calls it experimental.
- There is no built-in tooltip widget.
- There is no built-in system tray. The `tray-icon` crate can add one.
- Accessibility is limited.

Iced fits a Rust-only UI, but it adds work for the WiFi hover and the tray.

#### egui

egui is an immediate-mode Rust GUI library.
It has these strengths:

- Tooltips on hover.
- Easy card-like layouts with frames and rounding.
- Cross-platform native and web.
- AccessKit support on Windows and macOS.

It has these weaknesses:

- It does not aim for a native look.
- AccessKit support for Linux is not complete.
- There is no built-in system tray. The `tray-icon` crate can add one.
- The interface is not retained-mode, so complex layouts need more care.

egui is a good tool UI, but it is less suitable for a polished consumer app.

#### GTK4-rs

GTK4-rs gives native GTK 4 bindings in Rust.
It has these strengths:

- Native Fedora and GNOME integration with libadwaita.
- Cards, hover, and tooltips are native GTK features.
- One Rust codebase can also build on Windows and macOS.

It has these weaknesses:

- GTK on Windows looks foreign and needs a large runtime.
- A tray icon needs libappindicator or StatusNotifier.
- GNOME needs the AppIndicator extension for a tray icon.
- macOS GTK looks foreign.

GTK4-rs is the best choice for a native Fedora-only UI.
It is weaker as the shared UI for all three platforms.

#### Other Rust options

- Dioxus desktop uses a webview and is less mature than Tauri.
- Slint uses GPLv3 or a commercial license. Check license compatibility before use.
- Freya and Makepad are promising but less mature for a form-heavy app.
- Xilem is experimental.

### Native options that consume the Rust core

#### Windows: WinUI 3

WinUI 3 with the Windows App SDK is the modern native Windows UI.
It has these strengths:

- Native Windows look and feel.
- Cards, hover, and tooltips are native controls.
- Good accessibility.
- MSIX packaging.

It has these weaknesses:

- No built-in tray icon. Use Win32 `Shell_NotifyIcon` or a library.
- A separate UI codebase from macOS and Fedora.
- C# or C++ is a third language in the project.
- The Rust core is consumed through a C ABI, a local service, or the CLI.

#### Fedora: GTK 4 and libadwaita

GTK 4 with libadwaita is the native Fedora and GNOME UI.
It has these strengths:

- Native GNOME look and feel.
- Cards, hover, and tooltips are native GTK features.
- Good accessibility.
- The UI can be written in Rust with gtk-rs and link the core directly.
- RPM or Flatpak packaging.

It has these weaknesses:

- A separate UI codebase from macOS and Windows.
- A tray icon needs libappindicator or StatusNotifier.
- GNOME needs the AppIndicator extension for a tray icon.
- Flatpak adds portal and network complexity.

#### Native per platform

This means SwiftUI on macOS, WinUI 3 on Windows, and GTK 4 on Fedora.
It gives the best integration on each platform.
It costs three UI codebases and three release pipelines.

### Other cross-platform options

#### Avalonia

Avalonia is a mature .NET UI framework for Windows, Linux, and macOS.
It has a built-in tray icon.
It can produce cards, hover, and tooltips.
It consumes the Rust core through P/Invoke.
It adds .NET and C# to the project.
It is a strong choice if the team accepts C#.

#### Flutter

Flutter is a mature UI toolkit for Windows, Linux, and macOS.
It has tray plugins and excellent card and animation support.
It consumes the Rust core through FFI.
It adds Dart and the Flutter runtime.
It is a strong choice if the team accepts Dart.

## comparison

Scores run from 1 (weak) to 5 (strong).

| Option | Card UI | Hover tooltip | Tray | Native look | One UI codebase | Maturity | App Intents | Overall risk |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| SwiftCrossUI | 4 | 4 | 1 | 5 | 5 | 2 | 2 | High |
| SwiftUI macOS + SwiftCrossUI | 4 | 4 | 2 | 5 | 3 | 2 | 5 | High |
| Tauri v2 | 5 | 5 | 5 | 2 | 5 | 5 | 2 | Low |
| Iced | 4 | 2 | 2 | 3 | 5 | 2 | 2 | High |
| egui | 4 | 5 | 2 | 2 | 5 | 4 | 2 | Medium |
| GTK4-rs | 5 | 5 | 3 | 4 | 4 | 5 | 2 | Medium |
| WinUI 3 + GTK 4 + SwiftUI | 5 | 5 | 5 | 5 | 1 | 5 | 5 | High effort |
| Avalonia | 4 | 4 | 4 | 3 | 5 | 5 | 2 | Medium |
| Flutter | 5 | 5 | 4 | 2 | 5 | 5 | 2 | Medium |

Notes:

- "Tray" for GTK and Tauri on Fedora assumes the AppIndicator extension on GNOME.
- "App Intents" means native Shortcuts actions without a separate helper.
- SwiftCrossUI's App Intents need a standard app target or a helper.
- Tauri's App Intents need a Swift helper.

## recommendation

Use a shared Rust core.
Use SwiftUI for the macOS app.
Use Tauri v2 for the Windows and Fedora app.

```text
crates/keylight-core     protocol, config, discovery
crates/keylight          CLI
crates/keylight-ffi      C ABI for the macOS app
apps/desktop             Tauri v2 app for Windows and Fedora
apps/macos               SwiftUI app with App Intents
```

Why this choice:

- The core is shared across all three platforms.
- macOS gets a native app and native App Intents with no helper.
- Windows and Fedora get a proper card UI, hover tooltips, and a tray from one Tauri codebase.
- The macOS app is small and the Tauri app is the larger surface.
- The Python CLI stays as the reference implementation and test oracle.

The cost is two UI codebases.
The alternative is Tauri on all three platforms plus a Swift App Intents helper.
That alternative shares the UI code, but the macOS UI is web-based and the helper is a second app.
Choose it only if one UI codebase matters more than a native macOS app.

### why not SwiftCrossUI as the shared UI

SwiftCrossUI is credible and active.
It can do cards, hover, and tooltips.
It cannot do a tray icon without three native shims.
It is pre-1.0 and has a smaller ecosystem.
It does not remove the platform work, and it adds risk to the main interface.

Use it only if the team wants a Swift-only UI and accepts the tray shims.

### why not native per platform

Native WinUI 3, GTK 4, and SwiftUI give the best integration.
They also cost three UI codebases, three release pipelines, and three sets of tests.
The card interface is simple enough that one Tauri UI is a better return on effort.

Choose native per platform only if native look on Windows and Fedora is a hard requirement.

### why not Zig

A Zig core can expose a C ABI that Swift and Rust both link.
It does not remove the tray work or the App Intents work.
Its HTTP, JSON, and mDNS libraries are less mature.
It adds a third ecosystem without removing work.

## proposed structure

```text
elgato-key-light/
  Cargo.toml
  crates/
    keylight-core/
    keylight/
    keylight-ffi/
  apps/
    desktop/            Tauri v2
      src-tauri/
      src/
    macos/              SwiftUI
      KeyLight/
  tests/
```

`keylight-core` holds these parts:

- `LightState`, `AccessoryInfo`, `WifiInfo`, `DeviceSettings`, and `Device`.
- `KeyLightClient` with retry and timeout.
- `DeviceStore` for the JSON registry.
- `Discovery` with `mdns-sd`.
- A `snapshot()` call that fetches state and accessory info together.

`keylight` mirrors the Python commands:

```sh
keylight status [--json]
keylight on [--all|NAME]
keylight off [--all|NAME]
keylight toggle [--all|NAME]
keylight brightness PERCENT [NAME]
keylight warmth PERCENT [NAME]
keylight set --on --brightness 60 --warmth 40 [NAME]
keylight rename NAME "New Name"
keylight power-on [NAME]
keylight power-on --last [NAME]
keylight power-on --defaults [--brightness N] [--warmth N] [NAME]
keylight identify [NAME]
keylight discover
```

`apps/desktop` is a Tauri v2 app.
The frontend uses a card grid.
Each card shows the name, power, brightness, warmth, and a WiFi icon.
The WiFi icon has a hover tooltip with SSID, frequency, RSSI, and quality.
The app has a tray icon with Open, All On, All Off, and Quit.
All HTTP calls go through Tauri commands.

`apps/macos` is a SwiftUI app.
It has the same card grid and WiFi tooltip.
It has a `MenuBarExtra` tray item.
It includes App Intents for these actions:

- Turn a light on.
- Turn a light off.
- Toggle a light.
- Set brightness.
- Set warmth.
- Get the state of a light.
- Rename a light.
- Set the power-on behaviour.

The app declares these Info.plist keys:

- `NSLocalNetworkUsageDescription`.
- `NSBonjourServices` with `_elg._tcp`.

`keylight-ffi` exposes the core to Swift.
The first version can use the CLI instead of FFI.
FFI is cleaner for a native app and avoids `Process` overhead.

## platform notes

- macOS: the SwiftUI app needs local network permission.
- macOS: App Intents are part of the app, so no helper is needed.
- Windows: Tauri needs WebView2. Windows 11 includes it. Windows 10 may need the Evergreen runtime.
- Windows: the firewall may prompt on first network access.
- Fedora: Tauri needs `webkit2gtk4.1-devel` and other build packages.
- Fedora GNOME: the tray needs the AppIndicator extension.
- Fedora: a native RPM avoids the network and portal complexity of Flatpak.
- All platforms: the CLI run from a terminal inherits the terminal network permission.

## packaging

- macOS: Xcode archive for the SwiftUI app. Sign it. Notarisation is optional.
- Windows: Tauri bundles `.msi` and `.exe`. Code signing is optional.
- Fedora: Tauri bundles `.rpm` and `.deb`, and can build an AppImage.
- CLI: prebuilt binaries for each platform and a Homebrew formula.
- Python CLI: keep it for tests and protocol reference.

## phases

1. Build `keylight-core` and the Rust CLI. Match the Python CLI, including rename, power-on, and WiFi info.
2. Build the Tauri app for Windows and Fedora. Add cards, hover tooltips, tray, rename, and power-on.
3. Build the SwiftUI macOS app. Add cards, hover tooltips, tray, and App Intents.
4. Add `keylight-ffi` and switch the macOS app from the CLI to the library.
5. Add packaging and release workflows.
6. Keep the Python CLI as the protocol oracle.

## open questions

1. Do you accept two UI codebases for a native macOS app, or do you want Tauri on all three plus a Swift helper?
2. Is native look on Windows and Fedora a hard requirement, or is a polished Tauri UI enough?
3. Do you want the macOS app to be a menu-bar app with a window, or a normal window app with a menu-bar extra?
4. Which Linux package format do you want first: RPM, Flatpak, or AppImage?
5. Do you want Windows code signing, or is an unsigned installer acceptable?
