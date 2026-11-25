# How Elgato Control Center talks to Key Light

This document records how Elgato Control Center 1.9.2 (21032) controls the
Key Light and Key Light Air on firmware 1.0.3 (222).
It separates behaviour proven on the hardware from behaviour inferred from the
app binary, its bundled firmware images, and its logs.

Tested devices:

| Name | Product | Firmware |
| --- | --- | --- |
| Back | Key Light Air | 1.0.3 (222) |
| Left | Key Light | 1.0.3 (222) |
| Right | Key Light | 1.0.3 (222) |

No serial number or MAC address is reproduced here.
Right was returned to `off`, brightness 50, temperature 231 after the probes.

## Executive answer

**Control Center does not use WebSocket with the Key Light or Key Light Air on
firmware 1.0.3 (222).** It uses plain HTTP/1.1 on TCP 9123, with one persistent
`NSURLSession` connection per light and `PUT /elgato/lights`.

WebSocket support exists in the app, but only for the Key Light Air Mk2 in this
build. The capability is **model-gated**, not firmware-gated:
`ACCLightDevice.supportsWebSockets` returns false, and only
`ACCKeyLightAirMK2Device` overrides it. The bundled Key Light and Key Light Air
firmware images contain no WebSocket server at all; the Air Mk2 image contains
a full WSS server with mbedTLS and JSON-RPC.

For live slider changes the app sends **intermediate integer frames** over that
persistent connection, roughly 3 to 6 writes per second. The request body is a
`LightValues` object containing `numberOfLights`, `lights`, `on`, `brightness`,
and `temperature`. It does **not** contain `durationMs` or `transitionMs`; the
light's stored `colorChangeDurationMs` does the smoothing. The app also polls
light state, and settings at a slower interval, on the same connection.

Elopli cannot use WebSocket with these lights. It should stay on HTTP, keep one
connection per light, coalesce to the latest value, and write at most about
10 times per second.

## Transport comparison

| Property | Key Light / Key Light Air | Key Light Air Mk2 |
| --- | --- | --- |
| Device control | HTTP/1.1 | WSS + JSON-RPC |
| Port | TCP 9123 | Not verified |
| Encryption | None | TLS 1.2/1.3 (mbedTLS 3.4.0 in firmware) |
| Authentication | None | Certificates under `pki_Key_Light_Air_Mk2` |
| App class | `ACCHTTPDeviceConnection` | `WebSocketDeviceConnection` |
| App capability | `supportsWebSockets == false` | override present; certificate-gated |
| Bundled firmware | HTTP server only | WSS server, `Sec-WebSocket-*` |

## What Control Center actually does

### HTTP for the tested lights

The app's own log records requests such as:

```text
will send request PUT /elgato/lights
got response PUT /elgato/lights {"numberOfLights":1,"lights":[{"on":1,"brightness":50,"temperature":267}]}
```

Error paths in older logs show the complete URL shape:

```text
http://elgato-key-light-<suffix>.local.:9123/elgato/lights/settings
http://elgato-key-light-<suffix>.local.:9123/elgato/accessory-info
```

The app uses `ACCHTTPDeviceConnection`, which implements
`NSURLSessionDataDelegate` (`HTTPDeviceConnection.swift`). The unified
CFNetwork log shows ordinary HTTP/1.1 tasks with request and resource timeouts
of 15 s and 604800 s, connection reuse, and one connection per light.

During a rapid write sequence on 2026-11-25, CFNetwork reported three
connections in use:

```text
Task <...> summary for task success {transaction_duration_ms=15, response_status=200,
  connection=3, protocol="http/1.1", reused=1, reused_after_ms=0,
  request_bytes=528, response_bytes=184}
```

Connections 3, 4, and 5 were reused for successive tasks; `reused=1` appears on
every write after the first. The connections were cleaned up about 44 seconds
after the last request. This is NSURLSession HTTP/1.1 keep-alive, not a new TCP
connection per frame.

### WebSocket only for newer hardware

The relevant source paths and strings in the app are:

```text
ControlCenterSDK/LightDevice+Websocket.swift
Device does not support websockets
Device with websocket support needs certificates directory
could not create WebSocket device connection: %@
startWebSocketConnection()
```

The gate is the `supportsWebSockets` property. Disassembly of the x86_64 slice
shows:

| Class | `supportsWebSockets` implementation | Result |
| --- | --- | --- |
| `ACCLightDevice` (base) | `0x1000e0cd0` | `xor eax, eax; ret` — false |
| `ACCKeyLightAirMK2Device` | `0x1000d1260` | override; enabled for this model when certificates are present |

