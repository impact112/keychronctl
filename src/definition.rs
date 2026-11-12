//! Device definitions and product catalog (docs/PROTOCOL.md §7).
//!
//! Per-device JSON "definitions" from `launcher.keychron.com` drive all
//! higher-level features: keymap layout (`layouts.keys[]` geometry + `row`/`col`
//! matrix addressing), lighting capabilities (`menus` in v3, `lighting` in v2),
//! and mouse keys/DPI config. Definitions are fetched at runtime from
//! `https://launcher.keychron.com/static/device/{vpid}/json/v3.json`, falling
//! back to `v2.json`, or a versioned `v3-{N}.json` when the device reports a
//! JSON version (HID `GET_KEYBOARD_VALUE` param 4). Product metadata comes
//! from `https://launcher.keychron.com/vapi/v2/product/{vpid}`.
//!
//! Parsing is deliberately lenient (serde defaults everywhere, unknown fields
//! ignored or captured in catch-all maps): the launcher itself consumes the
//! server JSONs without ajv validation, and the shapes drift between v2/v3
//! and keyboard/mouse devices.

// Data-model structs below keep full schema fields (docs/PROTOCOL.md §7)
// even where keychronctl does not read them yet; they are consumed as
// keymap/lighting/mouse features land.
#![allow(dead_code)]

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFINITION_BASE_URL: &str = "https://launcher.keychron.com/static/device";
pub const PRODUCT_BASE_URL: &str = "https://launcher.keychron.com/vapi/v2/product";
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// Hand-rolled error type (matches the style of [`crate::transceiver::ProtocolError`]).
#[derive(Debug)]
pub enum DefinitionError {
    /// Local cache / catalog file I/O failure.
    Io(String),
    /// Network-level failure (DNS, connect, read). HTTP status errors are
    /// treated as fetch misses for the fallback chain instead.
    Http(String),
    /// A definition or catalog JSON did not match the expected shape.
    Parse(String),
    /// `offline` was requested but no cached definition exists.
    NotCached { vpid: u32, path: PathBuf },
    /// The vpid is unknown: absent from the catalog, or the product endpoint
    /// returned no data for it.
    UnknownDevice(u32),
}

impl fmt::Display for DefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DefinitionError::Io(m) => write!(f, "i/o error: {m}"),
            DefinitionError::Http(m) => write!(f, "network error: {m}"),
            DefinitionError::Parse(m) => write!(f, "invalid definition JSON: {m}"),
            DefinitionError::NotCached { vpid, path } => {
                write!(f, "no cached definition for vpid {vpid} in {}", path.display())
            }
            DefinitionError::UnknownDevice(vpid) => write!(f, "unknown device vpid {vpid}"),
        }
    }
}

impl std::error::Error for DefinitionError {}

// ---------------------------------------------------------------------------
// Definition types (keyboard)
// ---------------------------------------------------------------------------

/// Keyboard definition, VIADefinitionV3-shaped (docs/PROTOCOL.md §7.1) plus
/// Keychron extension fields. Covers both v2 and v3 server shapes.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardDefinition {
    #[serde(default)]
    pub name: String,
    /// `(VID << 16) | PID`, e.g. 875823360 = 0x3434:0100 (Q1). Present in both
    /// v2 and v3 server files as a number.
    #[serde(default)]
    pub vendor_product_id: Option<u32>,
    /// Raw v2 form (`"0x3434"` / `"0x0100"` hex strings); absent in converted
    /// v3 files which carry `vendorProductId` instead.
    #[serde(default)]
    pub vendor_id: Option<String>,
    #[serde(default)]
    pub product_id: Option<String>,
    /// Numeric in the samples (e.g. `0`); kept permissive since the launcher
    /// treats it as opaque metadata.
    #[serde(default)]
    pub firmware_version: Option<Value>,
    /// Matrix dimensions — the authoritative keymap addressing space
    /// (docs/PROTOCOL.md §7 "JSON → HID mapping").
    #[serde(default)]
    pub matrix: Option<Matrix>,
    #[serde(default)]
    pub custom_keycodes: Vec<CustomKeycode>,
    /// Keycode table names, e.g. `["qmk_lighting"]` (v3 only).
    #[serde(default)]
    pub keycodes: Vec<String>,
    /// v3 lighting/configuration menu tree (docs/PROTOCOL.md §7.1). Arbitrary
    /// JSON for now; leaf `content` maps to HID writes `[0x07, group, value, ...]`.
    #[serde(default)]
    pub menus: Value,
    #[serde(default)]
    pub layouts: Option<Layouts>,
    /// v2 lighting capabilities (replaced by `menus` in v3).
    #[serde(default)]
    pub lighting: Option<LightingSpec>,
    /// Flashing route: `"XJ", "PCA", "XJ_NEW", "WB", "NXP", "Nations", "KLD",
    /// "telink"` (docs/PROTOCOL.md §7 extension fields).
    #[serde(default)]
    pub bootloader: Option<String>,
    /// `["Base"]` default; `"HE"` → hall-effect, `"SCREEN"`, `"DIP"`, ...
    #[serde(default)]
    pub feature: Vec<String>,
    /// v2 per-layer display names, e.g. `["Mac","Mac","Win","Win"]`; the real
    /// count always comes from HID `0x11`.
    #[serde(default)]
    pub layers: Vec<String>,
}

