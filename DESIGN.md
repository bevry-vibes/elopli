# DESIGN.md — Elopli

Assisted-by: OpenCode · DeepSeek V4.1 Flash <opencode-opencodego-deepseekv41flash@local>

**Status:** evergreen design reference · last agreed 2026-11-26 · product name: Elopli · core: Rust · first UI: macOS SwiftUI.

This document records what is true now, why it is true, and what would make us revisit it.
It is not a changelog.
Dated work lives in [`.plans/`](.plans/); hardware evidence lives in [docs/api.md](docs/api.md) and [docs/control-center.md](docs/control-center.md); the platform plan lives in [docs/platforms.md](docs/platforms.md); the verbatim prompt history lives in the [plan companion](.plans/1795599587000-official-client-language.prompts.md).

## 1. Problem

Elgato Control Center no longer provides Apple Shortcuts actions for the Key Light and Key Light Air.
The lights still expose a local HTTP API on the local network.
Elopli replaces the lost control surface with a CLI, native apps, and native Shortcuts actions.
The client must work without the cloud, without an account, and without the vendor app.

## 2. Goals and non-goals

Goals:

- Control power, brightness, and warmth from a CLI and a native UI.
- Discover lights on the local network.
- Offer the same actions to Apple Shortcuts through App Intents.
- Keep animations smooth, exact, and interruptible.
- Keep the Rust core free of UI and platform code.
- Ship native UIs per platform instead of one cross-platform toolkit.

Non-goals:

- Cloud control, accounts, authentication, or encryption.
- Firmware replacement, reverse engineering for modification, or a custom firmware.
- WebSocket control for the Key Light and Key Light Air.
- A draggable window or a Dock icon on macOS.
- Keeping the retired Python reference CLI.

## 3. Architecture

| Part | Choice | Reason |
| --- | --- | --- |
| Shared core | Rust | One protocol and animation implementation for every UI. |
| CLI | Rust | Non-interactive, scriptable, usable from Shortcuts and the shell. |
| macOS UI | SwiftUI `MenuBarExtra` with `LSUIElement` | Native menu-bar app with no Dock icon. |
| Rust to Swift bridge | UniFFI | Generated bindings keep the boundary explicit. |
| Windows UI | Rust with `windows-reactor` | Planned; same core, native controls. |
| Linux UI | Rust with `gtk4-rs` and `ksni` | Planned; GNOME needs the AppIndicator extension. |
| Linux packaging | AppImage built with `appimagetool`/`linuxdeploy`, distributed through Soar | Flatpak is shelved. |
| Windows packaging | Scoop bucket and a portable ZIP | Inno Setup is shelved. |
| macOS signing | Apple Development team signing | App Intents do not register from an ad-hoc build. |

The CLI is a thin wrapper around the core.
The macOS UI is currently a renderer plus a session layer for grouping, live slider writes, config reloads, and discovery presentation.
The core owns validation, transport, discovery, registry persistence, and animation; it depends on platform services such as `dirs`, `mdns-sd`, file locks, and `curl` only through the transport layer.
A UI may present state, but it must not implement protocol rules of its own.

## 4. Device protocol and transport

The protocol facts come from direct tests on firmware 1.0.3 (222).
The full reference is [docs/api.md](docs/api.md).

| Property | Value |
| --- | --- |
| Protocol | HTTP/1.1 |
| Port | TCP 9123 |
| Encryption | None |
| Authentication | None |
| Request and response | JSON |
| Connection | Keep-alive |
| Discovery | mDNS `_elg._tcp` |

Main endpoints:

| Method | Path | Use |
| --- | --- | --- |
| GET | `/elgato/lights` | Read power, brightness, and temperature. |
| PUT | `/elgato/lights` | Write one light object. |
| GET | `/elgato/accessory-info` | Read product, firmware, and WiFi details. |
| PUT | `/elgato/accessory-info` | Rename the light. |
| GET | `/elgato/lights/settings` | Read power-on behaviour and fade settings. |
| PUT | `/elgato/lights/settings` | Write the complete settings object. |
| POST | `/elgato/identify` | Flash the light. |

Rules:

- Send only the fields that change. An omitted field keeps its value.
- Send one object in the `lights` array. The light ignores the rest.
- Keep brightness in 2 to 100 for normal control. Values 0 and 1 switch the light off.
- Keep temperature in 143 to 344 mired. The device accepts out-of-range values and produces the wrong light.
- Keep `on` at 0 or 1.
- Do not call `/elgato/restart` or `/elgato/factory-reset` during normal control.
- Discard the serial number and the network interface address. Never print, store, log, or commit them.

Security stance:

- The API has no authentication and no encryption.
- Use Elopli only on a trusted local network.
- Do not expose a light or the client to the public internet.
- Keep `devices.json` out of version control. Commit `devices.example.json` only.

### Control Center comparison

Control Center uses plain HTTP for the Key Light and Key Light Air.
Its WebSocket support is model-gated to the Key Light Air Mk2; the tested models never negotiate WebSocket.
Elopli therefore stays on HTTP for these lights.
The evidence and the transport comparison are in [docs/control-center.md](docs/control-center.md).

### Persistent connection rule

Keep one persistent HTTP connection for an animation.
A new TCP connection for every keyframe overloads the small HTTP server and can make a light stop answering for about 18 seconds.
The native transport shares one process-wide `ureq::Agent`, so refresh, slider writes, and animations reuse the same connection pool.
The `curl` availability probe is cached once per process.
The `curl` transport opens a new connection for every request, so the core refuses to animate over `curl`.
This is a hard safety rule, not a performance hint.

The device registry is written atomically under a file lock.
Each load records a fingerprint; a save fails with `config_conflict` if another process changed the file first.
The app reloads the registry while it polls, and the CLI reports an absolute `config path`.
The loader accepts a UTF-8 BOM and validates duplicate names and hosts case-insensitively.

## 5. Fade and animation design

### 5.1 Firmware fade facts

These facts were verified on the Back, Left, and Right lights on firmware 1.0.3 (222):

- A `durationMs` value below 1000 ms makes the firmware fade the change.
- A `durationMs` value of 1000 ms or more makes the firmware apply the target at once.
- A write without `durationMs` applies at the API level, and the stored `colorChangeDurationMs` (100 ms by default) smooths it.
- The device has no documented minimum fade time. A 100 ms request can feel longer than 100 ms.
- Brightness 0 and 1 are physically off. Brightness 2 is the lowest visible level.
- A fade can pass through values below 2 on its way to off. Those values are not reachable through normal control.

The core keeps `MAX_FADE_MS = 900` and rejects explicit fades above it.
Animations do not use `durationMs`; they send their own immediate keyframes.

### 5.2 Frame schedule

The client sends keyframes itself.
Each keyframe is an ordinary write with no `durationMs`, so the stored colour fade smooths the step.

- The rate cap is 10 Hz (`FRAME_INTERVAL_MS = 100`). Pulse and sweep validation rejects a duration that cannot reach every endpoint at that rate.
- When the duration allows, the client sends one step per brightness percent or per mired.
- The step count is `min(distance, duration / 100 ms)`, with a minimum of one step.
- A long fade therefore visits every 1% point.
- A short fade uses fewer steps at up to 10 Hz.
- The final step always reaches the target and holds it until the segment deadline.
- Intermediate steps may be dropped by the schedule. Endpoints are never dropped.

The segment deadline is absolute, not per step.
The client computes each deadline from the segment start and sleeps only for the remaining time.
This keeps the total duration exact even when a write is slow.

### 5.3 Duration budget

`duration_ms` is the duration of the whole animation, not of each count.

Every segment receives a weight:

- A full peak-to-low move has weight 1.
- The initial move has weight `distance / (peak - low)`, clamped to 0 to 1.
- The final return has weight `distance / (peak - low)`, clamped to 0 to 1.
- The sweep uses the mired span for the same calculation.

`distribute_durations` normalises the weights, rounds each segment, keeps a floor of `min(100, total / segment_count)`, and adjusts the largest segment so the sum is exactly `duration_ms`.
Validated animations require at least 100 ms per endpoint segment, so the floor is 100 ms in normal use.
A 1 s pulse with count 1 from 50% to peak 100 and low 0 therefore splits 250 ms rise, 500 ms fall, and 250 ms return.

