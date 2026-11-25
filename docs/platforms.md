# Platforms

## Current status

| Platform | Status | UI | Packaging |
| --- | --- | --- | --- |
| macOS 14+ | Implemented and hardware-tested | SwiftUI `MenuBarExtra`, `LSUIElement`, App Intents | Local app bundle; Developer ID/notarization still open |
| Windows 10/11 | Planned | Rust `windows-reactor` | Portable ZIP and Scoop; Authenticode still open |
| Linux (Fedora/Ubuntu) | Planned | Rust `gtk4-rs` with `ksni` | AppImage through Soar; Flatpak is shelved |

The Rust core and CLI compile for all three platforms.
The macOS UI is the current behavioural reference.
It is not a shared UI implementation: grouping, live slider writes, registry reloads, and discovery rows still live in Swift and must be extracted before the Windows/Linux UIs are built.

## Shared rules

- The API is plain HTTP on TCP 9123 with no authentication or encryption.
- Use it only on a trusted local network.
- Never store, print, or commit a serial number or MAC address.
- Discovery requires IPv4 mDNS; devices without an IPv4 address are not added.
- Animations require a persistent native HTTP connection and never use `curl`.
- A superseded animation restores the original state before the next action writes.

## Per-platform work

### macOS

- `./scripts/build-macos.sh` with a development team for App Intents.
- `CODE_SIGNING_ALLOWED=NO` is supported for CI-only builds.
- Accessibility and localization are deferred until after v1.
- Release work: universal binary, hardened runtime, Developer ID signing, notarization, DMG/ZIP.

### Windows

- Decide the minimum supported Windows release and architectures.
- Spike `windows-reactor` tray, window, accessibility, single-instance, and packaging maturity.
- Add Windows Firewall and mDNS guidance.
- Add `.ico`, version resource, AppUserModelID, and portable ZIP.
- Choose Startup folder, Run key, or Task Scheduler for autostart.
- Sign releases with Authenticode and timestamp them.

### Linux

- Decide the supported distros, desktops, Wayland/X11 combinations, and architectures.
- Spike `gtk4-rs` and `ksni` with GNOME AppIndicator and KDE.
- Install GTK4, DBus, and AppIndicator build dependencies in CI.
- Add `.desktop`, AppStream metadata, hicolor/symbolic icons, and an AppImage recipe.
- Choose XDG autostart or a systemd user unit.
- Validate on clean Ubuntu and Fedora VMs.

## Before the first Windows/Linux beta

- Shared session/state layer extracted or an explicit duplication decision recorded.
- CI green on macOS, Linux, and Windows with formatting, clippy, tests, release builds, and cross-checks.
- Connection reuse, atomic registry writes, and structured errors in place.
- Packaging, autostart, single-instance, firewall, and update policy documented.
- Real-hardware smoke tests: discovery, power, brightness, warmth, pulse, sweep, grouping, reconnection.
