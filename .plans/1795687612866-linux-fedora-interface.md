# Linux and Fedora interface

Assisted-by: OpenCode · DeepSeek V4.1 Flash <opencode-opencodego-deepseekv41flash@local>

Provenance: [1795687612866-linux-fedora-interface.prompts.md](1795687612866-linux-fedora-interface.prompts.md)

**Status:** first delivery implemented on Fedora 45 Workstation, GNOME 51, Wayland · 2026-11-26 · full UI write smoke test, package installation, and Ubuntu 24.04 runtime validation open.

This plan starts the Linux UI recorded as an open item in [DESIGN.md](../../DESIGN.md) and [docs/platforms.md](../../docs/platforms.md). Evergreen decisions stay in DESIGN.md; this file records the dated work.

## Goal

Ship a native Elopli panel for Linux that uses the shared `elopli-core` and `elopli-session` crates, follows the macOS and Windows information architecture, and runs from a status-notifier tray on Fedora Workstation.

The first delivery is a working app, not a packaging-only spike:

- one card per light with power, brightness, warmth, WiFi details, rename, power-on behaviour, identify, pulse, and sweep;
- Ctrl-click linking with yellow pending and magenta committed borders;
- drag a card onto another card to swap positions;
- discovery sheet with Added, Missing, Update, and Add states;
- five-second refresh, live slider coalescing, and animation polling pause;
- XDG autostart, single instance, dark and light theming, and `--smoke-test`;
- desktop entry, AppStream metadata, hicolor and symbolic icons, and a build script.

## Verified environment

| Item | Value |
| --- | --- |
| OS | Fedora Linux 45 Workstation |
| Desktop | GNOME Shell 51.1, Wayland |
| GTK | 4.24.2 (`gtk4-devel` installed) |
| Tray host | `gnome-shell-extension-appindicator` 66 |
| Rust | 1.100.0, pinned by `rust-toolchain.toml` |
| AppIndicator | `libappindicator-gtk3` 12.10.1; not linked directly |
| Packaging tools | `desktop-file-validate`, `appstreamcli` present |

`ksni` 0.3.7 speaks StatusNotifierItem over D-Bus, so the app does not link `libappindicator`. GNOME only shows the item when the AppIndicator extension is enabled; Fedora enables it by default.

## Decisions for this delivery

| # | Decision | Reason | Revisit if |
| --- | --- | --- | --- |
| L1 | GTK4 through `gtk4-rs`, no libadwaita | The design already names GTK4; libadwaita adds a runtime dependency without carrying the custom Elopli look. | Adwaita widgets cover a required behaviour and the extra dependency is acceptable. |
| L2 | `ksni` blocking API on a background thread | The tray must not own the GTK main loop; the blocking handle lets GTK commands and tray callbacks rendezvous through channels. | The tray needs to start before the GTK app or run without D-Bus. |
| L3 | Keep `Session` on one worker thread, publish snapshots, update widgets in place | Matches the Windows renderer and keeps network calls off the GTK loop. Rebuilding the tree on every snapshot would break drags, focus, and slider gestures. | GTK observers or a model layer make full rebuilds cheap and stable. |
| L4 | Ctrl-click links, commit on Control release | Mirrors the Windows modifier and the macOS Command-click model. | GNOME users expect a context-menu or toggle workflow. |
| L5 | Custom-drawn sliders | GTK scales cannot express the tapered brightness track, warmth gradient, glow, or dual-handle pulse range. Drawing keeps the three platforms visually consistent. | GTK gains a styleable range widget that covers all four behaviours. |
| L6 | Regular undecorated window, tray menu is the primary surface | Wayland forbids client-side window placement, and GNOME has no layer-shell. The app cannot anchor a popover to the status icon. | A supported Wayland protocol or a GNOME extension exposes tray anchoring. |
| L7 | Single instance through `GtkApplication` and the `com.bevry.elopli` D-Bus name | GApplication already forwards activation to the running instance; no socket or lock file is needed. | The app must run without a session bus. |
| L8 | XDG autostart desktop entry under `~/.config/autostart` | The build skill's Linux login-autostart pattern; the app is a user-session tray app, not a daemon. | The app becomes a system service or a Flatpak. |
| L9 | Package as a portable tarball first, AppImage when `appimagetool` is available | Fedora runs the binary directly; the AppImage recipe stays optional so a missing tool does not block a build. | AppImage becomes the only supported distribution format. |
| L10 | The light's display name is authoritative; the registry key mirrors it | Per-interface nicknames made the same light appear under different names. The name stored on the light is the only one that follows the hardware. | The API stops exposing `displayName`, or users need per-machine aliases. |
| L11 | Tray click routing belongs to the host; the panel is a regular window | GNOME's AppIndicator extension opens the menu on a single left click and calls Activate on a double click. Wayland forbids client-side placement. | A supported Wayland protocol or a GNOME extension exposes tray anchoring and single-click activation. |