`ACCKeyLightDevice` and `ACCKeyLightAirDevice` do not override the property, so
they inherit the false result. Only `ACCKeyLightAirMK2Device` overrides both
`supportsWebSockets` and `certificatesDirectoryName` in this binary.

The WebSocket implementation uses `NSURLSessionWebSocketTask` and a custom
`WebSocketTLSSessionDelegate` with `URLSession:didReceiveChallenge:`. Strings
include `wss://`, `socketURL`, and `WebSocketTLSSessionDelegate.swift`.
The app bundle contains:

```text
pki_Key_Light_Air_Mk2/root.crt.pem
pki_Key_Light_Air_Mk2/intermediate.crt.pem
pki_Key_Light_Air_Mk2/intermediate.key.pem
credentials.enc
```

`credentials.enc` is encrypted (RNCryptor types are present in the binary).
It was not decrypted.

The Air Mk2 firmware image contains `Starting WebSocketServer-elgato`,
`Sec-WebSocket-Version`, `Sec-WebSocket-Protocol`, `Upgrade: websocket`,
mbedTLS 3.4.0, and JSON-RPC method names such as `elgato.api-info`,
`elgato.identify`, `elgato.restart`, `elgato.factory-reset`, and
`elgato.firmware-update.*`. The Key Light and Key Light Air images contain none
of those strings.

The WS port, path, subprotocol, and certificate exchange were not verified on
hardware. They are not needed for the tested lights.

The app also exposes a separate local JSON-RPC control interface on
`127.0.0.1:1804` (Swifter; returns HTTP 401 without authentication). Its
`getDevices`, `getDeviceConfiguration`, and `setDeviceConfiguration` methods are
for controlling Control Center itself, not for talking to a light. Do not
confuse it with the device WebSocket path.

## Hardware probes

All probes targeted Right (192.168.4.39). Left was read once and not changed.
Back was not probed.

### Ports

| Port | Result |
| --- | --- |
| 80 | Connection refused |
| 443 | Connection refused |
| 9123 | Open |
| 9124 | Connection refused |
| 8443 | Connection refused |

`GET /elgato/lights` returned:

```http
HTTP/1.1 200 OK
Content-Type: application/json; charset=utf-8
Content-Length: 74
Connection: keep-alive

{"numberOfLights":1,"lights":[{"on":0,"brightness":50,"temperature":231}]}
```

### WebSocket upgrade attempt

A standard handshake was sent to `/`, `/elgato`, and `/elgato/lights` on port
9123. None returned `101 Switching Protocols`:

| Path | Response |
| --- | --- |
| `/` | `200 OK`, 86698-byte setup page |
| `/elgato` | `404 Not Found` |
| `/elgato/lights` | `200 OK`, normal light JSON |

The server ignores `Upgrade: websocket` and handles the request as ordinary
HTTP. `curl`'s TLS attempt to 9123 failed with a protocol-version alert, while
the same port answers plain HTTP; there is no TLS service on that port.

### Firmware contents

The bundled firmware images were scanned with `strings`:

| Image | `Sec-WebSocket` | `websocket` | `mbedtls` | HTTP server | Verdict |
| --- | ---: | ---: | ---: | --- | --- |
| `Firmware_Key_Light.bin` | 0 | 0 | 0 | `HTTP/1.1 200 OK` | HTTP only |
| `Firmware_Key_Light_Air.bin` | 0 | 0 | 0 | `HTTP/1.1 200 OK` | HTTP only |
| `Firmware_Key_Light_Air_Mk2.bin` | 10 | 7 | 19 | HTTPS/WSS | WebSocket |

The Key Light firmware includes the generic Realtek string
`Please set CONFIG_SSL_CLIENT 1 in platform_opts.h to enable ATWL command`,
which indicates the SDK's TLS client option is not enabled in that build. It
has an HTTP server and no WebSocket module.

A firmware update would not add WebSocket to these models through Control
Center: the app's capability check is per device class, and the bundled
firmware payloads for the old models contain no WebSocket implementation. The
only model whose app class enables WebSockets in this build is the Key Light Air
Mk2.

## Live control on an old light

### Request shape

The app's `LightValues` encoder declares these keys:

```text
numberOfLights
lights
saturation
brightness
temperature
numberOfSceneElements
scene
metaData
```

A plain temperature/brightness write therefore has this shape:

```json
{"numberOfLights":1,"lights":[{"on":1,"brightness":13,"temperature":344}]}
```

CFNetwork measured the exact body sizes during the rapid brightness burst on
2026-11-25:

