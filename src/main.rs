mod definition;
mod hid;
mod keyboard;
mod mouse;
mod transceiver;

use clap::{Parser, Subcommand};
use hidapi::HidApi;
use std::path::PathBuf;
use std::process::ExitCode;

// Exit codes (docs/CLI.md); constructed at runtime since ExitCode::from is not const.
fn exit_runtime() -> ExitCode { ExitCode::from(1) }
fn exit_no_device() -> ExitCode { ExitCode::from(3) }
fn exit_protocol() -> ExitCode { ExitCode::from(4) }
fn exit_unsupported() -> ExitCode { ExitCode::from(5) }
const EXIT_OK: ExitCode = ExitCode::SUCCESS;

#[derive(Parser)]
#[command(name = "keychronctl", version, about = "Configure Keychron devices over USB HID")]
struct Cli {
    /// Device selector: vpid, VID:PID, or hidraw path
    #[arg(short, long, value_name = "SPEC")]
    device: Option<String>,

    /// Machine-readable JSON output
    #[arg(long)]
    json: bool,

    /// Protocol trace on stderr (hex dump of every report)
    #[arg(short, long)]
    verbose: bool,

    /// Use only cached definitions; never fetch
    #[arg(long)]
    offline: bool,

    /// Never read the definition cache; always fetch
    #[arg(long)]
    no_cache: bool,

    /// Override cache location (default: ~/.cache/keychronctl)
    #[arg(long, value_name = "DIR")]
    cache_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected Keychron HID interfaces
    List,
    /// Show device info (protocol, firmware, versions, battery, DPI)
    Info,
    /// Show battery status
    Battery,
    /// Show DPI levels
    Dpi,
    /// Manage the device-definition cache
    Definitions {
        #[command(subcommand)]
        cmd: DefinitionsCmd,
    },
}

#[derive(Subcommand)]
enum DefinitionsCmd {
    /// Fetch and cache a device definition
    Fetch {
        /// vpid to fetch (default: connected device)
        #[arg(long, value_name = "VPID")]
        vpid: Option<u32>,
    },
    /// List cached definitions
    List,
    /// Show a cached definition
    Show {
        #[arg(long, value_name = "VPID")]
        vpid: Option<u32>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            eprintln!("error: could not initialize HID API: {e}");
            return exit_runtime();
        }
    };

    let interfaces = hid::list_interfaces(&api);
    if interfaces.is_empty() {
        if cli.json {
            println!("[]");
        } else {
            eprintln!("no Keychron HID interfaces found");
        }
        // `list` with nothing attached is not an error; other commands are.
        return match cli.command {
            Command::List => EXIT_OK,
            _ => exit_no_device(),
        };
    }

    match &cli.command {
        Command::List => cmd_list(cli.json, &interfaces),
        Command::Info => cmd_info(&api, cli.json, cli.verbose, cli.device.as_deref(), &interfaces),
        Command::Battery => cmd_battery(&api, cli.json, cli.verbose, cli.device.as_deref(), &interfaces),
        Command::Dpi => cmd_dpi(&api, cli.json, cli.verbose, cli.device.as_deref(), &interfaces),
        Command::Definitions { cmd } => cmd_definitions(cmd, &cli, &api, &interfaces),
    }
}

fn cmd_list(json: bool, interfaces: &[hid::InterfaceInfo]) -> ExitCode {
    if json {
        let slim: Vec<JsonInterface> = interfaces.iter().map(JsonInterface::from).collect();
        match serde_json::to_string_pretty(&slim) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("error: {e}");
                return exit_runtime();
            }
        }
        return EXIT_OK;
    }

    println!("{:<12} {:<10} {:<10} {:<15} {}", "VPID", "VID:PID", "ROLE", "PATH", "PRODUCT");
    for i in interfaces {
        println!(
            "{:<12} {:<10} {:<10} {:<15} {}",
            i.vpid,
            i.vpid_hex(),
            i.role.as_str(),
            i.path,
            i.product_string.as_deref().unwrap_or("-")
        );
    }
    EXIT_OK
}

