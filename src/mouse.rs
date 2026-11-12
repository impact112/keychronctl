//! Mouse protocol commands (docs/PROTOCOL.md §3.9).
//!
//! Wire protocols by dongle/product (the launcher's `createByMouse` selection):
//! - Dongle with a 0xFFC1 `mouse` collection (e.g. Link-KM): **"8k"** protocol —
//!   output report 181, input report 182, 20-byte buffers.
//! - Dongle with only the 0x8C `bridge` collection: **"1k"** — feature-report
//!   path on report 81 (`_Y` transceiver).
//! - 0xFF0A `mouse_4k` collection: "4k"/"8k_nordic" — reportId 0, 64-byte
//!   buffers with checksum `buf[63] = 0xA1 - sum(buf[0..63])`.
//!
//! Key commands on the 8k path (reportId 181):
//! - cmd 2: link version info (vid/pid of the HID device, mouse fw version,
//!   work mode, feature flags)
//! - cmd 3: receiver state — three fixed 5-byte slots (kb1, kb2, mouse) with
//!   [vidLo, vidHi, pidLo, pidHi, state]; state==1 = connected. Identifies the
//!   paired mouse's vpid.
//! - cmd 6 on reportId 179 (63-byte "fun-keys" buffer): full settings
//!   snapshot — battery, DPI, polling rate, profile, sleep, debounce.

use crate::transceiver::{ProtocolError, Transceiver};
use hidapi::HidDevice;
use serde::Serialize;

pub const REPORT_1K: u8 = 81;
pub const REPORT_8K_VERSION: u8 = 181;
/// 8k "fun-keys" report (63-byte buffers); also carries the cmd-6 settings
/// snapshot (battery/DPI/polling).
pub const REPORT_8K_FUNKEYS: u8 = 179;
pub const FUNKEYS_BUF: usize = 63;
#[allow(dead_code)] // 4k/8k_nordic path, used when a 0xFF0A dongle is attached
pub const NORDIC_BUF: usize = 64;
pub const SMALL_BUF: usize = 20;

/// Apply the 4k/8k_nordic checksum: `buf[63] = 0xA1 - (sum(buf[0..63]) & 0xFF)`.
#[allow(dead_code)] // 4k/8k_nordic path
pub fn nordic_checksum(buf: &mut [u8]) {
    let sum: u8 = buf[..63].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    buf[63] = 0xA1u8.wrapping_sub(sum);
}

#[derive(Debug, Serialize, Default)]
pub struct MouseInfo {
    pub protocol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vid: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u16>,
    /// Device firmware version (frVersion in launcher terms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Numeric version code from the version response (semantics unclear).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_code: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_state: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub raw: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BondedDevice {
    pub slot: &'static str,
    pub vid: u16,
    pub pid: u16,
    pub vpid: u32,
    pub state: u8,
    pub active: bool,
}

fn work_mode_name(mode: u8) -> String {
    match mode {
        0 => "USB wired".into(),
        1 => "wireless (BT/2.4G)".into(),
        other => format!("unknown ({other})"),
    }
}

fn battery_state_name(state: u8) -> String {
    match state {
        0 => "discharging".into(),
        1 => "charging".into(),
        2 => "charged".into(),
        other => format!("unknown ({other})"),
    }
}

/// 8K version query: `[2, ...]` on reportId 181 (launcher class `k`).
/// Payload (after IN report ID 182): version code = r[2]<<8|r[1],
/// vid = r[4]<<8|r[3], pid = r[6]<<8|r[5],
/// fw version = r[8].(r[7]>>4&15).(r[7]&15), workMode = r[9] & 0x07,
/// feature flags r[11], r[12], r[13], r[15], r[16].
pub fn query_8k_version(dev: HidDevice, verbose: bool) -> Result<MouseInfo, ProtocolError> {
    let mut t = Transceiver::new(dev, REPORT_8K_VERSION, SMALL_BUF);
    t.verbose = verbose;
    let resp = t.request(&[2], 2)?;
    if resp.len() < 12 {
        return Err(ProtocolError { message: format!("short 8k version response: {} bytes", resp.len()) });
    }
    let mut info = MouseInfo {
        protocol: "8k".into(),
        vid: Some((resp[4] as u16) << 8 | resp[3] as u16),
        pid: Some((resp[6] as u16) << 8 | resp[5] as u16),
        version: Some(format!("{}.{}.{}", resp[8], (resp[7] >> 4) & 0x0F, resp[7] & 0x0F)),
        version_code: Some((resp[2] as u32) << 8 | resp[1] as u32),
        work_mode: Some(work_mode_name(resp[9] & 0x07)),
        battery_percent: None,
        battery_state: None,
        raw: Vec::new(),
    };
    if info.vid == Some(0) && info.pid == Some(0) {
        // No mouse paired / dongle idle.
        info.vid = None;
        info.pid = None;
    }
    Ok(info)
}

