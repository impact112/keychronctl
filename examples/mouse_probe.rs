//! Live probe for the 8k mouse path on a Keychron Link-KM dongle.
//!
//! Opens the dongle's mouse interface (default /dev/hidraw3) and runs the
//! 8k queries: version (cmd 2, report 181), bond list (cmd 3, report 181),
//! battery (cmd 6, report 179) and DPI (cmd 6, report 179).
//!
//! Run with: cargo run --example mouse_probe [-- <hidraw path>]

#![allow(dead_code)] // includes the whole mouse module; only part is used here

#[path = "../src/mouse.rs"]
mod mouse;
#[path = "../src/transceiver.rs"]
mod transceiver;

use hidapi::HidApi;
use std::ffi::CString;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/dev/hidraw3".to_string());
    let api = HidApi::new().expect("hidapi init failed");
    let cpath = CString::new(path.clone()).expect("device path contains NUL");

    // Fresh handle per query, mirroring the lockstep transceiver usage in
    // main.rs (one outstanding command per open device).
    let open = || -> hidapi::HidDevice {
        match api.open_path(&cpath) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("error: cannot open {path}: {e}");
                std::process::exit(1);
            }
        }
    };

    println!("== 8k version (cmd 2, report 181)");
    match mouse::query_8k_version(open(), true) {
        Ok(info) => println!("{}", serde_json::to_string_pretty(&info).unwrap()),
        Err(e) => println!("error: {e}\n"),
    }

    println!("== 8k bond list (cmd 3, report 181)");
    match mouse::query_8k_bond_list(open(), true) {
        Ok(list) => println!("{}", serde_json::to_string_pretty(&list).unwrap()),
        Err(e) => println!("error: {e}\n"),
    }

    println!("== 8k power (cmd 6, report 179)");
    match mouse::query_8k_power(open(), true) {
        Ok(p) => println!("{}", serde_json::to_string_pretty(&p).unwrap()),
        Err(e) => println!("error: {e}\n"),
    }

    println!("== 8k dpi (cmd 6, report 179)");
    match mouse::query_8k_dpi(open(), true) {
        Ok(d) => println!("{}", serde_json::to_string_pretty(&d).unwrap()),
        Err(e) => println!("error: {e}\n"),
    }
}