## Architecture

```text
apps/linux/Elopli
├── src/main.rs        process arguments, GtkApplication, single instance, smoke test
├── src/app.rs         panel window, cards, header/footer, commands, snapshots
├── src/worker.rs      Session thread, refresh/scan/animation child threads
├── src/sheets.rs      rename, power-on, pulse, sweep, discovery dialogs
├── src/slider.rs      brightness, warmth, and dual-handle range widgets
├── src/theme.rs       CSS, palette lookup, temperature colours
├── src/tray.rs        ksni StatusNotifierItem and menu
└── assets/            desktop entry, AppStream metainfo, icons
```

The worker is a near-port of `apps/windows/Elopli/src/session_worker.rs`. The UI sends `Command` values on a channel; the worker publishes `SessionSnapshot` values through a mutex and wakes GTK with `glib::MainContext::invoke`. The panel updates each card from the snapshot, guarded by an `updating` flag so programmatic slider writes do not echo back as user commands.

Tray menu callbacks run on the `ksni` thread. They send the same `Command` values to the GTK side through a channel that the main loop drains with a short timeout. The tray never touches widget state.

## UI sketch

```text
┌ Elopli                         ⟳  ☀  ☾  ⌕  ⚙ ┐
│ Ctrl-click a card, toggle, or slider to link. │
│ ┌───────────────────────────────────────────┐ │
│ │ Back                         (WiFi)  (on) │ │
│ │ Elgato Key Light          ⌄ menu          │ │
│ │ ☀ ───────●────────────── 62%              │ │
│ │ ♨ ────●───────────────── 44%              │ │
│ │ 267 mired · 3745 K                        │ │
│ └───────────────────────────────────────────┘ │
│ ┌───────────────────────────────────────────┐ │
│ │ Left                         (WiFi)  (on) │ │
│ └───────────────────────────────────────────┘ │
└───────────────────────────────────────────────┘
```

Header actions: refresh, all on, all off, discover, settings menu. Settings menu: Launch at Login, Reveal Config, Quit. The card menu mirrors Windows: Rename…, Power On Behaviour…, Pulse…, Pulse Group…, Sweep…, Sweep Group…, Identify.

## Milestones

1. **Scaffold and worker.** Workspace member with a Linux-only feature, worker thread, snapshot publishing, smoke test.
2. **Panel.** Header, cards, custom sliders, power switch, WiFi popover, errors, empty state, theming.
3. **Interactions.** Linking, drag reorder, card actions, rename, power-on behaviour, discovery, pulse and sweep sheets.
4. **Shell.** Tray icon and menu, autostart, single instance, reveal config, quit.
5. **Packaging and CI.** Desktop entry, AppStream metadata, icons, `scripts/build-linux.sh`, Linux CI job that builds with `--features linux`, docs.

## Risks and limitations