### 5.4 Low hold

A pulse may hold its low for a short time so the minimum is visible.

- The hold is `min(segment_duration / 4, 100 ms)`.
- The hold is part of the low segment's budget, not extra time.
- The cap stops long pulses from hanging at the low.
- A sweep has no low hold. Its hold is always zero.
- The only other hold is the end-of-segment endpoint hold in the frame schedule.

There is no user-facing hold setting.
The low hold is an implementation detail; the duration remains the full animation time.

### 5.5 Pulse

A pulse changes brightness only.

For an on light:

```text
original → peak → low → peak → low → … → original
```

For an off light:

```text
count 1: off → peak → off
count 2+: off → peak → low → peak → … → off
```

Rules:

- Count `n` means `n` peak-to-low fades.
- The peak is configurable from 2 to 100.
- The low defaults to 0 (off) and can range from 0 to the peak.
- The low is sent without an explicit power field, so brightness 0 turns the light off at the low.
- The next peak sends `on: true`, so the light returns for the next fade.
- For an on light, the final segment targets the original brightness and power.
- For an off light, the final segment targets 0%, then the core confirms power off.
- The final ramp always runs, even when the original brightness equals the low. The old code skipped it and snapped.
- An off light first turns on at 2%, then rises. It must not jump on at its stored brightness.

The exact original state is restored when the pulse completes.
If another action interrupts the pulse, that action wins and the pulse does not restore over it.
If the light changes outside Elopli, the pulse stops and preserves the external change.

### 5.6 Sweep

A sweep changes temperature only.

```text
current → warm → cool → warm → cool → … → original
```

Rules:

- Warm is 100% warmth (344 mired). Cool is 0% warmth (143 mired).
- Count `n` means `n` warm-to-cool moves.
- An optional brightness override holds one brightness for the whole sweep.
- A brightness override of 0 means "leave brightness alone".
- A light that is off or below 2% turns on at 2%, ramps to its stored brightness (or the override) over up to 1 s taken from the initial segment, then sweeps.
- The final segment restores the original temperature and brightness for a visible light; for an off light it fades to 0% and then restores the exact original power state.
- The final segment's weight is the larger of its temperature and brightness distances.
- A sweep has no low hold.
- The sweep reuses the pulse frame schedule, duration budget, cancellation, and external-change checks.

The sweep was deliberately shelved until the pulse was correct.
It now shares the pulse logic instead of carrying its own frame loop.

### 5.7 Cancellation

Every action takes the cross-process control lock and cancels the previous action before it writes.
The running pulse checks its token before every frame.
A superseded pulse stops without restoring.
The pulse also compares the light's state with the value it last wrote.
A difference means an external change; the pulse stops and keeps that change.

The pulse checks every frame for durations up to 2 s.
Longer pulses check every 250 ms.
The sweep checks every 250 ms.
This keeps short pulses safe without adding a GET to every long frame.

Animation frames, external checks, and final cancellation checks use a persistent-only request path.
If the native connection fails mid-animation, the core aborts instead of falling back to `curl`.
A superseded animation restores its original state before releasing the lock, so Stop and Quit leave the light unchanged.
An external change is still preserved.

### 5.8 No easing

The animation uses linear interpolation only.
Ease options were removed.
They added configuration and visual ambiguity without improving the endpoint timing.
The distance-weighted budget and the endpoint hold provide the smoothness.

## 6. Hard-won lessons and revisions

