# keychronctl(1) — Design Document

Manpage-style plan for the `keychronctl` CLI. Scope: **configuration and readout of Keychron devices over HID.** Firmware flashing is explicitly **out of scope** (wontdo).

---

## NAME

`keychronctl` — configure Keychron keyboards, mice, and receivers over USB HID

## SYNOPSIS

```
keychronctl [GLOBAL OPTIONS] COMMAND [ARGS]

keychronctl list
keychronctl info [-d DEVICE]
keychronctl keymap dump|get|set|clear|layers|layer [-d DEVICE] [ARGS]
keychronctl macro list|dump|set|reset|repeat [-d DEVICE] [ARGS]
keychronctl light get|set|save|led [-d DEVICE] [ARGS]
keychronctl he info|profile|travel|rt|curve|calibrate|reset [-d DEVICE] [ARGS]
keychronctl knob get|set [-d DEVICE] [ARGS]
keychronctl sleep get|set [-d DEVICE] [ARGS]
keychronctl debounce get|set [-d DEVICE] [ARGS]
keychronctl pollrate get|set [-d DEVICE] [ARGS]
keychronctl battery [-d DEVICE]
keychronctl reset keymap|eeprom|key [-d DEVICE] [ARGS]
keychronctl definitions fetch|list|show|prune [ARGS]
```

## DESCRIPTION

`keychronctl` is a stateless command-line utility for managing Keychron devices
(Q, K, V, B, C, J, HE series; M-series mice; 2.4 GHz receivers). Each invocation
is independent: the program opens the device, negotiates the protocol, performs
exactly one operation, and exits. There is no daemon and no persistent state
beyond an optional on-disk cache of device-definition JSONs.

The wire protocol is the Keychron/QMK raw-HID protocol (vendor usage page
`0xFF60`, usage `0x61`, report ID 0, 32-byte payloads by default). Devices are
identified by `vpid = (VID << 16) | PID` with Keychron's vendor ID `0x3434`.

Every command that talks to a device follows the same connect sequence:

1. Open the HID interface (blocking I/O, strict request/response lockstep —
   one outstanding command, ack = any input report, 1 s timeout, 3 retries).
2. Query protocol version (`0x01`) → gates v2/v3 behavior (`>= 11` ⇒ v3).
3. Query Keychron protocol version and instruction set (`0xA0`).
4. Query support features (`0xA2`) → feature bitmask gates per-command availability.
5. Load the device definition JSON (cache, else fetch from
   `https://launcher.keychron.com/static/device/{vpid}/json/v3.json`,
   fallback `v2.json`) → matrix size, layouts, lighting/macro capabilities.
6. Execute the command; print result; exit.

Commands that would require an unsupported feature (per the `0xA2` bitmask or
the definition JSON) fail fast with exit code 5 before writing anything.

## GLOBAL OPTIONS

```
-d, --device <SPEC>    Select device: vpid (decimal or 0x-prefixed), VID:PID,
                       or hidraw path (/dev/hidraw1). Default: auto-detect;
                       if multiple Keychron devices are attached, fail with exit 3
                       unless -d is given.
--json                 Machine-readable output (one JSON document per command).
--no-cache             Never read or write the definition cache; always fetch.
--cache-dir <DIR>      Override cache location (default: ~/.cache/keychronctl).
--offline              Use only cached definitions; never fetch (fail if missing).
--timeout <MS>         Per-command response timeout (default: 1000).
--retries <N>          Retries per command (default: 3).
-v, --verbose          Protocol trace to stderr: hex dump of every report sent
                       and received, with command/subcommand annotation.
-q, --quiet            Suppress non-essential output (errors only).
-h, --help             Usage for the command; -h anywhere prints help.
-V, --version          Version and protocol support summary.
```

## COMMANDS

### `list`

Scan `/dev/hidraw*` (Linux) for HID collections matching the Keychron
signatures — keyboard `0xFF60:0x61`, bridge `0x008C:0x01`, mouse `0xFFC1:0x01`,
mouse-4k `0xFF0A:0x01` — and print one line per interface:

```
VPID        VID:PID     ROLE      PATH            PRODUCT
875823265   3434:00A1   keyboard  /dev/hidraw3    Keychron Q10
```

Product names come from cached definitions when available (`--offline`-safe);
otherwise vpid/vid/pid/path only. Exits 3 if no devices are found.

### `info`

Connect and print the device summary:

- QMK protocol version (`0x01`), Keychron protocol version + instruction set (`0xA0`)
- Firmware version (`0xA1`, ASCII)
- Feature bitmask (`0xA2`), decoded to named flags
- Layer count (`0x11`), current layer (`0xA3`)
- Work mode (`0xAB` sub 5): USB / Bluetooth / 2.4 GHz
- Device ID (`0xAB` sub 9)
- Battery (`0xA7` sub 49) — only if the feature bit is set
- Definition-derived data: matrix rows×cols, layout count, macro capacity

### `keymap`

```
keymap dump [--layer N] [--format keys|matrix|json]
keymap get <LAYER> <ROW> <COL>
keymap set <LAYER> <ROW> <COL> <KEYCODE>
keymap set <LAYER> <KEYLABEL> <KEYCODE>          # address by layout label
keymap clear
keymap layers
keymap layer
```

- `dump` reads the full keymap in 14-key chunks (`0x12`, offset
  `28*i + layer*(rows*cols*2)`, big-endian 16-bit keycodes) and renders it
  annotated with QMK keycode names. `--format matrix` prints the raw
  row/col/code table; `keys` prints `LAYER  ROW  COL  LABEL  KEYCODE`.
- `get`/`set` address a single key (`0x04` / `0x05`). `<KEYCODE>` accepts a
  QMK keycode name (`KC_A`, `LCTL`, `MO(1)`, `LSFT(KC_A)`), or a raw value with
  `0x` prefix. `<KEYLABEL>` is resolved through the device definition's layout
  (e.g. `"Caps Lock"`, `"Esc"`) when row/col addressing is inconvenient.
- `clear` resets the entire keymap (`0x06`).
- `layers` prints the layer count (`0x11`); `layer` prints the current and
  active layer (`0xA3`).

### `macro`

```
macro list
macro dump <INDEX> [--all]
macro set <INDEX> --text "SEQ" | --file FILE [--repeat N]
macro reset
macro repeat get <INDEX>
macro repeat set <INDEX> <TIMES>
```

- `list` prints count (`0x0C`) and buffer size (`0x0D`).
- Macro sequences use a small text notation: `abc{500ms}d` types `a`, `b`, `c`,
  waits 500 ms, types `d`; modifiers as `CTRL+c`, `SHIFT+t`; media as
  `MEDIA:play`. Encoded to the device buffer using the V11 encoding when
  protocol ≥ 11, else V9 (`toMacroBufForV9/V11` semantics from the spec).
- `set` writes the buffer in chunks (`0x0F`, offset/len framing; the device
  must echo the request byte-for-byte — verified before success is reported).
- `repeat` maps to `0xA7` sub 27/28 (macro repeat get/set); gated on the
  Macro_Repeat feature bit.

### `light`

Backlight (CUSTOM_MENU `0x07`/`0x08`/`0x09`):

```
light get
light set [--brightness 0-255] [--effect N] [--speed 0-255] [--color RRGGBB]
light save
```

Parameter IDs are protocol-gated: v3 (protocol ≥ 11) uses bright=3/1,
effect=3/2, speed=3/3, color=3/4; v2 uses the definition JSON's
`supportedLightingValues`. `save` persists to EEPROM (`0x09`).

Per-key RGB (KC_RGB `0xA8`, gated on the Keychron_RGB feature bit):

```
light led get <ROW>            # LED numbers by row (sub 6)
light led set <INDEX> --rgb RRGGBB [--rgb RRGGBB ...]   # sub 10
light led get-effect           # sub 7
light led set-effect <N>       # sub 8
light led get-region           # sub 12
light led save                 # sub 2 (SaveLedConf)
```

