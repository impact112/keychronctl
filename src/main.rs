mod definition;
mod hid;
mod keyboard;
mod keymap;
mod r#macro;
mod mouse;
mod transceiver;

use clap::{Parser, Subcommand};
use hidapi::HidApi;
use std::path::PathBuf;
use std::process::ExitCode;

// Exit codes (docs/CLI.md); constructed at runtime since ExitCode::from is not const.
fn exit_runtime() -> ExitCode { ExitCode::from(1) }
fn exit_usage() -> ExitCode { ExitCode::from(2) }
fn exit_no_device() -> ExitCode { ExitCode::from(3) }
fn exit_protocol() -> ExitCode { ExitCode::from(4) }
fn exit_unsupported() -> ExitCode { ExitCode::from(5) }
const EXIT_OK: ExitCode = ExitCode::SUCCESS;

#[derive(Parser)]
#[command(name = "keychronctl", version, about = "Configure Keychron devices over USB HID")]
struct Cli {
    /// Device selector: vpid, VID:PID, or hidraw path
    #[arg(short, long, value_name = "SPEC", global = true)]
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
    /// Show or set DPI levels
    Dpi {
        #[command(subcommand)]
        cmd: Option<DpiCmd>,
    },
    /// Show or set the polling rate
    Pollrate {
        #[command(subcommand)]
        cmd: Option<SimpleGetCmd>,
    },
    /// Show or set the sleep timer (minutes; 0 = never)
    Sleep {
        #[command(subcommand)]
        cmd: Option<SimpleGetCmd>,
    },
    /// Show or set the debounce time (ms)
    Debounce {
        #[command(subcommand)]
        cmd: Option<SimpleGetCmd>,
    },
    /// Show or set mouse lighting (RGB devices only)
    Lighting {
        #[command(subcommand)]
        cmd: Option<SimpleGetCmd>,
    },
    /// Show or set sensor options (LOD, angle snapping, motion sync, ...)
    Sensor {
        #[command(subcommand)]
        cmd: Option<SimpleGetCmd>,
    },
    /// Show or set scroll-wheel parameters
    Scroll {
        #[command(subcommand)]
        cmd: Option<SimpleGetCmd>,
    },
    /// Dump the full settings snapshot in one query
    Settings,
    /// Show or set button bindings (fun keys), per profile
    Binds {
        #[command(subcommand)]
        cmd: BindsCmd,
    },
    /// List, switch, create, or remove mouse profiles
    Profile {
        #[command(subcommand)]
        cmd: ProfileCmd,
    },
    /// Manage the device-definition cache
    Definitions {
        #[command(subcommand)]
        cmd: DefinitionsCmd,
    },
}

#[derive(Subcommand)]
enum DpiCmd {
    /// Switch the active DPI stage, replace the ladder, or set one value
    Set {
        /// DPI value to write to the current (or --stage) stage,
        /// e.g. `dpi set 800`
        #[arg(value_name = "DPI")]
        value: Option<u16>,
        /// Active stage index, 0-based (e.g. 0 = lowest stage)
        #[arg(long, value_name = "N", alias = "level")]
        stage: Option<u8>,
        /// New DPI ladder, comma-separated (e.g. 400,800,1600,3200,5000)
        #[arg(long, value_name = "LIST")]
        levels: Option<String>,
    },
}

/// `get` / `set` pair used by the mouse setting groups. Without a
/// subcommand the command just reads.
#[derive(Subcommand)]
enum SimpleGetCmd {
    /// Read the current value
    Get,
    /// Write a new value
    Set {
        /// Pollrate: Hz (125|250|500|1000|2000|4000|8000); sleep: minutes
        /// (0 = never); debounce: ms; lighting: mode index; scroll: speed
        #[arg(value_name = "VALUE")]
        value: Option<String>,
        /// Additional option (lighting: brightness; scroll: inertia;
        /// sensor: see --help flags below)
        #[arg(long, value_name = "N")]
        brightness: Option<u8>,
        /// Lighting: speed
        #[arg(long, value_name = "N")]
        speed: Option<u8>,
        /// Lighting: color, R,G,B
        #[arg(long, value_name = "R,G,B")]
        rgb: Option<String>,
        /// Sensor: lift-off distance (0-3)
        #[arg(long, value_name = "N")]
        lod: Option<u8>,
        /// Sensor: ripple control on/off
        #[arg(long, value_name = "ON|OFF")]
        wave: Option<String>,
        /// Sensor: angle snapping on/off
        #[arg(long, value_name = "ON|OFF")]
        line: Option<String>,
        /// Sensor: motion sync on/off
        #[arg(long, value_name = "ON|OFF")]
        motion: Option<String>,
        /// Sensor: scroll direction normal/reverse
        #[arg(long, value_name = "DIR")]
        scroll_dir: Option<String>,
        /// Sensor: 20K max-speed mode on/off
        #[arg(long, value_name = "ON|OFF")]
        fps20k: Option<String>,
        /// Scroll: inertia
        #[arg(long, value_name = "N")]
        inertia: Option<u8>,
        /// Scroll: spl
        #[arg(long, value_name = "N")]
        spl: Option<u8>,
    },
}

#[derive(Subcommand)]
enum BindsCmd {
    /// List bindings (all profiles, or one with --profile)
    List {
        /// Only fetch this profile (default: all)
        #[arg(long, value_name = "N")]
        profile: Option<u8>,
    },
    /// Set a button binding (applies to the current or --profile)
    Set {
        /// Button index (see `binds list` or the definition)
        index: u8,
        /// Action: mouse:left|right|middle|forward|backward|..., media:volumeUp,
        /// keyboard:ctrl+a, dpi:loop|next|prev, pollingRate:loop, profile:N,
        /// shortcut:copy|paste|..., light:N, remove, disable
        action: String,
        /// Target profile (default: current)
        #[arg(long, value_name = "N")]
        profile: Option<u8>,
    },
}

