# AGENTS.md

This project conforms to Bevry's skills.
Reference their remote URLs only — do not pull their contents into this file.
When a referenced skill applies with your project's tweaks, the local `<name>.md` file at this repo root references the remote URL and lists the tweaks underneath;
this process is documented in the upstream repo's [local tweaks pattern](https://github.com/bevry-vibes/skills#local-tweaks-pattern).

- https://github.com/bevry-vibes/skills/blob/main/policy.md — **applies**. The agent must pass reciprocity detection before work.
- https://github.com/bevry-vibes/skills/blob/main/secrets.md — **applies**. Never probe for credentials. The project needs no secrets.
- https://github.com/bevry-vibes/skills/blob/main/author.md — **applies**. Agent commits carry the co-author trailer. Agent posts carry the assisted-by trailer. Agent commits are never signed.
- https://github.com/bevry-vibes/skills/blob/main/commits.md — **applies**. Conventional Commits, one logical change per commit.
- https://github.com/bevry-vibes/skills/blob/main/plans.md — **applies**. Plan files live in `.plans/` with a prompt companion.
- https://github.com/bevry-vibes/skills/blob/main/design.md — **applies**. Evergreen decisions live in `DESIGN.md`; dated history lives in `.plans/`.
- https://github.com/bevry-vibes/skills/blob/main/build.md — **applies**. The macOS app is a menu-bar app with no Dock icon.
- https://github.com/bevry-vibes/skills/blob/main/python.md — **does not apply**. The Python reference CLI is retired.
- https://github.com/bevry-vibes/skills/blob/main/zig.md — **does not apply**. The project uses Rust, Swift, and future native UIs.
- https://github.com/bevry-vibes/skills/blob/main/emdash.md — **does not apply**. The project has no CMS.
- https://github.com/bevry-vibes/skills/blob/main/powershell.md — **does not apply**. The project targets macOS, Linux, and Windows with native UIs.
- https://github.com/bevry-vibes/skills/blob/main/zcode.md — **does not apply**. The harness is OpenCode.
- https://github.com/bevry-vibes/skills/blob/main/minimax.md — **does not apply**. The model is DeepSeek.

## project rules

- Never write a device serial number or MAC address into documentation, plan or prompt files, logs, commits, or persisted configuration. MAC addresses are discarded at parse time. A serial number may be retained in memory only for an on-demand UI tooltip, hidden by default.
- Keep `devices.json` and `links.json` out of version control. Commit `devices.example.json` only.
- The API has no authentication and no encryption. Use it only on a trusted local network.
- Keep the CLI non-interactive and usable from Apple Shortcuts. Support absolute paths and JSON output.
- Read [docs/api.md](docs/api.md) before a change to the protocol layer.
- Update [DESIGN.md](DESIGN.md) when an evergreen decision changes. It records what is true now; it is not a changelog.
- Accessibility, keyboard navigation, and localization are deferred until after v1. Do not expand that scope without a new decision.
- Durations accept a bare number as seconds, `s`, or `ms`; the pulse and sweep default is `1.0s`.
- The macOS app uses SwiftUI `MenuBarExtra`, `LSUIElement`, and App Intents. It calls the Rust core through UniFFI.
- Build the macOS app with `./scripts/build-macos.sh`.
- Do not commit `apps/macos/Elopli/Generated`, `apps/macos/Elopli/build`, or `apps/macos/Elopli/Elopli.xcodeproj`. They are build artifacts.
- The Windows app uses Win32, a notification-area icon, and a GDI+ panel. It calls `elopli-core` and `elopli-session` directly.
- Build the Windows app with `cargo build -p elopli-app --features windows --release` or `./scripts/build-windows.ps1`.
- The Windows app crate is the only crate that allows `unsafe_code`; keep the unsafe blocks at the Win32 boundary.
- The Linux app uses GTK4, a `ksni` StatusNotifierItem, and the shared `elopli-session`. It calls `elopli-core` and `elopli-session` directly.
- Build the Linux app with `cargo build -p elopli-app-linux --features linux --release` or `./scripts/build-linux.sh`.
- Keep the Linux app safe Rust; it inherits the workspace's `unsafe_code = "forbid"`.
- Do not commit `dist/`; the Windows and Linux build scripts write packages and checksums there.
- The light's display name is authoritative. The registry key mirrors it, and link groups migrate when a name changes; do not add per-interface nicknames.
- Update [docs/linux.md](docs/linux.md) when Linux build, packaging, tray, or Wayland behaviour changes.
- Keep the Rust core and session layer free of UI and platform code. The CLI and the apps are thin wrappers.
