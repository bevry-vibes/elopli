Assisted-by: OpenCode · DeepSeek V4.1 Flash <opencode-opencodego-deepseekv41flash@local>

Companion: [1795599587000-official-client-language.prompts.md](./1795599587000-official-client-language.prompts.md)

# Official client: language and architecture

## context

Elgato Control Center removed its Apple Shortcuts actions.
The three lights still answer a local HTTP API on port 9123.
The API is cracked and verified. See [docs/api.md](../docs/api.md).
A Python CLI proves the interface and controls all three lights now.

The official client must run on macOS, Linux, and Windows.
The macOS build must also expose Apple Shortcuts actions.
The client must support these properties:

- Power on and off.
- Brightness.
- Warmth.
- Rename.
- Power-on behaviour: restore the last setting, or restore the defaults.

## what the API requires

- HTTP/1.1 over plain TCP, with JSON request and response bodies.
- A PUT request that carries a partial light object.
- A full settings object for the power-on behaviour.
- mDNS browse for `_elg._tcp.local`.
- Host name resolution for `<name>.local`.
- Retry on a dropped connection.
- Parallel requests to three devices.
- Input validation for brightness, mired, and the display name.

The API has no authentication and no encryption.
The client needs no secret store and no cloud service.

## requirements

| Requirement | Priority | Note |
| --- | --- | --- |
| macOS, Linux, and Windows | Must | The user uses all three |
| Shared core across platforms | Must | One protocol implementation |
| Control power, brightness, warmth | Must | The three requested properties |
| Rename | Must | Device display name |
| Power-on behaviour | Must | Restore last, or restore defaults |
| Native Apple Shortcuts actions | Should | The reason for the project |
| System tray on each platform | Should | A visual surface |
| Non-interactive CLI | Must | Shortcuts and scripts call it |
| JSON output | Must | Shortcuts parses the result |
| mDNS discovery | Should | Avoid a manual IP list |
| Signed and notarised macOS app | Should | Required for public distribution |

## language analysis

### Swift

Swift is the best language for the macOS surface.
App Intents creates real Shortcuts actions with typed parameters.
SwiftUI and AppKit create a native menu-bar app.
`URLSession`, `Codable`, and `Network.framework` cover the protocol.

Swift is weaker as the shared base:

- SwiftUI and AppKit are Apple-only.
- App Intents are Apple-only.
- Linux uses `FoundationNetworking` for `URLSession`.
- Windows support is official, but it lags Apple platforms.
- Windows needs bundled curl and zlib for Foundation.
- Bonjour needs a C library or custom code on Linux and Windows.

Swift can hold a shared protocol core on all three platforms.
It cannot hold a shared user interface.
It cannot create Shortcuts actions off macOS.

### Rust

Rust is the best language for the shared base.
The ecosystem covers every need:

- `clap` for arguments.
- `serde` for JSON.
- `ureq` or `reqwest` for HTTP.
- `mdns-sd` for discovery.
- Threads or `tokio` for parallel requests.
- `tray-icon` for a system tray.

A single Rust workspace can build the core, the CLI, and the tray app for all three platforms.
The Bevry project `aural-keyboard` already uses this pattern.
It is one Rust codebase with a native menu-bar app on macOS, a system tray on Linux, and an engine on Windows.
Its `scripts/package-app.sh` builds a signed `.app` bundle for macOS.

Rust cannot register App Intents.
Shortcuts need a Swift host app.

### Zig

Zig is the smallest runtime and has good C interop.
It can build a static library with a C ABI.
Swift and Rust can both link that library.

Zig is weaker as the shared base:

- The HTTP and JSON libraries are less mature.
- mDNS needs a hand-written client or platform C APIs.
- Zig 0.16 is pre-1.0 and still changes.
- The tray and UI work remains per platform.
- The macOS Shortcuts host still needs Swift.

A Zig core would share protocol code with Swift and Rust.
It would not share the user interface or the Shortcuts host.
It adds a third ecosystem and more platform work.

## does Swift work on Linux and Windows?

For the core, yes.
Use `Foundation` and `FoundationNetworking`.
Use a C library for Bonjour.
The Windows toolchain is official, but it is less mature.

For the user interface, no.
SwiftUI and AppKit do not exist on Linux or Windows.
App Intents do not exist outside macOS.