/// Matrix dimensions (`matrix {rows, cols}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Matrix {
    #[serde(default)]
    pub rows: u8,
    #[serde(default)]
    pub cols: u8,
}

/// A Keychron custom keycode (`customKeycodes[]`): `CUSTOM(n) = _QK_KB(24448) + n`.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomKeycode {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub short_name: Option<String>,
}

/// Physical layout (`layouts {width, height, optionKeys, keys}`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layouts {
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub height: f64,
    /// VIA layout alternates `{groupKey: {optionIndex: [keys]}}`. All sampled
    /// server JSONs ship `{}` (docs/PROTOCOL.md §8), but the shape is honored.
    #[serde(default)]
    pub option_keys: BTreeMap<String, BTreeMap<String, Vec<KeyDef>>>,
    pub keys: Vec<KeyDef>,
    /// Not seen in the sampled files; kept permissive.
    #[serde(default)]
    pub labels: Value,
    #[serde(default)]
    pub presets: Value,
}

/// A key in `layouts.keys[]`. `row`/`col` are the authoritative link to HID
/// matrix addressing; the geometry fields drive rendering (docs/PROTOCOL.md
/// §7.1 per-key fields). Unknown fields land in [`KeyDef::extra`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyDef {
    #[serde(default)]
    pub row: u8,
    /// `None` for encoder/decal keys not addressable in the matrix.
    #[serde(default)]
    pub col: Option<u8>,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default = "default_one")]
    pub w: f64,
    #[serde(default = "default_one")]
    pub h: f64,
    /// Stepped key secondary geometry.
    #[serde(default)]
    pub w2: Option<f64>,
    #[serde(default)]
    pub h2: Option<f64>,
    #[serde(default)]
    pub x2: Option<f64>,
    #[serde(default)]
    pub y2: Option<f64>,
    /// Rotation (`r` angle, `rx`/`ry` pivot).
    #[serde(default)]
    pub r: f64,
    #[serde(default)]
    pub rx: f64,
    #[serde(default)]
    pub ry: f64,
    /// Decal/ghost key: rendered but not configurable.
    #[serde(default)]
    pub d: bool,
    /// `accent | alpha | mod` keycap color class.
    #[serde(default)]
    pub color: Option<String>,
    /// Extra index — knob/non-key control, excluded from key counts.
    #[serde(default)]
    pub ei: Option<u8>,
    /// Encoder/knob id.
    #[serde(default)]
    pub wheel: Option<u8>,
    #[serde(default)]
    pub label: Option<String>,
    /// Catch-all for rendering-adjacent fields (`url`, `knobVertical`,
    /// `wheelReverse`, `stick`, `dual`, `li`, `radius`, `bg`, ...).
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn default_one() -> f64 {
    1.0
}

/// v2 `lighting`: either a preset name (`"qmk_rgblight"`, ...) or the object
/// form `{extends, effects, underglowEffects, supportedLightingValues}`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum LightingSpec {
    Preset(String),
    Object(Lighting),
}

