# Privacy

Elopli does not collect telemetry, analytics, accounts, or cloud data.

## Stored locally

- `devices.json` stores device names, hosts, and ports.
- `links.json` stores link groups next to the registry.
- The macOS app stores link groups and the launch-at-login preference in `UserDefaults`.
- The Windows app stores the launch-at-login choice in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
- The control lock uses a per-light file in the platform state directory.
- No logs are written by default.

## In memory only

- Device serial numbers are held in memory while the app is running and shown only when the user hovers a card's model name. They are never written to disk, logs, telemetry, documentation, plans, or commits.

## Discarded

- MAC addresses.
- The mDNS interface identifier.

mDNS hostnames may be shown during discovery, but the add path requires an IPv4 address and stores only that address, not the hostname.

## Network

- The client talks directly to lights over the local network.
- The public internet is not used.
- The light API has no authentication or encryption.

Accessibility, localization, and any future diagnostics must preserve these rules.
