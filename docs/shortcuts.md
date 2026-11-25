# Apple Shortcuts

Use the CLI from Shortcuts through the "Run Shell Script" action.
This bridge replaces the actions that Elgato Control Center removed.

## Install

```sh
uv tool install .
```

The command installs to `~/.local/bin/keylight`.
Use the absolute path in Shortcuts.
Shortcuts does not load your shell profile, so it may not find the command by name.

## Turn every light on

1. Open Shortcuts.
2. Create a shortcut.
3. Add the "Run Shell Script" action.
4. Set Shell to `/bin/zsh`.
5. Set the script to this text:

```sh
$HOME/.local/bin/keylight on --all
```

6. Run the shortcut.

## Turn every light off

Use the same action with this text:

```sh
$HOME/.local/bin/keylight off --all
```

## Set brightness

1. Add the "Ask for Input" action.
2. Set the input type to Number.
3. Add the "Run Shell Script" action.
4. Set the script to this text, and insert the input variable in place of `50`:

```sh
$HOME/.local/bin/keylight brightness 50 --all
```

The CLI validates the value.
A value outside 0 to 100 fails with exit code 2.

## Set warmth

Use the same pattern with the `warmth` command.
Warmth 0 is cool. Warmth 100 is warm.

```sh
$HOME/.local/bin/keylight warmth 70 --all
```

## Read the state

Use the `status --json` command.
Add a "Get Dictionary Value" action to read a field.

```sh
$HOME/.local/bin/keylight status --json
```

## Local network permission

The first run can show a local network permission prompt.
Allow it for Shortcuts and for Terminal.
Without the grant, the command cannot reach the lights.

## Notes

- `--all` applies the command to every device in the registry.
- A device name applies the command to one device.
- `--wait 180` retries an off light for three minutes.
- The command prints JSON with `--json`.
- The exit code is 0 on success and 1 when a device is unreachable.
