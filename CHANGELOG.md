# Changelog

All notable changes to Elopli are recorded here.
The format follows Keep a Changelog, and the project uses semantic versioning.

## Unreleased

### Added

- Human duration input for the CLI, macOS sheets, and App Intents: bare numbers are seconds; `s` and `ms` are accepted.
- Per-light write revisions so a stale refresh cannot overwrite a newer action.
- Atomic, locked, fingerprinted registry writes with BOM tolerance.
- Parallel multi-light App Intent actions with per-light outcomes.
- A termination hook that cancels animations and waits for their restore.
- A CI matrix for macOS, Linux, Windows, cross-target checks, docs, and the macOS app.
- A native Windows tray app with cards, linkage, pulse and sweep sheets, discovery, dark/light themes, single instance, and Launch at Login.
- A shared `elopli-session` crate for grouping, refresh revisions, live slider writes, discovery rows, and animation hand-off.
- A Windows portable ZIP, generated Scoop manifest, checksums, icon, and version resource.
- A Windows app CI job and `scripts/build-windows.ps1`.
- Ctrl-click linking and `links.json` persistence on Windows.
- A hidden-by-default serial tooltip on the Windows card model name; serials stay in memory and are never persisted or logged.
- A native Linux GTK4 tray panel with cards, custom-drawn sliders, Ctrl-click linking, pulse and sweep sheets, discovery, dark/light themes, single instance, and XDG autostart. The Linux warmth slider reuses the Fluent flame path from the Windows app, under the same MIT attribution.
- A Linux portable tarball build with a desktop entry, AppStream metadata, hicolor icons, checksums, and an optional AppImage.
- A Linux app CI job on Ubuntu 24.04.
- A Wayland-safe `ELOPLI_SCREENSHOT` render hook for panel visual checks.
- Soar installation for the Linux AppImage under the `elopli-app` name, so it does not shadow the `elopli` CLI.

### Changed

- Durations default to `1.0s`; pulse and sweep reject durations that cannot reach every endpoint at the 10 Hz frame limit.
- `on` and `toggle` now floor brightness at 2% and treat `on` with brightness below 2% as off.
- Animation frames use a persistent-only transport path and never fall back to `curl`.
- The registry path is platform-stable for the app and App Intents; the CLI keeps the local-file preference.
- Sweep runs warm on the left and cool/blue on the right. An off light ignores its stored temperature, starts at the warm end, ramps from 2% to its working brightness, then fades to 0% at the cool end.
- File locks use the standard library instead of `fs2`.
- Group links are maximal, non-overlapping card groups; field-only links no longer show group actions.
- Linked slider writes use one coordinator per target group.
- Accessibility and localization are explicitly deferred until after v1.
- Windows refresh and discovery run off the command thread so offline lights cannot starve UI actions.
- Newly discovered lights use the device display name when it is unique; a duplicate display name falls back to the mDNS instance.
- The light display name is now authoritative across interfaces: rename updates the registry key and link groups, refresh adopts a name changed elsewhere, and the CLI `rename` and `discover --write` follow the same rule.
- The Linux header uses the macOS and Windows icon order: Refresh, Discover, All On, All Off, Settings.
- Linux cards drag by their background; the grip icons are gone. The panel has close-only chrome, WiFi quality colours, a filled Fluent flame, rounded brightness tracks, and a libadwaita-style switch.
- The Windows panel uses Fluent UI System Icons, double-buffered painting, overlay sheets, field-anchored tooltips, and macOS-style tapered slider tracks and thumbs.
- mDNS picks a deterministic private IPv4 address and surfaces browse failures.
- The workspace version in `Cargo.toml` is the single version source for both apps.

### Fixed

- Dragging the Linux warmth slider no longer rewrites the brightness value label.
- Linux card drops use a string drag payload so the drop targets accept a card drag.

- Duration input no longer crashes on huge or non-finite values.
- `--timeout` and `--wait` reject unsafe values instead of panicking.
- `toggle --wait` probes reachability and writes once.
- JSON mode emits JSON for top-level errors, and broken pipes no longer panic.
- `identify` reports API failures correctly.
- Rename accepts a host and supports `--wait`.
- Sweep brightness 0/1 and power-on brightness 0/1 no longer produce invalid requests.
- Discovery matches by name, address, hostname, and port; newly added lights stay `Added`, and a changed address offers `Update`.
- Offline lights show the error and power-cycle advice instead of stale WiFi details.
- Group sync writes exact mired values and no longer leaves overlapping card groups.
- Invalid updates and unsupported transports are rejected before cancelling a running animation.
- The CLI config-path test passes on Windows, and tilde and state-directory expansion handle both separators.
- The one-time `curl --version` probe no longer flashes a console on Windows.
- Registry replacement retries Windows sharing violations and keeps the `.json` extension on temporary files.
- Animation frames keep the 10 Hz floor across segment boundaries, including after a slow write.
- Device names compare case-insensitively for Unicode, and accessory integers no longer truncate silently.
