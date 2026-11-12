//! Keyboard (QMK raw HID) commands — docs/PROTOCOL.md §3.1/§3.2.

use crate::transceiver::{ProtocolError, Transceiver};
use hidapi::HidDevice;
use serde::Serialize;

pub const REPORT_ID: u8 = 0;
pub const BUF_LEN: usize = 32;

pub const CMD_GET_PROTOCOL_VERSION: u8 = 0x01;
pub const CMD_KC_FIRMWARE_VERSION: u8 = 0xA1;
pub const CMD_KC_GET_PROTOCOL_VERSION: u8 = 0xA0;

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
            message: format!("short protocol version response: {}", crate::transceiver::hex(&resp)),
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
