# Security

Elopli controls lights over the local network.
The device API has no authentication and no transport encryption.
Use Elopli only on a trusted local network.

## Reporting

Report a vulnerability through a private GitHub security advisory for the repository.
Do not open a public issue for a vulnerability that could affect users on a shared network.

## Scope

- The client discards device serial numbers and MAC addresses.
- `devices.json` and `links.json` are local and are ignored by Git.
- The client has no cloud service, account system, or telemetry.
- The HTTP API is plain HTTP on TCP 9123.
- A request with an `Origin` header is rejected by the light with HTTP 403.
- The Windows app uses the same local API and stores no credentials.

## Hardening

- Keep the lights on a trusted VLAN or SSID.
- Do not expose a light or the client to the public internet.
- Keep the client and its dependencies current.
- On Windows, allow Elopli only on private networks in Windows Defender Firewall.
- Power-cycle a light if its HTTP server stops responding after a burst of requests.
