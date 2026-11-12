# Keychron Launcher Protocol Specification

Reverse-engineered from the Keychron Launcher web app (`launcher.keychron.com`, launcher version V1.5.4, captured 2026-11-12).

> **Methodology & scope.** This document records *factual protocol behavior* (USB HID report formats, command IDs, endpoint URLs, data schemas) extracted from the unobfuscated-but-minified launcher bundles and from public HTTP endpoints. It is the reference for writing an original CLI implementation. Do not copy launcher source code into the implementation; implement from this spec and validate against live device traffic where possible. Byte-offset references to the minified bundles are kept in [Appendix A](#appendix-a-source-index) for traceability.

**Terminology**
- **vpid** = `vendorProductId` = `(VID << 16) | PID` (decimal).
- **protocol version** = QMK raw-HID protocol version queried at connect (`cmd 0x01`); `>= 11` ⇒ "v3" JSON definitions, else "v2".
- **KC** = Keychron-specific extension commands (IDs `0xA0`–`0xAF`).

---

## 1. Device discovery & identification

The launcher uses **WebHID** (`navigator.hid`), not generic WebUSB. Devices are selected by their HID **collection signature** `(usagePage << 16) | usage`:

| usagePage | usage | Signature | Role |
|---|---|---|---|
| 0xFF60 (65376) | 0x61 (97) | `0xFF600061` | **Keyboard** (QMK raw HID interface) |
| 0x008C (140) | 0x01 | `0x008C0001` | **Bridge** (2.4 GHz receiver / RF dongle) |
| 0xFFC1 (65473) | 0x01 | `0xFFC10001` | **Mouse** (8K gaming mice) |
| 0xFF0A (65290) | 0x01 | `0xFF0A0001` | **Mouse 4K** (Nordic-based mice/receivers) |

Known VIDs:

| VID | ID | Meaning |
|---|---|---|
| 0x3434 | 13364 | Keychron (keyboards, mice, receivers) |
| 0x362D | 13869 | Mouse family (e.g. vpid 0x362DFF61 special-cased as "BT-switch optional mouse") |
| 0x3497 | 13495 | XJ / Nations bootloader |
| 0x19F5 | 6645 | GMJS / Native bootloader |
| 0x0BDA | 3034 | Realtek Bluetooth module (Microsoft CFU) |

Bootloader HID matching additionally filters product name by `/DFU|Bootloader|Boot ROM|XJ/i`.

One physical device can expose several of the above interfaces; the app groups them into roles `{keyboard, mouse, mouse_4k, bridge}` and instantiates one logical device per role.

**Identification flow:** `vpid = (VID<<16)|PID` → fetch product metadata from `GET {vapi}/v2/product/{vpid}` → fetch device definition from `GET {static}/device/{vpid}/json/v3.json` (fallback `v2.json`). The product API's `desc` field, when numeric, **overrides the HID payload buffer size** (default 32 bytes; e.g. 64 for some models).

---

## 2. Transport layer

### 2.1 Keyboard (QMK raw HID)

- Interface: usagePage `0xFF60`, usage `0x61`.
- **reportId 0**, payload = `Uint8Array(32)` by default (product-overridable).
- **No framing, no checksum, no sequence numbers.** Payload byte 0 = command ID; the device replies with an input report whose byte 0 echoes the command ID (often with argument bytes echoed too).
- Command matching: the app subscribes to input reports and filters on `r[0] === cmdId` (plus echoed args for some commands, e.g. macro buffer writes verify the entire echoed payload).

### 2.2 Transceiver semantics (request/response discipline)

| Transceiver | Behavior |
|---|---|
| **Serial (default, keyboards)** | FIFO command queue; strict lockstep — **one outstanding command; any input report dequeues it**; 1000 ms timeout, up to 3 retries, then drop. This is the de-facto ack. |
| Write-through | Fire-and-forget writes. |
| **Feature (1k mice, wired)** | `sendFeatureReport(reportId, data)` then `receiveFeatureReport(reportId)`; strip leading report-ID byte from the returned DataView. |
| **1k receiver** | Feature-report transceiver; watches input reports with `data[0] === 0xE4` or `0x52`; `data[1] === 0` → wait 500 ms then poll; `data[1] === 1` → poll immediately. |

### 2.3 Mouse / receiver transports

Selected by product JSON `protocol` field (`"1k" | "4k" | "8k" | "8k_nordic" | "kc_lite"`) and work mode. **The deciding rule (launcher `createByMouse`)**: if the dongle exposes a 0xFFC1 `mouse` collection, the wire protocol is **"8k"** (reportId 181/182) *regardless of the paired mouse's polling rate* — a 1k-polling M6 behind a Link-KM dongle speaks the 8k report protocol. "1k" applies only to dongles exposing just the bridge collection; "4k"/"8k_nordic" to 0xFF0A dongles.

| Protocol | reportId | Buffer | Checksum |
|---|---|---|---|
| 1k | 81 (0x51) | 20 B | none |
| 4k / 8k_nordic | 0 | 64 B | `buf[63] = 0xA1 - (sum(buf[0..62]) & 0xFF)` |
| 8k | 181 (0xB5) version/lighting, 179 (0xB3) fun-keys | 20 / 63 B | none |
| Bridge (own DFU) | 178 (0xB2) | 32 B | 0xAA framing (see §6.2) |
| Bridge (keyboard passthrough) | 0 | — | same as keyboard (forwarded) |

**Empirically verified (Link-KM dongle 0x3434:D026, M6 1k mouse):** the dongle's mouse interface (hidraw, usage page 0x8C reports) has NO report 81 and no feature reports — all mouse traffic goes through OUT report 181 / IN report 182 (20-byte). Requests: cmd 2 (version: link vid/pid, mouse fw version `r[8].(r[7]>>4&15).(r[7]&15)`, workMode = `r[9] & 7`, feature flags r[11..16]); cmd 3 (receiver state: three fixed 5-byte slots `[vidLo, vidHi, pidLo, pidHi, state]` at offsets 2/7/12 — identifies the paired mouse's vpid; state 1 = connected).