/// Object form of v2 `lighting`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lighting {
    /// Preset this config extends, e.g. `"qmk_rgblight"`.
    #[serde(default)]
    pub extends: Option<String>,
    /// Keycode table selector, e.g. `"qmk"`.
    #[serde(default)]
    pub keycodes: Option<String>,
    #[serde(default)]
    pub effects: Vec<LightingEffect>,
    #[serde(default)]
    pub underglow_effects: Vec<LightingEffect>,
    /// Not present in the sampled K6 Pro file; kept permissive.
    #[serde(default)]
    pub supported_lighting_values: Value,
}

/// An effect entry `["NAME", speed]`; permissive against other shapes.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum LightingEffect {
    Pair(String, u8),
    Other(Value),
}

impl LightingEffect {
    pub fn name(&self) -> Option<&str> {
        match self {
            LightingEffect::Pair(name, _) => Some(name),
            LightingEffect::Other(v) => v.get(0).and_then(Value::as_str),
        }
    }
}

// ---------------------------------------------------------------------------
// Definition types (mouse)
// ---------------------------------------------------------------------------

/// Mouse/trackball definition (docs/PROTOCOL.md §7 mouse JSONs; sample:
/// Keychron M6, vpid 875876452).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MouseDefinition {
    #[serde(default)]
    pub name: String,
    /// `"mouse"` in the samples.
    #[serde(rename = "type", default)]
    pub device_type: Option<String>,
    /// Wire protocol selector: `"1k" | "4k" | "8k" | "8k_nordic"`
    /// (docs/PROTOCOL.md §7 extension field `protocol`).
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub keys: Vec<MouseKey>,
    #[serde(default)]
    pub dpi: Option<Dpi>,
    /// Sensor/system switches (`{lod, disSensor}` in the M6 sample).
    #[serde(default)]
    pub sys: Option<MouseSys>,
    /// Observed as a plain bool (`false` on the M6); kept permissive in case
    /// other models carry an object with lighting capabilities.
    #[serde(default)]
    pub light: Value,
    /// Left/right-scroll sustain toggle (field name is literal `LRSustainScroll`).
    #[serde(rename = "LRSustainScroll", default)]
    pub lr_sustain_scroll: Option<bool>,
}

/// A mouse button/scroll zone (`keys[]`): `x`/`y` are percentages of the
/// device image, `index` is the fun-key button index used by the HID protocol.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MouseKey {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    /// Edge the key hugs on the rendered image: `left | right | up | down`.
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub index: u8,
    /// Stable id, e.g. `"left"`, `"middle"`, `"forward"`, `"leftTilt"`.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub side: Option<u8>,
    #[serde(rename = "type", default)]
    pub key_type: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// DPI configuration (`dpi {limit, level, reportRate, colors, maxReportRate}`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dpi {
    /// `[min, max]`, e.g. `[50, 12000]`.
    #[serde(default)]
    pub limit: Vec<u16>,
    /// Selectable DPI stages, e.g. `[400, 800, 1600, 3200, 5000]`.
    #[serde(default)]
    pub level: Vec<u16>,
    #[serde(default)]
    pub report_rate: Vec<ReportRate>,
    /// Indicator colors per DPI stage.
    #[serde(default)]
    pub colors: Vec<String>,
    #[serde(default)]
    pub max_report_rate: Option<u16>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct ReportRate {
    #[serde(default)]
    pub value: u16,
    #[serde(default)]
    pub color: Option<String>,
}

/// `sys` object (`{lod, disSensor}` in the M6 sample).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MouseSys {
    /// LOD (lift-off distance) setting; `null` in the M6 sample.
    #[serde(default)]
    pub lod: Value,
    #[serde(default)]
    pub dis_sensor: Option<bool>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ---------------------------------------------------------------------------
// Definition loading
// ---------------------------------------------------------------------------

/// A parsed device definition: keyboard or mouse shape (docs/PROTOCOL.md §7.1).
#[derive(Debug)]
pub enum Definition {
    Keyboard(KeyboardDefinition),
    Mouse(MouseDefinition),
}

impl Definition {
    pub fn name(&self) -> &str {
        match self {
            Definition::Keyboard(d) => &d.name,
            Definition::Mouse(d) => &d.name,
        }
    }

    /// Parse a definition JSON, auto-detecting keyboard vs mouse. Detection:
    /// `"type": "mouse"`, the presence of `dpi`, or `keys` without `layouts`.
    pub fn from_json(s: &str) -> Result<Definition, DefinitionError> {
        let value: Value = serde_json::from_str(s)
            .map_err(|e| DefinitionError::Parse(format!("not valid JSON: {e}")))?;
        Definition::from_value(value)
    }