#[derive(Subcommand)]
enum ProfileCmd {
    /// Show current profile and count
    List,
    /// Switch to a profile
    Select { index: u8 },
    /// Create a new profile (appends after the last)
    Create,
    /// Remove the last profile
    Remove,
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
    // Die quietly on SIGPIPE (e.g. `keychronctl list | head`) instead of
    // panicking on a broken stdout.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
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
        Command::Dpi { cmd } => cmd_dpi(cmd, &api, &cli, &interfaces),
        Command::Pollrate { cmd } => cmd_simple_setting(SettingKind::Pollrate, cmd, &api, &cli, &interfaces),
        Command::Sleep { cmd } => cmd_simple_setting(SettingKind::Sleep, cmd, &api, &cli, &interfaces),
        Command::Debounce { cmd } => cmd_simple_setting(SettingKind::Debounce, cmd, &api, &cli, &interfaces),
        Command::Lighting { cmd } => cmd_simple_setting(SettingKind::Lighting, cmd, &api, &cli, &interfaces),
        Command::Sensor { cmd } => cmd_simple_setting(SettingKind::Sensor, cmd, &api, &cli, &interfaces),
        Command::Scroll { cmd } => cmd_simple_setting(SettingKind::Scroll, cmd, &api, &cli, &interfaces),
        Command::Settings => cmd_settings(&api, cli.json, cli.verbose, cli.device.as_deref(), &interfaces),
        Command::Binds { cmd } => cmd_binds(cmd, &api, &cli, &interfaces),
        Command::Profile { cmd } => cmd_profile(cmd, &api, cli.json, cli.verbose, cli.device.as_deref(), &interfaces),
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
    // Role priority: mouse interfaces first (current focus), then keyboard,
    // then bridge. Also applies to explicit selectors, because one hidraw
    // node can carry several roles (Link-KM: bridge + mouse-8k).
    let priority = [hid::Role::Mouse8k, hid::Role::Mouse4k, hid::Role::Keyboard, hid::Role::Bridge];
    if let Some(spec) = spec {
        let matches = hid::match_selector_all(spec, interfaces);
        if matches.is_empty() {
            eprintln!("error: no interface matches device selector '{spec}'");
            return Err(exit_no_device());
        }
        for role in priority {
            if let Some(i) = matches.iter().find(|i| i.role == role) {
                return Ok(i);
            }
        }
        return Ok(matches[0]);
    }
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
    cmd: &Option<DpiCmd>,
    api: &HidApi,
    cli: &Cli,
    interfaces: &[hid::InterfaceInfo],
) -> ExitCode {
    let json = cli.json;
    let verbose = cli.verbose;
    let ctx = match mouse_context(api, cli, cli.device.as_deref(), interfaces) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let iface = ctx.iface.clone();
    let dev = match reopen(api, &iface) {
        Ok(d) => d,
        Err(c) => return c,
    };

    let Some(DpiCmd::Set { value, stage, levels }) = cmd else {
        // Plain `dpi` — show the current ladder.
        let result = mouse::query_8k_dpi(dev, verbose)
            .map(|d| serde_json::to_value(&d).unwrap_or_default())
            .map_err(|e| e.to_string());
        return print_query_result(result, json, &iface, verbose);
    };

    // Read-modify-write: cmd 64 always carries the full state.
    let snap = match mouse::query_8k_snapshot(dev, verbose) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return exit_protocol();
        }
    };
    if snap.dpi_level_num as usize > snap.dpi_values.len() {
        eprintln!("error: device reports {} DPI levels but only 5 fit the wire format", snap.dpi_level_num);
        return exit_unsupported();
    }
    let mut values = snap.dpi_values;
    let mut new_level = snap.dpi_levels[0];

    if let Some(l) = stage {
        if *l >= snap.dpi_level_num {
            eprintln!("error: stage {l} out of range (device has {})", snap.dpi_level_num);
            return exit_usage();
        }
        new_level = *l;
    }
    if let Some(list) = levels {
        let parsed: Vec<u16> = match list.split(',').map(|v| v.trim().parse::<u16>()).collect() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error: invalid levels list '{list}': {e}");
                return exit_usage();
            }
        };
        if parsed.is_empty() || parsed.len() > 5 {
            eprintln!("error: levels list must have 1..=5 entries");
            return exit_usage();
        }
        for (i, v) in parsed.iter().enumerate() {
            values[i] = *v;
        }
        if let Some(max) = values[..snap.dpi_level_num.max(1) as usize].iter().max() {
            let _ = max; // limit check below uses the final ladder
        }
    }
    if let Some(v) = value {
        values[new_level as usize] = *v;
    }
    // Enforce the device-reported DPI limits from the definition
    // (e.g. [50, 12000] on the M6) — the firmware itself accepts values
    // beyond its real sensor range.
    if let definition::Definition::Mouse(m) = &ctx.def {
        if m.dpi.as_ref().map(|d| d.limit.len()) == Some(2) {
            let (min, max) = (m.dpi.as_ref().unwrap().limit[0], m.dpi.as_ref().unwrap().limit[1]);
            if let Some(&bad) = values.iter().find(|&&v| v < min || v > max) {
                eprintln!("error: DPI {bad} outside device range {min}..={max}");
                return exit_usage();
            }
        }
    }
    if values.iter().any(|&v| v == 0) {
        eprintln!("error: DPI values must be > 0");
        return exit_usage();
    }

    let dev2 = match reopen(api, &iface) {
        Ok(d) => d,
        Err(c) => return c,
    };
    match mouse::set_8k_dpi(dev2, new_level, &values, snap.dpi_level_num, verbose) {
        Ok(()) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "stage": new_level,
                        "levels": values[..snap.dpi_level_num as usize],
                        "levelNum": snap.dpi_level_num,
                    })
                );
            } else {
                println!("DPI set: stage {} -> {}", new_level, values[new_level as usize]);
                for (i, v) in values.iter().enumerate().take(snap.dpi_level_num as usize) {
                    let marker = if i as u8 == new_level { "*" } else { " " };
                    println!(" {marker} stage {i}: {v}");
                }
            }
            EXIT_OK
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_protocol()
        }
    }
}

