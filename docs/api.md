# Elgato Key Light local API

This document records the local HTTP interface of the Elgato Key Light and Key Light Air.
The notes come from direct tests on firmware 1.0.3 (222).

## Transport

| Property | Value |
| --- | --- |
| Protocol | HTTP/1.1 |
| Encryption | None |
| Port | TCP 9123 |
| Authentication | None |
| Request body | JSON |
| Response body | JSON |
| Connection | Keep-alive |

The light answers on the local network only.
It has no cloud dependency for control.

## Discovery

The light announces the `_elg._tcp` service through mDNS.
The service instance has this shape:

```text
Elgato Key Light Air AB12._elg._tcp.local.
```

The TXT record has these keys:

| Key | Example | Meaning |
| --- | --- | --- |
| `mf` | `Elgato` | Manufacturer |
| `dt` | `200` | Board type. 53 is Key Light. 200 is Key Light Air. |
| `id` | `AA:BB:CC:DD:EE:FF` | Network interface address. Some devices randomise it. |
| `md` | `Elgato Key Light Air 20LAB9901` | Model name and model code |
| `pv` | `1.0` | Protocol version |

The host name has this shape:

```text
elgato-key-light-air-ab12.local.
```

The suffix comes from the network interface address.
It is not the serial number.

## Endpoints

| Method | Path | Function |
| --- | --- | --- |
| GET | `/elgato/accessory-info` | Read product, firmware, and network details |
| PUT | `/elgato/accessory-info` | Set the display name |
| GET | `/elgato/lights` | Read the light state |
| PUT | `/elgato/lights` | Set the light state |
| GET | `/elgato/lights/settings` | Read the stored settings |
| PUT | `/elgato/lights/settings` | Write the stored settings |
| POST | `/elgato/identify` | Flash the light |
| POST | `/elgato/restart` | Restart the light |
| POST | `/elgato/factory-reset` | Erase settings and restart |

Do not call `restart` or `factory-reset` during normal control.

## Read the light state

```http
GET /elgato/lights
```

```json
{
  "numberOfLights": 1,
  "lights": [
    {
      "on": 1,
      "brightness": 50,
      "temperature": 267
    }
  ]
}
```

| Field | Type | Range | Meaning |
| --- | --- | --- | --- |
| `on` | integer | 0 or 1 | Power state |
| `brightness` | integer | 0 to 100 | Brightness in percent |
| `temperature` | integer | 143 to 344 | Colour temperature in mired |

The device accepts `brightness` 0 and 1, but the light switches off below 2.
This client enforces 2 to 100 so a brightness change does not turn the light off.
Use the power command to turn a light off.

The mired value is the reciprocal of the colour temperature in kelvin.
A larger mired value is a warmer light.

| Mired | Kelvin | Appearance |
| --- | --- | --- |
| 143 | 6993 | Cool |
| 200 | 5000 | Neutral |
| 344 | 2907 | Warm |

## Write the light state

```http
PUT /elgato/lights
Content-Type: application/json

{"lights":[{"on":0}]}
```

The request can carry one field or several fields.
A field that the request omits keeps its current value.
The response is the complete new state.

```json
{"numberOfLights":1,"lights":[{"on":0,"brightness":50,"temperature":267}]}
```

The device uses the first object in the `lights` array and ignores the rest.
An empty array returns HTTP 400.
An extra top-level field is ignored.

### Transitions

Add `durationMs` to one light object to set a fade time for that change.

```json
{"lights":[{"brightness":80,"durationMs":500}]}
```

The device does not echo `durationMs` in the response.

### Validation rules

The device has weak validation.
A client must validate values before it sends them.

| Input | Device result |
| --- | --- |
| `brightness` 101 | Request returns 200. The old value stays. |
| `brightness` 120 | Request returns 200. The old value stays. |
| `brightness` -5 | Clamps to 0 |
| `brightness` 99.5 | Truncates to 99 |
| `brightness` `"50"` | Stores 0 |
| `on` 2 | Stores 1 |
| `on` true | Stores 1 |
| `temperature` 0 | Stores 0 and echoes 0 |
| `temperature` 400 | Stores 400 and echoes 400 |
| Empty `lights` array | HTTP 400 |
| Invalid JSON | HTTP 400 |
| Empty object | HTTP 400 |

The device accepts `temperature` values outside the documented range.
It echoes them without an error.
The physical output still has a limit.
A client must keep the value in the 143 to 344 range.

## Read the stored settings

```http
GET /elgato/lights/settings
```

```json
{
  "powerOnBehavior": 1,
  "powerOnBrightness": 20,
  "powerOnTemperature": 213,
  "switchOnDurationMs": 100,
  "switchOffDurationMs": 300,
  "colorChangeDurationMs": 100
}
```

`powerOnBehavior` value 1 restores the last state.
Value 2 uses the `powerOnBrightness` and `powerOnTemperature` values.

| Field | Range | Meaning |
| --- | --- | --- |
| `powerOnBehavior` | 1 or 2 | 1 restores the last state. 2 uses the defaults. |
| `powerOnBrightness` | 0 to 100 | Default brightness for behaviour 2 |
| `powerOnTemperature` | 143 to 344 | Default mired value for behaviour 2 |
| `switchOnDurationMs` | milliseconds | Fade-in time |
| `switchOffDurationMs` | milliseconds | Fade-out time |
| `colorChangeDurationMs` | milliseconds | Fade time for a colour or brightness change |

### Set the power-on behaviour

Read the settings first.
Change the fields.
Send the complete object back.