    /// [`Definition::from_json`] on an already-parsed value.
    pub fn from_value(value: Value) -> Result<Definition, DefinitionError> {
        let obj = value
            .as_object()
            .ok_or_else(|| DefinitionError::Parse("definition is not a JSON object".into()))?;
        let is_mouse = obj.get("type").and_then(Value::as_str) == Some("mouse")
            || obj.contains_key("dpi")
            || (obj.contains_key("keys") && !obj.contains_key("layouts"));
        if is_mouse {
            let d: MouseDefinition = serde_json::from_value(value)
                .map_err(|e| DefinitionError::Parse(format!("mouse definition: {e}")))?;
            Ok(Definition::Mouse(d))
        } else {
            let d: KeyboardDefinition = serde_json::from_value(value)
                .map_err(|e| DefinitionError::Parse(format!("keyboard definition: {e}")))?;
            Ok(Definition::Keyboard(d))
        }
    }

    /// Load the definition for `vpid`, using the cache per `opts`
    /// (docs/PROTOCOL.md §7.1 URL selection).
    ///
    /// - `offline`: cache only; [`DefinitionError::NotCached`] if absent.
    /// - `no_cache`: always fetch (the fetched copy still refreshes the cache).
    /// - default: cache-then-fetch.
    ///
    /// Candidate files, in fallback order: `v3-{N}.json` (when the device
    /// reported JSON version N > 0), then `v3.json`, then `v2.json`. A fetch
    /// is stored under the filename it was fetched as (e.g. `v2.json` stays
    /// `v2.json` under `{cache}/device/{vpid}/`).
    pub fn load(vpid: u32, opts: &LoadOpts) -> Result<Definition, DefinitionError> {
        let cache_dir = opts.cache_dir();
        let names = candidate_filenames(opts.json_version);

        if !opts.no_cache {
            if let Some(def) = read_cached(&cache_dir, vpid, &names) {
                return Ok(def);
            }
        }
        if opts.offline {
            // Cache-only mode: a miss is fatal.
            return Err(DefinitionError::NotCached {
                vpid,
                path: cache_dir.join("device").join(vpid.to_string()),
            });
        }

        // Fetch with fallback v3.json → v2.json (HTTP status errors are
        // treated as misses so the chain can continue).
        for name in &names {
            let url = format!("{DEFINITION_BASE_URL}/{vpid}/json/{name}");
            match http_get(&url) {
                Ok(body) => {
                    let def = Definition::from_json(&body)?;
                    store_cached(&cache_dir, vpid, name, &body)?;
                    return Ok(def);
                }
                Err(DefinitionError::Http(e)) => return Err(DefinitionError::Http(e)),
                Err(_) => continue, // 404 etc. → try the next candidate
            }
        }
        Err(DefinitionError::UnknownDevice(vpid))
    }
}

/// Options for [`Definition::load`].
#[derive(Debug, Clone, Default)]
pub struct LoadOpts {
    /// Cache root; `None` means the default `dirs::cache_dir()/keychronctl`.
    pub cache_dir: Option<PathBuf>,
    /// Never touch the network; fail if the definition is not cached.
    pub offline: bool,
    /// Bypass the cache for reading (a successful fetch still refreshes it).
    pub no_cache: bool,
    /// Device-reported JSON version (HID `GET_KEYBOARD_VALUE` param 4):
    /// `Some(n)` with n > 0 selects `v3-{n}.json` first (docs/PROTOCOL.md §7.1).
    pub json_version: Option<u32>,
}

impl LoadOpts {
    /// Resolved cache root (`{cache}/device/{vpid}/` lives below this).
    pub fn cache_dir(&self) -> PathBuf {
        match &self.cache_dir {
            Some(dir) => dir.clone(),
            None => match dirs::cache_dir() {
                Some(base) => base.join("keychronctl"),
                None => PathBuf::from(".keychronctl-cache"),
            },
        }
    }
}

/// Candidate definition filenames in fallback order (docs/PROTOCOL.md §7.1).
fn candidate_filenames(json_version: Option<u32>) -> Vec<String> {
    match json_version {
        Some(n) if n > 0 => vec![format!("v3-{n}.json"), "v3.json".into(), "v2.json".into()],
        _ => vec!["v3.json".into(), "v2.json".into()],
    }
}