/// 8K receiver state: cmd 3 on reportId 181. Three fixed 5-byte slots
/// (kb1, kb2, mouse): [vidLo, vidHi, pidLo, pidHi, state]; state==1 = active.
pub fn query_8k_bond_list(dev: HidDevice, verbose: bool) -> Result<Vec<BondedDevice>, ProtocolError> {
    let mut t = Transceiver::new(dev, REPORT_8K_VERSION, SMALL_BUF);
    t.verbose = verbose;
    let resp = t.request(&[3], 3)?;
    if resp.len() < 17 {
        return Err(ProtocolError { message: format!("short bond-list response: {} bytes", resp.len()) });
    }
    let slots = [("slot1", 2usize), ("slot2", 7), ("slot3", 12)];
    let mut out = Vec::new();
    for (name, base) in slots {
        let vid = (resp[base + 1] as u16) << 8 | resp[base] as u16;
        let pid = (resp[base + 3] as u16) << 8 | resp[base + 2] as u16;
        if vid == 0 && pid == 0 {
            continue;
        }
        let state = resp[base + 4];
        out.push(BondedDevice {
            slot: name,
            vid,
            pid,
            vpid: ((vid as u32) << 16) | pid as u32,
            state,
            active: state == 1,
        });
    }
    Ok(out)
}

/// Battery readout from the 8k cmd-6 settings snapshot.
#[derive(Debug, Serialize)]
pub struct PowerInfo {
    pub protocol: &'static str,
    /// Battery charge, percent (low 7 bits of response byte 19).
    pub battery_percent: u8,
    /// Charging flag (bit 7 of response byte 19).
    pub charging: bool,
    /// "charging" / "discharging"
    pub battery_state: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub raw: Vec<String>,
}

/// DPI configuration from the 8k cmd-6 settings snapshot.
#[derive(Debug, Serialize)]
pub struct DpiInfo {
    pub protocol: &'static str,
    /// Current profile index the reported levels belong to (byte 1).
    pub profile: u8,
    /// Current DPI level per work mode: low nibbles of bytes 2..4.
    pub current_levels: [u8; 3],
    /// Number of DPI levels (byte 16).
    pub level_num: u8,
    /// DPI value of each level: five u16 LE pairs at bytes 5..14.
    pub levels: [u16; 5],
    /// Maximum settable DPI (u16 LE at bytes 40..41), when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level_val_max: Option<u16>,
    /// DPI step (byte 42); the launcher defaults it to 50 when zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level_val_step: Option<u16>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub raw: Vec<String>,
}

/// 8K settings snapshot: cmd 6 on reportId 179 with 63-byte buffers
/// (launcher class `W`, the default/"8k" branch of `Pe.get`). Unlike the
/// 20-byte report-181 commands, one response carries the whole settings
/// page. Offsets (payload after the IN report-ID byte):
/// r[1] current profile, r[50] profile count,
/// r[2..4] one byte per work mode: low nibble = current DPI level,
///   high nibble = current polling-rate level,
/// r[5..14] five DPI values (u16 LE), r[16] DPI level count,
/// r[40..41] DPI max (u16 LE), r[42] DPI step (launcher: `|| 50`),
/// r[43..48] polling-rate table, r[49] polling level count (launcher: `|| 6`),
/// r[15] sys-feature bits, r[17] debounce, r[18] sleep minutes,
/// r[19] battery: percent = 127 & r[19], charging = r[19] >> 7 & 1,
/// r[57..59] shell / middle-wheel / side-wheel colors,
/// r[26] / r[53] / r[60] feature flags (fw < 6; fw >= 6 reports them in
///   the cmd-2 version response instead).
fn query_8k_settings(dev: HidDevice, verbose: bool) -> Result<Vec<u8>, ProtocolError> {
    let mut t = Transceiver::new(dev, REPORT_8K_FUNKEYS, FUNKEYS_BUF);
    t.verbose = verbose;
    let resp = t.request(&[6], 6)?;
    if resp.len() < 43 {
        return Err(ProtocolError {
            message: format!("short 8k settings response: {} bytes", resp.len()),
        });
    }
    Ok(resp)
}

/// 8K battery readout: cmd 6 on reportId 179, battery percent = 127 & r[19],
/// charging = r[19] >> 7 & 1. The dongle forwards the query to the paired
/// wireless mouse.
#[allow(dead_code)] // wired into the `mouse info` output by the integrator
pub fn query_8k_power(dev: HidDevice, verbose: bool) -> Result<PowerInfo, ProtocolError> {
    let resp = query_8k_settings(dev, verbose)?;
    let charging = (resp[19] >> 7) & 1 == 1;
    Ok(PowerInfo {
        protocol: "8k",
        battery_percent: 127 & resp[19],
        charging,
        battery_state: if charging { "charging" } else { "discharging" },
        raw: vec![crate::transceiver::hex(&resp)],
    })
}

