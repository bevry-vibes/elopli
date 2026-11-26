# Windows app readiness: code and project quality review

Assisted-by: OpenCode · DeepSeek V4.1 Flash <opencode-opencodego-deepseekv41flash@local>

Provenance: [1795665609108-windows-app-review-and-build.prompts.md](1795665609108-windows-app-review-and-build.prompts.md)

**Status:** dated review and delivery plan · 2026-11-26 · review of `a5801aa` (`main`) · source changes follow in this session.

This review prepares the first native Windows app. Evergreen decisions remain in [DESIGN.md](../../DESIGN.md); the platform plan remains in [docs/platforms.md](../../docs/platforms.md). This file is dated history, not a changelog.

## Governance

Reciprocity was checked with `agent-detect check-reciprocal` in recipe mode, using the identity reported by the harness (`harness = opencode`, `provider = opencode-go`, `model = deepseek-v4.1-flash`). Result: **is reciprocal** (exit 0). The live auto-detection path is unavailable in this harness, so recipe mode was used as documented.

## Verified baseline

- The working tree was clean at `a5801aa`.
- A Windows development shell exists, but the machine has no MSVC linker, Windows SDK, `vswhere`, or Visual Studio Build Tools. The GNU toolchain (`1.100.0-x86_64-pc-windows-gnu`) plus `mingw-winlibs` was installed locally to get a baseline. CI still uses MSVC; local GNU builds are a development fallback, not a supported release toolchain.
- `cargo test --workspace --locked` on the GNU host: 3 of 4 CLI tests pass; `config_path_json_is_machine_readable` fails because it asserts a POSIX leading slash. This confirms the prior review's first critical finding on real Windows.
- The core still has no `std::os::unix`, no hardcoded `/tmp`, and no platform UI code; `unsafe_code = "forbid"` is inherited by every crate.
- Privacy rules still hold: serial and MAC fields are not parsed or stored; `devices.json` is ignored; generated Xcode artifacts are ignored.

## Review findings

### Critical

1. **The Windows CLI test fails on every Windows run.** `crates/elopli/tests/cli.rs` asserts `path.starts_with('/')`. Replace with `Path::new(path).is_absolute()`. This blocks the Windows CI gate.

2. **There is no Windows UI, build script, package, or release path.** `scripts/build-macos.sh` is the only app build. There is no `apps/windows`, no `.ico`, no version resource, no AppUserModelID, no tray, no autostart, no single-instance handling, no firewall guidance, no ZIP or Scoop manifest, and no release workflow.

### High

3. **The session layer is Swift-only.** Grouping, live slider writes, registry reloads, discovery rows, revision guards, and animation lifecycle still live in `AppModel.swift`. A second UI must not reimplement them. Extract a platform-free `elopli-session` crate and use it from Windows; record the macOS migration as debt.

4. **`windows-reactor` does not fit this product yet.** A spike of `windows-reactor 0.100.0` shows a declarative WinUI 3 framework with native controls, but no tray API, no window show/hide control (only `request_close`), and a Windows App SDK bootstrap/runtime dependency. Elopli is a tray-first popup panel; a portable ZIP should not require a separate runtime. DECISION: use Win32 + `windows` crate + Direct2D/DirectWrite for the panel, with a Win32 tray icon and popup window. This supersedes the planned `windows-reactor` choice; see DESIGN.md D19.

5. **mDNS address selection is arbitrary and errors are swallowed.** `discovery.rs` takes the first address from an unordered set, ignores channel errors, and ignores shutdown errors. On Windows (Hyper-V, WSL, VPN adapters) this can store an unreachable address. Implement a deterministic rank, surface browse errors, and document Windows Firewall/mDNS behaviour.

6. **Tilde expansion is Unix-shaped.** `--config ~\devices.json` loses the home prefix on Windows. `ELOPLI_STATE_DIR` is not expanded at all. Fix both and add path-form tests.

7. **Registry rename can fail on Windows under AV/indexer locks.** `fs::rename` has no retry, and the temporary name drops the `.json` extension. Add a bounded retry and keep the extension. Add path context to lock errors.

8. **The curl probe can flash a console on Windows.** The request path sets `CREATE_NO_WINDOW`, but the one-time `curl --version` probe does not. Share one helper.

9. **Version drift.** `Cargo.toml` says `0.1.0`; `Info.plist` says `0.1.1` build 6. Make `Cargo.toml` the single source and pass the version into the macOS and Windows builds.

10. **CI has no Windows app job and no artifact upload.** Add a Windows app build, package script, artifact upload, and a release workflow with per-platform assets and checksums.

### Medium

11. **Hygiene.** `.gitattributes` disables EOL normalization while `.editorconfig` requires LF; add `*.sh text eol=lf` at minimum. `.gitignore` lacks Windows packaging artifacts. There is no CODEOWNERS, dependabot, third-party notices, or release template.

12. **Test gaps.** No tests for `transport.rs`, `discovery.rs`, `elopli-ffi`, cross-process lock contention, Windows path forms, or the new session logic. Two animation tests sleep rather than poll.

