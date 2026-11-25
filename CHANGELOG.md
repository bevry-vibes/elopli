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

### Changed

- Durations default to `1.0s`; pulse and sweep reject durations that cannot reach every endpoint at the 10 Hz frame limit.
- `on` and `toggle` now floor brightness at 2% and treat `on` with brightness below 2% as off.
- Animation frames use a persistent-only transport path and never fall back to `curl`.
- The registry path is platform-stable for the app and App Intents; the CLI keeps the local-file preference.
- Sweep ramps an off light from 2% to its working brightness over up to 1 s before the temperature sweep, then fades to 0%, and weights the final brightness restore.
- File locks use the standard library instead of `fs2`.
- Group links are maximal, non-overlapping card groups; field-only links no longer show group actions.
- Linked slider writes use one coordinator per target group.
- Accessibility and localization are explicitly deferred until after v1.

### Fixed

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