#[derive(serde::Serialize)]
struct JsonInterface<'a> {
    vpid: u32,
    vid: u16,
    pid: u16,
    role: &'a str,
    usage_page: u16,
    usage: u16,
    path: &'a str,
    product: Option<&'a str>,
}

impl<'a> From<&'a hid::InterfaceInfo> for JsonInterface<'a> {
    fn from(i: &'a hid::InterfaceInfo) -> Self {
        JsonInterface {
            vpid: i.vpid,
            vid: i.vid,
            pid: i.pid,
            role: i.role.as_str(),
            usage_page: i.usage_page,
            usage: i.usage,
            path: i.path.as_str(),
            product: i.product_string.as_deref(),
        }
    }
}

fn select_interface<'a>(
    spec: Option<&str>,
    interfaces: &'a [hid::InterfaceInfo],
) -> Result<&'a hid::InterfaceInfo, ExitCode> {
    if let Some(spec) = spec {
        return match hid::match_selector(spec, interfaces) {
            Some(i) => Ok(i),
            None => {
                eprintln!("error: no interface matches device selector '{spec}'");
                Err(exit_no_device())
            }
        };
    }
    // Auto-select by role priority: mouse interfaces first (current focus),
    // then keyboard, then bridge.
    let priority = [hid::Role::Mouse8k, hid::Role::Mouse4k, hid::Role::Keyboard, hid::Role::Bridge];
    for role in priority {
        if let Some(i) = interfaces.iter().find(|i| i.role == role) {
            return Ok(i);
        }
    }
    eprintln!("error: ambiguous device selection; use -d");
    Err(exit_no_device())
}

fn cmd_info(
    api: &HidApi,
    json: bool,
    verbose: bool,
    spec: Option<&str>,
    interfaces: &[hid::InterfaceInfo],
) -> ExitCode {
    let iface = match select_interface(spec, interfaces) {
        Ok(i) => i,
        Err(code) => return code,
    };

    let path = match std::ffi::CString::new(iface.path.as_str()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: invalid device path: {e}");
            return exit_runtime();
        }
    };
    let dev = match api.open_path(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: cannot open {}: {e}", iface.path);
            eprintln!("hint: check permissions on {} (udev rule may be required)", iface.path);
            return exit_runtime();
        }
    };

    let result: Result<serde_json::Value, String> = match iface.role {
        // The dongle's mouse interface serves both 1k and 8k mice. The paired
        // mouse's protocol decides the path: 1k uses feature reports (the
        // launcher's `_Y` transceiver), 8k uses output reports on 181.
        hid::Role::Mouse8k | hid::Role::Mouse4k => mouse_info(api, &path, verbose),
        hid::Role::Bridge => mouse::query_1k_feature_base_info(dev, verbose)
            .map(|i| serde_json::to_value(&i).unwrap_or_default())
            .map_err(|e| e.to_string()),
        hid::Role::Keyboard => keyboard::query_info(dev, verbose)
            .map(|i| serde_json::to_value(&i).unwrap_or_default())
            .map_err(|e| e.to_string()),
        hid::Role::Unknown => Err("unknown interface role; nothing to query".into()),
    };

    match result {
        Ok(mut value) => {
            if let serde_json::Value::Object(map) = &mut value {
                map.insert("vpid".into(), iface.vpid.into());
                map.insert("vid:pid".into(), iface.vpid_hex().into());
                map.insert("role".into(), iface.role.as_str().into());
                map.insert("path".into(), iface.path.clone().into());
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
            } else {
                print_info_human(&value);
            }
            EXIT_OK
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_protocol()
        }
    }
}

