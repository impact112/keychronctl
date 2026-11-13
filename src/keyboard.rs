//! Keyboard (QMK raw HID) commands — docs/PROTOCOL.md §3.1/§3.2.

use crate::transceiver::{ProtocolError, Transceiver};
use hidapi::HidDevice;
use serde::Serialize;

pub const REPORT_ID: u8 = 0;
pub const BUF_LEN: usize = 32;

pub const CMD_GET_PROTOCOL_VERSION: u8 = 0x01;
pub const CMD_KC_FIRMWARE_VERSION: u8 = 0xA1;
pub const CMD_KC_GET_PROTOCOL_VERSION: u8 = 0xA0;
// Not yet referenced from main.rs (the integrator wires the feature-flag
// query into the `info`/feature-gating path); silence dead-code in the bin
// target until then.
#[allow(dead_code)]
pub const CMD_KC_GET_SUPPORT_FEATURE: u8 = 0xA2;

/// Keychron support-feature flags from `KC_GET_SUPPORT_FEATURE` (0xA2) —
/// docs/PROTOCOL.md §3.2.
///
/// The raw `feature1`/`feature2` words are kept alongside the decoded bits
/// so `--json` output can be audited against a logic-analyzer capture.
#[allow(dead_code)] // wired into main.rs by the integrator
#[derive(Debug, Default, Serialize)]
pub struct FeatureFlags {
    /// `r[4]<<8 | r[3]` (V4 parse); raw byte 1 (V2 parse).
    pub feature1: u16,
    /// `r[8]<<16 | r[7]<<8 | r[6]` (V4 parse); raw byte 2 (V2 parse).
    pub feature2: u32,
    /// feature1 bit 0 — wired (LE) connection support.
    pub support_le: bool,
    /// feature1 bit 1 — 2.4 GHz receiver (RF) support.
    pub support_rf: bool,
    /// feature2 bit 0 — default-layer readback supported.
    pub get_default_layer: bool,
    /// feature2 bit 2 — per-key RGB (KC_RGB 0xA8) supported.
    pub keychron_rgb: bool,
    /// feature2 bit 6 — debounce configuration supported.
    pub debounce: bool,
    /// feature2 bit 7 — snap tap supported.
    pub snap: bool,
    /// feature2 bit 8 — sleep timers supported.
    pub sleep: bool,
    /// feature2 bit 9 — poll-rate configuration supported.
    pub poll_rate: bool,
    /// feature2 bit 12 — default-layer upload supported.
    pub upload_default_layer: bool,
    /// feature2 bit 16 — bootloader jump supported.
    pub bootloader_jump: bool,
    /// feature2 bit 17 — performance mode supported.
    pub performance_mode: bool,
    /// feature2 bit 18 — latch/toggle keys supported.
    pub latch: bool,
    /// feature2 bit 20 — macro repeat supported.
    pub macro_repeat: bool,
    /// feature1 bit 3 (V2 parse only) — mock-key support.
    pub mock_key: bool,
}

#[derive(Debug, Serialize, Default)]
pub struct KeyboardInfo {
    pub protocol_version: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keychron_protocol_version: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instruction_set: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_version: Option<String>,
}

pub fn query_info(dev: HidDevice, verbose: bool) -> Result<KeyboardInfo, ProtocolError> {
    let mut info = KeyboardInfo::default();
    let mut t = Transceiver::new(dev, REPORT_ID, BUF_LEN);
    t.verbose = verbose;

    // QMK protocol version: [0x01] -> [0x01, ?, version]
    let resp = t.request(&[CMD_GET_PROTOCOL_VERSION], CMD_GET_PROTOCOL_VERSION)?;
    if resp.len() < 3 {
        return Err(ProtocolError {
            message: format!(
                "short protocol version response: {}",
                crate::transceiver::hex(&resp)
            ),
        });
    }
    info.protocol_version = Some(resp[2]);

    // Keychron protocol version: [0xA0] -> [0xA0, kcVersion, ?, instruction_set]
    if let Ok(resp) = t.request(&[CMD_KC_GET_PROTOCOL_VERSION], CMD_KC_GET_PROTOCOL_VERSION) {
        if resp.len() >= 4 {
            info.keychron_protocol_version = Some(resp[1]);
            info.instruction_set = Some(resp[3]);
        }
    }

    // Firmware version: [0xA1] -> ASCII from byte 1
    if let Ok(resp) = t.request(&[CMD_KC_FIRMWARE_VERSION], CMD_KC_FIRMWARE_VERSION) {
        if resp.len() > 1 {
            let ascii: String = resp[1..]
                .iter()
                .take_while(|b| **b >= 0x20 && **b < 0x7F)
                .map(|b| *b as char)
                .collect();
            if !ascii.is_empty() {
                info.firmware_version = Some(ascii);
            }
        }
    }

    Ok(info)
}