Bridge async notifications: after `sendReport(0, [0xBC, 0, ...])`, events arrive as `[0xE2, connectState]` and `[0xBC, kb1, kb2, mouse]`.

### 2.4 Screen keyboards

`KC_SCREEN (0xAC)` with 64-byte buffers and a **16-bit big-endian checksum at bytes 62–63** = sum of bytes 1–5. GUIF opcodes: FLASH_WRITE=1, FLASH_READ=2, FLASH_ERASE=3, FS_FILE_OPEN=4, FS_FILE_WRITE=5, FS_FILE_READ=6, FS_FILE_CLOSE=7, FS_FILE_REMOVE=8, FS_FORMAT=9, GETTIME=15, SETTIME=16, RESPONSE=119, SET_FONT_COLOR=32, SET_WALLPAPER=33. *(FS-op packet bodies not fully traced — see §8.)*

---

## 3. Command reference — keyboard (reportId 0, byte 0 = command ID)

### 3.1 QMK base commands

| ID | Name | Notes |
|---|---|---|
| 0x01 | GET_PROTOCOL_VERSION | resp `[0x01, ?, version]`; `protocol = resp[2] || 5` |
| 0x02 | GET_KEYBOARD_VALUE | sub 3 = matrix/switch mask (payload from byte 2; byte 3 if protocol ≥ 12; rows × ⌈cols/8⌉ bitmask); sub 4 = JSON version (`r[2]<<32 \| r[3]<<16 \| r[4]<<8 \| r[5]`; >100 ⇒ 0) |
| 0x03 | SET_KEYBOARD_VALUE | |
| 0x04 | DYNAMIC_KEYMAP_GET_KEYCODE | |
| 0x05 | DYNAMIC_KEYMAP_SET_KEYCODE | `[0x05, layer, row, col, code>>8, code&0xFF]` |
| 0x06 | DYNAMIC_KEYMAP_CLEAR_ALL | keymap reset |
| 0x07 / 0x08 / 0x09 | CUSTOM_MENU SET / GET / SAVE (= backlight config) | see §3.4 |
| 0x0A | EEPROM_RESET | |
| 0x0B | BOOTLOADER_JUMP | |
| 0x0C | MACRO_GET_COUNT | resp `[0x0C, count]` |
| 0x0D | MACRO_GET_BUFFER_SIZE | `resp[1]<<8 \| resp[2]` |
| 0x0E | MACRO_GET_BUFFER | `[0x0E, offHi, offLo, size]` → `[0x0E, offHi, offLo, size, data...]` |
| 0x0F | MACRO_SET_BUFFER | `[0x0F, offHi, offLo, len, data...]`; **response must byte-for-byte echo the request** |
| 0x10 | MACRO_RESET | |
| 0x11 | DYNAMIC_KEYMAP_GET_LAYER_COUNT | resp `[0x11, count]` |
| 0x12 | DYNAMIC_KEYMAP_GET_BUFFER | 14-key chunks: `[0x12, offHi, offLo, 2*n]`, offset = `28*i + layer*(rows*cols*2)`; keycodes big-endian 2 B each |
| 0x13 | DYNAMIC_KEYMAP_SET_BUFFER | |
| 0x14 | DYNAMIC_KEYMAP_GET_ENCODER | `[0x14, layer, knobId, direction]` → `[0x14, ?, knobId, direction, code>>8, code&0xFF]` |
| 0x15 | DYNAMIC_KEYMAP_SET_ENCODER | `[0x15, layer, knobId, direction, code>>8, code&0xFF]` |

Macro bytes encoding differs by protocol version: ≥ 11 uses the "V11" encoding (items prefixed with `1`, delay digits ASCII-terminated); older uses "V9" encoding.

### 3.2 Keychron extension commands

| ID | Name | Notes |
|---|---|---|
| 0xA0 | KC_GET_PROTOCOL_VERSION | resp `[0xA0, kcVersion, ?, instruction_set]`; `instruction_set` (1–4) selects feature-flag parsing variant |
| 0xA1 | KC_FIRMWARE_VERSION | ASCII from byte 1 |
| 0xA2 | KC_GET_SUPPORT_FEATURE | feature1 = `r[4]<<8 \| r[3]`; feature2 = `r[8]<<16 \| r[7]<<8 \| r[6]` — bits below |
| 0xA3 | KC_GET_CURRENT_LAYER | `[0xA3, 0, 255]` → `[0xA3, currentLayer, activeLayer]`; `activeLayer == 255` ⇒ upload-current-layer unsupported |
| 0xA7 | KC_MISC_CMD_GROUP | subcommands in byte 1 — §3.3 |
| 0xA8 | KC_RGB | per-key RGB — §3.5 |
| 0xA9 | KC_HE | hall-effect/magnetic switches — §3.6 |
| 0xAB | KC_FACTORY | factory/test — §3.7 |
| 0xAC | KC_SCREEN | screen keyboards — §2.4 |

