mod hid;
mod keyboard;
mod mouse;
mod transceiver;

use clap::{Parser, Subcommand};
use hidapi::HidApi;
use std::process::ExitCode;

// Exit codes (docs/CLI.md); constructed at runtime since ExitCode::from is not const.
fn exit_runtime() -> ExitCode { ExitCode::from(1) }
fn exit_no_device() -> ExitCode { ExitCode::from(3) }
fn exit_protocol() -> ExitCode { ExitCode::from(4) }
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

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected Keychron HID interfaces
    List,
    /// Show device info (protocol, firmware, versions)
    Info,
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

    match cli.command {
        Command::List => cmd_list(cli.json, &interfaces),
        Command::Info => cmd_info(&api, cli.json, cli.verbose, cli.device.as_deref(), &interfaces),
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
    match open().and_then(|d| mouse::query_8k_bond_list(d, verbose).map_err(|e| e.to_string())) {
        Ok(bonded) => {
            report.insert("bonded".into(), serde_json::to_value(&bonded).unwrap_or_default());
        }
        Err(e) => {
            report.insert("bond_error".into(), e.into());
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
    ];
    for (key, label) in rows {
        match obj.get(key) {
            Some(serde_json::Value::String(s)) => println!("{label:<12}{s}"),
            Some(serde_json::Value::Number(n)) => println!("{label:<12}{n}"),
            _ => {}
        }
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