/// Decode a 0xA2 response with the **V4** layout (Keychron protocol
/// version >= 4): `feature1 = r[4]<<8 | r[3]`,
/// `feature2 = r[8]<<16 | r[7]<<8 | r[6]`, bits per docs/PROTOCOL.md §3.2
/// (launcher module 50993, `handleFeatureForV4`, lines 33377–33411).
#[allow(dead_code)] // wired into main.rs by the integrator
pub fn parse_features_v4(resp: &[u8]) -> Result<FeatureFlags, ProtocolError> {
    if resp.len() < 9 {
        return Err(ProtocolError {
            message: format!(
                "short support-feature response: {}",
                crate::transceiver::hex(resp)
            ),
        });
    }
    let feature1 = ((resp[4] as u16) << 8) | resp[3] as u16;
    let feature2 = ((resp[8] as u32) << 16) | ((resp[7] as u32) << 8) | resp[6] as u32;
    Ok(FeatureFlags {
        feature1,
        feature2,
        support_le: feature1 & (1 << 0) != 0,
        support_rf: feature1 & (1 << 1) != 0,
        get_default_layer: feature2 & (1 << 0) != 0,
        keychron_rgb: feature2 & (1 << 2) != 0,
        debounce: feature2 & (1 << 6) != 0,
        snap: feature2 & (1 << 7) != 0,
        sleep: feature2 & (1 << 8) != 0,
        poll_rate: feature2 & (1 << 9) != 0,
        upload_default_layer: feature2 & (1 << 12) != 0,
        bootloader_jump: feature2 & (1 << 16) != 0,
        performance_mode: feature2 & (1 << 17) != 0,
        latch: feature2 & (1 << 18) != 0,
        macro_repeat: feature2 & (1 << 20) != 0,
        mock_key: false, // not part of the V4 layout
    })
}

/// Decode a 0xA2 response with the **V2** layout (Keychron protocol
/// version <= 3): the two flag bytes sit at instruction-set-dependent
/// offsets — instruction set 1 or 3 → `r[1], r[2]`; 2 or 4 → `r[2], r[3]`
/// (launcher module 50993, `handleFeatureForV2`, lines 33412–33434).
///
/// V2 bit layout: byte 1 bits 0–7 = get_default_layer, LE, RF, mock_key,
/// upload_default_layer, debounce, snap, keychron_rgb; byte 2 bit 0 =
/// performance_mode (only when instruction set != 2), bit 3 =
/// bootloader_jump (only for instruction sets 2 and 4).
#[allow(dead_code)] // wired into main.rs by the integrator
pub fn parse_features_v2(resp: &[u8], instruction_set: u8) -> Result<FeatureFlags, ProtocolError> {
    // Launcher indexes the full report (Y[0] = 0xA2): sets 1/3 read
    // Y[1], Y[2]; sets 2/4 read Y[2], Y[3].
    let (b1, b2) = match instruction_set {
        1 | 3 => (resp.get(1).copied(), resp.get(2).copied()),
        2 | 4 => (resp.get(2).copied(), resp.get(3).copied()),
        _ => (None, None),
    };
    let (Some(b1), Some(b2)) = (b1, b2) else {
        return Err(ProtocolError {
            message: format!(
                "short support-feature response for instruction set {instruction_set}: {}",
                crate::transceiver::hex(resp)
            ),
        });
    };
    Ok(FeatureFlags {
        feature1: b1 as u16,
        feature2: b2 as u32,
        get_default_layer: b1 & (1 << 0) != 0,
        support_le: b1 & (1 << 1) != 0,
        support_rf: b1 & (1 << 2) != 0,
        mock_key: b1 & (1 << 3) != 0,
        upload_default_layer: b1 & (1 << 4) != 0,
        debounce: b1 & (1 << 5) != 0,
        snap: b1 & (1 << 6) != 0,
        keychron_rgb: b1 & (1 << 7) != 0,
        performance_mode: b2 & (1 << 0) != 0 && instruction_set != 2,
        bootloader_jump: b2 & (1 << 3) != 0 && (instruction_set == 2 || instruction_set == 4),
        ..FeatureFlags::default()
    })
}