/// Mouse info via the dongle's mouse interface. The Link-KM dongle routes all
/// mouse commands through output report 181 / input report 182 (usage page
/// 0x8C); the 1k feature-report path only exists on older receivers.
fn mouse_info(api: &HidApi, path: &std::ffi::CString, verbose: bool) -> Result<serde_json::Value, String> {
    let open = || -> Result<hidapi::HidDevice, String> { api.open_path(path).map_err(|e| e.to_string()) };
    let mut report = serde_json::Map::new();

    // Link version info (cmd 2 on reportId 181).
    let info = open()
        .and_then(|d| mouse::query_8k_version(d, verbose).map_err(|e| e.to_string()))?;
    merge_mouse_info(&mut report, &info);

    // Bond list (cmd 3 on reportId 181): identifies the paired mouse.
    let mut paired_vpid: Option<u32> = None;
    match open().and_then(|d| mouse::query_8k_bond_list(d, verbose).map_err(|e| e.to_string())) {
        Ok(bonded) => {
            paired_vpid = bonded.iter().find(|b| b.active).map(|b| b.vpid);
            report.insert("bonded".into(), serde_json::to_value(&bonded).unwrap_or_default());
        }
        Err(e) => {
            report.insert("bond_error".into(), e.into());
        }
    }

    // Settings snapshot (cmd 6 on reportId 179): battery + DPI + profile.
    if let Ok(power) = open().and_then(|d| mouse::query_8k_power(d, verbose).map_err(|e| e.to_string())) {
        report.insert("battery_percent".into(), power.battery_percent.into());
        report.insert("battery_state".into(), power.battery_state.into());
        report.insert("charging".into(), power.charging.into());
    }
    if let Ok(dpi) = open().and_then(|d| mouse::query_8k_dpi(d, verbose).map_err(|e| e.to_string())) {
        report.insert("dpi".into(), serde_json::to_value(&dpi).unwrap_or_default());
    }

    // Product name for the paired mouse (network, non-fatal).
    if let Some(vpid) = paired_vpid {
        if let Ok(product) = definition::ProductInfo::fetch(vpid) {
            report.insert("product".into(), product.name.into());
            report.insert("product_vpid".into(), vpid.into());
        }
    }

    Ok(serde_json::Value::Object(report))
}

fn merge_mouse_info(report: &mut serde_json::Map<String, serde_json::Value>, info: &mouse::MouseInfo) {
    let json = serde_json::to_value(info).unwrap_or_default();
    if let serde_json::Value::Object(map) = json {
        for (k, v) in map {
            report.insert(k, v);
        }
    }
}

/// Open the selected device's HID interface; shared by device commands.
fn open_selected(
    api: &HidApi,
    spec: Option<&str>,
    interfaces: &[hid::InterfaceInfo],
) -> Result<(hid::InterfaceInfo, hidapi::HidDevice), ExitCode> {
    let iface = select_interface(spec, interfaces)?;
    let path = std::ffi::CString::new(iface.path.as_str()).map_err(|e| {
        eprintln!("error: invalid device path: {e}");
        exit_runtime()
    })?;
    let dev = api.open_path(&path).map_err(|e| {
        eprintln!("error: cannot open {}: {e}", iface.path);
        eprintln!("hint: check permissions on {} (udev rule may be required)", iface.path);
        exit_runtime()
    })?;
    Ok((iface.clone(), dev))
}

fn cmd_battery(
    api: &HidApi,
    json: bool,
    verbose: bool,
    spec: Option<&str>,
    interfaces: &[hid::InterfaceInfo],
) -> ExitCode {
    let (iface, dev) = match open_selected(api, spec, interfaces) {
        Ok(x) => x,
        Err(code) => return code,
    };
    let result = match iface.role {
        hid::Role::Mouse8k | hid::Role::Mouse4k => mouse::query_8k_power(dev, verbose)
            .map(|p| serde_json::to_value(&p).unwrap_or_default())
            .map_err(|e| e.to_string()),
        _ => Err("battery is not supported for this interface role yet".into()),
    };
    print_query_result(result, json, &iface, verbose)
}

fn cmd_dpi(
    api: &HidApi,
    json: bool,
    verbose: bool,
    spec: Option<&str>,
    interfaces: &[hid::InterfaceInfo],
) -> ExitCode {
    let (iface, dev) = match open_selected(api, spec, interfaces) {
        Ok(x) => x,
        Err(code) => return code,
    };
    let result = match iface.role {
        hid::Role::Mouse8k | hid::Role::Mouse4k => mouse::query_8k_dpi(dev, verbose)
            .map(|d| serde_json::to_value(&d).unwrap_or_default())
            .map_err(|e| e.to_string()),
        _ => Err("DPI is not supported for this interface role yet".into()),
    };
    print_query_result(result, json, &iface, verbose)
}