/// Reopen a device by path (HidDevice is not Clone).
fn reopen(api: &HidApi, iface: &hid::InterfaceInfo) -> Result<hidapi::HidDevice, ExitCode> {
    let path = std::ffi::CString::new(iface.path.as_str()).map_err(|e| {
        eprintln!("error: invalid device path: {e}");
        exit_runtime()
    })?;
    api.open_path(&path).map_err(|e| {
        eprintln!("error: cannot reopen {}: {e}", iface.path);
        exit_runtime()
    })
}

#[derive(Clone, Copy, PartialEq)]
enum SettingKind {
    Pollrate,
    Sleep,
    Debounce,
    Lighting,
    Sensor,
    Scroll,
}

fn parse_on_off(s: &str, name: &str) -> Result<bool, ExitCode> {
    match s.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Ok(true),
        "off" | "false" | "0" | "no" => Ok(false),
        _ => {
            eprintln!("error: {name} expects on|off, got '{s}'");
            Err(exit_usage())
        }
    }
}

fn parse_rgb(s: &str) -> Result<[u8; 3], ExitCode> {
    let parts: Vec<u8> = s
        .split(',')
        .map(|v| v.trim().parse::<u8>())
        .collect::<Result<_, _>>()
        .map_err(|e| {
            eprintln!("error: invalid rgb '{s}': {e}");
            exit_usage()
        })?;
    if parts.len() != 3 {
        eprintln!("error: rgb expects R,G,B");
        return Err(exit_usage());
    }
    Ok([parts[0], parts[1], parts[2]])
}

