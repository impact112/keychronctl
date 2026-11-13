//! Dynamic keymap commands — docs/PROTOCOL.md §3.1 (0x04–0x06, 0x11–0x15) and §3.2 (0xA3).
//!
//! All functions speak QMK raw HID through a [`Transceiver`] (reportId 0,
//! 32-byte payloads, payload byte 0 = command ID, echoed by the device).
//!
//! Bulk keymap reads use `DYNAMIC_KEYMAP_GET_BUFFER` (0x12) in 14-key chunks:
//! request `[0x12, offHi, offLo, 2*n]` with `offset = 28*chunk +
//! layer*(rows*cols*2)`; the response carries `n` big-endian 16-bit keycodes
//! starting at payload byte 4 (launcher `main.b77853dfb65d3e44.js`, module
//! 50993, `getKeymapBuffer` — offset math and row-major decode at lines
//! 32927–32975).

use crate::transceiver::{hex, ProtocolError, Transceiver};
use serde::Serialize;

/// DYNAMIC_KEYMAP_GET_KEYCODE — single key read (docs/PROTOCOL.md §3.1).
pub const CMD_DYNAMIC_KEYMAP_GET_KEYCODE: u8 = 0x04;
/// DYNAMIC_KEYMAP_SET_KEYCODE — single key write (docs/PROTOCOL.md §3.1).
pub const CMD_DYNAMIC_KEYMAP_SET_KEYCODE: u8 = 0x05;
/// DYNAMIC_KEYMAP_CLEAR_ALL — reset the whole keymap (docs/PROTOCOL.md §3.1).
pub const CMD_DYNAMIC_KEYMAP_CLEAR_ALL: u8 = 0x06;
/// DYNAMIC_KEYMAP_GET_LAYER_COUNT (docs/PROTOCOL.md §3.1).
pub const CMD_DYNAMIC_KEYMAP_GET_LAYER_COUNT: u8 = 0x11;
/// DYNAMIC_KEYMAP_GET_BUFFER — bulk keymap read in 14-key chunks (§3.1).
pub const CMD_DYNAMIC_KEYMAP_GET_BUFFER: u8 = 0x12;
/// DYNAMIC_KEYMAP_GET_ENCODER — knob mapping read (§3.1).
pub const CMD_DYNAMIC_KEYMAP_GET_ENCODER: u8 = 0x14;
/// DYNAMIC_KEYMAP_SET_ENCODER — knob mapping write (§3.1).
pub const CMD_DYNAMIC_KEYMAP_SET_ENCODER: u8 = 0x15;
/// KC_GET_CURRENT_LAYER — current/active layer (docs/PROTOCOL.md §3.2).
pub const CMD_KC_GET_CURRENT_LAYER: u8 = 0xA3;

/// Keys transferred per `DYNAMIC_KEYMAP_GET_BUFFER` request
/// (28 data bytes = 14 big-endian keycodes).
pub const CHUNK_KEYS: usize = 14;

/// Encoder direction: counter-clockwise / left turn (docs/PROTOCOL.md §7.1).
pub const ENCODER_LEFT: u8 = 0;
/// Encoder direction: clockwise / right turn (docs/PROTOCOL.md §7.1).
pub const ENCODER_RIGHT: u8 = 1;

/// Current and active layer as reported by `KC_GET_CURRENT_LAYER` (0xA3).
///
/// `active` is `None` when the device answers 255 ("no active layer"),
/// which per the launcher means upload-current-layer is unsupported
/// (module 50993, `getCurrentLayer`, lines 32844–32861).
#[derive(Debug, Default, Serialize)]
pub struct CurrentLayer {
    pub current: u8,
    /// `None` when the device reports 255 (unsupported / no active layer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<u8>,
}

/// Number of 14-key chunks needed to cover `total_keys` keys.
pub fn chunk_count(total_keys: usize) -> usize {
    total_keys.div_ceil(CHUNK_KEYS)
}