/// Query the support-feature bitmask — `KC_GET_SUPPORT_FEATURE` (0xA2),
/// docs/PROTOCOL.md §3.2.
///
/// The response layout depends on the Keychron protocol version reported by
/// 0xA0: version >= 4 uses the V4 parse (feature words at bytes 3–8),
/// version <= 3 the V2 parse (flag bytes at instruction-set-dependent
/// offsets). The selection mirrors the launcher's `getSupportFeat`
/// (module 50993, lines 33350–33375).
#[allow(dead_code)] // wired into main.rs by the integrator
pub fn support_features(
    t: &Transceiver,
    kc_protocol_version: u8,
    instruction_set: u8,
) -> Result<FeatureFlags, ProtocolError> {
    let resp = t.request(&[CMD_KC_GET_SUPPORT_FEATURE], CMD_KC_GET_SUPPORT_FEATURE)?;
    if kc_protocol_version <= 3 {
        parse_features_v2(&resp, instruction_set)
    } else {
        parse_features_v4(&resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // V4 response: [0xA2, ..., f1lo@3, f1hi@4, f2@6, f2@7, f2@8].
    // feature1 = r[4]<<8|r[3]; feature2 = r[8]<<16|r[7]<<8|r[6]
    // (launcher handleFeatureForV4, lines 33381-33382).
    #[test]
    fn parse_features_v4_bits() {
        // feature1 = 0x0103 (LE + RF), feature2 = 0x000105
        // (get_default_layer + keychron_rgb + sleep).
        let resp = [0xA2, 0, 0, 0x03, 0x01, 0, 0x05, 0x01, 0x00];
        let f = parse_features_v4(&resp).unwrap();
        assert_eq!(f.feature1, 0x0103);
        assert_eq!(f.feature2, 0x000105);
        assert!(f.support_le);
        assert!(f.support_rf);
        assert!(f.get_default_layer);
        assert!(f.keychron_rgb);
        assert!(f.sleep);
        assert!(!f.debounce);
        assert!(!f.snap);
        assert!(!f.poll_rate);
        assert!(!f.upload_default_layer);
        assert!(!f.bootloader_jump);
        assert!(!f.performance_mode);
        assert!(!f.latch);
        assert!(!f.macro_repeat);
    }

    #[test]
    fn parse_features_v4_word_assembly() {
        // feature2 = r[8]<<16 | r[7]<<8 | r[6] = 0x110215:
        // bits 0 (get_default_layer), 2 (keychron_rgb), 9 (poll_rate),
        // 16 (bootloader_jump), 20 (macro_repeat).
        let resp = [0xA2, 0, 0, 0x00, 0x00, 0, 0x15, 0x02, 0x11];
        let f = parse_features_v4(&resp).unwrap();
        assert_eq!(f.feature1, 0);
        assert_eq!(f.feature2, 0x110215);
        assert!(f.get_default_layer); // bit 0
        assert!(f.keychron_rgb); // bit 2
        assert!(f.poll_rate); // bit 9
        assert!(f.bootloader_jump); // bit 16
        assert!(f.macro_repeat); // bit 20
        assert!(!f.sleep); // bit 8
        assert!(!f.snap); // bit 7
        assert!(!f.debounce); // bit 6
        assert!(!f.upload_default_layer); // bit 12
        assert!(!f.performance_mode); // bit 17
        assert!(!f.latch); // bit 18
                           // Short response errors instead of panicking.
        assert!(parse_features_v4(&[0xA2, 0, 0]).is_err());
    }

    // V2 response: flag bytes at instruction-set-dependent offsets
    // (launcher handleFeatureForV2, lines 33416-33419).
    #[test]
    fn parse_features_v2_offsets_and_gates() {
        // Instruction set 2/4 read r[2], r[3].
        // 0x8F = bits 0,1,2,3,7; 0x09 = bits 0,3.
        let resp = [0xA2, 0xAA, 0x8F, 0x09, 0, 0, 0, 0, 0];
        let f = parse_features_v2(&resp, 2).unwrap();
        assert_eq!(f.feature1, 0x8F);
        assert_eq!(f.feature2, 0x09);
        assert!(f.get_default_layer); // bit 0
        assert!(f.support_le); // bit 1
        assert!(f.support_rf); // bit 2
        assert!(f.mock_key); // bit 3
        assert!(f.keychron_rgb); // bit 7
        assert!(!f.upload_default_layer); // bit 4
        assert!(!f.debounce); // bit 5
        assert!(!f.snap); // bit 6
        assert!(f.bootloader_jump); // byte2 bit 3, instruction set 2 allowed
        assert!(!f.performance_mode); // byte2 bit 0 set, but instruction set == 2
                                      // Same bytes with instruction set 4: performance_mode now allowed.
        let f = parse_features_v2(&resp, 4).unwrap();
        assert!(f.performance_mode);
        assert!(f.bootloader_jump);
        // Instruction set 1/3 read r[1], r[2] instead: 0xAA = bits 1,3,5,7.
        let f = parse_features_v2(&resp, 1).unwrap();
        assert_eq!(f.feature1, 0xAA);
        assert_eq!(f.feature2, 0x8F);
        assert!(f.support_le); // 0xAA bit 1
        assert!(f.mock_key); // 0xAA bit 3
        assert!(f.debounce); // 0xAA bit 5
        assert!(f.keychron_rgb); // 0xAA bit 7
        assert!(!f.snap); // 0xAA bit 6 clear
        assert!(!f.get_default_layer); // 0xAA bit 0 clear
        assert!(f.performance_mode); // 0x8F bit 0, instruction set != 2
        assert!(!f.bootloader_jump); // 0x8F bit 3 set, but instruction set 1
                                     // Unknown instruction set errors on missing bytes.
        assert!(parse_features_v2(&[0xA2], 5).is_err());
    }

    #[test]
    fn feature_flags_serialize() {
        let f = FeatureFlags::default();
        let json = serde_json::to_string(&f).unwrap();
        assert!(json.contains("\"support_le\":false"));
        assert!(json.contains("\"feature1\":0"));
    }
}
