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
- https://github.com/bevry-vibes/skills/blob/main/build.md — **applies**. The macOS app is a menu-bar app with no Dock icon.
- https://github.com/bevry-vibes/skills/blob/main/python.md — **does not apply**. The Python reference CLI is retired.
- https://github.com/bevry-vibes/skills/blob/main/zig.md — **does not apply**. The project uses Rust, Swift, and future native UIs.
- https://github.com/bevry-vibes/skills/blob/main/emdash.md — **does not apply**. The project has no CMS.
- https://github.com/bevry-vibes/skills/blob/main/powershell.md — **does not apply**. The project targets macOS, Linux, and Windows with native UIs.
- https://github.com/bevry-vibes/skills/blob/main/zcode.md — **does not apply**. The harness is OpenCode.
- https://github.com/bevry-vibes/skills/blob/main/minimax.md — **does not apply**. The model is DeepSeek.

## project rules

- Never print, store, log, or commit a device serial number or MAC address. The client discards both fields.
- Keep `devices.json` out of version control. Commit `devices.example.json` only.
- The API has no authentication and no encryption. Use it only on a trusted local network.
- Keep the CLI non-interactive and usable from Apple Shortcuts. Support absolute paths and JSON output.
- Read [docs/api.md](docs/api.md) before a change to the protocol layer.
- The macOS app uses SwiftUI `MenuBarExtra`, `LSUIElement`, and App Intents. It calls the Rust core through UniFFI.
- Build the macOS app with `./scripts/build-macos.sh`.
- Do not commit `apps/macos/Elopli/Generated`, `apps/macos/Elopli/build`, or `apps/macos/Elopli/Elopli.xcodeproj`. They are build artifacts.
- Keep the Rust core free of UI and platform code. The CLI and the app are thin wrappers.
