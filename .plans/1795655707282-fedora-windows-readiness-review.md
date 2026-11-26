# Fedora and Windows readiness: code and project quality review

Assisted-by: OpenCode · Hy4 Flash <opencode-opencodego-hy4flash@local>

Provenance: [1795655707282-fedora-windows-readiness-review.prompts.md](1795655707282-fedora-windows-readiness-review.prompts.md)

**Status:** dated review · 2026-11-26 · review of `b2adffb` (`main`) · no source files changed by this review.

This is a new review, following the platform-readiness gates recorded in [docs/platforms.md](../../docs/platforms.md) by `065ca39`. It assesses the current codebase for adding Fedora and Windows support. Evergreen decisions remain in [DESIGN.md](../../DESIGN.md); this file is dated history and is not a changelog.

## Governance

Before any work, reciprocity was checked with `agent-detect check-reciprocal` in recipe mode, using the exact identity read from the live OpenCode session store (`harness = opencode`, `provider = opencode-go`, `model = hy4-flash-free`). Result: **is reciprocal** (exit 0). Live auto-detection of the provider/model returned exit 8 in this harness; recipe mode is agent-detect's documented verification path.

## Verified baseline

- `cargo test --workspace --locked`: 41 tests pass.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean.
- `.github/workflows/ci.yml` runs `ubuntu-latest`, `macos-latest`, `windows-latest` (MSVC by default), an `x86_64-pc-windows-gnu` cross-check, rustdoc with `-D warnings`, cargo-deny, and the macOS app build.
- The core is genuinely platform-free in the main: no `std::os::unix`, no hardcoded `/tmp` or `/etc`, no signals or sockets, one narrowly scoped `#[cfg(windows)]` block, `unsafe_code = "forbid"` inherited by all crates.
- Privacy rules hold in code: serial and MAC fields are not parsed or stored (`crates/elopli-core/src/model.rs:101-110`, `274-303`); `devices.json` is ignored and untracked; generated Xcode artifacts are ignored.

## Critical — fix before Fedora/Windows work begins

1. **Windows CI test hardcodes a POSIX path.** `crates/elopli/tests/cli.rs:48-60` asserts the `config path` JSON value `starts_with('/')`. On Windows, `platform_config_path()` (`crates/elopli-core/src/config.rs:27-35`) returns `%APPDATA%\elopli\devices.json`, so this test fails on every Windows run. Replace with `Path::new(path).is_absolute()`. This single assertion blocks the "CI green on Windows" gate in `docs/platforms.md:55`.

2. **mDNS picks an arbitrary IPv4 address, and failures are silent.** `crates/elopli-core/src/discovery.rs:57` takes `.next()` from an unordered set. On Fedora (docker0, virbr, VPN tun) and Windows (Hyper-V, WSL, VPN adapters), discovery can store an unreachable address, and all later HTTP calls fail. `discovery.rs:79` swallows channel errors and `:82` ignores shutdown errors; `mdns-sd` is built with `default-features = false` (`Cargo.toml:26`, no `log` feature), so a firewall conflict (Avahi owning UDP/5353 on Fedora, Bonjour/Firewall on Windows) yields an empty list with no diagnostic. Needs a deterministic address-selection policy, surfaced errors, diagnostics, and the firewall guidance outstanding in `docs/platforms.md:38,49`.

## High

3. **Tilde expansion is Unix-shaped and asymmetric.** `crates/elopli-core/src/config.rs:14-22`: `--config ~\devices.json` on Windows leaves a rooted `\devices.json` that discards the home prefix, silently reading or writing the wrong file. `~user` is unsupported. `ELOPLI_STATE_DIR` (`crates/elopli-core/src/control.rs:24-27`) is not expanded, while `ELOPLI_CONFIG` is. Needs unit tests for the path forms.

4. **Everything above the core is unimplemented.** There is no release workflow (`.github/workflows/ci.yml` has no artifact upload, checksums, or publish job), no Linux/Windows UI build scripts (`scripts/build-macos.sh` is macOS-only, Swift/UniFFI-only), and no packaging assets: no `.spec`, `.desktop`, AppStream metadata, AppImage recipe, Scoop manifest, ZIP layout, `.ico`, version resource, AppUserModelID, autostart, single-instance, or update policy. `CHANGELOG.md` has only `## Unreleased`; no version has been cut. `scripts/make-icon.swift` uses AppKit and an SF Symbol, so there is no platform-neutral icon master. The planned Windows/Linux UIs consume `elopli-core` directly, so UniFFI need not be generalized.

5. **Shared session/state layer still lives in Swift.** `docs/platforms.md:13` and `DESIGN.md:52,419` identify grouping, live slider writes, registry reloads, and discovery rows as macOS-only. The `docs/platforms.md:54` beta gate ("extracted or explicit duplication decision recorded") is unmet. This is the largest architectural decision before a second UI.

6. **Licensing needs an early Fedora decision.** The project is RPL-1.5 (`Cargo.toml:14`, `LICENSE.md`). Fedora's license-approval rules for RPL need verification before an RPM can be submitted, and RPL source/attribution obligations affect ZIP and AppImage bundles. There is no third-party notices file. `deny.toml` allowing RPL/LGPL for dependencies does not resolve this.