- **Wayland anchoring.** The panel cannot be positioned above the status icon. It opens as a regular window and the tray provides a menu. This is a GNOME/Wayland constraint, not a bug.
- **GNOME AppIndicator behaviour.** GNOME opens the tray menu on left click; `activate` is not guaranteed. The menu therefore carries Show Elopli. KDE and other hosts may call `activate` directly.
- **Animations.** The app uses the native transport only, matching the Windows rule. `curl` is never used.
- **Privacy.** The serial number remains in memory for the model hover popover only. It is never logged, persisted, or shown by default.
- **Hardware.** This machine has no Key Light attached. Protocol behaviour is verified by the core test suite; UI and discovery need a real light before the delivery is called hardware-tested.

## Acceptance

- [x] `cargo build -p elopli-app-linux --features linux --release` on Fedora.
- [x] `cargo clippy -p elopli-app-linux --features linux --all-targets --locked -- -D warnings`.
- [x] `cargo fmt --all -- --check`.
- [x] `./target/release/elopli-app --smoke-test` reads the registry and exits 0.
- [x] The panel opens from the tray menu and displays a card for each configured light; the three-light panel was rendered against real hardware.
- [x] `desktop-file-validate` and `appstreamcli validate` accept the packaged metadata.
- [x] The tray registers with `org.kde.StatusNotifierWatcher`, single instance forwards activation, and Launch at Login writes the XDG entry.
- [ ] Write actions from the panel (power, sliders, rename, pulse, sweep, discovery add/update) need a full click-through smoke test.
- [ ] Install the tarball on a clean Fedora and Ubuntu 24.04 machine and verify the desktop icon, tray icon, and autostart.
- [ ] KDE Plasma validation and the AppImage branch with `appimagetool`.

## Delivery notes added after the first hardware run

- The Linux panel ran against a Key Light Air on Fedora 45 / GNOME 51 Wayland and showed live brightness, warmth, mired, Kelvin, WiFi details, and power state.
- The tray registered with `org.kde.StatusNotifierWatcher`; GApplication single instance, XDG autostart, and `--smoke-test` were verified.
- Header order is now Refresh, Discover, All On, All Off, Settings, matching macOS and Windows.
- Header title is packed at the start, so it reads left-aligned like the other apps.
- The panel starts at a content-sized height instead of a fixed 640 px.
- The light's display name is now authoritative in the shared session. Rename updates the registry key and link groups; refresh adopts a remote name changed elsewhere; the CLI `rename` and `discover --write` follow the same rule. DESIGN D25 records the decision.
- Tray click routing is host-defined. GNOME's AppIndicator extension opens the menu on a single left click, calls Activate on a double left click, and calls SecondaryActivate on a middle click. The app maps both Activate and SecondaryActivate to the panel. Wayland prevents anchoring the window next to the tray icon. DESIGN D26 records the limitation.
- `ELOPLI_SCREENSHOT` renders the panel through the GTK render node for Wayland-safe visual checks.
- Packaging assets: desktop entry, AppStream metainfo, hicolor app icons, `scripts/build-linux.sh`, and a Linux CI job.

## Remote names from the macOS registry

The macOS registry listed Back on 192.168.4.36, Left on 192.168.4.37, and Right on 192.168.4.39.
A read-only check from Fedora found that Left and Right were already stored on those lights, while the 192.168.4.36 light had no display name.
`elopli rename 192.168.4.36 Back` wrote the missing name, and `elopli discover --write` added Left and Right to the Fedora registry from their remote names.
The Fedora registry now holds `Back`, `Left`, and `Right`, and the three-card panel renders them in that order.

## Post-delivery polish

The first Soar install exposed the difference between a portable tarball and a desktop-integrated app.