/// 8K DPI readout: cmd 6 on reportId 179, current level per work mode
/// (low nibbles of r[2..4]) and the per-level DPI values of the active
/// profile (five u16 LE at r[5..14], count r[16]).
#[allow(dead_code)] // wired into the `mouse info` output by the integrator
pub fn query_8k_dpi(dev: HidDevice, verbose: bool) -> Result<DpiInfo, ProtocolError> {
    let resp = query_8k_settings(dev, verbose)?;
    let max = (resp[41] as u16) << 8 | resp[40] as u16;
    Ok(DpiInfo {
        protocol: "8k",
        profile: resp[1],
        current_levels: [resp[2] & 0x0F, resp[3] & 0x0F, resp[4] & 0x0F],
        level_num: resp[16],
        levels: [
            (resp[6] as u16) << 8 | resp[5] as u16,
            (resp[8] as u16) << 8 | resp[7] as u16,
            (resp[10] as u16) << 8 | resp[9] as u16,
            (resp[12] as u16) << 8 | resp[11] as u16,
            (resp[14] as u16) << 8 | resp[13] as u16,
        ],
        level_val_max: if max == 0 { None } else { Some(max) },
        level_val_step: if resp[42] == 0 { None } else { Some(resp[42] as u16) },
        raw: vec![crate::transceiver::hex(&resp)],
    })
}

/// 4k / 8k_nordic version query: `[0, 0, 0x81, 0, workMode<<6, ...]` on
/// reportId 0 with 64-byte checksummed buffers. Response matches
/// `(r[0]==0||r[0]==64)&&r[3]==0`, version = r[9].(r[8]>>4).(r[8]&15).
pub fn query_nordic_version(dev: HidDevice, verbose: bool) -> Result<MouseInfo, ProtocolError> {
    let mut t = Transceiver::new(dev, 0, NORDIC_BUF);
    t.verbose = verbose;
    let mut payload = [0u8; NORDIC_BUF];
    payload[2] = 0x81;
    payload[4] = 1 << 6; // workMode = 1 (wireless, via receiver)
    nordic_checksum(&mut payload);
    let resp = t.request(&payload, 0x81)?;
    if resp.len() < 10 {
        return Err(ProtocolError { message: format!("short nordic version response: {} bytes", resp.len()) });
    }
    let status_ok = (resp[0] == 0 || resp[0] == 64) && resp[3] == 0;
    if !status_ok {
        return Err(ProtocolError {
            message: format!("nordic version response status mismatch: {}", crate::transceiver::hex(&resp)),
        });
    }
    Ok(MouseInfo {
        protocol: "4k/8k_nordic".into(),
        version: Some(format!("{}.{}.{}", resp[9], (resp[8] >> 4) & 0x0F, resp[8] & 0x0F)),
        work_mode: Some(work_mode_name(1)),
        raw: vec![crate::transceiver::hex(&resp)],
        ..Default::default()
    })
}

/// 1k base info over an older receiver (feature-report path, `_Y`
/// transceiver): cmd 6 on feature report 81. Battery: value = r[10],
/// state = 3 & r[11].
pub fn query_1k_feature_base_info(dev: HidDevice, verbose: bool) -> Result<MouseInfo, ProtocolError> {
    let mut t = Transceiver::new(dev, REPORT_1K, SMALL_BUF);
    t.verbose = verbose;
    let resp = t.feature_request(&[6], 6)?;
    if resp.len() < 12 {
        return Err(ProtocolError { message: format!("short 1k base-info response: {} bytes", resp.len()) });
    }
    Ok(MouseInfo {
        protocol: "1k".into(),
        vid: Some((resp[4] as u16) << 8 | resp[3] as u16),
        pid: Some((resp[6] as u16) << 8 | resp[5] as u16),
        version: Some(format!("{}.{}.{}", resp[8], (resp[7] >> 4) & 0x0F, resp[7] & 0x0F)),
        version_code: Some((resp[2] as u32) << 8 | resp[1] as u32),
        work_mode: Some(work_mode_name(resp[9])),
        battery_percent: Some(resp[10]),
        battery_state: Some(battery_state_name(3 & resp[11])),
        raw: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Fun keys (button bindings) — 8k protocol, reportId 179, 63-byte buffers.
// Launcher module 98502: class `ee` (get-all cmd 97 / get cmd 98), class `W`
// (set cmd 48, ack [0xE4, 0, 48]). Enums from module 75994.
// ---------------------------------------------------------------------------

/// Fun-key action types (launcher `EFunKey`).
pub const FK_REMOVE: u8 = 0;
pub const FK_MOUSE: u8 = 1;
pub const FK_KEYBOARD: u8 = 2;
pub const FK_MEDIA: u8 = 3;
pub const FK_MACRO: u8 = 4;
pub const FK_DPI: u8 = 5;
pub const FK_LIGHT: u8 = 6;
pub const FK_GAME: u8 = 7;
pub const FK_SHORTCUT: u8 = 8;
pub const FK_DISABLE: u8 = 9;
pub const FK_PROFILE: u8 = 10;
pub const FK_POLLING_RATE: u8 = 13;

pub fn funkey_type_name(t: u8) -> &'static str {
    match t {
        FK_REMOVE => "remove",
        FK_MOUSE => "mouse",
        FK_KEYBOARD => "keyboard",
        FK_MEDIA => "media",
        FK_MACRO => "macro",
        FK_DPI => "dpi",
        FK_LIGHT => "light",
        FK_GAME => "game",
        FK_SHORTCUT => "shortcut",
        FK_DISABLE => "disable",
        FK_PROFILE => "profile",
        FK_POLLING_RATE => "pollingRate",
        _ => "unknown",
    }
}

/// Basic mouse-button codes (launcher `EBasicKey`, 24-bit values).
pub fn basic_key_name(v: u32) -> Option<&'static str> {
    Some(match v {
        0x010000 => "left",
        0x020000 => "right",
        0x040000 => "middle",
        0x100000 => "forward",
        0x080000 => "backward",
        0x800000 => "leftDouble",
        0x000200 => "upScroll",
        0x00FE00 => "downScroll",
        0x0000FE => "leftScroll",
        0x000002 => "rightScroll",
        0x4000FF => "leftScrollSustain",
        0x400001 => "rightScrollSustain",
        _ => return None,
    })
}

/// Media key codes (launcher `EMediaKey`).
pub fn media_key_name(v: u16) -> Option<&'static str> {
    Some(match v {
        233 => "volumeUp",
        234 => "volumeDown",
        226 => "mute",
        387 => "player",
        205 => "pause",
        182 => "previous",
        181 => "next",
        183 => "stop",
        _ => return None,
    })
}

