# Apple Shortcuts

Optoli provides native Shortcuts actions through App Intents.
The actions are part of `Optoli.app`.

## Signing requirement

Native App Intents require a validated app bundle with a team ID.
An ad-hoc signed app is rejected by the system `linkd` service with:

```text
Unable to get teamId from com.bevry.optoli
```

Use a free Apple ID Personal Team in Xcode. This is not the paid Apple Developer Program.
Sign in to Xcode with your Apple ID, then build with your team ID:

```sh
DEVELOPMENT_TEAM=XXXXXXXXXX ./scripts/build-macos.sh
```

The team ID is shown in Xcode under Settings > Accounts.

Without a team-signed build, use the CLI fallback below.

## Install

1. Build with your team ID, or copy a team-signed `Optoli.app` to `~/Applications` or `/Applications`.
2. Open the app once.
3. Open Shortcuts.

The Optoli actions appear in the action list.

## Actions

- **Turn Light On** — turn one light, or all lights, on.
- **Turn Light Off** — turn one light, or all lights, off.
- **Toggle Light** — toggle one light, or all lights.
- **Set Light Brightness** — set brightness from 0 to 100.
- **Set Light Warmth** — set warmth from 0 (cool) to 100 (warm).
- **Get Light State** — read the power, brightness, and warmth.
- **Rename Light** — change the light display name.
- **Set Power-On Behaviour** — restore the last setting, or restore defaults.

The **Light** parameter accepts a device name from the registry, or `all`.

## Local network permission

The first command can show a local network permission prompt.
Allow it for Optoli.
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
$HOME/.cargo/bin/optoli on --all
```

Use the absolute path.
Shortcuts does not load your shell profile.