```http
PUT /elgato/lights/settings
Content-Type: application/json

{
  "powerOnBehavior": 2,
  "powerOnBrightness": 25,
  "powerOnTemperature": 244,
  "switchOnDurationMs": 100,
  "switchOffDurationMs": 300,
  "colorChangeDurationMs": 100
}
```

A partial object returns HTTP 400.
A complete object returns HTTP 200 with an empty body.
The device applies the new behaviour at the next power-on.

## Read accessory information

```http
GET /elgato/accessory-info
```

```json
{
  "productName": "Elgato Key Light",
  "hardwareBoardType": 53,
  "hardwareRevision": "1",
  "macAddress": "<redacted>",
  "firmwareBuildNumber": 222,
  "firmwareVersion": "1.0.3",
  "serialNumber": "<redacted>",
  "displayName": "Left",
  "features": ["lights"],
  "wifi-info": {
    "ssid": "example",
    "frequencyMHz": 2400,
    "rssi": -36
  }
}
```

The response contains the serial number and the network interface address.
Treat both values as private data.
This client discards them.

### WiFi details

`wifi-info` is the light's own view of its network.
It is not the host's WiFi state.

| Field | Meaning |
| --- | --- |
| `ssid` | The network that the light joined |
| `frequencyMHz` | 2400 or 5000 |
| `rssi` | Signal strength in dBm |

The client can show a WiFi icon for each light.
The hover text can show the SSID, the frequency, and the RSSI.
A request failure means the light is offline.
The API does not return the light's IP address.
The client knows the address from the registry or from mDNS.

### Set the display name

```http
PUT /elgato/accessory-info
Content-Type: application/json

{"displayName":"Desk light"}
```

The device returns HTTP 200 with an empty body.
The new name appears in the next `GET`.

The name can have 1 to 63 bytes.
A longer name returns HTTP 400 with this body:

```json
{"errors":[{"message":"Too much data","code":103}]}
```

The device accepts an empty name and clears the value.
This client rejects an empty name.

A rename does not change the mDNS instance name.
The instance stays in the `Elgato Key Light <suffix>` form.

## Identify a light

```http
POST /elgato/identify
```

Send an empty body.
The light flashes.
The response is HTTP 200 with an empty body.

## Pulse (client-side)

The light has no pulse endpoint.
`POST /elgato/identify` flashes the light, but it is not configurable.
The client ramps brightness or temperature from the current value to a peak, then down to 0% and back to the peak for each count.
A count of 3 is:

```text
current → peak → 0 → peak → 0 → peak → 0 → original
```

The last 0% returns to the original values instead of another peak.
Brightness 0% switches the light off for that phase; the next peak turns it on again.
Temperature 0% is the coolest end of the range, not off.
The pulse restores the exact original on, brightness, and temperature values when it finishes.
If the light was off, the client turns it on for the pulse and turns it off again at the end.
An interrupting action applies its own value and the pulse does not restore over it.

The pulse is deliberately conservative.
The light's HTTP server is fragile under bursts of short-lived connections, and prior art limits write pressure:

- [ioBroker's Elgato adapter](https://www.npmjs.com/package/iobroker.elgato-key-light) debounces writes by 200 ms.
- [python-elgato](https://github.com/frenck/python-elgato) shares one `aiohttp` session for all requests.
- [Harness Automation](https://docs.harnessautomation.com/gem/admin/drivers/elgato_key_light) notes that the HTTP server on stale firmware occasionally locks up and needs a power cycle.

A native client that keeps one HTTP connection open sends a frame every 50 ms.
The `curl` fallback opens a new connection per request, so it sends a frame at most every 250 ms.
That keeps the default pulse to one frame per ramp and avoids exhausting the light's connection table.
Easing is visible when the duration is long enough for more than one frame.

A cross-process lock stops two pulses from overlapping.
Every other action writes a cancellation token and waits for the lock before it writes.
The pulse checks the token before each frame, and compares the light's state with the value it last wrote.
A user action wins, and an external change is preserved without a restore.

## Errors

An error response uses this shape:

```json
{"errors":[{"message":"Fail to parse JSON data","code":-1}]}
```

| Status | Meaning |
| --- | --- |
| 200 | Success |
| 400 | Malformed request or invalid value |
| 403 | Request carried an `Origin` header |
| 404 | Unknown path or method |
| 503 | Service is not available |

A request with an `Origin` header always returns HTTP 403.
This rule stops a browser page from controlling the light.

## Reliability

The HTTP server is small.
It can refuse or drop a connection during a burst of requests.
A client must retry a failed connection.

The light can drop off the network while it is off.
The off state can last for minutes.
A client that must turn the light on again needs a retry window.

macOS Local Network Privacy can block a non-Apple process.
A process launched from Terminal inherits the Terminal grant.
A CLI that runs from `launchd` or a bundled app needs its own grant.

## Verified devices

| Name | Product | Firmware | Serial |
| --- | --- | --- | --- |
| Back | Key Light Air | 1.0.3 (222) | Redacted |
| Left | Key Light | 1.0.3 (222) | Redacted |
| Right | Key Light | 1.0.3 (222) | Redacted |

## Prior art

- [schlarpc/elgato-key-light-mini-firmware-re](https://github.com/schlarpc/elgato-key-light-mini-firmware-re) documents the firmware and the HTTP interface.
- [frenck/python-elgato](https://github.com/frenck/python-elgato) is an asynchronous Python client.
- [adamesch/elgato-key-light-api](https://github.com/adamesch/elgato-key-light-api) records the identify action.
- [p-conrad/klc](https://github.com/p-conrad/klc) is a Bash client.
- [bitfocus/companion-module-elgato-keylight](https://github.com/bitfocus/companion-module-elgato-keylight) has request examples.