**Feature flags (0xA2, V4 parse):** feature1 bit0 = Support_LE, bit1 = Support_RF. feature2: bit0 Get_Default_Layer, bit2 Keychron_RGB, bit6 DEBOUNCE, bit7 Snap, bit8 Sleep, bit9 PollRate, bit12 Upload_Default_Layer, bit16 Bootloader_Jump, bit17 Performance_Mode, bit18 Latch, bit20 Macro_Repeat. V2 parse selects byte offsets by `instruction_set`.

### 3.3 KC_MISC_CMD_GROUP (0xA7) subcommands (byte 1)

| Sub | Function | Layout |
|---|---|---|
| 1 | poll-rate capability query | resp `[0xA7,1,?,verLo,verHi,mask,flags]`; ver 3 ⇒ "v2 poll-rate" parser |
| 5 / 6 | debounce get / set | get `[0xA7,5,255]`; set Telink `[0xA7,6,press,release]`, QMK `[0xA7,6,subType,press]`; legacy raw cmds 0xB0 (set) / 0xB1 (get) |
| 11 / 12 | sleep get / set | set `[0xA7,12, backlLo, backlHi, sleepLo, sleepHi, scanLo, scanHi]`; get resp: backlight=`r[4]<<8\|r[3]`, sleep=`r[6]<<8\|r[5]`, magnetScan=`r[8]<<8\|r[7]` |
| 13 / 14 | poll-rate get / set | v2 parser: support masks at bytes 5 & 10, level = `8000/(1<<r[6])` / `8000/(1<<r[11])`; set `[0xA7,14, usbLevel, frLevel]` |
| 15 / 16 | "func" get / set | *(semantics inferred from fake-device data — gap, §8)* |
| 17 / 18 | performance mode get / set | |
| 20 | reset single key(s) | `[0xA7,20, 1<<layer, ?, data...]` |
| 21 | jump to bootloader | |
| 22 / 23 / 24 | latch/toggle GetCount / GetToggle / SetToggle | entries 5 B: `[layer,row,col,type,repeat]` |
| 27 / 28 | macro repeat get / set | set `[0xA7,28,1,macroIdx,times]` |
| 32 | "Nape_Settings" container | sub-sub byte 2: Set_Tapholds=7, Get_Tapholds=8, Set_Combos=9, Get_Combos=10, Set_Gesture=11, Get_Gesture=12, Del_Combos=14, Del_Tapholds=15, Get/Set_Force_Gesture_Scroll=18/19, Get/Set_Ori=20/21, Get/Set_Layer_Ori=24/25, Get/Set_Auto_Mouse=33/34, Get/Set_Auto_Mouse_Params=35/36, Get/Set_Touch_Speed=37/38. Response status at byte 3 (0 = OK) |
| 32 (DPI) | mouse DPI container | sub-sub: Get/Set_Dpi=1/2, Set/Get_Dpi_Value=3/4, Get/Set_Dpi_Counts=5/6, Get/Set_Custom_Dpi=22/23, Get/Set_Layer_Dpi=29/30, Get/Set_Layer_Dpi_Value=31/32 |
| 48 | battery subscribe | unsolicited battery reports |
| 49 | battery get | resp value = `r[2]`, status = `r[3]` |

### 3.4 Backlight / lighting (CUSTOM_MENU 0x07/0x08/0x09)

Protocol ≥ 11 (v3): parameter IDs — bright=3/1, effect=3/2, speed=3/3, color=3/4.
- Get: `[0x08, 3, paramId, ...]`
- Set: `[0x07, 3, paramId, value...]`
- Save: `[0x09, 3]`

Older (v2): parameter ID from the product JSON `supportedLightingValues`. A byteLen-6 variant uses `[6]` as the second byte.

### 3.5 KC_RGB (0xA8) — per-key RGB / mix

Subcommands (byte 1): Version=1, SaveLedConf=2, LedIndicationGet=3, LedIndicationSet=4, LedCount=5, LedNumber=6, GetEffect=7, SetEffect=8, GetLedColor=9, SetLedColor=10, Info=11, GetLedRegion=12, SaveLedRegion=13, GetMixConf=14, SetMixConf=15.

Examples: LED numbers by row `[0xA8, 6, row, 255,255,255]` → indices at bytes 3–25 (255 = none); set LED color `[0xA8, 10, ledIndex, ?, colors...]`.

### 3.6 KC_HE (0xA9) — hall-effect / magnetic switches

Subcommands (byte 1): AMC_GET_VERSION=1, AMC_GET_PROFILES_INFO=16, AMC_SELECT_PROFILE=17, AMC_GET_PROFILE_BUFFER=18, AMC_SET_PROFILE_NAME=19, AMC_SET_TRAVAL=20, AMC_SET_ADVANCE_MODE=21, AMC_SET_ACT_ON_AXIS=22, AMC_GET_CURVE=32, AMC_SET_CURVE=33, AMC_GET_REALTIME_TRAVEL=48, AMC_CALIBRATE=64, AMC_GET_CALIBRATE_STATE=65, AMC_GET_CALIBRATED_VALUE=66, AMC_RESET_SINGLE_KEY=29, AMC_GET_AXIS_TYPE=80, AMC_SET_AXIS_TYPE=81, AMC_CLEAR_PROFILE_BUFFER=30, AMC_SAVE_PROFILE_BUFFER=31.