/// Try each candidate filename under `{cache}/device/{vpid}/`.
fn read_cached(cache_dir: &Path, vpid: u32, names: &[String]) -> Option<Definition> {
    for name in names {
        let path = cache_dir.join("device").join(vpid.to_string()).join(name);
        if let Ok(body) = std::fs::read_to_string(&path) {
            if let Ok(def) = Definition::from_json(&body) {
                return Some(def);
            }
        }
    }
    None
}

/// Store the fetched definition under the filename it was fetched as.
fn store_cached(cache_dir: &Path, vpid: u32, name: &str, body: &str) -> Result<(), DefinitionError> {
    let dir = cache_dir.join("device").join(vpid.to_string());
    std::fs::create_dir_all(&dir)
        .map_err(|e| DefinitionError::Io(format!("create {}: {e}", dir.display())))?;
    let path = dir.join(name);
    std::fs::write(&path, body)
        .map_err(|e| DefinitionError::Io(format!("write {}: {e}", path.display())))
}

/// GET a URL. HTTP status errors surface as [`DefinitionError::Parse`]-free
/// "miss" errors so callers can fall back; transport errors as `Http`.
fn http_get(url: &str) -> Result<String, DefinitionError> {
    let resp = ureq::get(url)
        .timeout(HTTP_TIMEOUT)
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(code, _) => {
                DefinitionError::Parse(format!("HTTP {code} for {url}"))
            }
            ureq::Error::Transport(t) => DefinitionError::Http(format!("{url}: {t}")),
        })?;
    resp.into_string()
        .map_err(|e| DefinitionError::Http(format!("read {url}: {e}")))
}

// ---------------------------------------------------------------------------
// Product info (vapi) & offline catalog
// ---------------------------------------------------------------------------

/// Product metadata from `https://launcher.keychron.com/vapi/v2/product/{vpid}`
/// (docs/PROTOCOL.md §7.2), or from the offline catalog via
/// [`ProductInfo::from_catalog_file`].
#[derive(Debug, Clone, Default)]
pub struct ProductInfo {
    pub vpid: u32,
    pub name: String,
    pub category: Option<String>,
    /// Manual/description URL (or empty).
    pub desc: Option<String>,
    /// Latest firmware version (`firmware.lasted.version`), when published.
    pub fw_version: Option<String>,
    /// Latest firmware binary URL (`firmware.lasted.path`).
    pub fw_path: Option<String>,
    /// 1 if the device supports firmware update via the launcher.
    pub support_update: Option<u8>,
    /// Definition JSON version reported by the API (`json[0].version` /
    /// catalog `json_version`); 0 means plain v3/v2 selection.
    pub json_version: Option<u32>,
}

impl ProductInfo {
    /// Fetch from the live vapi endpoint (requires network).
    pub fn fetch(vpid: u32) -> Result<ProductInfo, DefinitionError> {
        let url = format!("{PRODUCT_BASE_URL}/{vpid}");
        let body = http_get(&url)?;
        let wrapper: ProductResponse = serde_json::from_str(&body)
            .map_err(|e| DefinitionError::Parse(format!("product response: {e}")))?;
        match wrapper.code {
            Some(200) | None => {}
            // Unknown vpids surface as a non-200 API code (not an HTTP error).
            Some(_) => return Err(DefinitionError::UnknownDevice(vpid)),
        }
        let data = wrapper
            .data
            .ok_or_else(|| DefinitionError::UnknownDevice(vpid))?;
        let product = data
            .product
            .ok_or_else(|| DefinitionError::UnknownDevice(vpid))?;
        let lasted = data.firmware.and_then(|f| f.lasted);
        Ok(ProductInfo {
            vpid,
            name: product.name.unwrap_or_default(),
            category: data.category.and_then(|c| c.name),
            desc: product.desc,
            fw_version: lasted.as_ref().and_then(|l| l.version.clone()),
            fw_path: lasted.as_ref().and_then(|l| l.path.clone()),
            support_update: product.support_update,
            json_version: data.json.first().and_then(|j| j.version),
        })
    }

