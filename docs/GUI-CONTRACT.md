# GUI ↔ CLI contract

The keychron-launcher GUI is **fully decoupled** from keychronctl: no
build-time dependency, no shared code, no compiled-in schema. It talks to
*any* external binary honoring this contract.

## Invocation

```
<binary> --json <command> [subcommand] [args...]
```

- Binary resolution (runtime, in order):
  1. `KEYCHRONCTL` env var (absolute or PATH-relative path)
  2. `keychronctl` on `PATH`
- `--json` is always passed as the first argument.
- Device selection: the GUI passes `-d <path>` (a hidraw path from `list`)
  before the subcommand. Backends must accept and ignore unknown `-d`
  values only if they can select; otherwise exit 3.

## Output

- **stdout**: a single JSON document (object or array), UTF-8, followed by a
  newline. Empty stdout = error.
- **stderr**: human-readable diagnostics; error lines start with
  `error: `.
- **Exit codes**:

| Code | Meaning | GUI behavior |
|---|---|---|
| 0 | success (stdout carries JSON) | parse & display |
| 1 | runtime error (I/O, backend missing) | toast + status bar |
| 2 | usage error (bad arguments) | toast, usually a GUI bug |
| 3 | no device / selector matched nothing | mark device disconnected |
| 4 | protocol timeout (device asleep/unreachable) | "wake the device" hint |
| 5 | unsupported operation for this device | disable the control |

## Commands and JSON shapes (as implemented by keychronctl)

### `list` → array
```json
[{"vpid":875876390,"vid":13364,"pid":53286,"role":"mouse-8k",
  "usage_page":65473,"usage":1,"path":"/dev/hidraw1","product":"Keychron Link-KM"}]
```
`role` ∈ `bridge | keyboard | mouse-8k | mouse-4k | unknown`. The GUI shows
`mouse-8k`, `mouse-4k`, and `keyboard` entries.

### `info [-d SPEC]` → object
```json
{"vpid":875876390,"vid:pid":"3434:D026","role":"mouse-8k","path":"/dev/hidraw1",
 "product":"Keychron M6","protocol":"8k","version":"1.0.3",
 "work_mode":"wireless (BT/2.4G)","battery_percent":96,"charging":false,
 "product_vpid":875876452,
 "bonded":[{"slot":"slot3","state":1,"active":true,"vid":13364,"pid":53348,"vpid":875876452}]}
```

### `settings` → object (one-shot snapshot)
```json
{"protocol":"8k","profile":1,"profile_count":2,
 "dpi_levels":[4,4,4],"dpi_values":[400,800,1600,3200,5000],"dpi_level_num":5,
 "rate_levels":[2,2,0],"rate_table":[0,1,2,0,0,0],"rate_level_num":3,
 "sys":{"lod":1,"wave":true,"line":false,"motion":true,"scroll_dir":false,
        "fps20k":false,"angle":0,"lod_level":0,"lod_count":0},
 "debounce":4,"debounce_options":[4,...],
 "sleep_minutes":10,"battery_percent":96,"charging":false,
 "scroll":{"speed":0,"inertia":0,"spl":0},
 "feature1":55,"color_materials":[0,0,0]}
```

### Individual reads
| Command | JSON |
|---|---|
| `battery` | `{"battery_percent":96,"charging":false,"battery_state":"discharging"}` |
| `dpi` | `{"profile":1,"current_levels":[4,4,4],"level_num":5,"levels":[400,800,1600,3200,5000],"level_val_max":12000,"level_val_step":50}` |
| `pollrate` | `{"currentLevel":[2,2,0],"hz":1000,"levels":[0,1,2,0,0,0],"levelsHz":[125,500,1000,125,125,125],"levelNum":3}` |
| `sleep` | `{"minutes":10,"never":false}` |
| `debounce` | `{"ms":4,"options":[4,...]}` |
| `sensor` | the `sys` object from `settings` |
| `scroll` | `{"speed":0,"inertia":0,"spl":0}` |
| `lighting` | `{"mode":1,"brightness":50,"speed":50,"rgb":[255,255,255]}` — times out (exit 4) on devices without RGB |
| `profile list` | `{"current":1,"count":2}` |
| `binds list` | see below |
| `definitions list` | `[{"vpid":875876452,"name":"Keychron M6"},...]` |

`binds list`:
```json
{"vpid":875876452,"product":"Keychron M6","current_profile":1,"profile_count":2,
 "profiles":[{"profile":0,"bindings":[
    {"index":8,"key":"rightTilt","action":"media","value":"volumeUp","data":[233,0]}]}]}
```
`action` is the type name; `value` the decoded detail; `key` the definition
key id when known.

### Writes (all print a small JSON object or nothing; exit 0 on success)
| Command | Args |
|---|---|
| `dpi set` | `[--level N] [--levels 400,800,...] [--value DPI]` |
| `pollrate set` | `<HZ>` (125/250/500/1000/2000/4000/8000) |
| `sleep set` | `<MINUTES>` (0 = never) |
| `debounce set` | `<MS>` |
| `sensor set` | `[--lod 0..3] [--wave on\|off] [--line on\|off] [--motion on\|off] [--scroll-dir normal\|reverse] [--fps20k on\|off]` |
| `scroll set` | `<SPEED> [--inertia N] [--spl N]` |
| `lighting set` | `<MODE> [--brightness N] [--speed N] [--rgb R,G,B]` |
| `binds set` | `<INDEX> <ACTION> [--profile N]` |
| `profile` | `select <N>` / `create` / `remove` |

### Bind action vocabulary (`binds set`)
`mouse:left|right|middle|forward|backward|leftDouble|upScroll|downScroll|leftScroll|rightScroll|leftScrollSustain|rightScrollSustain`
· `media:volumeUp|volumeDown|mute|player|pause|previous|next|stop`
· `keyboard:<mods+keys>` e.g. `keyboard:ctrl+shift+t` (mods: ctrl/shift/alt/win + R variants; ≤2 keys)
· `dpi:loop|next|prev` · `pollingRate:loop|next|prev` · `profile:<N>`
· `shortcut:copy|paste|cut|refresh|switchApp|task|brightnessUp|brightnessDown|home|email|...`
· `light:<N>` · `remove` · `disable`

## Notes
- Backends must be stateless per invocation; the GUI re-reads after every
  write to refresh state.
- Timeouts (exit 4) are normal when a wireless mouse sleeps — the GUI shows
  a "wake the device" hint and keeps polling.
- A mock backend (`gui/mock/keychronctl-mock`) implements this contract for
  hardware-free UI development.
