# Platforms

## Current status

| Platform | Status | UI | Packaging |
| --- | --- | --- | --- |
| macOS 14+ | Implemented and hardware-tested | SwiftUI `MenuBarExtra`, `LSUIElement`, App Intents | Local app bundle; Developer ID/notarization still open |
| Windows 10 1809+ x64 | Implemented; hardware discovery verified | Rust Win32 tray panel with GDI+ rendering | Portable ZIP and generated Scoop manifest; Authenticode open |
| Linux (Fedora/Ubuntu) | Planned | Rust `gtk4-rs` with `ksni` | AppImage through Soar; Flatpak is shelved |

The Rust core and CLI compile for all three platforms.
The shared `elopli-session` crate owns grouping, refresh revisions, live slider writes, discovery rows, and animation hand-off.
The macOS UI still carries its Swift session layer; migrating it to the shared crate is an open item.
The Windows UI is the second renderer over the shared session layer.

## Shared rules

- The API is plain HTTP on TCP 9123 with no authentication or encryption.
- Use it only on a trusted local network.
- Never store, print, or commit a serial number or MAC address.
- Discovery requires IPv4 mDNS; devices without an IPv4 address are not added.
- Animations require a persistent native HTTP connection and never use `curl`.
- A superseded animation restores the original state before the next action writes.
- The FFI is synchronous. Run its network calls on a worker thread until async wrappers exist.
- The Windows app uses the native transport only.

## Per-platform work

### macOS

- `./scripts/build-macos.sh` with a development team for App Intents.
- `CODE_SIGNING_ALLOWED=NO` is supported for CI-only builds.
- Accessibility and localization are deferred until after v1.
- Release work: universal binary, hardened runtime, Developer ID signing, notarization, DMG/ZIP.

### Windows

- Supported baseline: Windows 10 1809 or later, x64.
- Build: `cargo build -p elopli-app --features windows --release`.
- Package: `pwsh -NoProfile -File scripts/build-windows.ps1`.
- The Win32 shell uses a tray icon, a `WS_POPUP` panel, a named-mutex single instance, and `HKCU\...\Run` for autostart.
- The panel uses GDI+ because Direct2D HWND targets did not present on the Windows 11 test machine.
- The app embeds the icon and version resource; the version comes from `Cargo.toml`.
- Firewall guidance: allow `ElopliApp.exe` on private networks, keep UDP 5353 and TCP 9123 reachable, and avoid VPN/Hyper-V adapters that shadow the light subnet.
- Release work: Authenticode signing and timestamping, a public Scoop bucket, ARM64, and update policy.

### Linux

- Decide the supported distros, desktops, Wayland/X11 combinations, and architectures.
- Spike `gtk4-rs` and `ksni` with GNOME AppIndicator and KDE.
- Install GTK4, DBus, and AppIndicator build dependencies in CI.
- Add `.desktop`, AppStream metadata, hicolor/symbolic icons, and an AppImage recipe.
- Choose XDG autostart or a systemd user unit.
- Validate on clean Ubuntu and Fedora VMs.

## Before the first Windows/Linux beta

- Shared session/state layer extracted: done for Windows; macOS migration open.
- CI green on macOS, Linux, and Windows with formatting, clippy, tests, release builds, and cross-checks: Windows app job added; Fedora runtime still open.
- Connection reuse, atomic registry writes, and structured errors in place: done.
- Packaging, autostart, single-instance, firewall, and update policy documented: Windows done; Linux open.
- Real-hardware smoke tests: macOS only; Windows mDNS discovery verified, while protocol read/write and UI actions are still to exercise.