| App log value | CFNetwork body bytes | Matching payload |
| --- | ---: | --- |
| brightness 13, temperature 344 | 74 | `{"numberOfLights":1,"lights":[{"on":1,"brightness":13,"temperature":344}]}` |
| brightness 30, temperature 344 | 74 | same shape |
| brightness 100, temperature 344 | 75 | same shape, one extra digit |
| brightness 3, temperature 344 | 73 | same shape, one fewer digit |

The byte counts match the `numberOfLights` form exactly. They do not match a
body that includes `durationMs` or `transitionMs`.

The app sends the current full values, not a delta: a brightness-only write
still carries `on` and the unchanged `temperature`. `docs/api.md` notes that an
extra top-level field is ignored, so `numberOfLights` is harmless.

`durationMs` and `transitionMs` are fields of `ACCSceneElement`, not
`ACCLightValues`. They are used for scene elements, not for live slider writes.
The live request therefore omits both, and the light applies its stored
`colorChangeDurationMs` (100 ms by default in `docs/api.md`).

The exact header set was not captured. The API expects JSON through
`Content-Type: application/json` (see `docs/api.md`); the app sends a normal
NSURLSession request and receives the complete new state in the response.

### Cadence during a slider drag

Existing app logs contain several real drags:

| Log window | Values written | Writes |
| --- | --- | --- |
| 2026-11-18 09:45:10–11 | brightness 29 → 36 → 43 → 44; temperature 295 | 4 in about 1 s |
| 2026-11-22 11:10:00–01 | brightness 42 → 59 → 82 → 100; temperature 267 | 4 in about 1 s |
| 2026-11-25 10:21:14–17 | temperature 255 → 250 → 159 → 143, then 146 → 185 → 230 → 295 → 299 → 304 | 10 in about 3 s |
| 2026-11-25 16:46:53–56 | temperature 169 → 183 → 214 → 243 → 305 → 332 → 331 → 225 (likely a scene or scripted animation) | 8 in about 3 s |

The app sends intermediate values, not only the start and end points. The
values are integers and jump by several steps at a time. The observed rate is
about 3 to 6 writes per second, with short bursts near 5 per second. It does not
send 20 to 60 frames per second.

The app log always shows a request followed by its response before the next
request for that light. Combined with one persistent connection per light, this
is consistent with one write in flight per light. Writes to different lights
run in parallel. The exact throttling mechanism (NSSlider event rate, a timer,
or a response-driven loop) was not decompiled; the rate is measured, not
explained.

### Polling while writing

CFNetwork shows empty-body `GET` requests interleaved with the `PUT`s:

```text
00:47:03.033 sent request, body N 0          GET
00:47:03.033 sent request, body S 74         PUT brightness 13
00:47:03.219 sent request, body S 74         PUT brightness 30
00:47:03.321 sent request, body S 75         PUT brightness 100
00:47:03.620 sent request, body N 0          GET
00:47:03.920 sent request, body S 73         PUT brightness 3
```

The GET responses were 184 bytes, the normal `/elgato/lights` state. The app
also issues empty-body GETs whose 259-byte responses are consistent with
`/elgato/lights/settings` (about 5 s apart in the same capture) and
`/elgato/accessory-info` at startup.

A drag-specific CFNetwork capture was not made. The interleaved GETs are
verified during a rapid animation; the same poll manager is likely active
during a drag, but that specific case is inferred.

### Why this does not lock up

The light's HTTP server is small. A separate test found that opening a new TCP
connection for every animation frame (20 to 30 per second) made the light stop
answering for about 18 seconds, while 11 PUTs on one persistent `curl`
connection at about 20 Hz were fine.

Control Center avoids that pattern:

- one keep-alive connection per light;
- one write in flight per light;
- moderate write rate (roughly 3 to 6 per second during a drag);
- connection reuse across the whole interaction;
- idle cleanup after about 44 seconds.

The 44-second cleanup matches an NSURLSession keep-alive idle timeout; it does
not create a new connection between frames.

## Can Elopli use this?

**No WebSocket for the Key Light or Key Light Air on 1.0.3 (222).**
The hardware has no listener other than 9123, ignores the Upgrade handshake,
and its firmware has no WebSocket server. A firmware update does not change the
app's per-model capability check.

Use HTTP with these rules:

| Concern | Recommendation |
| --- | --- |
| Connection | One persistent HTTP/1.1 connection per light. Reuse it for the whole interaction. |
| Writes | Send the latest value only. Do not queue every slider event. |
| Rate | At most about 10 writes/s; 4 to 6/s is enough for a drag. Keep an unpaced burst at or below 15 writes. A paced animation stream may exceed 15 total writes while staying below the rate limit. |
| Body | `{"numberOfLights":1,"lights":[{"on":1,"brightness":50,"temperature":267}]}`. `on` is 0/1. |
| Fade | Omit `durationMs` to use the stored `colorChangeDurationMs`, or send `durationMs` per keyframe with a value no shorter than the interval. |
| Polling | The PUT response already contains the full state. Poll `GET /elgato/lights` at 2 to 5 s, not during a drag. |
| Failure | Retry a dropped connection with backoff. After the known lockup, wait up to 60 s before hammering. |
| Destructive calls | Never send `/elgato/restart`, `/elgato/factory-reset`, `/elgato/uart`, or firmware calls during normal control. |

The existing Rust transport shares one process-wide `ureq` agent and connection
pool, but native sockets from this agent shell fail with `EHOSTUNREACH` because
macOS Local Network Privacy blocks the responsible process. Apple-signed `curl` and `nc`
work. This is a TCC restriction, not a missing service on the light. The
bundled Elopli app needs its own Local Network grant; a CLI launched from
Terminal inherits Terminal's grant.

If Elopli ever supports a WebSocket-capable model, gate it by model and
firmware, use `wss://` with the bundled certificate chain, and keep the HTTP
path as the fallback. That work is out of scope for the current lights.

## Verification matrix

| Claim | Status | Evidence | Caveats |
| --- | --- | --- | --- |
| Key Light / Key Light Air run firmware 1.0.3 (222) | Verified | `GET /elgato/accessory-info` on Right; `docs/api.md`; app log device list | Firmware values from the devices themselves |
| The tested lights listen only on TCP 9123 | Verified | `nc -vz` on 80, 443, 9123, 9124, 8443 | Right only; Left/Back assumed identical |
| Port 9123 is plain HTTP/1.1 with keep-alive | Verified | `curl -D -` response: `HTTP/1.1 200 OK`, `Connection: keep-alive` | No TLS on 9123 |
| A WebSocket Upgrade is not honored | Verified | `curl` handshake to `/`, `/elgato`, `/elgato/lights`; no `101` | The server treats it as a normal GET |
| The light firmware has no WebSocket server | Verified | `strings` on `Firmware_Key_Light.bin` and `Firmware_Key_Light_Air.bin`: zero `Sec-WebSocket`, `websocket`, `mbedtls` | Firmware images are the bundled update payloads, not a flash dump |
| The Air Mk2 firmware has a WSS server | Verified | `Firmware_Key_Light_Air_Mk2.bin`: `Starting WebSocketServer-elgato`, 10 `Sec-WebSocket`, mbedTLS 3.4.0 | Not tested on hardware |
| Control Center uses HTTP for the tested lights | Verified | App log URLs `http://elgato-key-light-*.local.:9123/elgato/lights`; CFNetwork `protocol="http/1.1"` | Logs redact serials but show host and path |
| `supportsWebSockets` is false for the base light class | Verified | x86_64 disassembly at `0x1000e0cd0`: `xor eax, eax; ret` | Static analysis |
| Only `ACCKeyLightAirMK2Device` overrides `supportsWebSockets` | Verified | ObjC metadata: only that class has the override at `0x1000d1260` | Other models inherit the false base implementation in this build |
| The app uses one keep-alive connection per light | Verified | CFNetwork connections 3/4/5, `reused=1` on successive tasks, cleanup after ~44 s | Connection-to-light mapping inferred from three devices and response sizes |
| Writes are one in flight per light | Inferred | App log ordering: request, response, next request; one connection per light | No packet capture; concurrent tasks on other lights exist |
| A slider drag sends intermediate values | Verified | App logs: 29→36→43→44; 42→59→82→100; 255→250→159→143 | Values are from the app's own responses |
| Drag write rate is about 3–6 per second | Verified | Four writes in ~1 s in two separate drags; ten in ~3 s in another; a scripted sequence showed ~3/s | Log timestamps have 1 s resolution; rate is approximate |
| The request body includes `numberOfLights`, `on`, `brightness`, `temperature` | Verified (length), Inferred (order) | CFNetwork body sizes 73/74/75 match the `numberOfLights` form exactly; `LightValues` encoder keys | Exact bytes were not captured; field order is inferred |
| The live request omits `durationMs` / `transitionMs` | Inferred | `ACCLightValues` has no such fields; `ACCSceneElement` has both; body lengths match the no-duration form | Exact body bytes not captured |
| The light's own fade smooths live changes | Inferred | No duration field in live writes; `colorChangeDurationMs` is 100 ms in `docs/api.md` | Could not observe the optical fade directly |
| The app polls GET while writing | Verified (animation), Inferred (drag) | CFNetwork `body N 0` GETs interleaved with PUTs; 184-byte state responses | No drag-specific capture |
| New TCP per frame causes the lockup | Verified by prior test | 20–30 connections/s → ~18 s unresponsive; 11 PUTs on one connection at 20 Hz fine | Not repeated in this investigation |
| Elopli can use WebSocket today | No | No listener, no upgrade, no firmware support, app gate false | Do not implement for these models |
| Local Network Privacy blocks native sockets in this shell | Verified | Rust/Python/`openssl` get `EHOSTUNREACH`; `curl` and `nc` work | TCC is per responsible process; not a light fault |

