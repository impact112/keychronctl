//! Dynamic macro commands and buffer encodings — docs/PROTOCOL.md §3.1
//! (commands 0x0C–0x10).
//!
//! # Wire encoding (V9 vs V11)
//!
//! The macro byte buffer is a concatenation of per-macro item streams, each
//! terminated by a zero byte. Item opcodes come from the launcher's macro
//! enum (`main.b77853dfb65d3e44.js`, module 55416, lines 61773–61778):
//! Terminate=0, Tap=1, Down=2, Up=3, Delay=4, CharacterStream=5,
//! DelayTerminate=124 (`'|'`).
//!
//! Two encodings exist (launcher module 81617, `KeyMacro`):
//!
//! * **V9** (QMK protocol < 11, `toMacroBufForV9`, lines 61893–61916):
//!   `Tap/Down/Up` push `[opcode, keycode]`; delay items push **nothing**
//!   (V9 cannot represent delays); text pushes the raw character bytes.
//! * **V11** (QMK protocol >= 11, `toMacroBufForV11`, lines 61866–61892):
//!   every `Tap/Down/Up` item is prefixed with a literal `0x01`
//!   (`[0x01, opcode, keycode]`); a delay is
//!   `[0x01, 0x04, ASCII digits of ms..., 0x7C]`; text is again raw
//!   character bytes with no prefix.
//!
//! In both encodings keycodes are stored as a single byte: the launcher
//! pushes the full keycode value and relies on `Uint8Array` assignment
//! truncating it to `code & 0xFF` (module 50993 `bufferGenerate`,
//! line 45272). Text characters are likewise truncated
//! (`charCodeAt(0) & 0xFF`).

use crate::transceiver::{hex, ProtocolError, Transceiver};

/// MACRO_GET_COUNT (0x0C) — response `[0x0C, count]` (docs/PROTOCOL.md §3.1).
pub const CMD_MACRO_GET_COUNT: u8 = 0x0C;
/// MACRO_GET_BUFFER_SIZE (0x0D) — size = `resp[1]<<8 | resp[2]`.
pub const CMD_MACRO_GET_BUFFER_SIZE: u8 = 0x0D;
/// MACRO_GET_BUFFER (0x0E) — request `[0x0E, offHi, offLo, size]`.
pub const CMD_MACRO_GET_BUFFER: u8 = 0x0E;
/// MACRO_SET_BUFFER (0x0F) — request `[0x0F, offHi, offLo, len, data...]`;
/// the device must echo the request byte-for-byte.
pub const CMD_MACRO_SET_BUFFER: u8 = 0x0F;
/// MACRO_RESET (0x10).
pub const CMD_MACRO_RESET: u8 = 0x10;

/// Data bytes per macro buffer request: 32-byte payload minus the 4 header
/// bytes (launcher chunks reads and writes by 28, module 50993 lines 45136
/// and 45222).
pub const CHUNK_BYTES: usize = 28;

// Item opcodes (launcher module 55416, lines 61773–61778).
/// Terminator between macros (enum `Terminate`).
pub const ITEM_TERMINATE: u8 = 0;
/// Tap item opcode (enum `Tap`).
pub const ITEM_TAP: u8 = 1;
/// Key-down item opcode (enum `Down`).
pub const ITEM_DOWN: u8 = 2;
/// Key-up item opcode (enum `Up`).
pub const ITEM_UP: u8 = 3;
/// Delay item opcode (enum `Delay`).
pub const ITEM_DELAY: u8 = 4;
/// Character-stream item opcode (enum `CharacterStream`).
pub const ITEM_CHARACTER_STREAM: u8 = 5;
/// Terminates the ASCII digits of a V11 delay (enum `DelayTerminate`, `'|'`).
pub const ITEM_DELAY_TERMINATE: u8 = 124;

/// Minimum QMK protocol version that uses the V11 macro encoding.
pub const V11_MIN_PROTOCOL: u8 = 11;

/// One parsed macro item.
///
/// Keycodes are full 16-bit QMK values; the encoders truncate them to the
/// low byte on the wire (see the [module docs](self)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroItem {
    /// Tap a keycode (press + release).
    Tap(u16),
    /// Press a keycode down (no release).
    Down(u16),
    /// Release a keycode.
    Up(u16),
    /// Wait `ms` milliseconds (not representable in V9 — dropped there).
    Delay(u32),
    /// Type the characters verbatim.
    Text(String),
}