fn cmd_simple_setting(
    kind: SettingKind,
    cmd: &Option<SimpleGetCmd>,
    api: &HidApi,
    cli: &Cli,
    interfaces: &[hid::InterfaceInfo],
) -> ExitCode {
    let (iface, dev) = match open_selected(api, cli.device.as_deref(), interfaces) {
        Ok(x) => x,
        Err(code) => return code,
    };
    if !matches!(iface.role, hid::Role::Mouse8k | hid::Role::Mouse4k) {
        eprintln!("error: this setting requires a mouse interface; use `keychronctl list`");
        return exit_unsupported();
    }
    let json = cli.json;
    let verbose = cli.verbose;
    let set = matches!(cmd, Some(SimpleGetCmd::Set { .. }));

    // ---------------- reads ----------------
    if !set {
        let result: Result<serde_json::Value, String> = match kind {
            SettingKind::Pollrate => mouse::query_8k_snapshot(dev, verbose).map(|s| {
                let hz: Vec<u32> = s.rate_table.iter().map(|&l| mouse::polling_hz(l).unwrap_or(0)).collect();
                serde_json::json!({
                    "currentLevel": s.rate_levels,
                    "hz": mouse::polling_hz(s.rate_levels[0]),
                    "levels": s.rate_table,
                    "levelsHz": hz,
                    "levelNum": s.rate_level_num,
                })
            }).map_err(|e| e.to_string()),
            SettingKind::Sleep => mouse::get_8k_sleep(dev, verbose)
                .map(|m| serde_json::json!({ "minutes": m, "never": m == 255 }))
                .map_err(|e| e.to_string()),
            SettingKind::Debounce => mouse::query_8k_snapshot(dev, verbose)
                .map(|s| serde_json::json!({ "ms": s.debounce, "options": s.debounce_options }))
                .map_err(|e| e.to_string()),
            SettingKind::Lighting => mouse::query_8k_light(dev, verbose)
                .map(|l| serde_json::to_value(&l).unwrap_or_default())
                .map_err(|e| {
                    if e.to_string().contains("timeout") {
                        "timed out — this device likely has no RGB lighting (the M6 has none)".to_string()
                    } else {
                        e.to_string()
                    }
                }),
            SettingKind::Sensor => mouse::query_8k_snapshot(dev, verbose)
                .map(|s| serde_json::to_value(&s.sys).unwrap_or_default())
                .map_err(|e| e.to_string()),
            SettingKind::Scroll => mouse::query_8k_snapshot(dev, verbose)
                .map(|s| serde_json::to_value(&s.scroll).unwrap_or_default())
                .map_err(|e| e.to_string()),
        };
        return match result {
            Ok(v) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                } else {
                    print_setting_human(kind, &v);
                }
                EXIT_OK
            }
            Err(e) => {
                eprintln!("error: {e}");
                exit_protocol()
            }
        };
    }

    // ---------------- writes ----------------
    let SimpleGetCmd::Set {
        value,
        brightness,
        speed,
        rgb,
        lod,
        wave,
        line,
        motion,
        scroll_dir,
        fps20k,
        inertia,
        spl,
    } = cmd.as_ref().unwrap()
    else {
        unreachable!()
    };

    match kind {
        SettingKind::Pollrate => {
            let Some(v) = value else {
                eprintln!("error: pollrate set expects a Hz value (e.g. 1000)");
                return exit_usage();
            };
            let hz: u32 = match v.parse() {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("error: invalid Hz '{v}': {e}");
                    return exit_usage();
                }
            };
            let snap = match mouse::query_8k_snapshot(dev, verbose) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {e}");
                    return exit_protocol();
                }
            };
            let Some(level) = snap
                .rate_table
                .iter()
                .position(|&l| mouse::polling_hz(l) == Some(hz))
            else {
                let mut supported: Vec<u32> = snap
                    .rate_table
                    .iter()
                    .filter_map(|&l| mouse::polling_hz(l))
                    .collect();
                supported.sort_unstable();
                supported.dedup();
                eprintln!("error: {hz} Hz not supported (device offers {supported:?})");
                return exit_unsupported();
            };
            let dev2 = match reopen(api, &iface) {
                Ok(d) => d,
                Err(c) => return c,
            };
            match mouse::set_8k_polling_rate(dev2, level as u8, &snap.rate_table, snap.rate_level_num, verbose) {
                Ok(()) => {
                    if !json {
                        println!("polling rate set to {hz} Hz (level {level})");
                    } else {
                        println!("{{\"hz\": {hz}, \"level\": {level}}}");
                    }
                    EXIT_OK
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    exit_protocol()
                }
            }
        }
        SettingKind::Sleep => {
            let Some(v) = value else {
                eprintln!("error: sleep set expects minutes (0 = never)");
                return exit_usage();
            };
            let minutes: u8 = match v.parse() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: invalid minutes '{v}': {e}");
                    return exit_usage();
                }
            };
            match mouse::set_8k_sleep(dev, minutes, verbose) {
                Ok(()) => {
                    if minutes == 0 || minutes == 255 {
                        println!("sleep timer disabled");
                    } else {
                        println!("sleep timer set to {minutes} minutes");
                    }
                    EXIT_OK
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    exit_protocol()
                }
            }
        }
        SettingKind::Debounce => {
            let Some(v) = value else {
                eprintln!("error: debounce set expects milliseconds");
                return exit_usage();
            };
            let ms: u8 = match v.parse() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: invalid ms '{v}': {e}");
                    return exit_usage();
                }
            };
            match mouse::set_8k_debounce(dev, ms, verbose) {
                Ok(()) => {
                    println!("debounce set to {ms} ms");
                    EXIT_OK
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    exit_protocol()
                }
            }
        }
        SettingKind::Lighting => {
            let current = mouse::query_8k_light(dev, verbose).ok();
            let dev = match reopen(api, &iface) {
                Ok(d) => d,
                Err(c) => return c,
            };
            let mode = match value {
                Some(v) => match v.parse::<u8>() {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("error: invalid mode '{v}': {e}");
                        return exit_usage();
                    }
                },
                None => current.as_ref().map(|c| c.mode).unwrap_or(0),
            };
            let b = brightness.or(current.as_ref().map(|c| c.brightness)).unwrap_or(50);
            let s = speed.or(current.as_ref().map(|c| c.speed)).unwrap_or(50);
            let color = match rgb {
                Some(r) => match parse_rgb(r) {
                    Ok(c) => c,
                    Err(c) => return c,
                },
                None => current.as_ref().map(|c| c.rgb).unwrap_or([255, 255, 255]),
            };
            match mouse::set_8k_light(dev, mode, b, s, color, verbose) {
                Ok(l) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&l).unwrap_or_default());
                    } else {
                        println!("lighting set: mode {}, brightness {}, speed {}, rgb {:?}", l.mode, l.brightness, l.speed, l.rgb);
                    }
                    EXIT_OK
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    if e.to_string().contains("timeout") {
                        eprintln!("hint: this device likely has no RGB lighting (the M6 has none)");
                    }
                    exit_protocol()
                }
            }
        }
        SettingKind::Sensor => {
            let snap = match mouse::query_8k_snapshot(dev, verbose) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {e}");
                    return exit_protocol();
                }
            };
            let mut sys = snap.sys;
            let mut changed = false;
            if let Some(v) = lod {
                if *v > 3 {
                    eprintln!("error: lod must be 0..3");
                    return exit_usage();
                }
                sys.lod = *v;
                changed = true;
            }
            for (arg, name, field) in [
                (wave, "wave", &mut sys.wave as *mut bool),
                (line, "line", &mut sys.line as *mut bool),
                (motion, "motion", &mut sys.motion as *mut bool),
                (fps20k, "fps20k", &mut sys.fps20k as *mut bool),
            ] {
                if let Some(v) = arg {
                    // SAFETY: field points at a bool inside `sys`, which lives
                    // for the rest of this scope.
                    let on = match parse_on_off(v, name) {
                        Ok(b) => b,
                        Err(c) => return c,
                    };
                    unsafe { *field = on }
                    changed = true;
                }
            }
            if let Some(v) = scroll_dir {
                sys.scroll_dir = match v.to_ascii_lowercase().as_str() {
                    "reverse" | "reversed" => true,
                    "normal" | "forward" => false,
                    _ => {
                        eprintln!("error: scroll-dir expects normal|reverse");
                        return exit_usage();
                    }
                };
                changed = true;
            }
            if !changed {
                eprintln!("error: sensor set needs at least one of --lod --wave --line --motion --scroll-dir --fps20k");
                return exit_usage();
            }
            let dev2 = match reopen(api, &iface) {
                Ok(d) => d,
                Err(c) => return c,
            };
            match mouse::set_8k_sensor(dev2, &sys, verbose) {
                Ok(()) => {
                    println!(
                        "sensor set: lod {} wave {} line {} motion {} scrollDir {} fps20k {}",
                        sys.lod,
                        if sys.wave { "on" } else { "off" },
                        if sys.line { "on" } else { "off" },
                        if sys.motion { "on" } else { "off" },
                        if sys.scroll_dir { "reverse" } else { "normal" },
                        if sys.fps20k { "on" } else { "off" },
                    );
                    EXIT_OK
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    exit_protocol()
                }
            }
        }
        SettingKind::Scroll => {
            let snap = match mouse::query_8k_snapshot(dev, verbose) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {e}");
                    return exit_protocol();
                }
            };
            let speed = match value {
                Some(v) => match v.parse::<u8>() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: invalid speed '{v}': {e}");
                        return exit_usage();
                    }
                },
                None => snap.scroll.speed,
            };
            let inertia = inertia.unwrap_or(snap.scroll.inertia);
            let spl = spl.unwrap_or(snap.scroll.spl);
            let dev2 = match reopen(api, &iface) {
                Ok(d) => d,
                Err(c) => return c,
            };
            match mouse::set_8k_scroll(dev2, speed, inertia, spl, verbose) {
                Ok(()) => {
                    println!("scroll set: speed {speed}, inertia {inertia}, spl {spl}");
                    EXIT_OK
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    exit_protocol()
                }
            }
        }
    }
}