Examples: set travel `[0xA9, 20, profile, workMode, press, sensPress, sensRelease, keyMode, ...]`; profile buffer read `[0xA9, 18, profile, offLo, offHi, 26]` → data at bytes 6–31; calibrate `[0xA9, 64, 2]`, stop `[0xA9, 64, 4]`.

### 3.7 KC_FACTORY (0xAB)

- Get work mode: `[0xAB, 5, 0...0, 5@byte30]` → resp `[0xAB, 5, mode]`; mode: 1=USB, 2=BT, 4=2.4G.
- Get device ID: `[0xAB, 9, 0...0, 9@byte30]` → resp length at byte 2, hex ID at bytes 3..3+len.
- Factory-test subcommands: BACKLIGHT=1, OS_SWITCH=2, JUMP_TO_BL=3, INT_PIN=4, GET_TRANSPORT=5, CHARGING_ADC=6, RADIO_CARRIER=7, GET_BUILD_TIME=8, GET_DEVICE_ID=9.

### 3.8 FR — keyboard-over-receiver (bridge interface, reportId 0)

| ID | Name | Notes |
|---|---|---|
| 0xB1 | FR_GET_PROTOCOL_VERSION | resp `[0xB1, verLo, verHi, feature1, feature2]`; feature bits (MSB-first): bit7 state_notify_over_via, bit6 multi_device_connect, bit5 mouse_driver_over_via, bit4 via_disable_gamepad_input |
| 0xB2 | FR_GET_STATE | resp vid=`r[3]<<8\|r[2]`, pid=`r[5]<<8\|r[4]`, connected=`r[6]===1` |
| 0xB3 | FR_GET_FW_VERSION | ASCII from byte 1 |
| 0xB5 | FR_CTL_GAMEPAD_RPT_ENABLE | |
| 0xBA | FR_DFU_OVER_VIA | |

After FR connect, normal QMK keyboard commands are tunneled through the receiver on reportId 0.

### 3.9 Mouse commands (summary)

- **1k** (reportId 81, 20 B): cmd 2 = version; 3 = bonded receivers (count + 5-byte entries `[vidLo,vidHi,pidLo,pidHi,state]`); 4 = firmware; 6 = base info (vid/pid/version/workMode/power at byte 10, state bits 0–1 of byte 11); 18/24/34/35 lighting; 49/50 fun-keys; 48 fun-key set `[0xE4, layer, keyIdx, ...]`.
- **8k** (reportId 181/179): cmd 2 = version (vid=`r[4]<<8|r[3]`, pid=`r[6]<<8|r[5]`, version=`r[2]<<8|r[1]`, frVersion=`r[8].(r[7]>>4).(r[7]&15)`, workMode, feature flags bytes 11–16); 32/36 lighting get/set `[36, target, effect, ?, speed, r, g, b]`; 49/50 fun-key get/set `[48, layer, keyIdx, type, data...]` — types: Remove=0, Mouse=1, Keyboard=2, Media=3, Macro=4, Dpi=5, Light=6, Game=7, ShortCut=8, Disable=9, Profile=10, PollingRate=13; 0xE1 monitor-light notifications; 0xE4 set-ack echo.
- **4k/8k_nordic** (reportId 0, 64 B + checksum): version query `[0, ?, 0x81, 0, workMode<<6, ...]`; resp `(r[0]===0||r[0]===64)&&r[3]===0`, version = `r[9].(r[8]>>4).(r[8]&15)`.

**8k fun keys (button bindings, reportId 179, 63-byte buffers) — live-verified on M6/Link-KM:**

| Operation | Request | Response |
|---|---|---|
| Get-all bound indexes | cmd 97 | `[97, count, entries...]`; each entry is `2 + (entry[0] >> 5)` bytes, key index = `entry[0] & 0x1F` |
| Get one binding | `[98, keyIndex]` | `[98, keyIdx, ?, type, data...]` |
| Set one binding | `[82, keyIndex, 0, type, data...]` | ack `[0xE4, 0, 82]` (cmd 82, not 48 — cmd 48 is the bridge-module variant) |

Types (`EFunKey`): Remove=0, Mouse=1, Keyboard=2, Media=3, Macro=4, Dpi=5, Light=6, Game=7, ShortCut=8, Disable=9, Profile=10, PollingRate=13. Data encodings (launcher `numToHighLow(value)` defaults to **2 bytes LowToHigh** — mind the order): Mouse = 24-bit HighToLow (`EBasicKey`: left=0x010000, right=0x020000, middle=0x040000, backward=0x080000, forward=0x100000, leftDouble=0x800000, upScroll=0x200, downScroll=0xFE00, leftScroll=0xFE, rightScroll=0x02, sustain variants 0x4000FF/0x400001); Keyboard = `[modifierMask, key1, key2]` (modifiers ctrl=1, shift=2, alt=4, win=8, +R variants 16–128); Media = 16-bit LowToHigh (`EMediaKey`: mute=226, next=181, previous=182, stop=183, pause=205, player=387, volumeUp=233, volumeDown=234); Dpi/PollingRate = `[ELoopKey, 0]` (loop=1, next=2, prev=3); ShortCut = 24-bit HighToLow (`EShortcutKey` table, e.g. copy=459014, paste=459033); Profile = `[index]`; Light = `[value]`.

**8k profiles — live-verified:** info comes from the cmd-6 settings snapshot (current = r[1], count = r[50]). Switch/create/remove is one command: `[14, profile, count]` on reportId 181, ack `[0xE4, 0, 14]`. Switching passes the current count; changing the count creates/removes profiles (launcher `switchProfileLevelNum`; if active ≥ new count it resets to 0). Profile-change async notifications arrive on IN report 229 (0xE5) with the new current index at byte 1.

