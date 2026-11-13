//! HID device discovery and identification.
//!
//! Keychron devices are identified by their HID collection signature
//! `(usage_page << 16) | usage` (see docs/PROTOCOL.md §1).

use hidapi::HidApi;

pub const VID_KEYCHRON: u16 = 0x3434;

pub const USAGE_PAGE_KEYBOARD: u16 = 0xFF60;
pub const USAGE_KEYBOARD: u16 = 0x61;
pub const USAGE_PAGE_BRIDGE: u16 = 0x008C;
pub const USAGE_PAGE_MOUSE_8K: u16 = 0xFFC1;
pub const USAGE_PAGE_MOUSE_4K: u16 = 0xFF0A;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Keyboard,
    Bridge,
    Mouse8k,
    Mouse4k,
    Unknown,
}

impl Role {
    pub fn from_signature(usage_page: u16, usage: u16) -> Self {
        match (usage_page, usage) {
            (USAGE_PAGE_KEYBOARD, USAGE_KEYBOARD) => Role::Keyboard,
            (USAGE_PAGE_BRIDGE, 0x01) => Role::Bridge,
            (USAGE_PAGE_MOUSE_8K, 0x01) => Role::Mouse8k,
            (USAGE_PAGE_MOUSE_4K, 0x01) => Role::Mouse4k,
            _ => Role::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Keyboard => "keyboard",
            Role::Bridge => "bridge",
            Role::Mouse8k => "mouse-8k",
            Role::Mouse4k => "mouse-4k",
            Role::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // metadata kept for future use (list --verbose, JSON output)
pub struct InterfaceInfo {
    pub vid: u16,
    pub pid: u16,
    pub vpid: u32,
    pub usage_page: u16,
    pub usage: u16,
    pub role: Role,
    pub path: String,
    pub product_string: Option<String>,
    pub manufacturer_string: Option<String>,
    pub serial: Option<String>,
}

impl InterfaceInfo {
    pub fn vpid_hex(&self) -> String {
        format!("{:04X}:{:04X}", self.vid, self.pid)
    }
}

/// Enumerate connected Keychron HID interfaces, sorted by vpid then role.
pub fn list_interfaces(api: &HidApi) -> Vec<InterfaceInfo> {
    let mut out: Vec<InterfaceInfo> = api
        .device_list()
        .filter(|d| d.vendor_id() == VID_KEYCHRON)
        .map(|d| {
            let (page, usage) = (d.usage_page(), d.usage());
            InterfaceInfo {
                vid: d.vendor_id(),
                pid: d.product_id(),
                vpid: ((d.vendor_id() as u32) << 16) | d.product_id() as u32,
                usage_page: page,
                usage,
                role: Role::from_signature(page, usage),
                path: d.path().to_string_lossy().into_owned(),
                product_string: d.product_string().map(|s| s.to_string()),
                manufacturer_string: d.manufacturer_string().map(|s| s.to_string()),
                serial: d.serial_number().map(|s| s.to_string()),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        a.vpid
            .cmp(&b.vpid)
            .then(a.role.as_str().cmp(b.role.as_str()))
    });
    out
}

/// Parse a device selector: decimal/hex vpid, `VID:PID`, or a hidraw path.
pub fn match_selector<'a>(
    spec: &str,
    interfaces: &'a [InterfaceInfo],
) -> Option<&'a InterfaceInfo> {
    match_selector_all(spec, interfaces).into_iter().next()
}

/// All interfaces matching a selector. A single hidraw node can expose several
/// roles (e.g. the Link-KM dongle carries `bridge` and `mouse-8k` on the same
/// node), so callers pick among the matches by role priority.
pub fn match_selector_all<'a>(
    spec: &str,
    interfaces: &'a [InterfaceInfo],
) -> Vec<&'a InterfaceInfo> {
    let spec_lower = spec.to_ascii_lowercase();
    let parse_hex = |s: &str| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok();
    let _ = spec_lower;

    // Absolute or relative path (e.g. /dev/hidraw2)
    if spec.contains('/') || spec.starts_with("hidraw") {
        return interfaces
            .iter()
            .filter(|i| i.path.contains(spec) || spec.ends_with(&i.path))
            .collect();
    }

    if let Some((vid_s, pid_s)) = spec.split_once(':') {
        if let (Some(vid), Some(pid)) = (parse_hex(vid_s), parse_hex(pid_s)) {
            return interfaces
                .iter()
                .filter(|i| i.vid == vid && i.pid == pid)
                .collect();
        }
    }

    // vpid: decimal or 0x-prefixed
    let vpid = if let Some(v) = spec.strip_prefix("0x").or_else(|| spec.strip_prefix("0X")) {
        u32::from_str_radix(v, 16).ok()
    } else {
        spec.parse::<u32>().ok()
    };
    if let Some(vpid) = vpid {
        return interfaces.iter().filter(|i| i.vpid == vpid).collect();
    }

    Vec::new()
}
