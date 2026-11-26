# Apple Shortcuts

Elopli provides native Shortcuts actions through App Intents.
The actions are part of `Elopli.app`.

## Signing requirement

Native App Intents require a validated app bundle with a team ID.
An ad-hoc signed app is rejected by the system `linkd` service with:

```text
Unable to get teamId from com.bevry.elopli
```

Use your Apple Development team in Xcode.
The paid Apple Developer Program is not required for local App Intents.

```sh
DEVELOPMENT_TEAM=XXXXXXXXXX ./scripts/build-macos.sh
```

The team ID is shown in Xcode under Settings > Accounts.

A team-signed build registers the App Intents with Shortcuts.
Without a team-signed build, use the CLI fallback below.

## Install

1. Build with your team ID, or copy a team-signed `Elopli.app` to `~/Applications` or `/Applications`.
2. Open the app once.
3. Open Shortcuts.

The Elopli actions appear in the action list.

## Actions

- **Turn Light On** — turn one light, or all lights, on.
- **Turn Light Off** — turn one light, or all lights, off.
- **Toggle Light** — toggle one light, or all lights.
- **Set Light Brightness** — set brightness from 2 to 100.
- **Set Light Warmth** — set warmth from 0 (cool) to 100 (warm).
- **Get Light State** — read the power, brightness, and warmth.
- **Rename Light** — change the light display name.
- **Set Power-On Behaviour** — restore the last setting, or restore defaults with a default brightness and warmth.
- **Pulse Light** — pulse brightness, then restore the original values.
- **Sweep Light** — sweep temperature across the full range, then restore the original values.

A new action stops a running pulse and applies its own change; the pulse restores the original state before the action writes.
A pulse also stops when the light changes outside Elopli, and that external change is preserved.
Quitting the app cancels running animations and waits for the restore before exiting.
Pulse changes use one persistent connection and immediate keyframes at about 10 Hz.
The `curl` transport is refused for animations because it opens a new connection for every request.

The **Lights** parameter is a multi-select picker.
Select one or more lights, or **All Lights**.
An empty selection is an error; it never means all lights.
Multi-light actions run in parallel and report per-light failures.
The **Rename Light** action uses a single-light picker.

Pulse parameters:

- **Count** — number of peak-to-minimum pulses, 1 to 20.
- **Duration** — full duration of the pulse animation (default `1.0s`). Accepts seconds by default, or an explicit unit: `1.0s`, `500ms`, `1.5`, `10m`, or `1h`. It is split by distance across the initial rise, each fade, and the final restore. The app sends its own keyframes so the duration stays exact. The duration must leave 100 ms per endpoint segment and may be up to one hour.
- **Peak Value** — peak brightness percentage.
- **Low Value** — low brightness percentage (default 0). Set it to 2% or higher to keep the light on during each pulse.

The low defaults to 0% (off).
The next rise turns the light on again.
The low hold is part of the low segment's budget, up to 100 ms.
An off light starts and ends off: count 1 runs off → peak → off, and count 2 adds the low between the peaks.

Sweep parameters:

- **Count** — number of warm-to-cool sweeps, 1 to 20.
- **Duration** — full duration of the sweep animation (default `1.0s`). Accepts seconds by default, or `s`, `ms`, `m`, or `h`. The first and final moves are weighted by distance, and the final weight includes any brightness restoration.
- **Brightness** — brightness to hold during the sweep. 0 or 1 means N/A and keeps the current brightness; 2 to 100 is held for the whole sweep. An off light ignores its stored temperature, starts at the warm end, ramps up to its stored brightness, sweeps to the cool end, then turns off.

## Local network permission

The first command can show a local network permission prompt.
Allow it for Elopli when using the App Intents.
When using the CLI fallback, grant local network access to Shortcuts or Terminal, because the CLI inherits the responsible parent process.
Without the grant, the app cannot reach the lights.

## Shortcut examples

- Turn all lights on when a meeting starts.
- Turn all lights off when a meeting ends.
- Set brightness to 40% for evening work.
- Set warmth to 75% for a warm scene.
- Read the state and show it in a notification.

## CLI fallback

The CLI can also run from **Run Shell Script**:

```sh
$HOME/.cargo/bin/elopli on --all
```

Use the absolute path.
Shortcuts does not load your shell profile.