---

## 4. Protocol version detection (connect sequence)

1. `navigator.hid` connect on collection signature (§1).
2. Send `[0x01]` (reportId 0) → `protocol = resp[2] || 5`.
3. `protocol >= 11` ⇒ "v3" definitions, else "v2".
4. Send `[0x02, 4]` → JSON version (32-bit, treated as opaque; >100 ⇒ 0). Version N > 0 selects a versioned definition file `v3-N.json`; otherwise plain `v3.json` (or `v2.json` for protocol < 11).
5. Send `[0xA0]` → Keychron protocol version + `instruction_set` (selects feature-flag parsing variant).
6. Fetch product metadata `{vapi}/v2/product/{vpid}` and definition `{static}/device/{vpid}/json/v3.json` (fallback `v2.json`).
7. Feature probing with per-probe timeouts: DIP, sleep, lang, layout JSON, debounce, poll-rate, decorate-light → `supportFeature` map.

---

## 5. Backend HTTP API

Base URLs (US region; `.cn` mirrors exist):

| Base | Purpose |
|---|---|
| `https://launcher.keychron.com/static/` | device JSONs, layouts, i18n, assist app |
| `https://launcher.keychron.com/vapi/` | REST metadata API |

Endpoints (all return `{code, msg, data}`; CORS permissive — any Origin reflected):

| Endpoint | Purpose |
|---|---|
| `GET /vapi/v2/product/{vpid}` | product metadata: name, `vendor_product_id`, `desc` (numeric ⇒ HID buffer size), `bootloader`, `protocol`, `workMode` |
| `GET /vapi/v2/firmware/{vpid}` | firmware list for device |
| `GET /vapi/v2/layouts` | layout catalog: `[{id, layout_name, layout, langs, sort}]` |
| `GET /vapi/v2/version` | launcher version + release notes |
| `POST /vapi/v2/feedback` | issue reporting (header `Content-Device: {pid}`) |
| `GET /static/device/{vpid}/json/v3.json` | device definition (fallback `v2.json`; versioned `v3-N.json` when the device reports JSON version N) |
| `GET /static/layouts/keycode-{layout}-{mac\|win}.json` | keycode catalog per layout/locale |
| `GET /static/i18n/{lang}.json` | UI translations (35 languages) |
| `GET /static/assist/KeychronAssist-{ver}.{zip\|dmg}` | Quick Start companion app (ver 1.1.6) |

The Quick Start feature requires the companion app running a local daemon at `ws://localhost:23333`.

Layout catalog (19 layouts): us, japan, uk, de, nordic, franch, swiss, Spanish, Italiano, Turkey, Ukraine, Portugal, Czech, Slovenia, Hungary, Belgium, Korean, Slovakia, Thai.

---

## 6. Firmware upgrade

### 6.1 Keyboard MCU

1. From running keyboard: `0xA7` sub 21 (JumpBootloader) or QMK `0x0B`.
2. Device re-enumerates as bootloader: HID VID ∈ {0x3497, 0x3434, 0x19F5} + name `/DFU|Bootloader|Boot ROM|XJ/i` ⇒ HID protocol; WebUSB names `STM|WB|DFU in FS Mode` ⇒ USB/DFU protocol. Product JSON `bootloader` field: `"STM32","WB","XJ","XJ_NEW","NXP","Nations","PCA"`.
3. Per-bootloader protocol:

| Bootloader | Transport | Framing |
|---|---|---|
| STM32 / WB | WebUSB DFU | standard DFU |
| XJ | HID, reportId 1 | frames start `0xA5`; ACK `[0xA1]`; data `[0xA5, lenHi, lenLo, 4, 3, 0, 0, addr..., payload]` in 508-byte chunks |
| XJ_NEW | HID, reportId 0 | init `[0x99, 0x55, 0x80, 3]` → version at resp[4..5]; data `[1,1,seq,len,...]` + checksum `1+~sum`, 64-byte reports; final ACK `[1,1,seq,0,checksum]` |
| Nations (N32) | HID | 64-byte frames, per-chunk checksum `1 + (~sum & 0xFF)` |
| NXP | HID, reportId 1 = cmd / 2 = data / 3 = resp | blhost: buffer `[0, lenLo, lenHi, data...]`; get-property=7, flash-erase-region=2, write-memory=4, reset=11; status word (LE u32 @ offset 4) must be 0 |
| PCA | **WebSerial** | SLIP framing, CRC-16, prefix byte 0xC0 |

4. Firmware validation: search the `.bin` for the byte pattern `[vidLo, vidHi, pidLo, pidHi]` (skipped for 0x3434:0x0A33 and 0x3434:0x0437).

### 6.2 Bluetooth module (keyboard-internal or receiver)

0xAA-framed protocol. Frame: `[0xAA, 0x55|0x56, len, ~len, payload...]` (`0x55` = no-ack, `0x56` = ack-required). Device responses start `0xAA 0x57` (keyboard interface, prefixed with module byte `0x03`) or `0xAA 0x55` (receiver, reportId 178).

Commands (byte 5; byte 4 = 1 get / 2 enter / 3 begin): 0x60 BT module version info (model @ 4–14, fw @ 16–25, hw @ 26–35); 0x61 DFU version (1 or 2; v1 inserts extra `[6]=0,[7]=seq`); 0x62 start; 0x63 begin (checksum `99+DFUVersion`); 0x64 write 16-byte chunks (seq 1–255, 16-bit sum checksum); 0x65 CRC32 verify (success ⇒ `AA5704FB..A3..65` with crc `0000`; on receiver `resp[8]!==255 && resp[9]===0`); 0x66 switch/reboot; 0x67 bootloader switch; 0x6E patch version.