/// Shared printer for query commands: wraps the result with device metadata,
/// prints human or JSON, maps errors to exit codes.
fn print_query_result(
    result: Result<serde_json::Value, String>,
    json: bool,
    iface: &hid::InterfaceInfo,
    _verbose: bool,
) -> ExitCode {
    match result {
        Ok(mut value) => {
            if let serde_json::Value::Object(map) = &mut value {
                map.insert("vpid".into(), iface.vpid.into());
                map.insert("vid:pid".into(), iface.vpid_hex().into());
                map.insert("role".into(), iface.role.as_str().into());
                map.insert("path".into(), iface.path.clone().into());
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
            } else {
                print_info_human(&value);
            }
            EXIT_OK
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_protocol()
        }
    }
}

fn load_opts(cli: &Cli) -> definition::LoadOpts {
    definition::LoadOpts {
        cache_dir: cli.cache_dir.clone(),
        offline: cli.offline,
        no_cache: cli.no_cache,
        json_version: None,
    }
}

fn cmd_definitions(cmd: &DefinitionsCmd, cli: &Cli, api: &HidApi, interfaces: &[hid::InterfaceInfo]) -> ExitCode {
    let json = cli.json;
    // A vpid argument, else the connected device's (paired mouse preferred).
    let resolve_vpid = |explicit: Option<u32>| -> Result<u32, ExitCode> {
        if let Some(vpid) = explicit {
            return Ok(vpid);
        }
        // Prefer the paired mouse on a dongle (bond list, cmd 3 on reportId 181).
        for iface in interfaces.iter().filter(|i| i.role == hid::Role::Mouse8k) {
            let Ok(path) = std::ffi::CString::new(iface.path.as_str()) else { continue };
            let Ok(dev) = api.open_path(&path) else { continue };
            if let Ok(bonded) = mouse::query_8k_bond_list(dev, false) {
                if let Some(active) = bonded.iter().find(|b| b.active) {
                    return Ok(active.vpid);
                }
            }
        }
        // Fall back to the first interface's vpid.
        match interfaces.first() {
            Some(i) => Ok(i.vpid),
            None => {
                eprintln!("error: no device connected and no --vpid given");
                Err(exit_no_device())
            }
        }
    };

    match cmd {
        DefinitionsCmd::Fetch { vpid } => {
            let vpid = match resolve_vpid(*vpid) {
                Ok(v) => v,
                Err(code) => return code,
            };
            match definition::Definition::load(vpid, &load_opts(cli)) {
                Ok(def) => {
                    if json {
                        println!("{{\"vpid\": {vpid}, \"name\": {}}}", serde_json::to_string(def.name()).unwrap_or_default());
                    } else {
                        println!("fetched {} (vpid {vpid})", def.name());
                    }
                    EXIT_OK
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    exit_runtime()
                }
            }
        }
        DefinitionsCmd::List => {
            let dir = load_opts(cli).cache_dir().join("device");
            let mut entries: Vec<(u32, String)> = Vec::new();
            if let Ok(read) = std::fs::read_dir(&dir) {
                for entry in read.flatten() {
                    let Ok(vpid) = entry.file_name().to_string_lossy().parse::<u32>() else { continue };
                    for name in ["v3.json", "v2.json"] {
                        let f = entry.path().join(name);
                        if let Ok(s) = std::fs::read_to_string(&f) {
                            if let Ok(def) = definition::Definition::from_json(&s) {
                                entries.push((vpid, def.name().to_string()));
                            }
                            break;
                        }
                    }
                }
            }
            entries.sort();
            if json {
                println!("{}", serde_json::to_string_pretty(&entries).unwrap_or_default());
            } else {
                if entries.is_empty() {
                    println!("definition cache is empty ({})", dir.display());
                }
                for (vpid, name) in &entries {
                    println!("{vpid:<12} {name}");
                }
            }
            EXIT_OK
        }
        DefinitionsCmd::Show { vpid } => {
            let vpid = match resolve_vpid(*vpid) {
                Ok(v) => v,
                Err(code) => return code,
            };
            let opts = definition::LoadOpts {
                offline: true,
                ..load_opts(cli)
            };
            // Validate via the loader, then print the raw cached JSON.
            match definition::Definition::load(vpid, &opts) {
                Ok(_) => {
                    let base = opts.cache_dir().join("device").join(vpid.to_string());
                    for name in ["v3.json", "v2.json"] {
                        let f = base.join(name);
                        if let Ok(s) = std::fs::read_to_string(&f) {
                            print!("{s}");
                            return EXIT_OK;
                        }
                    }
                    eprintln!("error: cached file for {vpid} disappeared");
                    exit_runtime()
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    eprintln!("hint: run `keychronctl definitions fetch --vpid {vpid}` first");
                    exit_runtime()
                }
            }
        }
    }
}

