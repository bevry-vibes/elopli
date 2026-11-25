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
- **Set Power-On Behaviour** — restore the last setting, or restore defaults.
- **Pulse Light** — pulse brightness, then restore the original values.
- **Sweep Light** — sweep temperature across the full range, then restore the original values.

A new action stops a running pulse and applies its own change.
A pulse also stops when the light changes outside Elopli, and that external change is preserved.
Pulse frames are paced conservatively to avoid overloading the light's HTTP server.
A native client leaves at least 120 ms between frames; the `curl` fallback leaves at least 250 ms.

The **Lights** parameter is a multi-select picker.
Select one or more lights, or **All Lights**.
The **Rename Light** action uses a single-light picker.

Pulse parameters:

- **Count** — number of peak-to-minimum pulses, 1 to 20.
- **Duration** — duration of the entire pulse animation in milliseconds (default 3000).
- **Ease** — linear (default), ease in, ease out, or ease in out.
- **Peak Value** — peak brightness percentage.

The minimum is 2%; the light stays on at that level.

Sweep parameters:

- **Count** — number of warm-to-cool sweeps, 1 to 20.
- **Duration** — duration of the entire sweep animation in milliseconds.
- **Ease** — linear (default), ease in, ease out, or ease in out.
- **Brightness** — brightness to hold during the sweep. 0 keeps the current brightness; any other value is held for the whole sweep.

## Local network permission

The first command can show a local network permission prompt.
Allow it for Elopli.
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