/// Keys carried by chunk `chunk` (0-based) of a `total_keys`-key layer.
pub fn chunk_keys(total_keys: usize, chunk: usize) -> usize {
    CHUNK_KEYS.min(total_keys.saturating_sub(chunk * CHUNK_KEYS))
}

/// Byte offset of chunk `chunk` (0-based) within layer `layer` of a
/// `rows`×`cols` matrix: `28*chunk + layer*(rows*cols*2)`
/// (docs/PROTOCOL.md §3.1, command 0x12).
pub fn chunk_offset(chunk: usize, layer: usize, rows: usize, cols: usize) -> usize {
    chunk * 2 * CHUNK_KEYS + layer * rows * cols * 2
}

/// Decode `keys` big-endian 16-bit keycodes from a 0x12 response payload
/// (`[0x12, offHi, offLo, len, keycodes...]`, data from byte 4).
///
/// Mirrors the launcher, which slices by the *requested* count rather than
/// the echoed length byte (module 50993, line 32956: `fr.slice(4, 4 + 2*Xn)`).
pub fn decode_keycodes(resp: &[u8], keys: usize) -> Result<Vec<u16>, ProtocolError> {
    let need = 4 + keys * 2;
    if resp.len() < need {
        return Err(ProtocolError {
            message: format!(
                "short keymap chunk response ({} bytes, need {need}): {}",
                resp.len(),
                hex(resp)
            ),
        });
    }
    Ok(resp[4..4 + keys * 2]
        .chunks_exact(2)
        .map(|p| u16::from_be_bytes([p[0], p[1]]))
        .collect())
}

/// Layer count — `DYNAMIC_KEYMAP_GET_LAYER_COUNT` (0x11), response
/// `[0x11, count]` (docs/PROTOCOL.md §3.1).
pub fn layer_count(t: &Transceiver) -> Result<u8, ProtocolError> {
    let resp = t.request(
        &[CMD_DYNAMIC_KEYMAP_GET_LAYER_COUNT],
        CMD_DYNAMIC_KEYMAP_GET_LAYER_COUNT,
    )?;
    if resp.len() < 2 {
        return Err(ProtocolError {
            message: format!("short layer-count response: {}", hex(&resp)),
        });
    }
    Ok(resp[1])
}

/// Read one full layer of the keymap as row-major keycodes
/// (`rows*cols` entries; flat index `i` → row `i/cols`, col `i%cols`).
///
/// Uses `DYNAMIC_KEYMAP_GET_BUFFER` (0x12) in 14-key chunks; the response
/// offset echo is verified per chunk, matching the launcher's response
/// filter (module 50993, line 32941).
pub fn keymap_buffer_read(
    t: &Transceiver,
    layer: u8,
    rows: usize,
    cols: usize,
) -> Result<Vec<u16>, ProtocolError> {
    if rows == 0 || cols == 0 {
        return Err(ProtocolError {
            message: format!("invalid matrix size {rows}x{cols}"),
        });
    }
    let total = rows * cols;
    let mut out = Vec::with_capacity(total);
    for chunk in 0..chunk_count(total) {
        let keys = chunk_keys(total, chunk);
        let offset = chunk_offset(chunk, layer as usize, rows, cols);
        if offset > 0xFFFF {
            return Err(ProtocolError {
                message: format!(
                    "keymap offset 0x{offset:X} for layer {layer} chunk {chunk} \
                     does not fit the 16-bit protocol field"
                ),
            });
        }
        let req = [
            CMD_DYNAMIC_KEYMAP_GET_BUFFER,
            (offset >> 8) as u8,
            (offset & 0xFF) as u8,
            (keys * 2) as u8,
        ];
        let resp = t.request(&req, CMD_DYNAMIC_KEYMAP_GET_BUFFER)?;
        if resp.len() < 4 || resp[1] != req[1] || resp[2] != req[2] {
            return Err(ProtocolError {
                message: format!(
                    "keymap chunk {chunk} response offset mismatch: {}",
                    hex(&resp)
                ),
            });
        }
        out.extend(decode_keycodes(&resp, keys)?);
    }
    Ok(out)
}