Realtek modules use **Microsoft CFU** instead (VID 0x0BDA:0x4762): offer via `sendReport(45, offer)`, payload via `sendReport(42, [flags (0x80 first/0x40 last), len, seqLo, seqHi, off0..3, data...])`, 52 data bytes per 60-byte report.

---

## 7. Device definitions & catalog

### 7.1 Definition JSONs (v2/v3)

**Form.** Server-side JSONs are **VIADefinitionV3-shaped** objects (post-KLE-conversion: `layouts.keys[]` with geometry) **plus Keychron-proprietary extension fields** (`feature`, `extra`, `bootloader`, `layers`). The ajv-compiled `KeyboardDefinitionV2/V3` schemas are used only to validate *user-uploaded* files, which are then converted via `keyboardDefinitionV3ToVIADefinitionV3`. Server JSONs are consumed without ajv validation.

**URL selection:** HID protocol ≥ 11 ⇒ `v3.json`, else `v2.json`; if the device-reported JSON version N > 0 (via `GET_KEYBOARD_VALUE` param 4), fetch `v3-N.json` (e.g. `v3-1.json` for J8 HE 8K).

**V2 required:** `layouts, lighting, matrix, name, productId, vendorId` (IDs as hex strings, `"0x3434"`). `lighting` is a preset enum (`none, qmk_backlight, qmk_backlight_rgblight, qmk_rgblight, wt_mono_backlight, wt_rgb_backlight`) or an object `{extends, effects?, underglowEffects?, keycodes?, supportedLightingValues?}`. `layouts` = `{width, height, keys[], optionKeys, labels?, presets?}`.

**V3 required:** `layouts, matrix, name, productId, vendorId` (+ `firmwareVersion, keycodes, menus, vendorProductId` in converted form). `lighting` is replaced by `menus`:

```json
{"label":"Lighting","content":[{"label":"Backlight","content":[
  {"label":"Brightness","type":"range","options":[0,255],
   "content":["id_qmk_rgb_matrix_brightness", 3, 1]},
  {"label":"Effect","type":"dropdown","content":["id_qmk_rgb_matrix_effect", 3, 2],
   "options":[["00. None",0], ...]}]}]}
```

A menu leaf's `content` = `[id, groupByte, valueByte]` maps directly to the HID write `[0x07, groupByte, valueByte, data...]`; group bytes: 1=qmk_backlight, 2=qmk_rgblight, 3=qmk_rgb_matrix, 4=qmk_audio, 6=decorate light. Save = `[0x09, 3]`. Leaf types: `button, toggle, dropdown, range, keycode, color, color-palette`; optional `showIf` (expression grammar not RE'd), `bytes` (1–4), `unit`.

**Per-key fields (`layouts.keys[]`):** `row`, `col` (**the authoritative link to HID matrix addressing**), geometry `x, y, w, h, w2/h2/x2/y2` (stepped keys), `r/rx/ry` (rotation), `d` (decal/ghost), `color` (`accent|alpha|mod`), `ei` (extra index — knob/non-key control, excluded from key counts), `wheel` (encoder/knob ID), `knobVertical`, `wheelReverse`, `stick`, `dual`, `li` (light index), `radius`, `bg`, `url`. `optionKeys` = `{groupKey: {optionIndex: [keys]}}` — VIA layout alternates (all sampled server JSONs ship `{}` and use `ei` keys instead).

**Keychron extension fields** (not in the ajv schemas):

| Field | Semantics |
|---|---|
| `bootloader` | `"XJ", "PCA", "XJ_NEW", "WB", "NXP", "Nations", "KLD", "telink"` — flashing route; `"KLD"` also hides `DF()` layer keycodes |
| `feature` | `["Base"]` default; `"HE"` → hall-effect device class; `"SCREEN"`; `"DIP"`; `"DECORATE_LIGHT"`; `"DEBOUNCE"` |
| `extra` | see HE fields below; also `debounceConf` (`[{name,min,default,max,type}]`, types 0–5), `mode: ["USB"]`, `splitCase`, `rotaryLight`, `disableKey [[row,col]]`, `touchKeys`, `onlyView` |
| `layers` | (v2) per-layer display names, e.g. `["Mac","Mac","Win","Win"]`; real count always read from HID `0x11` |
| `protocol` | **mouse/trackball JSONs only** (`"1k"/"4k"/"8k"/"8k_nordic"`); keyboards get the protocol number from HID `0x01` |
| `workMode` | 0=USB wired, 1=BT/2.4G — fallback default only |
| `style` | `{bg?, base?: {regex → image url}}` board/key-cap images |
| `stickKeys` | `[[row,col]]×4` joystick keys; `IsolatedLed` — extra rendered LEDs |

**HE fields** (`feature` contains `"HE"`):