/// Loop-action codes (launcher `ELoopKey`) for DPI / polling-rate bindings.
pub fn loop_key_name(v: u8) -> Option<&'static str> {
    Some(match v {
        1 => "loop",
        2 => "next",
        3 => "prev",
        _ => return None,
    })
}

/// Modifier bit values (launcher `EModifierKey`) for keyboard bindings.
pub fn modifier_name(bit: u8) -> Option<&'static str> {
    Some(match bit {
        1 => "ctrl",
        2 => "shift",
        4 => "alt",
        8 => "win",
        16 => "ctrlR",
        32 => "shiftR",
        64 => "altR",
        128 => "winR",
        _ => return None,
    })
}

/// Shortcut codes (launcher `EShortcutKey`, 24-bit values).
pub fn shortcut_key_name(v: u32) -> Option<&'static str> {
    Some(match v {
        814848 => "brightnessUp",
        815104 => "brightnessDown",
        823809 => "calculate",
        824321 => "myComputer",
        795394 => "homePage",
        821761 => "email",
        458814 => "refresh",
        459819 => "switchApp",
        827138 => "task",
        827394 => "launchPad",
        460807 => "home",
        461334 => "screenshot",
        459014 => "copy",
        459035 => "cut",
        459033 => "paste",
        461345 => "screenshotMac",
        460806 => "copyMac",
        460827 => "cutMac",
        460825 => "pasteMac",
        459053 => "webLessen",
        459054 => "webMagnify",
        460845 => "webLessenMac",
        460846 => "webMagnifyMac",
        _ => return None,
    })
}

/// One button binding, decoded.
#[derive(Debug, Serialize)]
pub struct FunKeyBinding {
    /// Button index (from the device definition's `keys[].index`).
    pub index: u8,
    /// Action type name (see `funkey_type_name`).
    pub action: &'static str,
    /// Decoded action detail, human-oriented (e.g. "left", "volumeUp", "loop").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Raw data bytes after the type byte in the get response.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<u8>,
}

/// Get-all bound keys (cmd 97 on reportId 179). Returns the bound key indexes.
/// Response: `[97, count, entries...]`; each entry is `2 + (entry[0] >> 5)`
/// bytes long with the key index in the low 5 bits of its first byte.
fn funkey_get_all_indexes(dev: HidDevice, verbose: bool) -> Result<Vec<u8>, ProtocolError> {
    let mut t = Transceiver::new(dev, REPORT_8K_FUNKEYS, FUNKEYS_BUF);
    t.verbose = verbose;
    let resp = t.request(&[97], 97)?;
    if resp.len() < 2 {
        return Err(ProtocolError { message: "short funkey get-all response".into() });
    }
    let count = resp[1] as usize;
    let mut rest = &resp[2..];
    let mut out = Vec::new();
    for _ in 0..count {
        if rest.is_empty() {
            break;
        }
        let len = 2 + (rest[0] >> 5) as usize;
        let index = rest[0] & 0x1F;
        out.push(index);
        if rest.len() < len {
            break;
        }
        rest = &rest[len..];
    }
    Ok(out)
}