/// Encode a set of macros with the V9 buffer encoding (QMK protocol < 11).
///
/// Mirrors `KeyMacro.toMacroBufForV9` (launcher module 81617, lines
/// 61893–61916): `Tap/Down/Up` → `[opcode, code & 0xFF]`, `Delay` → no
/// bytes (V9 has no delay support), `Text` → raw character bytes; each
/// macro ends with [`ITEM_TERMINATE`].
pub fn encode_macro_v9(macros: &[Vec<MacroItem>]) -> Vec<u8> {
    let mut buf = Vec::new();
    for items in macros {
        for item in items {
            match item {
                MacroItem::Tap(c) => buf.extend_from_slice(&[ITEM_TAP, code_byte(*c)]),
                MacroItem::Down(c) => buf.extend_from_slice(&[ITEM_DOWN, code_byte(*c)]),
                MacroItem::Up(c) => buf.extend_from_slice(&[ITEM_UP, code_byte(*c)]),
                MacroItem::Delay(_) => {} // case Delay: break — no bytes
                MacroItem::Text(s) => extend_text(&mut buf, s),
            }
        }
        buf.push(ITEM_TERMINATE);
    }
    buf
}

/// Encode a set of macros with the V11 buffer encoding (QMK protocol >= 11).
///
/// Mirrors `KeyMacro.toMacroBufForV11` (launcher module 81617, lines
/// 61866–61892): `Tap/Down/Up` → `[0x01, opcode, code & 0xFF]`,
/// `Delay(ms)` → `[0x01, 0x04, 'd','i','g','i','t','s'..., 0x7C]` with the
/// milliseconds as ASCII decimal terminated by [`ITEM_DELAY_TERMINATE`],
/// `Text` → raw character bytes; each macro ends with
/// [`ITEM_TERMINATE`].
pub fn encode_macro_v11(macros: &[Vec<MacroItem>]) -> Vec<u8> {
    let mut buf = Vec::new();
    for items in macros {
        for item in items {
            match item {
                MacroItem::Tap(c) => {
                    buf.extend_from_slice(&[0x01, ITEM_TAP, code_byte(*c)])
                }
                MacroItem::Down(c) => {
                    buf.extend_from_slice(&[0x01, ITEM_DOWN, code_byte(*c)])
                }
                MacroItem::Up(c) => buf.extend_from_slice(&[0x01, ITEM_UP, code_byte(*c)]),
                MacroItem::Delay(ms) => {
                    buf.extend_from_slice(&[0x01, ITEM_DELAY]);
                    // `${Number(ms)}` split into chars, each charCodeAt(0).
                    buf.extend(ms.to_string().bytes());
                    buf.push(ITEM_DELAY_TERMINATE);
                }
                MacroItem::Text(s) => extend_text(&mut buf, s),
            }
        }
        buf.push(ITEM_TERMINATE);
    }
    buf
}

/// Encode macros with the encoding selected by the device's QMK protocol
/// version: >= 11 → V11, else V9 (docs/PROTOCOL.md §3.1).
pub fn encode_macro(protocol_version: u8, macros: &[Vec<MacroItem>]) -> Vec<u8> {
    if protocol_version >= V11_MIN_PROTOCOL {
        encode_macro_v11(macros)
    } else {
        encode_macro_v9(macros)
    }
}

/// Keycode byte: the launcher pushes the full value into a `Uint8Array`,
/// truncating to the low byte (module 50993 `bufferGenerate`, line 45272).
fn code_byte(code: u16) -> u8 {
    (code & 0xFF) as u8
}

/// Text bytes: `charCodeAt(0)` per character, truncated to one byte by the
/// `Uint8Array` store (same launcher line as keycodes).
fn extend_text(buf: &mut Vec<u8>, s: &str) {
    buf.extend(s.chars().map(|c| (c as u32 & 0xFF) as u8));
}

/// Verify that `resp` byte-for-byte echoes the request `req`.
///
/// The launcher compares the full 32-byte report (`uint8ArrayEquals`,
/// module 50993 lines 45236 and 45261–45266); we require the response to
/// start with the meaningful request bytes (the transceiver zero-pads the
/// rest, and some devices answer with a short report).
pub fn echo_matches(req: &[u8], resp: &[u8]) -> bool {
    resp.len() >= req.len() && resp[..req.len()] == req[..]
}

/// Number of macros — MACRO_GET_COUNT (0x0C), response `[0x0C, count]`.
pub fn macro_count(t: &Transceiver) -> Result<u8, ProtocolError> {
    let resp = t.request(&[CMD_MACRO_GET_COUNT], CMD_MACRO_GET_COUNT)?;
    if resp.len() < 2 {
        return Err(ProtocolError {
            message: format!("short macro-count response: {}", hex(&resp)),
        });
    }
    Ok(resp[1])
}

