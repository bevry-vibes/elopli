Assisted-by: OpenCode · DeepSeek V4.1 Flash <opencode-opencodego-deepseekv41flash@local>

Companion: [1795599587000-official-client-language.prompts.md](./1795599587000-official-client-language.prompts.md)

# Official client language

## context

Elgato Control Center removed its Apple Shortcuts actions.
The three lights still answer a local HTTP API on port 9123.
The API is cracked and verified. See [docs/api.md](../docs/api.md).
A Python CLI proves the interface and controls all three lights now.

The next decision is the language for the official client.
The candidates are Swift, Rust, and Zig.

## what the API requires

The client needs these capabilities:

- HTTP/1.1 over plain TCP, with JSON request and response bodies.
- A PUT request that carries a partial light object.
- mDNS browse for `_elg._tcp.local`.
- Host name resolution for `<name>.local`.
- Retry on a dropped connection.
- Parallel requests to three devices.
- Input validation for brightness (0 to 100) and mired (143 to 344).

The API has no authentication and no encryption.
The client needs no secret store and no cloud service.

## requirements

| Requirement | Priority | Note |
| --- | --- | --- |
| Control on/off, brightness, and warmth | Must | The three requested properties |
| Run from Apple Shortcuts | Must | The reason for the project |
| Non-interactive CLI | Must | Shortcuts calls a shell command |
| JSON output | Must | Shortcuts parses the result |
| mDNS discovery | Should | Avoid a manual IP list |
| Menu-bar app | Could | A later surface |
| Cross-platform CLI | Could | macOS is the current target |
| Native Shortcuts actions | Could | App Intents, not a shell script |
| Small binary and fast start | Could | Nice for Shortcuts latency |
| Signed and notarised distribution | Should | Required for a public app |

## options

### Swift

Swift has the strongest Shortcuts story.
The App Intents framework creates real Shortcuts actions.
The actions appear in the Shortcuts editor with typed parameters.
This is the exact feature that Control Center removed.

Swift also has native support for the other needs:

- `URLSession` for HTTP.
- `Codable` for JSON.
- `Network.framework` `NWBrowser` for Bonjour.
- `SwiftUI` or `AppKit` for a menu-bar app.

A Swift package can hold a shared `KeyLightKit` library, a `keylight` CLI, and an app target.
The CLI and the app use the same protocol code.

The costs are these:

- macOS only. Linux support needs swift-corelibs-foundation and loses `Network.framework`.
- App Intents need an app bundle. A bare CLI cannot register Shortcuts actions.
- A SwiftUI menu-bar app needs an Xcode project, signing, and notarisation.

The CLI can use the shell-script bridge in Shortcuts today.
The app can add native actions later.

### Rust

Rust has the strongest CLI story.
The ecosystem covers every need:

- `clap` for arguments.
- `serde` for JSON.
- `ureq` or `reqwest` for HTTP.
- `mdns-sd` for discovery.
- `tokio` for parallel requests.

A Rust binary is small, fast to start, and easy to distribute.
It runs on macOS, Linux, and Windows.

The cost is the Shortcuts integration.
Rust has no App Intents.
Shortcuts must call the CLI through "Run Shell Script".
That works, but the action is a shell command, not a native action with typed fields.
A native action needs a Swift or Objective-C host app.

Tauri v2 can build a menu-bar app in Rust.
The Bevry build skill favours Tauri v2 for this case.
The app still cannot register App Intents.

### Zig

Zig has the smallest runtime and the best C interop.
It can call `DNSServiceBrowse` and `libcurl` directly.
`std.http.Client` and `std.json` cover the protocol.

The costs are these:

- The HTTP and JSON libraries are less mature.
- mDNS needs a hand-written client or the libdns_sd C API.
- Zig 0.16 still has breaking changes.
- There is no App Intents support.
- Fewer examples exist for macOS app bundles and local network permission.

Zig fits a tiny, dependency-free CLI.
It does not fit the Shortcuts goal as well as Swift.

## comparison

Scores run from 1 (weak) to 5 (strong).

| Criterion | Weight | Swift | Rust | Zig |
| --- | --- | --- | --- | --- |
| Native Shortcuts actions | high | 5 | 1 | 1 |
| CLI quality | high | 4 | 5 | 3 |
| macOS integration | high | 5 | 4 | 3 |
| Cross-platform | low | 2 | 5 | 5 |
| Ecosystem maturity | high | 5 | 4 | 2 |
| Build simplicity | medium | 3 | 4 | 2 |
| Binary size and start time | low | 3 | 4 | 5 |
| Menu-bar app path | medium | 5 | 4 | 1 |
| Local network permission | high | 5 | 4 | 3 |

Swift wins on the requirement that caused the project.
Rust wins on the CLI and portability.

## recommendation

Use Swift for the official client.

The project exists because Shortcuts lost native actions.
Swift restores those actions with App Intents.
A Rust or Zig client can control the lights, but it cannot restore the native actions without a Swift host app.

Keep the Python CLI as the reference client and the test oracle.
It documents the protocol and verifies a new client against the same behaviour.

### proposed architecture

```text
elgato-key-light/
  Package.swift
  Sources/
    KeyLightKit/        protocol, models, config, discovery
    keylight/           CLI executable
  App/
    KeyLightApp/        menu-bar app with App Intents
  Tests/
    KeyLightKitTests/   unit tests
    IntegrationTests/   fake device and live device tests
```

`KeyLightKit` holds these parts:

- `LightState`, `AccessoryInfo`, and `Device`.
- `KeyLightClient` with retry and timeout.
- `DeviceStore` for the JSON registry.
- `Discovery` with `NWBrowser` for `_elg._tcp`.

`keylight` mirrors the Python commands:

```sh
keylight status [--json]
keylight on [--all|NAME]
keylight off [--all|NAME]
keylight toggle [--all|NAME]
keylight brightness PERCENT [NAME]
keylight warmth PERCENT [NAME]
keylight set --on --brightness 60 --warmth 40 [NAME]
keylight identify [NAME]
keylight discover
```

`KeyLightApp` is a menu-bar agent.
It has no Dock icon.
It adds App Intents for these actions:

- Turn a light on.
- Turn a light off.
- Toggle a light.
- Set brightness.
- Set warmth.
- Get the state of a light.

The app declares these Info.plist keys:

- `NSLocalNetworkUsageDescription`.
- `NSBonjourServices` with `_elg._tcp`.

The CLI launched from Terminal inherits the Terminal local network grant.
A standalone app needs its own grant.
A bare SwiftPM CLI that browses Bonjour can embed an Info.plist with `-sectcreate`.

### phases

1. Build `KeyLightKit` and the Swift CLI. Match the Python CLI behaviour.
2. Add the menu-bar app and the App Intents actions.
3. Add Homebrew distribution and notarisation.
4. Keep the Python CLI for protocol regression tests.

### short-term bridge

The Python CLI already works from Shortcuts through "Run Shell Script".
Use the absolute path to the installed binary.
The command is non-interactive and supports `--json`.

```sh
/Users/balupton/.local/bin/keylight on --all
```

This bridge needs no app bundle and no signing.

## open questions

1. Is the official client macOS only, or must it run on Linux and Windows?
2. Must the Shortcuts actions be native App Intents, or is "Run Shell Script" enough?
3. Is a menu-bar app in scope, or only a CLI?
4. Which distribution channel do you want: Homebrew, direct download, or the App Store?
5. Should the Python CLI stay as the reference implementation?