- `extra.he.distance`: `{min, max, deadzone?}` in mm (e.g. J3 HE: `{max:3.3, min:0.2, deadzone:0.2}`); defaults `{min:0.2, max:3.8}`.
- `extra.he.degree`: `[{min,max,value}]` — travel-resolution table (e.g. 0.01 mm below 2.5 mm, 0.005 mm above).
- `extra.he.preventSwitchProfile`, `preventSwitchProfileTip` (bool).
- HE firmware version (1–5, from `AMC_GET_VERSION`) selects the command-class variant; response bit flags: switchAxis, resetSingleKey, dkr (dynamic key repeat), turbo, gamepadDisable, supportMechanical. Snap Tap needs HE version ≥ 3.
- **Actuation (`AMC_SET_TRAVAL` 20):** `[0xA9, 20, profile, workMode, press, sensPress, sensRelease, keyMode, ...24-byte key bitmap]`; V1–V4 single-byte values, V5 16-bit (`numToHighLow`). `workMode`: Global=0, Static=1, QuickTrigger=2 (rapid trigger). `keyMode`: 0=per-key (bitmap bit at `3*row + col/8`, bit `col%8`, col<24), 1=global. Device units = `round(mm/unit)`.
- **DKS (`AMC_SET_ADVANCE_MODE` 21):** `[0xA9, 21, profile, 1, row, col, position(0–19), d0, d3, d1, d2, kc0.lo, kc0.hi, ...×4, actions]`; action enum: 0=none, 1=release, 2=press, 3=release+press. Max 20 DKS/profile. Distances 0–63 (V1–V4) or 0–1023 (V5). Snap-tap (SOCD) uses cmd 21 with types 2=SC, 3=LKP, 4=AP1st, 5=AP2nd, 6=CM; max 5 pairs/profile.
- **Profile buffer (`AMC_GET_PROFILE_BUFFER` 18):** size = `rows*cols*4 + 20*19` bytes; per-key 4 bytes LE: bits 30–31 mode, 24–29 distance, 18–23 advance_press, 12–17 advance_release, 8–11 advanceMode (3=DKS, 4/5=snap-tap), 0–7 dksIndex; then 20 DKS entries × 19 bytes (3-byte id + 4×2-byte keycodes + actions).
- **Curve (`AMC_GET/SET_CURVE` 32/33):** 4 (x,y) points; up to 3 profiles (Default/Gaming/Gamepad), names as 16-bit chars.

**Keycode ranges:** `CUSTOM(n) = _QK_KB(24448) + n` (max 24363... i.e. 24463), `MACRO(n) = _QK_MACRO(24338) + n` (max 24353). Protocol-specific keycode enum tables are selected by protocol number (12/13, 11, 10, default — differing `_QK_MOD_TAP` bases: 24576 vs 8192). Layer codes MO/DF/TG/OSL/TO/TT computed from `_QK_*` ranges.

**JSON → HID mapping summary:**
- Set key: `[0x05, layer, row, col, code>>8, code&0xFF]` — `row`/`col` taken directly from the key object.
- Bulk read: 14-key chunks, offset `28*chunk + layer*rows*cols*2`; response pairs = big-endian keycodes at flat index `i` → `row = i/cols`, `col = i%cols` (row-major, layer-strided).
- Encoders: `knobId = key.wheel ?? key.ei`; direction Left=0/Right=1.
- Lighting/config: `[0x07, group, value, data...]` then `[0x09, 3]`.

### 7.1.1 Layout files (`static/layouts/keycode-{layout}-{os}.json`)

These are **keycode-language catalogs, not physical layouts**: an array of category objects (`basic, wt_lighting, media, macro, mouse, modtap, special, qmk_lighting, custom`), each with `keycodes: [{name, code: "KC_*", shortName?, capName?, keys?, width?, plat?, mode?}]`. The `special` category's names get remapped for Korean `km_os_switch` products. Selection: product name matches `/jis/i` → japan, `/iso/i` → uk, else us; `-mac`/`-win` suffix by platform. Embedded fallback copies exist in the bundles (modules 61507/33550) plus a full embedded "Tester" device JSON (module 5504) useful as a schema example.

### 7.2 Device catalog (scanned 2026-11-12)

A full scan of vpid space `0x3434:0x0000–0xFFFF` found **998 v3 definition JSONs** and **988 product records**. Keychron's HID PID space is dense: most PIDs cluster in `0x00xx` (Q series), `0x01xx`–`0x02xx` (K/V/B/HE), `0x0Exx`/`0x0Fxx`/`0xDxxx` (mice and newer 8K models).

Product metadata (`vapi/v2/product/{vpid}`) shape:

```json
{
  "code": 200, "data": {
    "product": { "id", "name", "pid": "0xA1", "vid": "0x3434",
                 "vendor_product_id": "875823265", "desc": "",
                 "support_update": 1, "support_bluetooth_update": 0, ... },
    "json": [ { "vp_id", "id", "version": 0 } ],
    "category": { "name": "Q系列", "type": 0, ... },
    "firmware": { "lasted": { "version": "1.0.3", "path": "https://...", "type": 0, "updateTime": ... } }
  }
}
```

- `product.desc` (numeric) overrides the HID payload buffer size (default 32).
- `firmware.lasted.path` points to the latest firmware binary — hosted mostly on `cdn.shopify.com` (654), some on `sysmgr.keychron.cn` (57) and `launcher.keychron.com` (3); ~150 records contain malformed paths (bare vpid strings).

Catalog by category (988 products): Q系列 59, M-鼠标 37, DVT_键盘 263, K Max系列 98, HE系列 109, K X系列 47, V系列 43, V Max 系列 20, Q Max系列 29, C Pro系列 32, Q Pro系列 21, K Pro系列 66, DVT_鼠标 18, V Ultra 18, B系列 23, K HE系列 11, Lemokey-鼠标 3, K系列 14, Q Ultra系列 9, Keychron Ultra 系列 10, B Pro系列 19, V HE 4, X系列 3, K Ultra 12, 其他键盘 6, KM-鼠标 4, 接收器 4, R系列 2, 轨迹球 2, JamesDonkey J系列 1, 第三方键盘 1.