| Earlier approach | Current approach | Why |
| --- | --- | --- |
| `durationMs` on each device write | Immediate keyframes with no `durationMs` | The firmware snaps at 1000 ms or more, and device fades were too slow or unreliable. |
| A frame rate with a state GET before every write | One frame per percent or mired, with external checks at 250 ms for long animations | GETs on every frame made long pulses steppy and consumed the connection budget. |
| Per-count duration | One duration for the whole animation | The user asked for the full animation to take the requested time. |
| Drop frames when the schedule is tight | Drop only intermediate frames | The peak, low, and final value must always be reached. Dropped endpoints looked like an 80% pulse. |
| Fixed 10 Hz for every duration | 10 Hz cap, or one step per percent/mired when possible | Long animations need every 1% point; short animations need the 10 Hz cap. |
| Final restore sent immediately after the last fade | Final segment included in the budget, then no extra restore for an on light | The old snap rushed the last part of the fade. |
| Long or uncapped low hold | `min(segment / 4, 100 ms)` | The low hung on long pulses. |
| Off light starts at its stored brightness | Off light starts at 0 and turns on at 2% | The stored brightness made an off light jump on. |
| Off light count 1 used the on-light target plan | Off → peak → off | The off light has no original visible state to return to. |
| Ease options | Linear only | Removed per user decision. |
| Sweep had its own slower frame loop | Sweep reuses the pulse ramp | The separate loop produced different motion and more bugs. |
| Low shown as a single value | Dual-handle low/peak range slider | The low must be configurable from 0 to the peak. |
| UI shows 2% as 0% | UI shows the actual value | The user rejected the translation; the light is what it is. |
| Group values sync only when a value changes | Sync on group creation, using the first card in UI order | A new group must start consistent. |
| Grouped slider follows the dragged slider with a delay | Shared drag state moves both in tandem | A visible lag looked broken. |
| Discover Lights shows Add for known lights | Known lights show Added; offline known lights show Missing | Re-adding a known light is meaningless. |
| Native tooltip delay | `NSInitialToolTipDelay = 500` | System tooltips took too long. |
| WiFi details hidden in the system tooltip | 500 ms custom popover with SSID, frequency, RSSI, quality, and IP | The details need multiple lines and a fast reveal. |

## 7. macOS app design

### 7.1 Shell

- Use SwiftUI `MenuBarExtra` with `.menuBarExtraStyle(.window)`.
- Use `LSUIElement`; there is no Dock icon and no draggable window.
- The menu panel polls state every five seconds, reloads the registry, and refreshes on activation and after an action.
- Refresh polling pauses for a light that is animating.
- The app uses App Intents for Shortcuts.
- Multi-light intents run in parallel and report per-light outcomes.
- An empty light selection is an error; only the explicit All Lights entity means all lights.
- Native App Intents require team signing; an ad-hoc build is rejected by `linkd`.

### 7.2 Cards and controls

Each light is a card with power, brightness, warmth, WiFi, rename, power-on behaviour, and animation actions.
The card title is the local registry name; the subtitle shows the light's hardware display name when it differs, so Rename is visible immediately.
The brightness slider colour follows the active warmth.
The brightness slider is thin at the low end and thick at the high end.
The slider control has a glow whose size follows brightness.
The low glow is absent when the light is off and the slider is not being dragged.
The card uses material and a border; it has no drop shadow.

Durations in the pulse and sweep sheets accept `1.0s`, `500ms`, `10m`, `1h`, or a bare number as seconds, with a default of 1.0s.
The CLI accepts the same forms.
Pulse and sweep durations must leave at least 100 ms per endpoint segment and may be up to one hour.
The pulse sheet has a dual-handle low/peak range slider.
The sweep sheet has a brightness override; 0% reads N/A and means "leave brightness alone".

### 7.3 Grouping

- Hold Command and click a card, toggle, or slider to start a link.
- Pending selections use yellow; committed links use magenta.
- A card link is shorthand for linking all three fields on that card.
- A field on an unlinked card joins only the matching field group.
- Command-clicking a complete group ungroups its fields and cards.
- If a card is grouped and one of its fields is Command-clicked, the card unlinks and the field enters the pending state.
- Group creation copies values from the first card in UI order.
- Card groups are maximal and must not overlap; a newly complete card group replaces any intersecting card group.
- A field group that is a subset of a card group is redundant and is removed.
- Group actions apply only to a card group; field-only links do not show Pulse Group or Sweep Group.
- Grouped sliders move in tandem during a drag, with one write coordinator per linked group.
- Temperature sync writes mired, not a warmth percentage, so it does not re-quantise.
- A refresh never overwrites a device whose local revision changed after the refresh started.

### 7.4 Actions

Each card menu offers:

```text
Pulse…
Sweep…
Pulse Group…   (only when the card is grouped)
Sweep Group…   (only when the card is grouped)
```