/// Total macro buffer size in bytes — MACRO_GET_BUFFER_SIZE (0x0D),
/// `resp[1]<<8 | resp[2]` (launcher module 50993, `getMacroSize`,
/// lines 45182–45192).
pub fn macro_buffer_size(t: &Transceiver) -> Result<u16, ProtocolError> {
    let resp = t.request(&[CMD_MACRO_GET_BUFFER_SIZE], CMD_MACRO_GET_BUFFER_SIZE)?;
    if resp.len() < 3 {
        return Err(ProtocolError {
            message: format!("short macro-buffer-size response: {}", hex(&resp)),
        });
    }
    Ok(((resp[1] as u16) << 8) | resp[2] as u16)
}

/// Read one chunk of the macro buffer — MACRO_GET_BUFFER (0x0E):
/// request `[0x0E, offHi, offLo, size]`, data at response byte 4
/// (launcher module 50993, `readMacroChunk`, lines 45145–45163).
///
/// `size` must be <= [`CHUNK_BYTES`] (28).
pub fn macro_read(t: &Transceiver, offset: u16, size: u8) -> Result<Vec<u8>, ProtocolError> {
    if size as usize > CHUNK_BYTES {
        return Err(ProtocolError {
            message: format!("macro read size {size} exceeds chunk limit {CHUNK_BYTES}"),
        });
    }
    let req = [
        CMD_MACRO_GET_BUFFER,
        (offset >> 8) as u8,
        (offset & 0xFF) as u8,
        size,
    ];
    let resp = t.request(&req, CMD_MACRO_GET_BUFFER)?;
    let need = 4 + size as usize;
    if resp.len() < need {
        return Err(ProtocolError {
            message: format!(
                "short macro read response ({} bytes, need {need}): {}",
                resp.len(),
                hex(&resp)
            ),
        });
    }
    Ok(resp[4..need].to_vec())
}

/// Read the whole macro buffer in 28-byte chunks (launcher
/// `readMacroBuffer`, module 50993 lines 45134–45143).
pub fn macro_read_all(t: &Transceiver, total: u16) -> Result<Vec<u8>, ProtocolError> {
    let mut out = Vec::with_capacity(total as usize);
    let mut offset = 0u16;
    while offset < total {
        let size = CHUNK_BYTES.min((total - offset) as usize) as u8;
        out.extend(macro_read(t, offset, size)?);
        offset += size as u16;
    }
    Ok(out)
}

/// Write one chunk of the macro buffer — MACRO_SET_BUFFER (0x0F):
/// request `[0x0F, offHi, offLo, len, data...]`. The device must echo the
/// request byte-for-byte; the echo is verified before reporting success
/// and re-sent up to 3 times on mismatch, mirroring the launcher's
/// re-queue logic (module 50993 `setMacro`, lines 45222–45254).
///
/// `data` must be <= [`CHUNK_BYTES`] (28) bytes.
pub fn macro_write(t: &Transceiver, offset: u16, data: &[u8]) -> Result<(), ProtocolError> {
    if data.len() > CHUNK_BYTES {
        return Err(ProtocolError {
            message: format!(
                "macro write of {} bytes exceeds chunk limit {CHUNK_BYTES}",
                data.len()
            ),
        });
    }
    let mut req = vec![
        CMD_MACRO_SET_BUFFER,
        (offset >> 8) as u8,
        (offset & 0xFF) as u8,
        data.len() as u8,
    ];
    req.extend_from_slice(data);
    let mut last_resp = Vec::new();
    for _ in 0..3 {
        let resp = t.request(&req, CMD_MACRO_SET_BUFFER)?;
        if echo_matches(&req, &resp) {
            return Ok(());
        }
        // Echo mismatch: re-send (launcher re-queues the chunk).
        last_resp = resp;
    }
    Err(ProtocolError {
        message: format!(
            "macro write at offset {offset:#06x} not echoed by device; last response: {}",
            hex(&last_resp)
        ),
    })
}

/// Reset the macro buffer — MACRO_RESET (0x10).
pub fn macro_reset(t: &Transceiver) -> Result<(), ProtocolError> {
    t.request(&[CMD_MACRO_RESET], CMD_MACRO_RESET)?;
    Ok(())
}