Swift is a good macOS surface, not a cross-platform base.

## would a Zig base help?

A Zig base would give Swift and Rust one shared library.
That is technically sound.
It does not solve the two hard parts:

- The system tray on three platforms.
- The native Shortcuts host on macOS.

Rust already solves the first part.
Swift already solves the second part.

A Zig base adds work without removing work.
It is not the shortest path for this project.

## architecture options

| Option | Shared core | macOS surface | Linux and Windows surface | Shortcuts | Cost |
| --- | --- | --- | --- | --- | --- |
| A. Swift everywhere | Swift core | SwiftUI | Per-OS UI | App Intents | Two or three UI codebases |
| B. Zig core | Zig C ABI | SwiftUI | Rust or per-OS UI | Swift host | Three ecosystems |
| C. Rust core and tray | Rust | Rust tray | Rust tray | Swift host | Two languages, one UI codebase |
| D. Tauri v2 | Rust | Tauri tray | Tauri tray | Swift host | Web UI plus Swift host |
| E. Rust core, Swift macOS UI | Rust | SwiftUI | Rust tray | App Intents | Two UI codebases |

Option C shares the most code.
Option E gives the most native macOS feel.

## recommendation

Use a Rust workspace for the shared core, the CLI, and the tray app.
Add a small Swift app for the macOS App Intents actions.

This choice follows these facts:

- Rust gives one protocol implementation for all three platforms.
- Rust gives one tray implementation for all three platforms.
- Swift gives native Shortcuts actions on macOS.
- The Python CLI stays as the reference client and test oracle.

Do not use Zig as the base.
Use Zig only if the goal is a small experimental core, and accept the extra platform work.

### proposed structure

```text
elgato-key-light/
  Cargo.toml
  crates/
    keylight-core/      protocol, models, config, discovery
    keylight/           CLI
    keylight-tray/      tray app for macOS, Linux, and Windows
  apps/
    macos/
      KeyLightShortcuts/  Swift app with App Intents
  docs/
  tests/
```

`keylight-core` holds these parts:

- `LightState`, `AccessoryInfo`, `DeviceSettings`, and `Device`.
- `KeyLightClient` with retry and timeout.
- `DeviceStore` for the JSON registry.
- `Discovery` with `mdns-sd` for `_elg._tcp`.

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

`keylight-tray` is a native tray app.
It uses the aural-keyboard pattern:

- `tray-icon` for the tray.
- A small menu with the devices and the common actions.
- Login autostart per platform.
- A `.app` bundle on macOS for local network permission.

`KeyLightShortcuts` is a macOS app with no Dock icon.
It adds App Intents for these actions:

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

### how the Swift host talks to the core

Two options:

1. Embed the `keylight` CLI in `KeyLightShortcuts.app/Contents/Helpers/` and call it with `Process`.
2. Link `keylight-core` as a static library with a C ABI and call it from Swift.

Option 1 is the first step.
It keeps one implementation, one test suite, and one set of retry rules.
The helper inherits the app local network permission.
The app signs the helper with the same identity.

Option 2 is an optimisation for later.
It removes the process start and the helper binary.
It adds an FFI layer and a second build step.

### platform notes

- macOS: the tray app and the Swift host need local network permission.
- macOS: a bare SwiftPM CLI that browses Bonjour can embed an Info.plist with `-sectcreate`.
- Linux: the tray needs the AppIndicator extension on GNOME. The aural-keyboard README documents this.
- Windows: the tray uses the native notification area. The CLI needs no permission.
- All platforms: the CLI run from a terminal inherits the terminal network permission.

### phases

1. Build `keylight-core` and the Rust CLI. Match the Python CLI, including rename and power-on.
2. Build `keylight-tray` with the aural-keyboard pattern.
3. Build `KeyLightShortcuts` with the App Intents actions.
4. Add packaging, signing, notarisation, and Homebrew.
5. Keep the Python CLI for protocol regression tests.

## open questions

1. Should the macOS tray use the shared Rust tray, or a native SwiftUI menu bar?
2. Must the Shortcuts actions be native App Intents, or is "Run Shell Script" enough?
3. Is a system tray required on Windows, or is the CLI enough there?
4. Which distribution channels do you want: Homebrew, prebuilt binaries, or the App Store?
5. Should the Python CLI stay as the reference implementation?