- Soar 0.14.2 already installs local AppImages. `soar install --name elopli-app --version 0.1.0 ./Elopli-0.1.0-linux-x86_64.AppImage` copies the AppImage, links `elopli-app`, and writes the desktop entry and icon. The `elopli-app` name is deliberate: installing as `elopli` shadows the CLI.
- The launcher showed no icon because the user hicolor icon cache was stale. Installing a standard hicolor `index.theme` and running `gtk-update-icon-cache` made `elopli-soar` resolve to the 256 px icon. GTK's icon lookup now returns the file.
- The slider glow was clipped at the widget edge. The slider height is now 40 px, the glow radius is capped to fit, and the horizontal inset is 21 px.
- The brightness track is drawn with a rounded left cap and a rounded right cap instead of a sliced rectangle.
- The warmth slider uses Microsoft's Fluent filled fire path. The Windows app already embeds the same Fluent icon set; the MIT attribution in THIRD-PARTY-NOTICES now names both apps.
- WiFi icons use the light's RSSI quality: green for excellent/good, amber for fair, red for weak or offline.
- The panel chrome is close-only. Close hides the panel to the tray; Quit is in the tray menu. The titlebar/content divider is removed.
- The switch uses a white knob and a subtle border so it matches the libadwaita switch in GNOME Settings without adding libadwaita.
- Cards drag from the background; the grip icons are gone. The drag source ignores presses that start on a slider, switch, or button. The DnD payload is a plain string so drop targets accept it.
- Fixed a label bug where dragging the warmth slider rewrote the brightness value text and made it flicker.
- The installed AppImage is live and the tray is registered. The remaining user check is the drop half of card reordering.

- The power switch is measured against the brightness slider at runtime. Its margin and the WiFi button's margin are both set so the on-knob centre matches the slider's rightmost thumb centre and the gaps on each side of the switch stay equal. The measured offset was 2 px in the Fedora 45 Adwaita dark theme.

- A horizontal `GtkSizeGroup` now binds the brightness and warmth value labels, so both slider tracks end at the same X even when one label reads `100%` and the other `0%`. The rightmost thumb of both sliders and the on knob of the power switch share one column in the rendered panel.
- The header title is vertically centred and nudged 2 px down to sit on the same centreline as the action icons.

- The runtime measurement and size group were removed. Each value label now has a fixed 32 px width and the WiFi and power controls use fixed 2 px margins. Changing a value from `100%` to `99%` cannot change any card geometry or shift the WiFi icon, toggle, or warmth row.
- The header title is 19 px and uses cap-height centring. The alignment reference is the baseline of E/o/l/i, the reference height is the cap height, and the p descender is allowed to hang below the icon centreline.

- The header title now uses x-height centring: the lowercase `o` shares the icon centreline (measured o centroid 23.0 px vs icon centroid 22.5 px), while the E/l ascenders rise above and the p descender hangs below.
- The card menu is flat and the same 30 px allocation as the WiFi button. Fixed 5 px power and 10 px WiFi margins align the on knob with the rightmost slider thumb and leave 29 px and 28 px of visible whitespace on each side. No runtime measurement remains, so value changes cannot nudge the controls.

- The card title CSS class was never attached to the label, so the 600 and 700 weights had no effect. The class is now applied and the weight is 800 (Extra Bold).
- The toggle moved another 2 px left; its right edge now matches the slider thumb's right edge. The WiFi margin is compensated so the visible gaps stay equal at 30 px on both sides.
- The card menu regains its inset-border hover state while staying flat at rest.

- The WiFi button now uses the same 7 px margin as the power switch, so the allocation and hover borders sit equally far from the toggle. The WiFi icon gains the same inset hover treatment as the menu button.
- Card names use weight 600 now that the CSS class is actually attached; 800 was confirmed working but 600 is the chosen semibold.

- The switch slider has an explicit 200 ms CSS transition, so programmatic All On/All Off changes animate like user toggles (subject to the desktop's enable-animations setting).
- The card menu and settings menu share hover, active, checked, and transition rules. The card menu regains a visible pressed state.