13. **CLI quality debt.** `unreachable!()` in dispatch, ignored stdout write errors, six near-identical JSON record helpers, and Unicode lowercasing versus ASCII comparisons. Clear the safe items without changing behaviour.

14. **Docs drift.** README says macOS app only in Requirements while documenting Windows config paths later; CHANGELOG calls the matrix a UI matrix; PRIVACY says launch-at-login uses `UserDefaults` while macOS uses `SMAppService`. Fix while documenting Windows.

## Decisions for the Windows app

| # | Decision | Reason | Revisit if |
| --- | --- | --- | --- |
| D19 | Windows UI: Win32 + `windows` crate + GDI+ | Tray-first popup, no runtime dependency, portable ZIP, one native window, full control of layout and DPI. `windows-reactor` has no tray or visibility control and needs the Windows App SDK. Direct2D HWND targets also presented blank in the test environment, so the panel uses GDI+ (revised after the spike). | `windows-reactor` gains tray/visibility APIs and a self-contained deployment story, or Direct2D presents reliably on the supported Windows releases. |
| D20 | Shared `elopli-session` crate | One implementation of grouping, revisions, slider coalescing, discovery rows, and animation lifecycle. Windows uses it now; macOS migrates later. | A UI needs behaviour the crate cannot express without platform code. |
| D21 | Tray-first panel with Ctrl-click linking | Matches the menu-bar product model without a Dock/taskbar window. Ctrl is the Windows multi-select modifier. | Windows users expect a normal window or a different modifier. |
| D22 | Windows 10 1809+ x64 for v1; ARM64 later | Covers Direct2D/DirectWrite and per-monitor DPI; ARM64 adds CI and package work. | Demand or a Windows-on-ARM release requirement appears. |
| D23 | Portable ZIP and Scoop; Authenticode optional | The prior platform plan elected ZIP/Scoop over Inno Setup. Signing is a release-time choice. | A signed installer or Microsoft Store distribution becomes required. |

The Windows app uses the native transport only. It never falls back to `curl`, so no console can flash and animations keep their persistent connection.

## Delivery plan

1. Fix the Windows blockers and add tests: CLI path assertion, tilde/state-dir expansion, mDNS rank and error surfacing, curl probe flags, registry rename retry, version source.
2. Add `elopli-session` with link graph, revisions, refresh, live slider queue, discovery rows, and group animations; unit-test the pure logic.
3. Build `apps/windows/Elopli`: Win32 tray shell, custom Direct2D panel, cards, sliders, links, sheets, discovery, single instance, autostart, config reveal, quit cancellation.
4. Add icon generation, version resource, AppUserModelID, `scripts/build-windows.ps1`, portable ZIP, checksums, Scoop template, firewall and mDNS guidance.
5. Update CI, README, DESIGN, CHANGELOG, PRIVACY, AGENTS, and run fmt, clippy, tests, docs, deny, and a Windows build.

## Acceptance gates

- `cargo test --workspace --locked` passes on Windows.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo doc`, and `cargo deny check` pass.
- `cargo build -p elopli-app --features windows --locked --release` produces `Elopli.exe`.
- `Elopli.exe --smoke-test` reads the registry and prints per-light state; exit 0 when every light answers.
- The panel opens from the tray, shows one card per light, writes power/brightness/warmth, runs pulse and sweep, discovers lights, links with Ctrl, reorders by drag, and quits without leaving a light mid-animation.
- `scripts/build-windows.ps1` produces a portable ZIP with the app, CLI, docs, and `SHA256SUMS`.

## Delivery notes (2026-11-26)

- The Windows shell is Win32 with a tray icon and a `WS_POPUP` panel. The renderer is GDI+, not Direct2D: a minimal Direct2D HWND test also presented a blank window on the Windows 11 test machine, while GDI+ rendered correctly. DESIGN.md D19 records the change and the revisit trigger.
- The shared `elopli-session` crate now owns refresh plans, link groups, slider coalescing, discovery rows, and animation hand-off. The Windows app uses it; the macOS Swift session is migration debt.
- Refresh and discovery run on child threads in the Windows worker, so offline-light timeouts cannot starve UI commands.
- The GNU local build required two fixes: `windres` output must be linked as an object instead of an archive, and the release build must be cleaned after changing `build.rs` so the resource link argument is applied. CI uses a clean MSVC checkout.
- The app icon and version resource are embedded and visible through `FileVersionInfo`.
- Real-hardware mDNS discovery found the Key Light Air and Key Light on the test network, and the discovery sheet rendered the Added/Missing/Add states. The protocol read against those addresses was stopped after it did not return promptly, so a full hardware read/write smoke test remains open.
- Local checks: workspace tests, fmt, clippy (workspace and Windows app), docs, and `cargo deny check` pass with the GNU toolchain. The CI MSVC job and macOS build were not run from this Windows session.
- The portable ZIP, Scoop manifest, SHA-256 checksums, screenshots, README, DESIGN, platform plan, privacy notes, third-party notices, and release workflow are in place.