/// Read a single keycode — `DYNAMIC_KEYMAP_GET_KEYCODE` (0x04).
///
/// Request `[0x04, layer, row, col]`; the response echoes the arguments and
/// appends the big-endian keycode at bytes 4–5 (symmetric with 0x05, which
/// the launcher sends as `[0x05, layer, row, col, code>>8, code&0xFF]`,
/// module 50993 line 45425).
pub fn keycode_get(t: &Transceiver, layer: u8, row: u8, col: u8) -> Result<u16, ProtocolError> {
    let req = [CMD_DYNAMIC_KEYMAP_GET_KEYCODE, layer, row, col];
    let resp = t.request(&req, CMD_DYNAMIC_KEYMAP_GET_KEYCODE)?;
    if resp.len() < 6 {
        return Err(ProtocolError {
            message: format!("short keycode response: {}", hex(&resp)),
        });
    }
    Ok(u16::from_be_bytes([resp[4], resp[5]]))
}

/// Write a single keycode — `DYNAMIC_KEYMAP_SET_KEYCODE` (0x05):
/// `[0x05, layer, row, col, code>>8, code&0xFF]` (docs/PROTOCOL.md §3.1).
pub fn keycode_set(
    t: &Transceiver,
    layer: u8,
    row: u8,
    col: u8,
    code: u16,
) -> Result<(), ProtocolError> {
    let req = [
        CMD_DYNAMIC_KEYMAP_SET_KEYCODE,
        layer,
        row,
        col,
        (code >> 8) as u8,
        (code & 0xFF) as u8,
    ];
    t.request(&req, CMD_DYNAMIC_KEYMAP_SET_KEYCODE)?;
    Ok(())
}

/// Reset the entire keymap — `DYNAMIC_KEYMAP_CLEAR_ALL` (0x06).
pub fn keymap_clear(t: &Transceiver) -> Result<(), ProtocolError> {
    t.request(
        &[CMD_DYNAMIC_KEYMAP_CLEAR_ALL],
        CMD_DYNAMIC_KEYMAP_CLEAR_ALL,
    )?;
    Ok(())
}

/// Current and active layer — `KC_GET_CURRENT_LAYER` (0xA3):
/// request `[0xA3, 0, 255]`, response `[0xA3, current, active]`
/// (docs/PROTOCOL.md §3.2). `active == 255` maps to `None`.
pub fn current_layer(t: &Transceiver) -> Result<CurrentLayer, ProtocolError> {
    let resp = t.request(
        &[CMD_KC_GET_CURRENT_LAYER, 0, 255],
        CMD_KC_GET_CURRENT_LAYER,
    )?;
    if resp.len() < 3 {
        return Err(ProtocolError {
            message: format!("short current-layer response: {}", hex(&resp)),
        });
    }
    Ok(CurrentLayer {
        current: resp[1],
        active: if resp[2] == 255 { None } else { Some(resp[2]) },
    })
}

/// Read one encoder/knob mapping — `DYNAMIC_KEYMAP_GET_ENCODER` (0x14):
/// request `[0x14, layer, knob, direction]`, response keycode at bytes 4–5
/// big-endian (launcher module 50993, `getKnobVal`, lines 45429–45452).
///
/// `direction`: [`ENCODER_LEFT`] (0) or [`ENCODER_RIGHT`] (1).
pub fn encoder_get(
    t: &Transceiver,
    layer: u8,
    knob: u8,
    direction: u8,
) -> Result<u16, ProtocolError> {
    let req = [CMD_DYNAMIC_KEYMAP_GET_ENCODER, layer, knob, direction];
    let resp = t.request(&req, CMD_DYNAMIC_KEYMAP_GET_ENCODER)?;
    if resp.len() < 6 {
        return Err(ProtocolError {
            message: format!("short encoder response: {}", hex(&resp)),
        });
    }
    Ok(u16::from_be_bytes([resp[4], resp[5]]))
}