fn print_setting_human(kind: SettingKind, v: &serde_json::Value) {
    let get = |k: &str| v.get(k);
    match kind {
        SettingKind::Pollrate => {
            let hz = get("hz").and_then(|x| x.as_u64());
            let current = get("currentLevel").and_then(|x| x.as_array()).and_then(|a| a.first()).and_then(|x| x.as_u64());
            let levels = get("levelsHz").and_then(|x| x.as_array()).map(|a| {
                a.iter().filter_map(|x| x.as_u64()).collect::<Vec<_>>()
            }).unwrap_or_default();
            println!("polling rate: {} Hz", hz.map(|h| h.to_string()).unwrap_or_else(|| "?".into()));
            for (i, h) in levels.iter().enumerate() {
                let marker = if Some(i as u64) == current { "*" } else { " " };
                println!(" {marker} level {i}: {h} Hz");
            }
        }
        SettingKind::Sleep => {
            let m = get("minutes").and_then(|x| x.as_u64()).unwrap_or(0);
            if m == 255 {
                println!("sleep: never");
            } else {
                println!("sleep: {m} minutes");
            }
        }
        SettingKind::Debounce => {
            println!("debounce: {} ms", get("ms").and_then(|x| x.as_u64()).unwrap_or(0));
            let opts = get("options").and_then(|x| x.as_array()).map(|a| {
                a.iter().filter_map(|x| x.as_u64()).collect::<Vec<_>>()
            }).unwrap_or_default();
            if !opts.is_empty() {
                println!("options: {:?}", opts);
            }
        }
        SettingKind::Lighting => {
            println!(
                "lighting: mode {}, brightness {}, speed {}, rgb {:?}",
                get("mode").and_then(|x| x.as_u64()).unwrap_or(0),
                get("brightness").and_then(|x| x.as_u64()).unwrap_or(0),
                get("speed").and_then(|x| x.as_u64()).unwrap_or(0),
                get("rgb").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64()).collect::<Vec<_>>()).unwrap_or_default(),
            );
        }
        SettingKind::Sensor => {
            let onoff = |b: bool| if b { "on" } else { "off" };
            println!(
                "sensor: lod {}, wave {}, line {}, motion {}, scrollDir {}, fps20k {}",
                get("lod").and_then(|x| x.as_u64()).unwrap_or(0),
                onoff(get("wave").and_then(|x| x.as_bool()).unwrap_or(false)),
                onoff(get("line").and_then(|x| x.as_bool()).unwrap_or(false)),
                onoff(get("motion").and_then(|x| x.as_bool()).unwrap_or(false)),
                if get("scroll_dir").and_then(|x| x.as_bool()).unwrap_or(false) { "reverse" } else { "normal" },
                onoff(get("fps20k").and_then(|x| x.as_bool()).unwrap_or(false)),
            );
        }
        SettingKind::Scroll => {
            println!(
                "scroll: speed {}, inertia {}, spl {}",
                get("speed").and_then(|x| x.as_u64()).unwrap_or(0),
                get("inertia").and_then(|x| x.as_u64()).unwrap_or(0),
                get("spl").and_then(|x| x.as_u64()).unwrap_or(0),
            );
        }
    }
}

fn cmd_settings(
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
    if !matches!(iface.role, hid::Role::Mouse8k | hid::Role::Mouse4k) {
        eprintln!("error: settings requires a mouse interface; use `keychronctl list`");
        return exit_unsupported();
    }
    let result = mouse::query_8k_snapshot(dev, verbose)
        .map(|s| serde_json::to_value(&s).unwrap_or_default())
        .map_err(|e| e.to_string());
    match result {
        Ok(v) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            } else {
                print_settings_human(&v);
            }
            EXIT_OK
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit_protocol()
        }
    }
}

fn print_settings_human(v: &serde_json::Value) {
    let get_u = |k: &str| v.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    println!("profile:    {} of {}", get_u("profile") + 1, get_u("profile_count"));
    let dpi_levels: Vec<u64> = v.get("dpi_values").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64()).collect()).unwrap_or_default();
    let cur = v.get("dpi_levels").and_then(|x| x.as_array()).and_then(|a| a.first()).and_then(|x| x.as_u64()).unwrap_or(0);
    println!("dpi:        stage {} ({} DPI)", cur, dpi_levels.get(cur as usize).copied().unwrap_or(0));
    for (i, d) in dpi_levels.iter().take(get_u("dpi_level_num") as usize).enumerate() {
        let marker = if i as u64 == cur { "*" } else { " " };
        println!(" {marker} stage {i}: {d}");
    }
    let rates: Vec<u64> = v.get("rate_table").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64()).collect()).unwrap_or_default();
    let rcur = v.get("rate_levels").and_then(|x| x.as_array()).and_then(|a| a.first()).and_then(|x| x.as_u64()).unwrap_or(0);
    let hz = rates.get(rcur as usize).and_then(|&l| mouse::POLLING_RATE_HZ.get(l as usize)).copied();
    println!("rate:       level {} ({} Hz)", rcur, hz.map(|h| h.to_string()).unwrap_or_else(|| "?".into()));
    let sleep = get_u("sleep_minutes");
    println!("sleep:      {}", if sleep == 255 { "never".to_string() } else { format!("{sleep} min") });
    println!("debounce:   {} ms", get_u("debounce"));
    println!("battery:    {}% {}", get_u("battery_percent"), if v.get("charging").and_then(|x| x.as_bool()).unwrap_or(false) { "(charging)" } else { "" });
    if let Some(sys) = v.get("sys") {
        let b = |k: &str| sys.get(k).and_then(|x| x.as_bool()).unwrap_or(false);
        println!(
            "sensor:     lod {}, wave {}, line {}, motion {}, scrollDir {}, fps20k {}",
            sys.get("lod").and_then(|x| x.as_u64()).unwrap_or(0),
            if b("wave") { "on" } else { "off" },
            if b("line") { "on" } else { "off" },
            if b("motion") { "on" } else { "off" },
            if b("scroll_dir") { "reverse" } else { "normal" },
            if b("fps20k") { "on" } else { "off" },
        );
    }
    if let Some(sc) = v.get("scroll") {
        println!(
            "scroll:     speed {}, inertia {}, spl {}",
            sc.get("speed").and_then(|x| x.as_u64()).unwrap_or(0),
            sc.get("inertia").and_then(|x| x.as_u64()).unwrap_or(0),
            sc.get("spl").and_then(|x| x.as_u64()).unwrap_or(0),
        );
    }
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

/// Resolve the mouse device context: interface, HID opener, paired mouse
/// vpid, and its definition (for key names/indexes).
struct MouseCtx {
    iface: hid::InterfaceInfo,
    path: std::ffi::CString,
    vpid: u32,
    def: definition::Definition,
}