Full catalog: `launcher-sources/vapi/device-catalog.json` (vpid, VID/PID, name, category, firmware version/path, JSON version per device). Per-device definitions: `launcher-sources/device/{vpid}/json/v3.json`.

---

## 8. Gaps / open questions

- **MISC subcommand numbering variance:** analysis of different device classes yields slightly different `0xA7` subcommand maps (e.g. debounce at 5/6 vs 3; "lang" at 11 in some classes vs sleep at 11/12 in others). The table in §3.3 reflects the main keyboard class; **validate subcommand IDs against live traffic per device class** before relying on them.
- Mouse fun-key value payload encoding per type (keyboard/media/dpi/game) — writers located, per-type byte layouts not decoded.
- Screen keyboard GUIF file-system opcodes: opcode table complete, FS-op packet bodies not traced; `KC_SCREEN`/SCREEN class internals not fully documented.
- Bridge pairing *initiation* flow (bond list query documented; pair command not traced).
- `KC_MISC_CMD_GROUP` sub 15/16 ("func") semantics inferred only from fake-device data.
- Battery path (`0xA7` sub 48/49) found in mouse/receiver context; confirm for plain keyboards.
- `AMC_SET_ACT_ON_AXIS` (22) and `AMC_GET_REALTIME_TRAVEL` (48) wire formats not fully decoded.
- `optionKeys` with real data never found in sampled server JSONs (all `{}`) — structure documented from VIA schema + KLE conversion code only.
- `showIf` expression grammar inside `menus` not reverse-engineered.
- Mouse/trackball definition JSONs (`keys[]` with `index`/`type`, `protocol` field, NAPE `D_` commands) only sketched.
- All findings should be validated against live USB traffic (Wireshark/usbmon) with a real device.

---

## Appendix A: Source index

Minified bundle locations (`file @ byte-offset`; `main` = `main.b77853dfb65d3e44.js`):

| What | File | Offset |
|---|---|---|
| Command enums (QMK + KC + keycodes) | main (module 44040) | 1523492 |
| FR command enum (module 88493) | main | 1523160 |
| vpid/byteNor/crc32/crc16/SLIP helpers (module 62856) | main | 2168328 |
| Keyboard protocol class + base commands (module 50993) | main | 538077–610410 |
| Cached/demo keyboard commands (module 44007) | main | 516900–538077 |
| Transceivers incl. ack/retry (module 61892) | main | 905127 |
| Keyboard service: discovery (module 54485) | main | 2073809–2085000 |
| Device factory + version detection + webdfu registry (module 98022) | main | 869950–911120 |
| BT-module DFU on keyboard interface (module 72281) | main | 495300 |
| Bridge/receiver + its DFU (module 24823) | main | 507100 |
| FR keyboard-over-receiver (module 9789) | main | 514324 |
| 8K mouse protocol (module 98004) | main | 501500 |
| Mouse device + transceiver choice (modules 62364/5497) | main | 695346 |
| Mouse command classes + 64 B checksum (module 98502) | main | 707000–830000 |
| Keyboard battery (class He) | main | 856539 |
| Keyboard getDeviceId (KC_FACTORY 9) | main | 604519 |
| Keyboard init/feature detection | main | 604900–607800 |
| Screen keyboard GUIF (modules 73306/98415) | main | 682400–691163 |
| HE keyboard command classes (module 45448) | main | 621013 |
| Keyboard DPI (module 12060) | main | 610709 |
| RGB mix/per-key (module 98095) | main | 613300 |
| Firmware-update component (bootloader detection) | main | 2247300–2252000 |
| Bootloader route table | main | 2271398 |
| Realtek CFU BT update | 311.46538efcbcc62387.js | 58110–62900 |
| Factory-test connect | 19.8962716924fe6269.js | 120310 |
| Demo device VID/PIDs (K2-HE 0x3434:0x0E20, M6 8K 0x3434:0xD049) | main | 491497 / 492858 |
| API service (product/firmware/layouts endpoints, module 33596) | main | 2140700 |
| Hostname/API config + JW constants (module 50446) | main | ~2130000 |
| KLE→VIA layout utils (`kleLayoutToVIALayout`, module 55944) | main | 21454 |
| `keyboardDefinitionV3ToVIADefinitionV3` + `getLightingDefinition` (module 45164) | main | 33539 |
| `KeyboardDefinitionV2Schema` (ajv, module 18733) | main | 40044 |
| `KeyboardDefinitionV3Schema` (ajv, module 91958) | main | 58958 |
| `VIADefinitionV2Schema` / `VIADefinitionV3Schema` | main | 76942 / 95882 |
| LightingValue enums / LightingPreset (module 78571) | main | 28765 |
| Keycode service: layout detect, code2Key (module 44148) | main | 2117300 |
| Protocol→keycode-enum selection (module 13814) | main | 2113122 |
| Embedded "Tester" device JSON (module 5504) | main | 3373445 |
| Embedded layout fallbacks win/mac (modules 61507/33550) | main | — |
| HE device class + command classes V1–V5 (module 45448) | main | 621013–682522 |
| HE AI tool docs (actuation/RT/DKS/snap-tap/turbo) | 547.5100d943e6dd3bf7.js | 228080–235596 |
| Upload UI (`checkViaForV3/V2/V0`) | 706.589752398554eb09.js | whole chunk |