### `he` — hall-effect / magnetic switches (KC_HE `0xA9`)

```
he info                          # AMC_GET_VERSION (sub 1), profiles info (sub 16)
he profile list                  # AMC_GET_PROFILES_INFO (sub 16)
he profile select <ID>           # AMC_SELECT_PROFILE (sub 17)
he profile name set <ID> <NAME> # AMC_SET_PROFILE_NAME (sub 19)
he travel get --key <ROW>,<COL> [--all]
he travel set --key <ROW>,<COL> --press 0.1..4.0 --release 0.1..4.0 \
              [--sens-press N] [--sens-release N] [--mode rapid|standard]
he rt get --key <ROW>,<COL>      # rapid-trigger settings
he rt set --key <ROW>,<COL> [--actuation N] [--release N]
he curve get --key <ROW>,<COL>   # AMC_GET_CURVE (sub 32)
he curve set --key <ROW>,<COL> --file FILE
he calibrate start               # AMC_CALIBRATE (sub 64, arg 2)
he calibrate stop                # sub 64, arg 4
he calibrate status              # AMC_GET_CALIBRATE_STATE (sub 65)
he reset --key <ROW>,<COL>       # AMC_RESET_SINGLE_KEY (sub 29)
he profile save                  # AMC_SAVE_PROFILE_BUFFER (sub 31)
```

Travel values are in millimetres, converted to the device's raw units per the
definition JSON. All HE commands are gated on `0xA9` support (probe `he info`
first; unsupported devices exit 5).

### `knob` — rotary encoders

```
knob get <LAYER> <KNOB> <cw|ccw>
knob set <LAYER> <KNOB> <cw|ccw> <KEYCODE>
```

Maps to `0x14`/`0x15` with direction 0/1.

### `sleep`

```
sleep get
sleep set [--backlight SECONDS] [--sleep SECONDS] [--scan SECONDS]
```

`0xA7` sub 11/12; 16-bit values, seconds. Gated on the Sleep feature bit.

### `debounce`

```
debounce get
debounce set <MS>
```

`0xA7` sub 5/6 (Telink variant takes press+release; QMK variant sub-type+press —
selected by instruction set). Gated on the DEBOUNCE feature bit.

### `pollrate`

```
pollrate get
pollrate set [--usb 1000|2000|4000|8000] [--rf 1000|...]
```

`0xA7` sub 13/14; the capability query (sub 1) determines supported levels
(`8000/(1<<r[6])` style divisors). Gated on the PollRate feature bit.

### `battery`

Prints charge percentage and charging status (`0xA7` sub 49). Exit 5 if the
device doesn't report battery support.

### `reset`

```
reset keymap                    # DYNAMIC_KEYMAP_CLEAR_ALL (0x06)
reset eeprom                    # EEPROM_RESET (0x0A) — full config wipe
reset key --layer N --row N --col N   # 0xA7 sub 20
```

Destructive commands print the target device and require `--yes` to proceed
(asked only when run without a TTY; in scripts `--yes` is mandatory).

### `definitions` — definition cache management

```
definitions fetch [--vpid N | --all-cached]
definitions list
definitions show [--vpid N]
definitions prune [--older-than DAYS]
```

Manages the local cache of device-definition JSONs
(`~/.cache/keychronctl/device/{vpid}/v3.json`). `fetch --vpid` retrieves and
stores one device's definition (and its layout files); `list` summarizes cached
devices; `show` pretty-prints a definition. This is the only command that
touches the network besides the implicit fetch in the connect sequence, and it
is the recommended way to prepare for `--offline` usage.

## OUTPUT

Human-readable by default. `--json` emits a single JSON document with stable
field names, suitable for scripting (`jq`). `--verbose` adds a protocol trace
on stderr without polluting stdout. Exit codes:

```
0   success
1   general runtime error (I/O, malformed response)
2   usage error (bad arguments)
3   no device found / ambiguous device selection
4   protocol error (timeout after retries, unexpected response)
5   unsupported feature (device or protocol version lacks the capability)
```