/// Get one button binding (cmd 98 on reportId 179): `[98, keyIndex]` →
/// `[98, keyIndex, ?, type, data...]`.
pub fn query_8k_funkey(dev: HidDevice, index: u8, verbose: bool) -> Result<FunKeyBinding, ProtocolError> {
    let mut t = Transceiver::new(dev, REPORT_8K_FUNKEYS, FUNKEYS_BUF);
    t.verbose = verbose;
    let resp = t.request(&[98, index], 98)?;
    if resp.len() < 4 {
        return Err(ProtocolError { message: format!("short funkey get response: {} bytes", resp.len()) });
    }
    let ftype = resp[3];
    let data = resp[4..].to_vec();
    Ok(FunKeyBinding {
        index,
        action: funkey_type_name(ftype),
        value: decode_funkey_value(ftype, &data),
        data,
    })
}

/// HID keyboard usage ID → printable name (for keyboard-binding display).
pub fn hid_key_name(code: u8) -> Option<&'static str> {
    const LETTERS: [&str; 26] = ["a","b","c","d","e","f","g","h","i","j","k","l","m","n","o","p","q","r","s","t","u","v","w","x","y","z"];
    const DIGITS: [&str; 10] = ["1","2","3","4","5","6","7","8","9","0"];
    Some(match code {
        4..=29 => LETTERS[(code - 4) as usize],
        30..=39 => DIGITS[(code - 30) as usize],
        40 => "enter", 41 => "esc", 42 => "backspace", 43 => "tab", 44 => "space",
        45 => "-", 46 => "=", 47 => "[", 48 => "]", 49 => "\\", 50 => ";", 51 => "'",
        52 => "`", 53 => ",", 54 => ".", 55 => "/", 57 => "capslock",
        58..=69 => return Some(match code {
            58 => "f1", 59 => "f2", 60 => "f3", 61 => "f4", 62 => "f5", 63 => "f6",
            64 => "f7", 65 => "f8", 66 => "f9", 67 => "f10", 68 => "f11", 69 => "f12",
            _ => unreachable!(),
        }),
        _ => return None,
    })
}

/// Decode the action detail per type (launcher `accountKey` semantics).
fn decode_funkey_value(ftype: u8, data: &[u8]) -> Option<String> {
    let u24 = |d: &[u8]| ((d[0] as u32) << 16) | ((d[1] as u32) << 8) | d[2] as u32;
    match ftype {
        FK_MOUSE if data.len() >= 3 => Some(
            basic_key_name(u24(data)).map(|s| s.to_string()).unwrap_or_else(|| format!("0x{:06X}", u24(data))),
        ),
        FK_KEYBOARD if !data.is_empty() => {
            let mut parts = Vec::new();
            for bit in [1u8, 2, 4, 8, 16, 32, 64, 128] {
                if data[0] & bit != 0 {
                    parts.push(modifier_name(bit).unwrap_or("?").to_string());
                }
            }
            for k in data.iter().skip(1) {
                if *k != 0 {
                    parts.push(hid_key_name(*k).map(|s| s.to_string()).unwrap_or_else(|| format!("KC_{k:02X}")));
                }
            }
            Some(parts.join("+"))
        }
        FK_MEDIA if data.len() >= 2 => {
            let v = (data[1] as u16) << 8 | data[0] as u16;
            Some(media_key_name(v).map(|s| s.to_string()).unwrap_or_else(|| format!("{v}")))
        }
        FK_DPI | FK_POLLING_RATE if !data.is_empty() => {
            Some(loop_key_name(data[0]).map(|s| s.to_string()).unwrap_or_else(|| format!("{}", data[0])))
        }
        FK_SHORTCUT if data.len() >= 3 => {
            let v = u24(data);
            Some(shortcut_key_name(v).map(|s| s.to_string()).unwrap_or_else(|| format!("0x{v:06X}")))
        }
        FK_PROFILE if !data.is_empty() => Some(format!("profile {}", data[0])),
        FK_LIGHT if !data.is_empty() => Some(format!("{}", data[0])),
        _ => None,
    }
}

/// Set one button binding (cmd 82 on reportId 179, launcher class `be`):
/// `[82, keyIndex, 0, type, data...]`, ack `[0xE4, 0, 82]`.
/// (Cmd 48 is the bridge-module variant used for mice behind keyboard
/// bridges; the mouse module uses 82.)
pub fn set_8k_funkey(
    dev: HidDevice,
    index: u8,
    ftype: u8,
    data: &[u8],
    verbose: bool,
) -> Result<(), ProtocolError> {
    let mut t = Transceiver::new(dev, REPORT_8K_FUNKEYS, FUNKEYS_BUF);
    t.verbose = verbose;
    let mut payload = vec![82u8, index, 0, ftype];
    payload.extend_from_slice(data);
    let resp = t.request(&payload, 0xE4)?;
    // Ack echo: [0xE4, status, cmd]; status 0 = ok.
    if resp.len() < 3 || resp[2] != 82 {
        return Err(ProtocolError { message: "funkey set ack mismatch".into() });
    }
    if resp[1] != 0 {
        return Err(ProtocolError { message: format!("funkey set rejected (status {})", resp[1]) });
    }
    Ok(())
}

