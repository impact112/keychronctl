# Launcher → keychronctl Feature Parity

Tracks feature parity between the Keychron Launcher web app (reference:
`docs/PROTOCOL.md`) and `keychronctl`. Firmware flashing is a **wontdo**.

Legend: ✅ done · 🔄 in progress · ⬜ todo · 🚫 wontdo

## Core / device management

| Feature | Status | Notes |
|---|---|---|
| List connected interfaces (WebHID signatures) | ✅ | hid.rs, verified on Link-KM dongle |
| Device selector (`-d vpid / VID:PID / path`) | ✅ | |
| `info` — mouse link version + paired device | ✅ | 8k protocol: cmd 2 + cmd 3 on reportId 181 |
| `info` — keyboard protocol/firmware | ✅ | implemented; untested (no keyboard paired) |
| JSON output (`--json`) | ✅ | |
| Protocol trace (`-v`) | ✅ | hex dump in/out, unmatched-report logging |
| Exit codes per docs/CLI.md | ✅ | 0/1/3/4 exercised |
| Definitions fetch/cache (`definitions fetch/list/show`) | ✅ | live-verified; `--offline`/`--no-cache`/`--cache-dir` flags |
| Product catalog lookup (name for vpid) | ✅ | via vapi product info; shown in `info` |
| Keyboard keymap/macro CLI commands | ⬜ | modules ready (`keymap.rs`, `macro.rs`); wiring pending a test keyboard |

## Mouse (M6 via Link-KM dongle — 8k wire protocol, reportId 181/182)

| Feature | Status | Notes |
|---|---|---|
| Link version info | ✅ | cmd 2: fw 1.0.3, work mode, feature flags |
| Receiver state / bond list | ✅ | cmd 3: 3 fixed slots, identifies paired mouse |
| Battery readout | ✅ | cmd 6 snapshot: 96 %, discharging (live) |
| DPI levels get | ✅ | stock 400–5000 ladder, level 5 active (live) |
| DPI levels set | ✅ | cmd 64; level switch + value writes round-tripped (live) |
| **Binds: list per profile** | ✅ | cmds 97/98; annotated with definition key names (live) |
| **Binds: set** | ✅ | cmd 82, ack 0xE4; mouse/media/keyboard/dpi/pollingRate/profile/shortcut/light/remove/disable (live round-trip) |
| **Profiles: list/select/create/remove** | ✅ | cmd 14 on reportId 181; count field creates/removes (live round-trip) |
| Polling rate get/set | ✅ | cmd 65; levels are indexes into [125,500,1000,2000,4000,8000,250]; live round-trip 500↔1000 Hz |
| Sleep get/set | ✅ | cmds 10 get/set on reportId 181; live round-trip (restored 10 min) |
| Debounce get/set | ✅ | cmd 67; live round-trip (restored 4 ms; M6 table is fixed 4 ms ×10) |
| Sensor get/set (LOD/wave/line/motion/scrollDir/fps20k) | ✅ | cmd 66; live round-trip (--line on/off) |
| Scroll wheel get/set | ✅ | cmd 69 `[69, speed, inertia, spl]`; live round-trip |
| Settings snapshot dump (`settings`) | ✅ | full cmd-6 parse: profile/dpi/rate/sleep/debounce/battery/sensor/scroll |
| Lighting get/set | ✅ | cmds 35/36 implemented per launcher; untestable on M6 (no RGB — device ignores cmd 35) |
| 1k / 4k protocol paths (non-dongle mice) | ⬜ | only the 8k path is wired into the CLI |

## Keyboard (QMK raw HID — reportId 0, 32-byte)

| Feature | Status | Notes |
|---|---|---|
| Protocol version (0x01) / KC version (0xA0) / fw (0xA1) | ✅ | implemented; untested |
| Feature flags (0xA2) | ✅ | V4/V2 parsers + named bits, unit-tested |
| Layer count (0x11) / current layer (0xA3) | ✅ | implemented; untested |
| Keymap dump (0x12, 14-key chunks) | ✅ | chunk math + decode unit-tested |
| Keymap get/set (0x04/0x05) | ✅ | 0x04 shape inferred (spec §8) |
| Keymap clear (0x06) | ✅ | implemented; untested |
| Knob/encoder get/set (0x14/0x15) | ✅ | implemented; untested |
| Macro list/dump/set/reset (0x0C–0x10) | ✅ | V9/V11 encoders unit-tested; echo-verified writes |
| Macro repeat (0xA7 sub 27/28) | ⬜ | |
| Lighting get/set/save (0x07/0x08/0x09) | 🔄 | v3 param IDs (protocol ≥ 11) |
| Per-key RGB (0xA8) | ⬜ | |
| HE: travel/RT/DKS/calibrate (0xA9) | ⬜ | |
| Sleep (0xA7 sub 11/12) | ⬜ | |
| Debounce (0xA7 sub 5/6) | ⬜ | |
| Poll rate (0xA7 sub 13/14) | ⬜ | |
| Battery (0xA7 sub 49) | ⬜ | |
| Reset keymap/EEPROM (0x06/0x0A) | ⬜ | destructive; needs `--yes` |

## Firmware

| Feature | Status | Notes |
|---|---|---|
| All bootloader flashing (STM32/WB/XJ/XJ_NEW/NXP/Nations/PCA) | 🚫 | wontdo (documented in spec §6) |
| BT module DFU / Realtek CFU | 🚪 | 🚫 wontdo |

## Test hardware available

- **Keychron M6 (1k, vpid 875876452)** via **Link-KM dongle (0x3434:D026)** —
  mouse interface `/dev/hidraw3` (reports 181/182), keyboard interface
  `/dev/hidraw5` (idle — no keyboard paired).