fn mouse_context(api: &HidApi, cli: &Cli, spec: Option<&str>, interfaces: &[hid::InterfaceInfo]) -> Result<MouseCtx, ExitCode> {
    let iface = match select_interface(spec, interfaces) {
        Ok(i) => i.clone(),
        Err(code) => return Err(code),
    };
    if iface.role != hid::Role::Mouse8k && iface.role != hid::Role::Mouse4k {
        eprintln!("error: selected device is not a mouse interface ({}); use `keychronctl list`", iface.role.as_str());
        return Err(exit_unsupported());
    }
    let path = match std::ffi::CString::new(iface.path.as_str()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: invalid device path: {e}");
            return Err(exit_runtime());
        }
    };
    // Paired mouse vpid from the bond list.
    let vpid = match api.open_path(&path) {
        Ok(dev) => match mouse::query_8k_bond_list(dev, false) {
            Ok(bonded) => bonded.iter().find(|b| b.active).map(|b| b.vpid),
            Err(_) => None,
        },
        Err(e) => {
            eprintln!("error: cannot open {}: {e}", iface.path);
            return Err(exit_runtime());
        }
    };
    let Some(vpid) = vpid else {
        eprintln!("error: no paired mouse found on the receiver");
        return Err(exit_no_device());
    };
    let def = match definition::Definition::load(vpid, &load_opts(cli)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: cannot load definition for vpid {vpid}: {e}");
            eprintln!("hint: `keychronctl definitions fetch --vpid {vpid}`");
            return Err(exit_runtime());
        }
    };
    Ok(MouseCtx { iface, path, vpid, def })
}

fn cmd_binds(cmd: &BindsCmd, api: &HidApi, cli: &Cli, interfaces: &[hid::InterfaceInfo]) -> ExitCode {
    let ctx = match mouse_context(api, cli, cli.device.as_deref(), interfaces) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let verbose = cli.verbose;
    let json = cli.json;

    match cmd {        BindsCmd::List { profile } => {
            let (current, count) = match api.open_path(&ctx.path).ok().map(|d| mouse::query_8k_profile(d, verbose)) {
                Some(Ok(p)) => (p.current, p.count),
                _ => {
                    eprintln!("error: cannot read profile info");
                    return exit_protocol();
                }
            };
            let profiles: Vec<u8> = match profile {
                Some(p) => vec![*p],
                None => (0..count).collect(),
            };

            let key_name = |index: u8| -> String {
                match &ctx.def {
                    definition::Definition::Mouse(m) => m
                        .keys
                        .iter()
                        .find(|k| k.index == index)
                        .and_then(|k| k.id.clone())
                        .unwrap_or_default(),
                    _ => String::new(),
                }
            };
            let def_indexes: Vec<u8> = match &ctx.def {
                definition::Definition::Mouse(m) => m.keys.iter().map(|k| k.index).collect(),
                _ => vec![],
            };

            let mut out = serde_json::Map::new();
            let mut binds_by_profile: Vec<serde_json::Value> = Vec::new();
            for p in profiles {
                if p >= count {
                    eprintln!("error: profile {p} does not exist (device has {count})");
                    return exit_unsupported();
                }
                if p != current {
                    if let Err(e) = api.open_path(&ctx.path).map(|d| mouse::set_8k_profile(d, p, count, verbose)) {
                        eprintln!("error: cannot switch to profile {p}: {e:?}");
                        return exit_protocol();
                    }
                }
                let bindings = collect_bindings(api, &ctx.path, &def_indexes, verbose);
                if p != current {
                    // Restore the previously active profile.
                    let _ = api.open_path(&ctx.path).map(|d| mouse::set_8k_profile(d, current, count, verbose));
                }
                let annotated: Vec<serde_json::Value> = bindings
                    .iter()
                    .map(|b| {
                        let mut o = serde_json::to_value(b).unwrap_or_default();
                        if let serde_json::Value::Object(map) = &mut o {
                            let name = key_name(b.index);
                            if !name.is_empty() {
                                map.insert("key".into(), name.into());
                            }
                        }
                        o
                    })
                    .collect();
                binds_by_profile.push(serde_json::json!({ "profile": p, "bindings": annotated }));
            }
            out.insert("vpid".into(), ctx.vpid.into());
            out.insert("product".into(), ctx.def.name().into());
            out.insert("current_profile".into(), current.into());
            out.insert("profile_count".into(), count.into());
            out.insert("profiles".into(), serde_json::Value::Array(binds_by_profile));
            if json {
                println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(out)).unwrap_or_default());
            } else {
                print_binds_human(&serde_json::Value::Object(out));
            }
            EXIT_OK
        }
        BindsCmd::Set { index, action, profile } => {
            // Validate the button index against the device definition — the
            // dongle ACKs any index, even nonexistent buttons.
            if let definition::Definition::Mouse(m) = &ctx.def {
                if !m.keys.iter().any(|k| k.index == *index) {
                    let valid: Vec<u8> = m.keys.iter().map(|k| k.index).collect();
                    eprintln!("error: button index {index} is not a device button (valid: {valid:?}; see `binds list`)");
                    return exit_usage();
                }
            }

            let (ftype, data) = match parse_funkey_action(action) {
                Ok(x) => x,
                Err(e) => {
                    eprintln!("error: {e}");
                    return exit_usage();
                }
            };
            let (current, count) = match api.open_path(&ctx.path).ok().map(|d| mouse::query_8k_profile(d, verbose)) {
                Some(Ok(p)) => (p.current, p.count),
                _ => (0, 1),
            };
            let target = profile.unwrap_or(current);
            if target >= count {
                eprintln!("error: profile {target} does not exist (device has {count})");
                return exit_unsupported();
            }
            if target != current {
                if let Err(e) = api.open_path(&ctx.path).map(|d| mouse::set_8k_profile(d, target, count, verbose)) {
                    eprintln!("error: cannot switch to profile {target}: {e:?}");
                    return exit_protocol();
                }
            }
            let result = api
                .open_path(&ctx.path)
                .map_err(|e| e.to_string())
                .and_then(|d| mouse::set_8k_funkey(d, *index, ftype, &data, verbose).map_err(|e| e.to_string()));
            if target != current {
                let _ = api.open_path(&ctx.path).map(|d| mouse::set_8k_profile(d, current, count, verbose));
            }
            match result {
                Ok(()) => {
                    if !json {
                        println!("set button {index} -> {} on profile {target}", action);
                    } else {
                        println!("{{\"index\": {index}, \"action\": {}, \"profile\": {target}}}",
                            serde_json::to_string(action).unwrap_or_default());
                    }
                    EXIT_OK
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    exit_protocol()
                }
            }
        }
    }
}