## Reproduction

These commands were used for the hardware side. They are read-only except for
the final state check. Do not run them against Left while it is in use.

```bash
# State and headers
curl -sS --max-time 5 -D - http://192.168.4.39:9123/elgato/lights

# Ports
for p in 80 443 9123 9124 8443; do
  nc -vz -G 2 192.168.4.39 "$p"
done

# WebSocket upgrade (safe; no state change)
for path in / /elgato /elgato/lights; do
  curl -sS --max-time 5 --http1.1 -i -N \
    -H 'Connection: Upgrade' \
    -H 'Upgrade: websocket' \
    -H 'Sec-WebSocket-Version: 13' \
    -H 'Sec-WebSocket-Key: SGFsbG8gV29ybGQhISE=' \
    "http://192.168.4.39:9123$path"
done

# Restore check: must print on 0, brightness 50, temperature 231
curl -sS --max-time 5 http://192.168.4.39:9123/elgato/lights
```

Control Center logs:

```bash
ls -lt ~/Library/Logs/ElgatoControlCenter/
grep -aE 'will send request|got response' ~/Library/Logs/ElgatoControlCenter/ElgatoControlCenter0.log

log show --last 30m \
  --predicate 'process == "ControlCenter" AND subsystem == "com.apple.CFNetwork"' \
  --info --debug --style compact
```

Static inspection:

```bash
APP='/Applications/Elgato Control Center.app/Contents/MacOS/ControlCenter'
strings -a "$APP" | grep -iE 'websocket|jsonrpc|durationMs|transitionMs|wss://'
otool -oV "$APP" | grep -A4 -B2 'supportsWebSockets'
otool -arch x86_64 -tvV "$APP" | awk '/^00000001000e0cd0/{p=1} p{print; n++; if(n>10) exit}'

FW='/Applications/Elgato Control Center.app/Contents/Resources/ControlCenterSDK_ControlCenterSDK.bundle/Contents/Resources'
for f in "$FW"/Firmware_*.bin; do
  printf '%s ' "$(basename "$f")"
  strings -a "$f" | grep -c 'Sec-WebSocket'
done
strings -a "$FW/Firmware_Key_Light_Air_Mk2.bin" | grep -iE 'Sec-WebSocket|websocket|mbedtls'
```

## Unperformed experiments and limits

- **No packet capture.** `tcpdump` and passwordless `sudo` are unavailable, so
  the exact HTTP headers and the exact request bytes were not captured. Body
  lengths came from CFNetwork summaries; field order is inferred from the
  `LightValues` encoder keys.
- **No `CFNETWORK_DIAGNOSTICS=3` relaunch.** Setting it requires quitting and
  reopening Control Center, which would disturb the running app. This was not
  done. It would show URLs and headers directly.
- **No `lldb` attach.** Developer mode is disabled and the app is hardened;
  attaching was not attempted.
- **No UI drag by this agent.** Drag cadence comes from existing app logs, not
  from a new interaction.
- **No WebSocket test on an Air Mk2.** None was available. Its port, path,
  subprotocol, and certificate exchange remain unverified.
- **No TLS probe from a native client.** `openssl s_client` failed with
  `EHOSTUNREACH` because of Local Network Privacy; `curl` reached the port and
  reported a protocol-version alert, which is enough to rule out TLS on 9123.

## Summary for Elopli

1. Keep the HTTP transport on port 9123.
2. Use one persistent connection per light and one write in flight.
3. Send `{"numberOfLights":1,"lights":[{"on":1,"brightness":N,"temperature":M}]}`.
4. Do not include `durationMs` for live writes unless deliberately using
   keyframes; the app does not.
5. Write at 4 to 6 Hz for sliders; never open a connection per frame.
6. Poll rarely, and use the PUT response as the new state.
7. Gate any future WebSocket code by model and firmware; it does not apply to
   the current Key Light or Key Light Air.
