# Platforms

## Current status

| Platform | Status | UI | Packaging |
| --- | --- | --- | --- |
| macOS 14+ | Implemented and hardware-tested | SwiftUI `MenuBarExtra`, `LSUIElement`, App Intents | Local app bundle; Developer ID/notarization still open |
| Windows 10 1809+ x64 | Implemented; hardware discovery verified | Rust Win32 tray panel with GDI+ rendering | Portable ZIP and generated Scoop manifest; Authenticode open |
| Linux (Fedora/Ubuntu) | Implemented on Fedora 45; interaction and packaging validation open | Rust `gtk4-rs` with `ksni` | Portable tarball; AppImage through Soar when `appimagetool` is available; Flatpak is shelved |

The Rust core and CLI compile for all three platforms.
The shared `elopli-session` crate owns grouping, refresh revisions, live slider writes, discovery rows, and animation hand-off.
The macOS UI still carries its Swift session layer; migrating it to the shared crate is an open item.
The Windows and Linux UIs are renderers over the shared session layer; the macOS UI still has its own.

## Shared rules

- The API is plain HTTP on TCP 9123 with no authentication or encryption.
- Use it only on a trusted local network.
- Never write a serial number or MAC address into documentation, plans, prompts, logs, commits, or persisted configuration. MACs are discarded; a serial is held in memory only for the on-demand model-name tooltip.
- Discovery requires IPv4 mDNS; devices without an IPv4 address are not added.
- Animations require a persistent native HTTP connection and never use `curl`.
- A superseded animation restores the original state before the next action writes.
- The FFI is synchronous. Run its network calls on a worker thread until async wrappers exist.
- The Windows and Linux apps use the native transport only.
- The light's display name is authoritative; the registry key mirrors it, and link groups migrate when it changes.

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

- Supported baseline: Fedora 45 and Ubuntu 24.04, x86_64 and aarch64; GNOME Wayland tested.
- Build: `cargo build -p elopli-app-linux --features linux --release`.
- Package: `./scripts/build-linux.sh`; it validates the desktop entry and AppStream metadata, writes a portable tarball and `SHA256SUMS`, and writes an AppImage when `appimagetool` is available.
- The GTK4 shell uses `GtkApplicationWindow` and a header bar, a `ksni` StatusNotifierItem, a worker thread for `elopli-session`, custom-drawn sliders, and Ctrl-click links.
- GNOME needs the AppIndicator extension. Wayland forbids client-side placement, so the panel opens as a regular window from the tray menu.
- Launch at Login writes `~/.config/autostart/com.bevry.elopli.desktop`.
- Single instance uses the `com.bevry.elopli` D-Bus name through `GtkApplication`.
- Firewall guidance: allow multicast UDP 5353 and TCP 9123 on the light's network, and avoid VPN or bridge adapters that shadow the light subnet.
- Release work: KDE validation, a clean Ubuntu VM, AppImage signing/distribution, and a public install path.

## Before the first Windows/Linux beta

- Shared session/state layer extracted: done for Windows; macOS migration open.
- CI green on macOS, Linux, and Windows with formatting, clippy, tests, release builds, and cross-checks: Windows and Linux app jobs added; Fedora desktop runtime still open.
- Connection reuse, atomic registry writes, and structured errors in place: done.
- Packaging, autostart, single-instance, firewall, and update policy documented: Windows done; Linux packaging written, install/AppImage release open.
- Real-hardware smoke tests: macOS only; Windows mDNS discovery verified; Linux read one Key Light Air and two Key Lights and rendered their live state and remote names, while UI write actions and full interaction still need exercise.