fn print_dpi_human(dpi: &serde_json::Map<String, serde_json::Value>) {
    if let Some(current) = dpi.get("current_levels").and_then(|v| v.as_array()) {
        if let Some(level) = current.first().and_then(|v| v.as_u64()) {
            if let Some(levels) = dpi.get("levels").and_then(|v| v.as_array()) {
                let value = levels.get(level as usize).and_then(|v| v.as_u64());
                println!("dpi:        level {} of {}{}", level + 1, levels.len(),
                    value.map(|v| format!(" ({v} DPI)")).unwrap_or_default());
            }
        }
    }
    if let Some(levels) = dpi.get("levels").and_then(|v| v.as_array()) {
        let list: Vec<String> = levels.iter().filter_map(|v| v.as_u64().map(|n| n.to_string())).collect();
        println!("dpi levels: {}", list.join(" / "));
    }
}

fn print_info_human(value: &serde_json::Value) {
    let obj = value.as_object().unwrap();
    let get = |k: &str| obj.get(k).and_then(|v| v.as_str()).unwrap_or("-");
    println!("device:     {} ({} via {})", get("vid:pid"), get("role"), get("path"));

    let rows = [
        ("protocol", "protocol"),
        ("version", "firmware"),
        ("receiver_version", "receiver fw"),
        ("work_mode", "work mode"),
        ("firmware_version", "fw version"),
        ("protocol_version", "qmk protocol"),
        ("keychron_protocol_version", "kc protocol"),
        ("instruction_set", "instruction set"),
        ("product", "product"),
    ];
    for (key, label) in rows {
        match obj.get(key) {
            Some(serde_json::Value::String(s)) => println!("{label:<12}{s}"),
            Some(serde_json::Value::Number(n)) => println!("{label:<12}{n}"),
            _ => {}
        }
    }
    if let (Some(pct), Some(state)) = (
        obj.get("battery_percent").and_then(|v| v.as_u64()),
        obj.get("battery_state").and_then(|v| v.as_str()),
    ) {
        let charging = obj.get("charging").and_then(|v| v.as_bool()).unwrap_or(false);
        let extra = if charging { ", charging" } else { "" };
        println!("battery:    {pct}% ({state}{extra})");
    }
    if let Some(dpi) = obj.get("dpi").and_then(|v| v.as_object()) {
        print_dpi_human(dpi);
    } else if obj.contains_key("levels") {
        // Standalone `dpi` command: fields at top level.
        print_dpi_human(obj);
    }
    if let Some(vpid) = obj.get("vpid").and_then(|v| v.as_u64()) {
        println!("vpid:       {vpid}");
    }
    if let Some(serde_json::Value::Array(bonded)) = obj.get("bonded") {
        for b in bonded {
            if let Some(o) = b.as_object() {
                let vpid = o.get("vpid").and_then(|v| v.as_u64()).unwrap_or(0);
                let vid = o.get("vid").and_then(|v| v.as_u64()).unwrap_or(0);
                let pid = o.get("pid").and_then(|v| v.as_u64()).unwrap_or(0);
                let slot = o.get("slot").and_then(|v| v.as_str()).unwrap_or("?");
                let state = o.get("state").and_then(|v| v.as_u64()).unwrap_or(0);
                println!("bonded:     {slot}: {vid:04X}:{pid:04X} (vpid {vpid}, state {state})");
            }
        }
    }
}