The group actions run the same animation on every member of the group.

### 7.5 Discovery

- The discovery list shows known devices first and new devices last.
- A known device that is present shows a disabled **Added** button.
- A known device whose advertised address changed shows **Update** to save the new address.
- A known device that is absent shows a red **Missing** pill.
- Hovering **Missing** advises a power cycle, a wait for reconnection, and another scan.
- The header stays top-aligned while the scan runs.
- New devices show **Add**.

### 7.6 Tooltips

- All system help tooltips appear after 500 ms.
- The WiFi icon uses a custom 500 ms popover instead of the system tooltip.
- The WiFi popover shows the SSID, frequency, RSSI, quality, and the light's IP address.
- A missing light shows the error and the power-cycle advice.

## 8. Data and privacy

- Discard the serial number and MAC address at parse time.
- The client knows the IP address from the registry or from mDNS, not from the light API.
- Never print, store, log, or commit a serial number or MAC address.
- Keep `devices.json` out of version control.
- Commit `devices.example.json` only.
- The API has no authentication and no encryption. Use it only on a trusted local network.
- The client needs no secrets.

## 9. Build and packaging

- Build the macOS app with `./scripts/build-macos.sh`.
- Set `DEVELOPMENT_TEAM` to a real team for App Intents.
- Do not commit `apps/macos/Elopli/Generated`, `apps/macos/Elopli/build`, or `apps/macos/Elopli/Elopli.xcodeproj`.
- The Rust core stays free of UI and platform code.
- The CLI stays non-interactive and supports absolute paths and JSON output.
- macOS requires Local Network permission for the app or its parent process.

## 10. Decision register

| # | Decision | Revisit if |
| --- | --- | --- |
| D1 | Rust core with native per-platform UIs | A native toolkit can share UI code without losing platform behaviour. |
| D2 | HTTP only for Key Light and Key Light Air | A firmware update adds WebSocket to these models, or the target becomes the Air Mk2. |
| D3 | Client-side immediate keyframes | The firmware gains a reliable fade for long durations. |
| D4 | 10 Hz cap with one step per percent or mired | The light accepts a higher rate without dropping connections or missing endpoints. |
| D5 | Distance-weighted segment budgets | A different timing model produces visibly better motion and still hits every endpoint. |
| D6 | Low hold capped at 100 ms | The low becomes hard to see at short durations, or long pulses still hang. |
| D7 | Low defaults to 0 (off) | Users expect a visible low by default. |
| D8 | Off light runs off → peak → off for count 1 | The off-light start or end becomes visually wrong. |
| D9 | Sweep has no hold | A sweep needs a visible pause at warm or cool. |
| D10 | Linear motion only | A specific ease is proven to help and users ask for it. |
| D11 | Persistent connection, and no animation over `curl` | The `curl` path can reuse a connection safely. |
| D12 | Actual brightness values in the UI | Users prefer the 2% floor to read as 0%. |
| D13 | Group creation syncs to the first card in UI order | The UI order becomes unstable or users expect another source. |
| D14 | Durations in seconds, default 1.0 | Millisecond precision becomes necessary for very short animations. |
| D15 | 500 ms tooltips | The delay feels too fast or too slow in use. |
| D16 | Serial and MAC discarded | A feature needs a stable hardware identity. |
| D17 | No auth or encryption | The threat model changes or the client leaves the trusted LAN. |
| D18 | macOS menu bar only | A full window becomes a product requirement. |

## 11. Open items

- Windows UI with `windows-reactor`.
- Linux UI with `gtk4-rs` and `ksni`.
- AppImage packaging; Flatpak is shelved.
- Scoop bucket and portable ZIP; Inno Setup is shelved.
- A stable public repository remote for Elopli.
- Accessibility, keyboard navigation, and localization are deferred until after v1.
- The FFI is synchronous; a Windows/Linux UI must call it on a worker thread until async wrappers exist.
- Extract the remaining macOS session logic (link graph, slider coordinator, discovery rows) into a shared crate before the Windows/Linux UIs.
- Add a Windows/Linux CI matrix and real-hardware smoke tests.