7. **Windows curl probe flashes a console.** `crates/elopli-core/src/transport.rs:152-162`: the request path correctly applies `CREATE_NO_WINDOW` (`:194-199`), but the one-time `curl --version` probe does not. In a tray app this flashes a window on first network operation; PATH-dependent `curl` discovery can also silently disable the fallback for a Start-menu-launched GUI process.

8. **Documentation and version drift.** `README.md:92-99` is macOS-only while `:170-172` already documents Linux/Windows config paths; `CHANGELOG.md:15` calls the cargo matrix a "CI matrix for macOS, Linux, Windows" without qualifying that no UI or package is built; version is `0.1.0` in `Cargo.toml:11` but `0.1.1`/build 6 in `Info.plist:19-22`; `PRIVACY.md:8` says launch-at-login is in `UserDefaults` but it uses `SMAppService`; dated contradictions in `.plans/1795599587000-official-client-language.md` (easing, Python CLI, `ubuntu-22.04`, checksum publishing, notarisation) should be marked historical or reconciled.

## Medium

9. **CI gaps that multiply with new platforms:** no UI/package builds, no Fedora container (or any Fedora runtime), no ARM64 targets, no artifact uploads, unpinned `xcodegen` (`ci.yml:78` versus README's ≥2.48), team-signed App Intents path never validated (`CODE_SIGNING_ALLOWED: NO`), duplicated toolchain pin, and no `[targets]` section in `deny.toml` for future GTK4/windows-reactor dependencies.

10. **Config save hardening:** `crates/elopli-core/src/config.rs:243-265` uses `fs::rename` with no retry; Windows `MoveFileExW(REPLACE_EXISTING)` can fail when Explorer, antivirus, or an indexer holds the destination. The `.tmp-` name also drops the `.json` extension. Lock errors (`config.rs:207`, `control.rs:135-163`) lose path context.

11. **Record deliberate transport decisions:** `ureq` has no TLS/proxy features (`Cargo.toml:22`); that is fine for plain HTTP but should be an explicit DESIGN entry. IPv6 zone IDs (`fe80::1%eth0`, `%12`) are bracketed but likely rejected by `url` (`client.rs:77-84`); document them as unsupported or handle them explicitly.

12. **Quality debt worth clearing before new UIs:** timing-sensitive tests (`client.rs:347,376,401,426`) use tight wall-clock sleeps that are flakier on Windows/Fedora CI; pulse and sweep skeleton duplication (`client.rs:333-402`) and a 12-argument `ramp_to` (`:658-673`); six near-identical JSON record helpers (`crates/elopli/src/main.rs:445-609`); `unreachable!()` in CLI dispatch (`main.rs:1560`); ignored stdout write errors (`main.rs:17-22`); Unicode `to_lowercase()` versus ASCII-only comparisons in `config.rs` (`:176,183` versus `:288,292`); silent numeric truncation in `parse_accessory_info` (`model.rs:299-300`).

13. **Hygiene:** `.gitattributes:5` sets `* -text`, disabling LF normalization while `.editorconfig:7` requires `lf`; Windows contributors can CRLF-break `#!/bin/sh` scripts (add `*.sh text eol=lf`). `.gitignore` lacks `*.exe`, `*.msi`, `*.rpm`, `*.AppImage`, `*.dmg`, `bin/`, `obj/`, `dist/`. No CODEOWNERS, dependabot, release templates, or third-party notices. Existing hygiene is otherwise good: `devices.json` is ignored and untracked, generated Xcode artifacts are ignored, no secrets, no serial or MAC in tracked files.

14. **Test coverage gaps:** no tests at all in `transport.rs`, `discovery.rs`, or `elopli-ffi`; no cross-process lock contention test; no Windows config round-trip (CRLF/BOM/file-held-open); no non-interactive CLI exit-code, JSON-purity, or broken-pipe matrix.

## Readiness matrix against docs/platforms.md:52-58

| Gate | Status |
| --- | --- |
| Shared session/state extracted or duplication decision recorded | Not done |
| CI green on macOS/Linux/Windows with fmt, clippy, tests, release, cross-check | Windows test broken; no UI/package builds |
| Connection reuse, atomic registry writes, structured errors | In place |
| Packaging, autostart, single-instance, firewall, update policy documented | Not done |
| Real-hardware smoke tests: discovery, power, brightness, warmth, pulse, sweep, grouping, reconnection | macOS only |

## Recommended sequencing for the next session

1. Small PRs: fix the Windows path assertion; fix `expand_tilde` and `ELOPLI_STATE_DIR`; deterministic mDNS address selection plus error surfacing; `CREATE_NO_WINDOW` on the curl probe.
2. Add the missing test suites (paths, transport, discovery, FFI, cross-process locks, CLI contract) and replace timing assertions with state polls.
3. Decide extract-versus-duplicate for the session layer and record it in `DESIGN.md`, including siblings to D18 for Windows tray and Linux AppIndicator; move dated experiment narratives from DESIGN to `.plans/`.
4. Resolve RPL/Fedora licensing and add third-party notices; unify on one version source.
5. Then build UI build scripts, a release workflow with per-platform artifacts and checksums, packaging assets, and platform-specific network/firewall docs; finish with Fedora and Windows hardware smoke tests.