    /// Look up `vpid` in a local catalog JSON — an array of
    /// `{vpid, name, category, fw_version, ...}` records (sample:
    /// `launcher-sources/vapi/device-catalog.json`, 988 entries) — for offline
    /// lookups.
    pub fn from_catalog_file(path: &Path, vpid: u32) -> Result<ProductInfo, DefinitionError> {
        let body = std::fs::read_to_string(path)
            .map_err(|e| DefinitionError::Io(format!("read {}: {e}", path.display())))?;
        let entries: Vec<CatalogEntry> = serde_json::from_str(&body)
            .map_err(|e| DefinitionError::Parse(format!("catalog: {e}")))?;
        entries
            .into_iter()
            .find(|e| e.vpid == vpid)
            .map(ProductInfo::from)
            .ok_or_else(|| DefinitionError::UnknownDevice(vpid))
    }
}

/// One record of the device catalog (docs/PROTOCOL.md §7.2).
#[derive(Debug, Deserialize)]
pub struct CatalogEntry {
    pub vpid: u32,
    pub name: String,
    #[serde(default)]
    pub vid: Option<String>,
    #[serde(default)]
    pub pid: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub support_update: Option<u8>,
    #[serde(default)]
    pub support_bt_update: Option<u8>,
    /// `null` for ~135 of 988 catalog entries (no published firmware).
    #[serde(default)]
    pub fw_version: Option<String>,
    #[serde(default)]
    pub fw_path: Option<String>,
    #[serde(default)]
    pub json_version: Option<u32>,
}

impl From<CatalogEntry> for ProductInfo {
    fn from(e: CatalogEntry) -> ProductInfo {
        ProductInfo {
            vpid: e.vpid,
            name: e.name,
            category: e.category,
            desc: e.desc,
            fw_version: e.fw_version,
            fw_path: e.fw_path,
            support_update: e.support_update,
            json_version: e.json_version,
        }
    }
}

// vapi/v2/product/{vpid} response envelope (docs/PROTOCOL.md §7.2).
#[derive(Debug, Deserialize)]
struct ProductResponse {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    data: Option<ProductData>,
}

#[derive(Debug, Deserialize)]
struct ProductData {
    #[serde(default)]
    product: Option<ProductRecord>,
    #[serde(default)]
    category: Option<CategoryRecord>,
    #[serde(default)]
    firmware: Option<FirmwareRecord>,
    #[serde(default)]
    json: Vec<JsonVersionRecord>,
}