fn collect_bindings(api: &HidApi, path: &std::ffi::CString, def_indexes: &[u8], verbose: bool) -> Vec<mouse::FunKeyBinding> {
    let opener = || -> Result<hidapi::HidDevice, String> { api.open_path(path).map_err(|e| e.to_string()) };
    match mouse::query_8k_funkeys(
        || opener().map_err(|e| transceiver::ProtocolError { message: e }),
        def_indexes,
        verbose,
    ) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("warning: fun-key query failed: {e}");
            Vec::new()
        }
    }
}

/// Parse a `binds set` action spec into (type, data).
fn parse_funkey_action(spec: &str) -> Result<(u8, Vec<u8>), String> {
    match spec {
        "remove" => return Ok((mouse::FK_REMOVE, vec![])),
        "disable" => return Ok((mouse::FK_DISABLE, vec![])),
        _ => {}
    }
    let Some((kind, arg)) = spec.split_once(':') else {
        return Err(format!("unknown action '{spec}' (expected kind:value, remove, or disable)"));
    };
    let be24 = |v: u32| vec![(v >> 16) as u8, (v >> 8) as u8, v as u8];
    match kind {
        "mouse" => {
            let v = basic_key_by_name(arg)?;
            Ok((mouse::FK_MOUSE, be24(v)))
        }
        "media" => {
            let v = media_key_by_name(arg)?;
            // numToHighLow default: 2 bytes, LowToHigh.
            Ok((mouse::FK_MEDIA, vec![v as u8, (v >> 8) as u8]))
        }
        "dpi" => {
            let v = loop_key_by_name(arg)?;
            Ok((mouse::FK_DPI, vec![v, 0]))
        }
        "pollingRate" => {
            let v = loop_key_by_name(arg)?;
            Ok((mouse::FK_POLLING_RATE, vec![v, 0]))
        }
        "profile" => {
            let v: u8 = arg.parse().map_err(|_| format!("invalid profile index '{arg}'"))?;
            Ok((mouse::FK_PROFILE, vec![v]))
        }
        "shortcut" => {
            let v = shortcut_key_by_name(arg)?;
            Ok((mouse::FK_SHORTCUT, be24(v)))
        }
        "light" => {
            let v: u8 = arg.parse().map_err(|_| format!("invalid light value '{arg}'"))?;
            Ok((mouse::FK_LIGHT, vec![v]))
        }
        "keyboard" => {
            // keyboard:ctrl+shift+a — modifiers OR'd into byte 0, then up to
            // two keycodes (low byte; launcher truncates to 8 bits).
            let mut mask = 0u8;
            let mut keys: Vec<u8> = Vec::new();
            for token in arg.split('+') {
                let t = token.trim();
                if let Some(bit) = modifier_bit_by_name(t) {
                    mask |= bit;
                } else if let Some(code) = crate::r#macro::keycode_by_name(&t.to_uppercase()) {
                    keys.push((code & 0xFF) as u8);
                } else {
                    return Err(format!("unknown keyboard token '{t}'"));
                }
            }
            if keys.len() > 2 {
                return Err("keyboard bindings support at most 2 keys".into());
            }
            while keys.len() < 2 {
                keys.push(0);
            }
            let mut data = vec![mask];
            data.extend_from_slice(&keys);
            Ok((mouse::FK_KEYBOARD, data))
        }
        _ => Err(format!("unknown action kind '{kind}'")),
    }
}

fn basic_key_by_name(name: &str) -> Result<u32, String> {
    const TABLE: &[(&str, u32)] = &[
        ("left", 0x010000), ("right", 0x020000), ("middle", 0x040000),
        ("forward", 0x100000), ("backward", 0x080000), ("leftDouble", 0x800000),
        ("upScroll", 0x000200), ("downScroll", 0x00FE00), ("leftScroll", 0x0000FE),
        ("rightScroll", 0x000002), ("leftScrollSustain", 0x4000FF), ("rightScrollSustain", 0x400001),
    ];
    if let Some((_, v)) = TABLE.iter().find(|(n, _)| *n == name) {
        return Ok(*v);
    }
    if let Some(hex) = name.strip_prefix("0x") {
        return u32::from_str_radix(hex, 16).map_err(|_| format!("invalid hex value '{name}'"));
    }
    Err(format!("unknown mouse action '{name}'"))
}

fn media_key_by_name(name: &str) -> Result<u16, String> {
    const TABLE: &[(&str, u16)] = &[
        ("volumeUp", 233), ("volumeDown", 234), ("mute", 226), ("player", 387),
        ("pause", 205), ("previous", 182), ("next", 181), ("stop", 183),
    ];
    if let Some((_, v)) = TABLE.iter().find(|(n, _)| *n == name) {
        return Ok(*v);
    }
    name.parse::<u16>().map_err(|_| format!("unknown media key '{name}'"))
}

fn loop_key_by_name(name: &str) -> Result<u8, String> {
    match name {
        "loop" | "1" => Ok(1),
        "next" | "2" => Ok(2),
        "prev" | "3" => Ok(3),
        _ => Err(format!("unknown loop action '{name}' (loop|next|prev)")),
    }
}