/// Write a complete macro buffer, mirroring the launcher's `setMacro`
/// (module 50993, lines 45204–45259):
///
/// 1. reject data larger than `buffer_size`;
/// 2. `MACRO_RESET` (0x10);
/// 3. "format" the buffer to its full size by writing one byte (`0xFF`)
///    at offset `buffer_size - 1`;
/// 4. write the data in 28-byte chunks (each echo-verified);
/// 5. commit by writing `0x00` at offset `buffer_size - 1` (only when at
///    least one data chunk was written).
pub fn macro_write_all(
    t: &Transceiver,
    data: &[u8],
    buffer_size: u16,
) -> Result<(), ProtocolError> {
    if data.len() > buffer_size as usize {
        return Err(ProtocolError {
            message: format!(
                "macro data ({} bytes) exceeds buffer size {buffer_size}",
                data.len()
            ),
        });
    }
    macro_reset(t)?;
    if buffer_size == 0 {
        return Ok(());
    }
    let last = buffer_size - 1;
    let announce = [
        CMD_MACRO_SET_BUFFER,
        (last >> 8) as u8,
        (last & 0xFF) as u8,
        1,
        0xFF,
    ];
    t.request(&announce, CMD_MACRO_SET_BUFFER)?;

    let mut offset = 0usize;
    let mut wrote = false;
    while offset < data.len() {
        let end = (offset + CHUNK_BYTES).min(data.len());
        macro_write(t, offset as u16, &data[offset..end])?;
        offset = end;
        wrote = true;
    }
    if wrote {
        let commit = [
            CMD_MACRO_SET_BUFFER,
            (last >> 8) as u8,
            (last & 0xFF) as u8,
            1,
            0x00,
        ];
        t.request(&commit, CMD_MACRO_SET_BUFFER)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Text notation parser (docs/CLI.md, `macro set --text`)
// ---------------------------------------------------------------------------

/// Parse the CLI macro notation into macro items.
///
/// Notation (docs/CLI.md §macro):
///
/// * plain text types verbatim: `abc` → `Text("abc")`
/// * `{500ms}` or `{500}` → `Delay(500)`
/// * modifiers as `CTRL+c`, `CTRL+SHIFT+t` → press each modifier, tap the
///   key, release the modifiers in reverse order
/// * media as `MEDIA:play` → `Tap(KC_MPLY)`
/// * `{NAME}` taps a key by name, `{+NAME}` holds it down, `{-NAME}`
///   releases it (the launcher's own brace notation, module 81617
///   `strToMarcoItem`, lines 61920–61943)
/// * a bare ALL-CAPS known key name (`ENTER`, `F5`, `KC_A`) taps that key;
///   words containing lower-case letters are typed as text
/// * `\{` and `\}` escape literal braces
///
/// Unknown key names inside braces or combos are an error (the launcher
/// silently falls back to typing them verbatim; a CLI should be explicit).
pub fn parse_macro(text: &str) -> Result<Vec<MacroItem>, ProtocolError> {
    let chars: Vec<char> = text.chars().collect();
    let mut items = Vec::new();
    let mut pending = String::new();
    let mut i = 0usize;

    macro_rules! flush {
        () => {
            if !pending.is_empty() {
                items.push(MacroItem::Text(std::mem::take(&mut pending)));
            }
        };
    }

    while i < chars.len() {
        let c = chars[i];
        // Escaped braces.
        if c == '\\' && i + 1 < chars.len() && (chars[i + 1] == '{' || chars[i + 1] == '}') {
            pending.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '{' {
            let Some(close) = chars[i + 1..].iter().position(|&x| x == '}') else {
                return Err(ProtocolError {
                    message: "unterminated '{' in macro text".into(),
                });
            };
            let inner: String = chars[i + 1..i + 1 + close].iter().collect();
            flush!();
            items.extend(parse_braced(&inner)?);
            i += close + 2;
            continue;
        }
        // Special tokens starting at i: MEDIA:name, MOD+KEY combos, bare keys.
        if let Some((len, mut token)) = match_special(&chars[i..])? {
            flush!();
            items.append(&mut token);
            i += len;
            continue;
        }
        pending.push(c);
        i += 1;
    }
    flush!();
    Ok(items)
}

/// Parse the inside of a `{...}` group: `{N}`/`{Nms}` delay, `{+KEY}`
/// down, `{-KEY}` up, `{KEY}` tap.
fn parse_braced(inner: &str) -> Result<Vec<MacroItem>, ProtocolError> {
    let digits = inner.trim_end_matches("ms").trim_end_matches("MS");
    if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        return match digits.parse::<u32>() {
            Ok(ms) => Ok(vec![MacroItem::Delay(ms)]),
            Err(_) => Err(ProtocolError {
                message: format!("bad delay value '{{{inner}}}'"),
            }),
        };
    }
    let (dir, name) = match inner.as_bytes().first() {
        Some(b'+') => (MacroItem::Down(0), &inner[1..]),
        Some(b'-') => (MacroItem::Up(0), &inner[1..]),
        _ => (MacroItem::Tap(0), inner),
    };
    let code = keycode_by_name(name).ok_or_else(|| ProtocolError {
        message: format!("unknown key name '{{{inner}}}'"),
    })?;
    Ok(vec![match dir {
        MacroItem::Down(_) => MacroItem::Down(code),
        MacroItem::Up(_) => MacroItem::Up(code),
        _ => MacroItem::Tap(code),
    }])
}

/// Try to match a special token at the start of `s`: `MEDIA:name`,
/// `MOD+..+KEY`, or a bare ALL-CAPS key name.
/// Returns the consumed length and the produced items, or `None` when the
/// text at this position is plain.
fn match_special(s: &[char]) -> Result<Option<(usize, Vec<MacroItem>)>, ProtocolError> {
    // MEDIA:name
    if s.len() >= 6 && s[..5].iter().collect::<String>().eq_ignore_ascii_case("media")
        && s[5] == ':'
    {
        let name: String = s[6..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
            .collect();
        if name.is_empty() {
            return Err(ProtocolError {
                message: "MEDIA: requires a key name (e.g. MEDIA:play)".into(),
            });
        }
        let code = media_keycode(&name).ok_or_else(|| ProtocolError {
            message: format!("unknown media key 'MEDIA:{name}'"),
        })?;
        return Ok(Some((6 + name.chars().count(), vec![MacroItem::Tap(code)])));
    }

    // Word of [A-Za-z0-9_+]: candidate for combos and bare key names.
    let word_len = s
        .iter()
        .position(|c| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '+'))
        .unwrap_or(s.len());
    if word_len == 0 {
        return Ok(None);
    }
    let word: String = s[..word_len].iter().collect();

    if word.contains('+') {
        // MOD+MOD+KEY: every segment except the last must be a modifier.
        let parts: Vec<&str> = word.split('+').collect();
        let mods: Option<Vec<u16>> = parts[..parts.len() - 1]
            .iter()
            .map(|p| modifier_keycode(p))
            .collect();
        if let Some(mods) = mods {
            let key = parts[parts.len() - 1];
            let code = keycode_by_name(key).ok_or_else(|| ProtocolError {
                message: format!("unknown key '{key}' in combo '{word}'"),
            })?;
            let mut items: Vec<MacroItem> = mods.iter().map(|m| MacroItem::Down(*m)).collect();
            items.push(MacroItem::Tap(code));
            for m in mods.iter().rev() {
                items.push(MacroItem::Up(*m));
            }
            return Ok(Some((word_len, items)));
        }
        // Not a modifier chain (e.g. "key+value"): plain text.
        return Ok(None);
    }

    // Bare key name: must be ALL-CAPS with at least one letter so that
    // ordinary words and numbers keep typing as text.
    let has_letter = word.chars().any(|c| c.is_ascii_alphabetic());
    let all_upper = !word.chars().any(|c| c.is_ascii_lowercase());
    if has_letter && all_upper {
        if let Some(code) = keycode_by_name(&word) {
            return Ok(Some((word_len, vec![MacroItem::Tap(code)])));
        }
    }
    Ok(None)
}

/// Resolve a key name to a QMK keycode (case-insensitive; an optional
/// `KC_` prefix is accepted).
///
/// Values follow the Keychron keycode enum (launcher module 44040, lines
/// 61210–61300): `KC_A`=4 … `KC_Z`=29, `KC_1`=30 … `KC_0`=39, `KC_ENT`=40,
/// `KC_ESC`=41, `KC_BSPC`=42, `KC_TAB`=43, `KC_SPC`=44, `KC_CAPS`=57,
/// `KC_F1`=58 … `KC_F12`=69, `KC_DEL`=76, arrows 79–82, media
/// `KC_MUTE`=168 … `KC_MPLY`=174, modifiers `KC_LCTL`=224 …
/// `KC_RGUI`=231.
pub fn keycode_by_name(name: &str) -> Option<u16> {
    let upper = name.trim().to_ascii_uppercase();
    let name = upper.strip_prefix("KC_").unwrap_or(&upper);

    // Single characters: letters and digits.
    let mut chars = name.chars();
    if name.len() == 1 {
        let c = chars.next().unwrap();
        if c.is_ascii_uppercase() {
            return Some(4 + c as u16 - 'A' as u16);
        }
        if c.is_ascii_digit() {
            return Some(if c == '0' { 39 } else { 30 + c as u16 - '1' as u16 });
        }
        return None;
    }

    Some(match name {
        "ENT" | "ENTER" | "RETURN" => 40,
        "ESC" => 41,
        "BSPC" | "BACKSPACE" => 42,
        "TAB" => 43,
        "SPC" | "SPACE" => 44,
        "MINS" | "MINUS" => 45,
        "EQL" | "EQUAL" => 46,
        "LBRC" | "LBRACKET" => 47,
        "RBRC" | "RBRACKET" => 48,
        "BSLS" => 49,
        "SCLN" | "SEMICOLON" => 51,
        "QUOT" | "QUOTE" => 52,
        "GRV" | "GRAVE" => 53,
        "COMM" | "COMMA" => 54,
        "DOT" | "PERIOD" => 55,
        "SLSH" | "SLASH" => 56,
        "CAPS" | "CAPSLOCK" => 57,
        "PSCR" => 70,
        "SLCK" | "SCRLOCK" | "SCROLLLOCK" => 71,
        "PAUS" | "PAUSE" => 72,
        "INS" | "INSERT" => 73,
        "HOME" => 74,
        "PGUP" => 75,
        "DEL" | "DELETE" => 76,
        "END" => 77,
        "PGDN" => 78,
        "RGHT" | "RIGHT" => 79,
        "LEFT" => 80,
        "DOWN" => 81,
        "UP" => 82,
        "MUTE" => 168,
        "VOLU" | "VOLUP" => 169,
        "VOLD" | "VOLDN" | "VOLDOWN" => 170,
        "MNXT" | "NEXT" => 171,
        "MPRV" | "PREV" => 172,
        "MSTP" | "STOP" => 173,
        "MPLY" | "PLAY" => 174,
        // Modifiers (KC_LCTL=224 … KC_RGUI=231).
        "LCTL" | "CTRL" | "CONTROL" => 224,
        "LSFT" | "SHIFT" => 225,
        "LALT" | "ALT" => 226,
        "LGUI" | "GUI" | "CMD" | "WIN" | "META" => 227,
        "RCTL" | "RCTRL" => 228,
        "RSFT" | "RSHIFT" => 229,
        "RALT" => 230,
        "RGUI" | "RCMD" | "RWIN" => 231,
        _ => {
            // Function keys F1..F12 (58..69).
            if let Some(n) = name.strip_prefix('F') {
                if let Ok(n) = n.parse::<u16>() {
                    if (1..=12).contains(&n) {
                        return Some(57 + n);
                    }
                }
            }
            return None;
        }
    })
}

/// Modifier names for the `MOD+KEY` combo notation → left/right modifier
/// keycodes (KC_LCTL=224 … KC_RGUI=231).
fn modifier_keycode(name: &str) -> Option<u16> {
    Some(match name.trim().to_ascii_uppercase().as_str() {
        "CTRL" | "CONTROL" => 224,
        "SHIFT" => 225,
        "ALT" | "OPTION" => 226,
        "GUI" | "CMD" | "COMMAND" | "WIN" | "META" => 227,
        "RCTRL" | "RCONTROL" => 228,
        "RSHIFT" => 229,
        "RALT" | "ROPTION" => 230,
        "RGUI" | "RCMD" | "RCOMMAND" | "RWIN" => 231,
        _ => return None,
    })
}

/// Media names accepted after `MEDIA:` (docs/CLI.md: `MEDIA:play`),
/// mapped to the Keychron media keycodes (launcher module 44040:
/// KC_MUTE=168, KC_VOLU=169, KC_VOLD=170, KC_MNXT=171, KC_MPRV=172,
/// KC_MSTP=173, KC_MPLY=174). `MEDIA:pause` is the play/pause toggle
/// (KC_MPLY), not the Pause/Break key (KC_PAUS=72).
fn media_keycode(name: &str) -> Option<u16> {
    Some(match name.trim().to_ascii_uppercase().as_str() {
        "PLAY" | "PAUSE" | "PLAYPAUSE" | "TOGGLE" => 174, // KC_MPLY
        "NEXT" => 171,                                    // KC_MNXT
        "PREV" | "PREVIOUS" => 172,                       // KC_MPRV
        "STOP" => 173,                                    // KC_MSTP
        "MUTE" => 168,                                    // KC_MUTE
        "VOLUP" | "VOLUMEUP" => 169,                      // KC_VOLU
        "VOLDN" | "VOLDOWN" => 170,                       // KC_VOLD
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hand-derived from launcher `toMacroBufForV11` (module 81617, lines
    // 61866–61892) and macro enum values (module 55416, lines 61773–61778):
    // Tap pushes (1, Tap=1, keycode); Delay pushes (1, Delay=4, ASCII
    // digits, DelayTerminate=124); text pushes raw char codes; each macro
    // ends with Terminate=0.
    #[test]
    fn v11_encoding_vectors() {
        // Single tap of KC_A (4): [01 01 04] + terminator.
        let macros = vec![vec![MacroItem::Tap(4)]];
        assert_eq!(encode_macro_v11(&macros), vec![0x01, 0x01, 0x04, 0x00]);

        // Down/Up of KC_LCTL (0xE0): [01 02 E0] [01 03 E0].
        let macros = vec![vec![MacroItem::Down(0xE0), MacroItem::Up(0xE0)]];
        assert_eq!(
            encode_macro_v11(&macros),
            vec![0x01, 0x02, 0xE0, 0x01, 0x03, 0xE0, 0x00]
        );

        // Delay 500 ms: 01 04, "500" = 35 30 30, terminator 7C.
        let macros = vec![vec![MacroItem::Delay(500)]];
        assert_eq!(
            encode_macro_v11(&macros),
            vec![0x01, 0x04, 0x35, 0x30, 0x30, 0x7C, 0x00]
        );

        // Text has no prefix: "hi" = 68 69.
        let macros = vec![vec![MacroItem::Text("hi".into())]];
        assert_eq!(encode_macro_v11(&macros), vec![0x68, 0x69, 0x00]);

        // Two macros concatenate; each gets its own terminator.
        let macros = vec![vec![MacroItem::Tap(4)], vec![MacroItem::Tap(5)]];
        assert_eq!(
            encode_macro_v11(&macros),
            vec![0x01, 0x01, 0x04, 0x00, 0x01, 0x01, 0x05, 0x00]
        );

        // Mixed: CTRL down, 'c' tap, CTRL up (CLI `CTRL+c` expansion).
        let macros = vec![vec![
            MacroItem::Down(224),
            MacroItem::Tap(6),
            MacroItem::Up(224),
        ]];
        assert_eq!(
            encode_macro_v11(&macros),
            vec![0x01, 0x02, 0xE0, 0x01, 0x01, 0x06, 0x01, 0x03, 0xE0, 0x00]
        );
    }

    // Hand-derived from launcher `toMacroBufForV9` (module 81617, lines
    // 61893–61916): Tap/Down/Up push (opcode, keycode) with NO 0x01 prefix;
    // the Delay case is empty (no bytes); text is raw; terminator 0.
    #[test]
    fn v9_encoding_vectors() {
        let macros = vec![vec![MacroItem::Tap(4)]];
        assert_eq!(encode_macro_v9(&macros), vec![0x01, 0x04, 0x00]);

        let macros = vec![vec![MacroItem::Down(0xE0), MacroItem::Up(0xE0)]];
        assert_eq!(encode_macro_v9(&macros), vec![0x02, 0xE0, 0x03, 0xE0, 0x00]);

        // Delay items are silently dropped in V9 (launcher `case Delay: break`).
        let macros = vec![vec![MacroItem::Delay(500)]];
        assert_eq!(encode_macro_v9(&macros), vec![0x00]);

        let macros = vec![vec![MacroItem::Text("hi".into())]];
        assert_eq!(encode_macro_v9(&macros), vec![0x68, 0x69, 0x00]);

        let macros = vec![vec![MacroItem::Tap(4)], vec![MacroItem::Tap(5)]];
        assert_eq!(encode_macro_v9(&macros), vec![0x01, 0x04, 0x00, 0x01, 0x05, 0x00]);
    }

    // Keycodes are truncated to the low byte by the launcher's Uint8Array
    // store (module 50993 bufferGenerate line 45272: `We[ve] = Z`).
    #[test]
    fn keycode_truncated_to_low_byte() {
        let macros = vec![vec![MacroItem::Tap(0x0104)]];
        assert_eq!(encode_macro_v9(&macros), vec![0x01, 0x04, 0x00]);
        assert_eq!(encode_macro_v11(&macros), vec![0x01, 0x01, 0x04, 0x00]);
    }

    #[test]
    fn protocol_version_selects_encoding() {
        let macros = vec![vec![MacroItem::Delay(200)]];
        // Protocol >= 11 -> V11 (delay encoded), else V9 (delay dropped).
        assert_eq!(
            encode_macro(11, &macros),
            vec![0x01, 0x04, 0x32, 0x30, 0x30, 0x7C, 0x00]
        );
        assert_eq!(encode_macro(10, &macros), vec![0x00]);
        assert_eq!(encode_macro(12, &macros), encode_macro_v11(&macros));
    }

    #[test]
    fn parse_cli_notation() {
        // docs/CLI.md: `abc{500ms}d` types a, b, c, waits 500 ms, types d.
        assert_eq!(
            parse_macro("abc{500ms}d").unwrap(),
            vec![
                MacroItem::Text("abc".into()),
                MacroItem::Delay(500),
                MacroItem::Text("d".into())
            ]
        );
        // Bare `{500}` also denotes a delay.
        assert_eq!(parse_macro("{500}").unwrap(), vec![MacroItem::Delay(500)]);
        // docs/CLI.md example: `hello{200ms}world`.
        assert_eq!(
            parse_macro("hello{200ms}world").unwrap(),
            vec![
                MacroItem::Text("hello".into()),
                MacroItem::Delay(200),
                MacroItem::Text("world".into())
            ]
        );
        // Modifiers: CTRL+c -> down CTRL, tap c, up CTRL.
        assert_eq!(
            parse_macro("CTRL+c").unwrap(),
            vec![
                MacroItem::Down(224),
                MacroItem::Tap(6),
                MacroItem::Up(224)
            ]
        );
        // Stacked modifiers release in reverse order.
        assert_eq!(
            parse_macro("CTRL+SHIFT+t").unwrap(),
            vec![
                MacroItem::Down(224),
                MacroItem::Down(225),
                MacroItem::Tap(23),
                MacroItem::Up(225),
                MacroItem::Up(224)
            ]
        );
        // Media: MEDIA:play -> KC_MPLY (174).
        assert_eq!(parse_macro("MEDIA:play").unwrap(), vec![MacroItem::Tap(174)]);
        assert_eq!(parse_macro("MEDIA:next").unwrap(), vec![MacroItem::Tap(171)]);
        // Braced keys: {ENTER} tap, {+SHIFT} down, {-SHIFT} up.
        assert_eq!(parse_macro("{ENTER}").unwrap(), vec![MacroItem::Tap(40)]);
        assert_eq!(parse_macro("{+SHIFT}").unwrap(), vec![MacroItem::Down(225)]);
        assert_eq!(parse_macro("{-SHIFT}").unwrap(), vec![MacroItem::Up(225)]);
        // Bare ALL-CAPS names tap; lowercase words are text.
        assert_eq!(parse_macro("F5").unwrap(), vec![MacroItem::Tap(62)]);
        assert_eq!(parse_macro("KC_A").unwrap(), vec![MacroItem::Tap(4)]);
        assert_eq!(
            parse_macro("f5").unwrap(),
            vec![MacroItem::Text("f5".into())]
        );
        // Digits are text (so "123" types the digits), F-keys are not.
        assert_eq!(
            parse_macro("123").unwrap(),
            vec![MacroItem::Text("123".into())]
        );
        // Escaped braces are literal text.
        assert_eq!(
            parse_macro(r"\{500\}").unwrap(),
            vec![MacroItem::Text("{500}".into())]
        );
        // Unknown names in braces / combos are errors.
        assert!(parse_macro("{NOT_A_KEY}").is_err());
        assert!(parse_macro("CTRL+notakey").is_err());
        // Unterminated brace is an error.
        assert!(parse_macro("{500").is_err());
    }

    #[test]
    fn echo_verification() {
        // The 0x0F response must echo the request byte-for-byte
        // (launcher uint8ArrayEquals, module 50993 lines 45236/45261).
        let req = [0x0F, 0x00, 0x00, 0x03, 0x61, 0x62, 0x63];
        assert!(echo_matches(&req, &[0x0F, 0x00, 0x00, 0x03, 0x61, 0x62, 0x63]));
        // A full 32-byte echo (zero-padded like the outgoing report) matches.
        let mut full = req.to_vec();
        full.resize(32, 0);
        assert!(echo_matches(&req, &full));
        // Any deviation fails.
        assert!(!echo_matches(
            &req,
            &[0x0F, 0x00, 0x00, 0x03, 0x61, 0x62, 0x64]
        ));
        assert!(!echo_matches(&req, &[0x0F, 0x00, 0x00, 0x03, 0x61, 0x62]));
        // Short response fails; wrong command id fails.
        assert!(!echo_matches(&req, &[0x0F, 0x00, 0x00]));
        assert!(!echo_matches(&req, &[0x0E, 0x00, 0x00, 0x03, 0x61, 0x62, 0x63]));
    }

    #[test]
    fn chunking_limits() {
        // 28 data bytes max per 0x0E/0x0F request (32-byte payload - 4 header).
        assert_eq!(CHUNK_BYTES, 28);
        assert!(parse_macro("{500}").is_ok());
    }

    #[test]
    fn keycode_table() {
        // Values from the Keychron keycode enum (module 44040, lines 61210+).
        assert_eq!(keycode_by_name("KC_A"), Some(4));
        assert_eq!(keycode_by_name("a"), Some(4));
        assert_eq!(keycode_by_name("z"), Some(29));
        assert_eq!(keycode_by_name("KC_1"), Some(30));
        assert_eq!(keycode_by_name("KC_0"), Some(39));
        assert_eq!(keycode_by_name("ENTER"), Some(40));
        assert_eq!(keycode_by_name("KC_ENT"), Some(40));
        assert_eq!(keycode_by_name("F1"), Some(58));
        assert_eq!(keycode_by_name("F12"), Some(69));
        assert_eq!(keycode_by_name("KC_LCTL"), Some(224));
        assert_eq!(keycode_by_name("KC_RGUI"), Some(231));
        assert_eq!(keycode_by_name("KC_MPLY"), Some(174));
        assert_eq!(keycode_by_name("nope"), None);
        assert_eq!(keycode_by_name(""), None);
        // F13 is outside the built-in table.
        assert_eq!(keycode_by_name("F13"), None);
    }
}
