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

use crate::transceiver::{ProtocolError, Transceiver};
use hidapi::HidDevice;
use serde::Serialize;

pub const REPORT_1K: u8 = 81;
pub const REPORT_8K_VERSION: u8 = 181;
#[allow(dead_code)] // fun-keys path, wired when macro/keymap support lands
pub const REPORT_8K_FUNKEYS: u8 = 179;
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