/// All bindings for the current profile. Follows the launcher's `getAll`:
/// bound-key indexes from the cmd-97 get-all, plus explicit gets for
/// definition keys with index > 4 (the standard buttons 0..=4 are covered by
/// the get-all index response). `open` yields a fresh HID handle per query.
pub fn query_8k_funkeys<O>(
    open: O,
    definition_indexes: &[u8],
    verbose: bool,
) -> Result<Vec<FunKeyBinding>, ProtocolError>
where
    O: Fn() -> Result<HidDevice, ProtocolError>,
{
    let mut indexes: Vec<u8> = funkey_get_all_indexes(open()?, verbose)?;
    for i in definition_indexes.iter().filter(|i| **i > 4) {
        if !indexes.contains(i) {
            indexes.push(*i);
        }
    }
    indexes.sort_unstable();
    indexes.dedup();

    let mut out = Vec::new();
    for index in indexes {
        match query_8k_funkey(open()?, index, verbose) {
            Ok(b) => out.push(b),
            // Unbound keys may not answer; the launcher treats that as empty.
            Err(_) => continue,
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Profiles — 8k protocol. Info from the cmd-6 settings snapshot
// (current = r[1], count = r[50]); switch/create via cmd 14 on reportId 181:
/// `[14, profile, count]`, ack `[0xE4, 0, 14]`. The count parameter defines
/// how many profiles exist — switching passes the current count; creating a
/// profile = increasing the count (launcher `switchProfileLevelNum`).
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ProfileInfo {
    pub protocol: &'static str,
    /// Currently active profile index.
    pub current: u8,
    /// Number of profiles that exist.
    pub count: u8,
}

/// Profile info from the cmd-6 settings snapshot (r[1] current, r[50] count).
pub fn query_8k_profile(dev: HidDevice, verbose: bool) -> Result<ProfileInfo, ProtocolError> {
    let resp = query_8k_settings(dev, verbose)?;
    if resp.len() <= 50 {
        return Err(ProtocolError { message: "settings snapshot too short for profile info".into() });
    }
    Ok(ProfileInfo { protocol: "8k", current: resp[1], count: resp[50] })
}

/// Switch to `profile` (passing `count` = current profile count), or change
/// the profile count (`profile` = an index < count, `count` = new count).
/// Ack: `[0xE4, 0, 14]`.
pub fn set_8k_profile(dev: HidDevice, profile: u8, count: u8, verbose: bool) -> Result<(), ProtocolError> {
    let mut t = Transceiver::new(dev, REPORT_8K_VERSION, SMALL_BUF);
    t.verbose = verbose;
    let resp = t.request(&[14, profile, count], 0xE4)?;
    if resp.len() < 3 || resp[2] != 14 {
        return Err(ProtocolError { message: "profile set ack mismatch".into() });
    }
    if resp[1] != 0 {
        return Err(ProtocolError { message: format!("profile set rejected (status {})", resp[1]) });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Full settings snapshot + write commands (8k protocol).
// Snapshot: cmd 6 on reportId 179 (launcher class `W`). Writes: cmds 64 (DPI),
// 65 (polling rate), 66 (sensor/sysfeatures), 67 (debounce), 69 (scroll
// wheel), 36/35 (lighting), 10 (sleep) on reportId 181 with 20-byte buffers.
// ---------------------------------------------------------------------------

/// Global polling-rate table: level index → Hz (launcher UI DEFAULT_LEVELS).
pub const POLLING_RATE_HZ: [u32; 7] = [125, 500, 1000, 2000, 4000, 8000, 250];

pub fn polling_hz(level: u8) -> Option<u32> {
    POLLING_RATE_HZ.get(level as usize).copied()
}

/// Sensor / system feature flags from the cmd-6 snapshot (byte 15).
#[derive(Debug, Default, Serialize)]
pub struct SysFeatures {
    /// Lift-off distance (2 bits, byte 15).
    pub lod: u8,
    /// Ripple control (bit 2).
    pub wave: bool,
    /// Angle snapping / line (bit 3).
    pub line: bool,
    /// Motion sync (bit 4).
    pub motion: bool,
    /// Scroll direction: true = reversed (bit 6).
    pub scroll_dir: bool,
    /// 20K max-speed mode (bit 0 of byte 52).
    pub fps20k: bool,
    /// Angle tuning, signed (byte 55).
    pub angle: i8,
    /// LOD level (low nibble of byte 61) on devices with LOD levels.
    pub lod_level: u8,
    /// LOD level count (high nibble of byte 61).
    pub lod_count: u8,
}

/// Scroll wheel settings from the cmd-6 snapshot (bytes 27..29).
#[derive(Debug, Serialize)]
pub struct ScrollInfo {
    pub speed: u8,
    pub inertia: u8,
    pub spl: u8,
}

/// Everything the cmd-6 snapshot carries.
#[derive(Debug, Serialize)]
pub struct SettingsSnapshot {
    pub protocol: &'static str,
    /// Current profile index (r[1]).
    pub profile: u8,
    /// Profile count (r[50]).
    pub profile_count: u8,
    /// Current DPI level per work mode (low nibbles of r[2..4]).
    pub dpi_levels: [u8; 3],
    /// DPI values of the current profile (u16 LE at r[5..14]).
    pub dpi_values: [u16; 5],
    /// DPI level count (r[16]).
    pub dpi_level_num: u8,
    /// Current polling-rate level per work mode (high nibbles of r[2..4]).
    pub rate_levels: [u8; 3],
    /// Polling-rate table: level indexes (r[43..48], count r[49]).
    pub rate_table: Vec<u8>,
    /// Polling-rate level count (r[49]).
    pub rate_level_num: u8,
    /// Sensor / system features (r[15] etc.).
    pub sys: SysFeatures,
    /// Debounce, ms (r[17]); available options (r[30..40]).
    pub debounce: u8,
    pub debounce_options: Vec<u8>,
    /// Sleep timer, minutes (r[18]); 255 = never.
    pub sleep_minutes: u8,
    /// Battery percent (127 & r[19]) and charging flag (bit 7).
    pub battery_percent: u8,
    pub charging: bool,
    /// Scroll wheel (r[27..29]).
    pub scroll: ScrollInfo,
    /// Feature flags as reported by the firmware (r[26] on fw < 6).
    pub feature1: u8,
    /// Shell / middle-wheel / side-wheel color indexes (r[57..59]).
    pub color_materials: [u8; 3],
}

/// Read and fully parse the 8k settings snapshot (cmd 6, reportId 179).
pub fn query_8k_snapshot(dev: HidDevice, verbose: bool) -> Result<SettingsSnapshot, ProtocolError> {
    let r = query_8k_settings(dev, verbose)?;
    if r.len() < 62 {
        return Err(ProtocolError {
            message: format!("short 8k settings response: {} bytes", r.len()),
        });
    }
    Ok(SettingsSnapshot {
        protocol: "8k",
        profile: r[1],
        profile_count: r[50],
        dpi_levels: [r[2] & 0x0F, r[3] & 0x0F, r[4] & 0x0F],
        dpi_values: [
            (r[6] as u16) << 8 | r[5] as u16,
            (r[8] as u16) << 8 | r[7] as u16,
            (r[10] as u16) << 8 | r[9] as u16,
            (r[12] as u16) << 8 | r[11] as u16,
            (r[14] as u16) << 8 | r[13] as u16,
        ],
        dpi_level_num: r[16],
        rate_levels: [r[2] >> 4, r[3] >> 4, r[4] >> 4],
        rate_table: r[43..49].to_vec(),
        rate_level_num: if r[49] == 0 { 6 } else { r[49] },
        sys: SysFeatures {
            lod: 3 & r[15],
            wave: (r[15] >> 2) & 1 == 1,
            line: (r[15] >> 3) & 1 == 1,
            motion: (r[15] >> 4) & 1 == 1,
            scroll_dir: (r[15] >> 6) & 1 == 1,
            fps20k: 1 & r[52] == 1,
            angle: r[55] as i8,
            lod_level: 15 & r[61],
            lod_count: r[61] >> 4,
        },
        debounce: r[17],
        debounce_options: r[30..40].to_vec(),
        sleep_minutes: r[18],
        battery_percent: 127 & r[19],
        charging: (r[19] >> 7) & 1 == 1,
        scroll: ScrollInfo { speed: r[27], inertia: r[28], spl: r[29] },
        feature1: r[26],
        color_materials: [r[57], r[58], r[59]],
    })
}

/// Helper: run one 20-byte report-181 command, expecting the 0xE4 ack.
fn ack_command(
    dev: HidDevice,
    payload: &[u8],
    cmd: u8,
    verbose: bool,
) -> Result<Vec<u8>, ProtocolError> {
    let mut t = Transceiver::new(dev, REPORT_8K_VERSION, SMALL_BUF);
    t.verbose = verbose;
    let resp = t.request(payload, 0xE4)?;
    if resp.len() < 3 || resp[2] != cmd {
        return Err(ProtocolError {
            message: format!("cmd {cmd} ack mismatch: {}", crate::transceiver::hex(&resp)),
        });
    }
    if resp[1] != 0 {
        return Err(ProtocolError {
            message: format!("cmd {cmd} rejected (status {})", resp[1]),
        });
    }
    Ok(resp)
}

/// Set the DPI ladder (cmd 64, reportId 181): `[64, level, level, level,
/// 5×u16-LE values, levelNum]` (launcher class `un.set`). The level byte is
/// written three times (once per work mode).
pub fn set_8k_dpi(
    dev: HidDevice,
    level: u8,
    values: &[u16; 5],
    level_num: u8,
    verbose: bool,
) -> Result<(), ProtocolError> {
    let mut p = vec![64u8, level, level, level];
    for v in values {
        p.extend_from_slice(&v.to_le_bytes());
    }
    p.push(level_num);
    ack_command(dev, &p, 64, verbose)?;
    Ok(())
}

/// Set the polling rate (cmd 65, reportId 181): `[65, level, level,
/// table bytes…, levelNum]` (launcher class `jt.set`). The table holds level
/// indexes into `POLLING_RATE_HZ`, not Hz values.
pub fn set_8k_polling_rate(
    dev: HidDevice,
    level: u8,
    table: &[u8],
    level_num: u8,
    verbose: bool,
) -> Result<(), ProtocolError> {
    let mut p = vec![65u8, level, level];
    p.extend_from_slice(table);
    p.push(level_num);
    ack_command(dev, &p, 65, verbose)?;
    Ok(())
}

/// Sensor / system-feature write (cmd 66, reportId 181, launcher class
/// `Ce.set`): `[66, lod, wave?, line?, motion?, 0, scrollDir+1, 0,
/// fps20k+1, angleEnable?2:0, angle, lodLevel]`. The sensor bytes hold
/// "off markers" (2) rather than plain booleans on some firmwares, matching
/// the launcher's `|| 2` defaults.
pub fn set_8k_sensor(
    dev: HidDevice,
    s: &SysFeatures,
    verbose: bool,
) -> Result<(), ProtocolError> {
    let p = vec![
        66u8,
        s.lod,
        if s.wave { 1 } else { 2 },
        if s.line { 1 } else { 2 },
        if s.motion { 1 } else { 2 },
        0,
        if s.scroll_dir { 2 } else { 1 },
        0,
        if s.fps20k { 2 } else { 1 },
        if s.angle != 0 { 2 } else { 0 },
        s.angle as u8,
        s.lod_level,
    ];
    ack_command(dev, &p, 66, verbose)?;
    Ok(())
}

/// Set the debounce time (cmd 67, reportId 181, launcher class `mn.set`):
/// `[67, ms]`.
pub fn set_8k_debounce(dev: HidDevice, ms: u8, verbose: bool) -> Result<(), ProtocolError> {
    ack_command(dev, &[67, ms], 67, verbose)?;
    Ok(())
}

/// Set the scroll-wheel parameters (cmd 69, reportId 181, launcher class
/// `dr.set`): `[69, speed, inertia, spl]`.
pub fn set_8k_scroll(dev: HidDevice, speed: u8, inertia: u8, spl: u8, verbose: bool) -> Result<(), ProtocolError> {
    ack_command(dev, &[69, speed, inertia, spl], 69, verbose)?;
    Ok(())
}

/// Sleep timer (cmd 10, reportId 181, launcher class `fr`): get sends
/// `[10, 2]` and the response carries the minutes at byte 2; set sends
/// `[10, 1, minutes]` where 0/255 mean "never sleep".
pub fn get_8k_sleep(dev: HidDevice, verbose: bool) -> Result<u8, ProtocolError> {
    let mut t = Transceiver::new(dev, REPORT_8K_VERSION, SMALL_BUF);
    t.verbose = verbose;
    let resp = t.request(&[10, 2], 10)?;
    if resp.len() < 3 {
        return Err(ProtocolError { message: "short sleep response".into() });
    }
    Ok(resp[2])
}

pub fn set_8k_sleep(dev: HidDevice, minutes: u8, verbose: bool) -> Result<(), ProtocolError> {
    ack_command(dev, &[10, 1, if minutes == 0 { 255 } else { minutes }], 10, verbose)?;
    Ok(())
}

/// Lighting state (8k protocol, launcher class `En`).
#[derive(Debug, Serialize)]
pub struct LightInfo {
    pub protocol: &'static str,
    /// Effect index (device-specific; 0 is usually "off"/static).
    pub mode: u8,
    pub brightness: u8,
    pub speed: u8,
    pub rgb: [u8; 3],
}

/// Read lighting (cmd 35 on reportId 181 → response cmd 33).
pub fn query_8k_light(dev: HidDevice, verbose: bool) -> Result<LightInfo, ProtocolError> {
    let mut t = Transceiver::new(dev, REPORT_8K_VERSION, SMALL_BUF);
    t.verbose = verbose;
    let resp = t.request(&[35], 33)?;
    if resp.len() < 7 {
        return Err(ProtocolError { message: "short lighting response".into() });
    }
    Ok(LightInfo {
        protocol: "8k",
        mode: resp[1],
        brightness: resp[2],
        speed: resp[3],
        rgb: [resp[4], resp[5], resp[6]],
    })
}

/// Write lighting (cmd 36 on reportId 181): `[36, mode, brightness, speed,
/// r, g, b]`.
pub fn set_8k_light(
    dev: HidDevice,
    mode: u8,
    brightness: u8,
    speed: u8,
    rgb: [u8; 3],
    verbose: bool,
) -> Result<(), ProtocolError> {
    let mut t = Transceiver::new(dev, REPORT_8K_VERSION, SMALL_BUF);
    t.verbose = verbose;
    let payload = [36, mode, brightness, speed, rgb[0], rgb[1], rgb[2]];
    // The launcher does not wait for an ack on this write; mirror that and
    // verify by reading back.
    t.write(&payload)?;
    Ok(())
}