fn shortcut_key_by_name(name: &str) -> Result<u32, String> {
    const TABLE: &[(&str, u32)] = &[
        ("brightnessUp", 814848), ("brightnessDown", 815104), ("calculate", 823809),
        ("myComputer", 824321), ("homePage", 795394), ("email", 821761),
        ("refresh", 458814), ("switchApp", 459819), ("task", 827138),
        ("launchPad", 827394), ("home", 460807), ("screenshot", 461334),
        ("copy", 459014), ("cut", 459035), ("paste", 459033),
        ("screenshotMac", 461345), ("copyMac", 460806), ("cutMac", 460827),
        ("pasteMac", 460825), ("webLessen", 459053), ("webMagnify", 459054),
        ("webLessenMac", 460845), ("webMagnifyMac", 460846),
    ];
    if let Some((_, v)) = TABLE.iter().find(|(n, _)| *n == name) {
        return Ok(*v);
    }
    if let Some(hex) = name.strip_prefix("0x") {
        return u32::from_str_radix(hex, 16).map_err(|_| format!("invalid hex value '{name}'"));
    }
    Err(format!("unknown shortcut '{name}'"))
}

fn modifier_bit_by_name(name: &str) -> Option<u8> {
    match name.to_ascii_lowercase().as_str() {
        "ctrl" | "ctrlL" => Some(1),
        "shift" | "shiftL" => Some(2),
        "alt" | "altL" => Some(4),
        "win" | "winL" => Some(8),
        "ctrlR" => Some(16),
        "shiftR" => Some(32),
        "altR" => Some(64),
        "winR" => Some(128),
        _ => None,
    }
}

fn print_binds_human(value: &serde_json::Value) {
    let obj = value.as_object().unwrap();
    let get = |k: &str| obj.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
    println!("product:    {} (vpid {})", obj.get("product").and_then(|v| v.as_str()).unwrap_or("-"), get("vpid"));
    println!("profiles:   {} of {} active", get("current_profile") + 1, get("profile_count"));
    if let Some(serde_json::Value::Array(profiles)) = obj.get("profiles") {
        for p in profiles {
            let po = p.as_object().unwrap();
            let pidx = po.get("profile").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("\nprofile {}:", pidx + 1);
            if let Some(serde_json::Value::Array(bindings)) = po.get("bindings") {
                if bindings.is_empty() {
                    println!("  (no custom bindings)");
                }
                for b in bindings {
                    let bo = b.as_object().unwrap();
                    let index = bo.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                    let key = bo.get("key").and_then(|v| v.as_str()).unwrap_or("");
                    let action = bo.get("action").and_then(|v| v.as_str()).unwrap_or("?");
                    let value = bo.get("value").and_then(|v| v.as_str()).unwrap_or("");
                    let value = if value.is_empty() { String::new() } else { format!(":{value}") };
                    println!("  [{index}] {key:<10} {action}{value}");
                }
            }
        }
    }
}

fn cmd_profile(cmd: &ProfileCmd, api: &HidApi, json: bool, verbose: bool, spec: Option<&str>, interfaces: &[hid::InterfaceInfo]) -> ExitCode {
    let iface = match select_interface(spec, interfaces) {
        Ok(i) => i.clone(),
        Err(code) => return code,
    };
    let path = match std::ffi::CString::new(iface.path.as_str()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: invalid device path: {e}");
            return exit_runtime();
        }
    };

    let read_profile = || -> Result<(u8, u8), String> {
        let dev = api.open_path(&path).map_err(|e| e.to_string())?;
        let p = mouse::query_8k_profile(dev, verbose).map_err(|e| e.to_string())?;
        Ok((p.current, p.count))
    };

    match cmd {
        ProfileCmd::List => match read_profile() {
            Ok((current, count)) => {
                if json {
                    println!("{{\"current\": {current}, \"count\": {count}}}");
                } else {
                    println!("current: {current} of 0..{}", count.saturating_sub(1));
                    for i in 0..count {
                        let marker = if i == current { "*" } else { " " };
                        println!(" {marker} profile {i}");
                    }
                }
                EXIT_OK
            }
            Err(e) => {
                eprintln!("error: {e}");
                exit_protocol()
            }
        },
        ProfileCmd::Select { index } => match read_profile() {
            Ok((current, count)) => {
                if *index >= count {
                    eprintln!("error: profile {index} does not exist (device has {count})");
                    return exit_unsupported();
                }
                if *index == current {
                    if !json {
                        println!("already on profile {index}");
                    }
                    return EXIT_OK;
                }
                match api.open_path(&path).map(|d| mouse::set_8k_profile(d, *index, count, verbose)) {
                    Ok(Ok(())) => {
                        if !json {
                            println!("switched to profile {index}");
                        }
                        EXIT_OK
                    }
                    Ok(Err(e)) => {
                        eprintln!("error: {e}");
                        exit_protocol()
                    }
                    Err(e) => {
                        eprintln!("error: {e}");
                        exit_runtime()
                    }
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                exit_protocol()
            }
        },
        ProfileCmd::Create => match read_profile() {
            Ok((current, count)) => {
                if count >= 5 {
                    eprintln!("error: profile limit reached ({count})");
                    return exit_unsupported();
                }
                let new_count = count + 1;
                match api.open_path(&path).map(|d| mouse::set_8k_profile(d, current, new_count, verbose)) {
                    Ok(Ok(())) => {
                        if !json {
                            println!("created profile {new_count} (total {new_count})");
                        }
                        EXIT_OK
                    }
                    Ok(Err(e)) => {
                        eprintln!("error: {e}");
                        exit_protocol()
                    }
                    Err(e) => {
                        eprintln!("error: {e}");
                        exit_runtime()
                    }
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                exit_protocol()
            }
        },
        ProfileCmd::Remove => match read_profile() {
            Ok((current, count)) => {
                if count <= 1 {
                    eprintln!("error: cannot remove the last profile");
                    return exit_unsupported();
                }
                let new_count = count - 1;
                let target = if current >= new_count { 0 } else { current };
                match api.open_path(&path).map(|d| mouse::set_8k_profile(d, target, new_count, verbose)) {
                    Ok(Ok(())) => {
                        if !json {
                            println!("removed profile; {new_count} remain (active: {target})");
                        }
                        EXIT_OK
                    }
                    Ok(Err(e)) => {
                        eprintln!("error: {e}");
                        exit_protocol()
                    }
                    Err(e) => {
                        eprintln!("error: {e}");
                        exit_runtime()
                    }
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                exit_protocol()
            }
        },
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
                println!("dpi:        stage {} of {}{}", level, levels.len(),
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