/// Write one encoder/knob mapping — `DYNAMIC_KEYMAP_SET_ENCODER` (0x15):
/// `[0x15, layer, knob, direction, code>>8, code&0xFF]`
/// (launcher module 50993, `setKnobVal`, lines 45454–45464).
pub fn encoder_set(
    t: &Transceiver,
    layer: u8,
    knob: u8,
    direction: u8,
    code: u16,
) -> Result<(), ProtocolError> {
    let req = [
        CMD_DYNAMIC_KEYMAP_SET_ENCODER,
        layer,
        knob,
        direction,
        (code >> 8) as u8,
        (code & 0xFF) as u8,
    ];
    t.request(&req, CMD_DYNAMIC_KEYMAP_SET_ENCODER)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_math_6x16_matrix() {
        // 6 rows x 16 cols = 96 keys -> ceil(96/14) = 7 chunks.
        assert_eq!(chunk_count(96), 7);
        // Full chunks of 14 keys; the last chunk carries the remainder.
        for chunk in 0..6 {
            assert_eq!(chunk_keys(96, chunk), 14);
            assert_eq!(chunk_offset(chunk, 0, 6, 16), chunk * 28);
        }
        assert_eq!(chunk_keys(96, 6), 96 - 6 * 14);
        assert_eq!(chunk_offset(6, 0, 6, 16), 168);
        // Layer stride: each layer occupies rows*cols*2 = 192 bytes.
        assert_eq!(chunk_offset(0, 1, 6, 16), 192);
        assert_eq!(chunk_offset(0, 2, 6, 16), 384);
        // Combined: 28*3 + 2*192 = 468.
        assert_eq!(chunk_offset(3, 2, 6, 16), 468);
        // The wire request for layer 1, chunk 0 is [0x12, 0x00, 0xC0, 0x1C].
        let off = chunk_offset(0, 1, 6, 16);
        assert_eq!([(off >> 8) as u8, (off & 0xFF) as u8], [0x00, 0xC0]);
        assert_eq!((chunk_keys(96, 0) * 2) as u8, 0x1C);
    }

    #[test]
    fn chunk_math_boundaries() {
        assert_eq!(chunk_count(0), 0);
        assert_eq!(chunk_count(1), 1);
        assert_eq!(chunk_count(14), 1);
        assert_eq!(chunk_count(15), 2);
        assert_eq!(chunk_count(14), 1);
        assert_eq!(chunk_count(15), 2);
        // 5x15 = 75 keys -> 6 chunks, last one carries 5 keys.
        assert_eq!(chunk_count(75), 6);
        assert_eq!(chunk_keys(75, 5), 5);
        assert_eq!(chunk_offset(5, 0, 5, 15), 140);
        // Out-of-range chunks saturate to zero keys.
        assert_eq!(chunk_keys(96, 7), 0);
    }

    #[test]
    fn decode_keycodes_big_endian() {
        // [0x12, offHi, offLo, len, 0x04, 0x00, 0x00, 0x1C] -> [0x0400, 0x001C]
        let resp = [0x12, 0x00, 0x1C, 0x04, 0x04, 0x00, 0x00, 0x1C];
        assert_eq!(decode_keycodes(&resp, 2).unwrap(), vec![0x0400, 0x001C]);
        // Truncated response must error, not panic.
        let short = [0x12, 0x00, 0x1C, 0x04, 0x04, 0x00];
        assert!(decode_keycodes(&short, 2).is_err());
        // Zero keys -> empty.
        assert!(decode_keycodes(&[0x12, 0, 0, 0], 0).unwrap().is_empty());
    }

    #[test]
    fn decode_keycodes_matches_launcher_slice() {
        // 14 keycodes (28 bytes) as one full chunk, big-endian pairs.
        let mut resp = vec![0x12, 0x00, 0x00, 0x1C];
        for i in 0..14u16 {
            resp.extend_from_slice(&i.to_be_bytes());
        }
        let codes = decode_keycodes(&resp, 14).unwrap();
        assert_eq!(codes.len(), 14);
        assert_eq!(codes[13], 13);
    }
}