#[derive(Debug, Deserialize)]
struct ProductRecord {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    support_update: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct CategoryRecord {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FirmwareRecord {
    #[serde(default)]
    lasted: Option<LastedFirmware>,
}

#[derive(Debug, Deserialize)]
struct LastedFirmware {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsonVersionRecord {
    #[serde(default)]
    version: Option<u32>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(rel: &str) -> String {
        let path = format!("{}/launcher-sources/{rel}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }

    /// Keychron Q1 vpid 875823360 (0x3434:0100), v3 shape.
    const Q1_VPID: u32 = 875823360;
    /// Keychron K6 Pro vpid 875823714 (0x3434:0222), v2 shape.
    const K6_PRO_VPID: u32 = 875823714;
    /// Keychron M6 mouse vpid 875876452.
    const M6_VPID: u32 = 875876452;

    #[test]
    fn parse_keyboard_v3_q1() {
        let def = Definition::from_json(&sample("device/875823360/json/v3.json")).unwrap();
        let Definition::Keyboard(kb) = &def else {
            panic!("Q1 should parse as a keyboard, got {def:?}");
        };
        assert_eq!(kb.name, "Keychron Q1V1 ANSI");
        assert_eq!(kb.vendor_product_id, Some(Q1_VPID));
        assert_eq!(kb.matrix, Some(Matrix { rows: 6, cols: 15 }));
        assert_eq!(kb.custom_keycodes.len(), 11);
        assert_eq!(
            kb.custom_keycodes[0],
            parse::<CustomKeycode>(r#"{"name":"Mission Control","title":"Mission Control in macOS","shortName":"MCtrl"}"#)
        );
        assert_eq!(kb.keycodes, vec!["qmk_lighting"]);
        // v3 lighting lives in `menus`, not `lighting`.
        assert!(kb.menus.is_array());
        assert_eq!(kb.menus.as_array().map(Vec::len), Some(1));
        assert!(kb.lighting.is_none());

        let layouts = kb.layouts.as_ref().unwrap();
        assert_eq!(layouts.width, 16.25);
        assert_eq!(layouts.height, 6.5);
        assert!(layouts.option_keys.is_empty());
        assert_eq!(layouts.keys.len(), 82);
        let k0 = &layouts.keys[0];
        assert_eq!((k0.row, k0.col, k0.x, k0.y, k0.w, k0.h), (0, Some(0), 0.0, 0.0, 1.0, 1.0));
        assert_eq!(k0.color.as_deref(), Some("accent"));
        assert!(!k0.d);
        // Q1 keys carry r/rx/ry explicitly; no w2/h2/x2/y2, url, ei or wheel.
        assert!(k0.w2.is_none() && k0.h2.is_none());
        assert!(k0.extra.is_empty());
        // v3 extension fields absent in this file.
        assert!(kb.bootloader.is_none());
        assert!(kb.feature.is_empty());
        assert!(kb.layers.is_empty());
    }

    #[test]
    fn parse_keyboard_v2_k6_pro() {
        let def = Definition::from_json(&sample("device/875823714/json/v2.json")).unwrap();
        let Definition::Keyboard(kb) = &def else {
            panic!("K6 Pro should parse as a keyboard, got {def:?}");
        };
        assert_eq!(kb.name, "Keychron K6 Pro");
        assert_eq!(kb.vendor_product_id, Some(K6_PRO_VPID));
        assert_eq!(kb.matrix, Some(Matrix { rows: 5, cols: 15 }));
        assert_eq!(kb.layers, vec!["Mac", "Mac", "Win", "Win"]);
        assert_eq!(kb.custom_keycodes.len(), 15);
        // v2 has no `menus`/`keycodes`.
        assert!(kb.menus.is_null());
        assert!(kb.keycodes.is_empty());

        let lighting = match &kb.lighting {
            Some(LightingSpec::Object(l)) => l,
            other => panic!("K6 Pro lighting should be the object form, got {other:?}"),
        };
        assert_eq!(lighting.extends.as_deref(), Some("qmk_rgblight"));
        assert_eq!(lighting.keycodes.as_deref(), Some("qmk"));
        assert_eq!(lighting.underglow_effects.len(), 23);
        assert_eq!(lighting.underglow_effects[0].name(), Some("None"));
        assert_eq!(lighting.underglow_effects[1].name(), Some("SOLID_COLOR"));
        assert!(lighting.effects.is_empty());

        let layouts = kb.layouts.as_ref().unwrap();
        assert_eq!(layouts.width, 16.0);
        assert_eq!(layouts.height, 5.0);
        assert_eq!(layouts.keys.len(), 71);
        // `url` is a rendering-only field → lands in the catch-all.
        let k0 = &layouts.keys[0];
        assert_eq!(k0.extra.get("url").and_then(Value::as_str), Some("/assets/keyboard/keycap.png"));
    }

    #[test]
    fn parse_mouse_v3_m6() {
        let raw = sample("device/875876452/json/v3.json");
        // Detection via the mouse shape (type/dpi, no layouts).
        let def = Definition::from_json(&raw).unwrap();
        let Definition::Mouse(m) = &def else {
            panic!("M6 should parse as a mouse, got {def:?}");
        };
        assert_eq!(m.name, "Keychron M6");
        assert_eq!(m.device_type.as_deref(), Some("mouse"));
        assert_eq!(m.keys.len(), 11);
        let middle = m.keys.iter().find(|k| k.id.as_deref() == Some("middle")).unwrap();
        assert_eq!(middle.index, 1);
        assert_eq!(middle.side, Some(1));
        assert_eq!(middle.dir.as_deref(), Some("right"));
        assert!(m.protocol.is_none()); // M6 sample has no `protocol` field

        let dpi = m.dpi.as_ref().unwrap();
        assert_eq!(dpi.limit, vec![50, 12000]);
        assert_eq!(dpi.level, vec![400, 800, 1600, 3200, 5000]);
        assert_eq!(dpi.report_rate.len(), 3);
        assert_eq!(dpi.report_rate[0].value, 125);
        assert_eq!(dpi.report_rate[0].color.as_deref(), Some("#fff"));
        assert_eq!(dpi.colors.len(), 6);
        assert_eq!(dpi.max_report_rate, Some(1000));

        assert_eq!(m.light, Value::Bool(false));
        assert_eq!(m.lr_sustain_scroll, Some(true));
        let sys = m.sys.as_ref().unwrap();
        assert!(sys.lod.is_null());
        assert_eq!(sys.dis_sensor, Some(true));

        // Also parses via the direct serde path.
        let m2: MouseDefinition = serde_json::from_str(&raw).unwrap();
        assert_eq!(m2.keys.len(), 11);
    }

    #[test]
    fn catalog_slice() {
        let path = format!("{}/launcher-sources/vapi/device-catalog.json", env!("CARGO_MANIFEST_DIR"));
        let body = std::fs::read_to_string(&path).unwrap();
        let entries: Vec<CatalogEntry> = serde_json::from_str(&body).unwrap();
        assert_eq!(entries.len(), 988);

        // Q1: fw_version is null in the catalog (no published firmware).
        let q1 = ProductInfo::from_catalog_file(Path::new(&path), Q1_VPID).unwrap();
        assert_eq!(q1.vpid, Q1_VPID);
        assert_eq!(q1.name, "Q1");
        assert_eq!(q1.category.as_deref(), Some("Q系列"));
        assert_eq!(q1.fw_version, None);
        assert_eq!(q1.support_update, Some(1));

        // Q10 (875823265): has published firmware 1.0.3.
        let q10 = ProductInfo::from_catalog_file(Path::new(&path), 875823265).unwrap();
        assert_eq!(q10.name, "Q10");
        assert_eq!(q10.fw_version.as_deref(), Some("1.0.3"));
        assert!(q10.fw_path.as_deref().unwrap().starts_with("https://cdn.shopify.com/"));

        // Unknown vpid → error.
        let err = ProductInfo::from_catalog_file(Path::new(&path), 0xDEAD).unwrap_err();
        assert!(matches!(err, DefinitionError::UnknownDevice(0xDEAD)));
    }

    #[test]
    fn load_offline_from_cache() {
        let base = std::path::Path::new("/tmp/opencode")
            .join(format!("keychronctl-def-test-{}", std::process::id()));
        let dir = base.join("device").join(M6_VPID.to_string());
        std::fs::create_dir_all(&dir).unwrap();
        // Store the M6 mouse as v3.json: offline load must find and parse it.
        std::fs::write(dir.join("v3.json"), sample("device/875876452/json/v3.json")).unwrap();

        let opts = LoadOpts { cache_dir: Some(base.clone()), offline: true, ..Default::default() };
        let def = Definition::load(M6_VPID, &opts).unwrap();
        assert_eq!(def.name(), "Keychron M6");
        assert!(matches!(def, Definition::Mouse(_)));

        // Offline miss → NotCached pointing at the per-device cache dir.
        let err = Definition::load(0xDEAD, &opts).unwrap_err();
        assert!(matches!(err, DefinitionError::NotCached { vpid: 0xDEAD, .. }));

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn candidate_filename_order() {
        assert_eq!(candidate_filenames(None), vec!["v3.json", "v2.json"]);
        assert_eq!(candidate_filenames(Some(0)), vec!["v3.json", "v2.json"]);
        assert_eq!(
            candidate_filenames(Some(1)),
            vec!["v3-1.json", "v3.json", "v2.json"]
        );
    }

    fn parse<'a, T: Deserialize<'a>>(s: &'a str) -> T {
        serde_json::from_str(s).unwrap()
    }

    /// Live network test: fetch the Q1 definition and product info once to
    /// confirm the endpoints and shapes. Run manually:
    /// `cargo test -- --ignored`.
    #[test]
    #[ignore = "requires network access"]
    fn live_fetch_definition_and_product() {
        let def = Definition::load(Q1_VPID, &LoadOpts { no_cache: true, ..Default::default() }).unwrap();
        assert_eq!(def.name(), "Keychron Q1V1 ANSI");

        let info = ProductInfo::fetch(Q1_VPID).unwrap();
        assert_eq!(info.vpid, Q1_VPID);
        assert_eq!(info.name, "Q1");
        println!("live product info: {info:?}");

        // M6 mouse definition via the live endpoint.
        let mouse = Definition::load(M6_VPID, &LoadOpts { no_cache: true, ..Default::default() }).unwrap();
        assert_eq!(mouse.name(), "Keychron M6");
    }
}
