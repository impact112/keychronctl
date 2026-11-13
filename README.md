# keychronctl

A stateless command-line tool and desktop app for configuring Keychron
keyboards and mice over USB HID — a local, open replacement for the
[Keychron Launcher](https://launcher.keychron.com) web app.

`keychronctl` speaks the same HID wire protocol the Launcher uses (reverse
engineered from its unobfuscated JavaScript — see
[`docs/PROTOCOL.md`](docs/PROTOCOL.md)), so it works fully offline, with no
account, no telemetry, and no browser.

```
$ keychronctl list
VPID         VID:PID    ROLE       PATH            PRODUCT
875876390    3434:D026  mouse-8k   /dev/hidraw1    Keychron Link-KM
875876390    3434:D026  keyboard   /dev/hidraw3    Keychron Link-KM

$ keychronctl settings
profile:    1 of 2
dpi:        stage 2 (1600 DPI)
rate:       level 2 (1000 Hz)
sleep:      10 min
debounce:   4 ms
battery:    96%
sensor:     lod 1, wave on, line off, motion on, scrollDir normal

$ keychronctl dpi set 800
DPI set: stage 2 -> 800
```

## What works

**Mice** (live-verified on a Keychron M6 via Link-KM dongle, 8k wire
protocol):

- device/link info, bond list, battery, firmware version
- DPI stages (read/switch/rewrite the ladder), polling rate
- per-profile button bindings (mouse / media / keyboard combos / DPI /
  polling-rate / shortcuts / disable), with profile create/switch/remove
- sensor options (LOD, angle snapping, motion sync, ripple control, scroll
  direction), debounce, sleep timer, scroll-wheel parameters
- RGB lighting on devices that have it
- device-definition cache with offline mode

**Keyboards** (QMK-style raw HID): protocol/version queries, feature flags,
keymap read/write, macro engine (V9/V11), encoders — implemented and
unit-tested, still awaiting live validation on hardware.

**Not implemented (wontdo):** firmware flashing / bootloader updates. Use
the official Launcher for that.

## The desktop app

`gui/` contains **keychronctl-qt**, a Qt6/QML app (Rust + cxx-qt). It is
fully decoupled from the CLI: it spawns `keychronctl` (or whatever binary
`KEYCHRONCTL` points at) and talks JSON over stdio, per the contract in
[`docs/GUI-CONTRACT.md`](docs/GUI-CONTRACT.md). A mock backend
(`gui/mock/keychronctl-mock`) exercises the whole UI without hardware.

```
cd gui
cargo run                          # uses keychronctl from PATH
KEYCHRONCTL=../target/debug/keychronctl cargo run
```

## Building

Rust toolchain, `hidapi` (system lib or the `hidapi` crate's vendored
build), and for the GUI: Qt 6 dev packages (base + declarative) and a C++17
compiler.

```
cargo build          # CLI
cargo build          # GUI (from gui/)
```

Linux udev: if your user can't access `/dev/hidraw*`, install a udev rule
granting access to Keychron devices (VID `3434`).

## Repository layout

| Path | Contents |
|---|---|
| `src/` | `keychronctl` CLI (Rust) |
| `gui/` | `keychronctl-qt` desktop app (Rust + cxx-qt + QML) |
| `docs/PROTOCOL.md` | reverse-engineered wire protocol spec |
| `docs/CLI.md` | CLI design / manpage source |
| `docs/GUI-CONTRACT.md` | GUI ↔ CLI subprocess contract |
| `TODO_PARITY.md` | feature parity tracker |
| `examples/` | live device probe harness |

## License

[GPL-3.0](LICENSE). Reverse-engineered for interoperability; not affiliated
with or endorsed by Keychron. `launcher-sources/` (gitignored) contains
harvested copies of the official Launcher app used as a protocol reference.