## FILES

```
~/.cache/keychronctl/
    device/{vpid}/v3.json      device definitions (also v2.json fallback)
    layouts/*.json             keycode catalogs per layout/locale
    catalog.json               optional product catalog snapshot
```

On Linux, raw HID access requires udev permission for the `hidraw` device —
the user must be able to read/write `/dev/hidraw*` for Keychron devices
(`udevadm` rule tagging VID `0x3434` with `uaccess`); `keychronctl list` prints
a hint when permission is denied.

## ENVIRONMENT

```
KEYCHRONCTL_CACHE_DIR    equivalent of --cache-dir
KEYCHRONCTL_DEVICE       equivalent of -d
NO_COLOR                 disable colored output
```

## EXAMPLES

```
$ keychronctl list
$ keychronctl info
$ keychronctl keymap dump --layer 0
$ keychronctl keymap set 0 "Caps Lock" KC_LCTRL
$ keychronctl keymap set 0 4 3 MO(1)
$ keychronctl macro set 0 --text "hello{200ms}world"
$ keychronctl light set --brightness 120 --effect 5 --speed 40
$ keychronctl he travel set --key 2,4 --press 1.2 --release 1.0 --mode rapid
$ keychronctl pollrate set --usb 8000
$ keychronctl definitions fetch --vpid 875823265
$ keychronctl -v --json battery
```

## SEE ALSO

`docs/PROTOCOL.md` — the reverse-engineered protocol specification this tool
implements. `launcher-sources/` — captured reference materials.

---

# Design notes (implementation)

**Crate layout** (single binary crate, blocking I/O only — no async runtime):

```
src/
  main.rs          CLI entry, clap parsing, exit-code mapping
  hid.rs           hidapi wrapper: open by signature, 32-byte report I/O
  transceiver.rs   lockstep queue: send → await matching input report →
                   timeout/retry (mirrors the launcher's Serial transceiver)
  commands.rs      #[repr(u8)] enums for all command/subcommand IDs (§3 of spec)
  protocol/
    mod.rs         connect sequence, version negotiation, feature bitmask
    keymap.rs      0x04–0x06, 0x11–0x15
    macro.rs       0x0C–0x10, 0xA7 (27/28); V9/V11 encoders
    light.rs       0x07–0x09, 0xA8
    he.rs          0xA9
    misc.rs        0xA7 (sleep/debounce/pollrate/battery/reset-key)
    factory.rs     0xAB
  keycodes.rs      QMK keycode name ⇄ 16-bit value (built-in table; layout
                   JSONs extend it per-device)
  definition.rs    serde types for device JSON v2/v3; cache + fetch + select
  output.rs        human/json renderers
```

**Key decisions**

- `hidapi`, `rusb` (kept for future DFU work but unused now), `clap` (derive),
  `serde`/`serde_json`, `ureq` (blocking HTTP). All blocking; nothing async.
- Command IDs live in `#[repr(u8)]` enums with `#[non_exhaustive]` — unknown
  device responses parse to an `Unknown(u8)` variant instead of failing.
- Feature gating is centralized: the `0xA2` bitmask + definition JSON produce
  a `Capabilities` struct checked before any command runs.
- The transceiver is the only place that touches raw HID; everything above it
  speaks typed request/response values. This matches the spec's ack model
  (any input report dequeues; optional echo verification for macro writes).
- v2/v3 protocol differences are confined to `protocol/` per-area modules
  (e.g. lighting parameter IDs), selected once at connect and stored in the
  connection context.
- Definition JSONs are fetched with a short timeout; on network failure the
  command still runs if the operation doesn't need the definition (battery,
  info, pollrate), and fails with a clear message otherwise.

**Out of scope (wontdo):** firmware flashing (all bootloader protocols),
screen-keyboard asset upload (0xAC), mouse fun-keys, bridge pairing, the
Quick Start assistant daemon. These remain documented in the spec for later.
