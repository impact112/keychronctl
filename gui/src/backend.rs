//! Backend QObject: runs an external CLI binary (default `keychronctl`,
//! overridable via the `KEYCHRONCTL` env var) as a subprocess and returns
//! raw JSON to QML.
//!
//! Fully decoupled contract (docs/GUI-CONTRACT.md):
//!   `<binary> --json <subcommand...>` → JSON on stdout, exit codes
//!   0 ok / 1 runtime / 2 usage / 3 no device / 4 protocol-timeout /
//!   5 unsupported; errors on stderr as `error: <message>` lines.
//!
//! The GUI has no compile-time knowledge of the binary's schema: the raw
//! stdout JSON string is handed to QML, which parses it with JSON.parse().

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use std::pin::Pin;
use std::process::Command;
use std::sync::atomic::AtomicI32;

/// Resolve the backend binary path at runtime:
/// `KEYCHRONCTL` env var → `keychronctl` on PATH.
fn resolve_binary() -> Result<String, String> {
    if let Ok(p) = std::env::var("KEYCHRONCTL") {
        if !p.is_empty() {
            return Ok(p);
        }
    }
    // Probe PATH so the error is friendly instead of a spawn failure later.
    let probe = Command::new("keychronctl")
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match probe {
        Ok(s) if s.success() => Ok("keychronctl".to_string()),
        _ => Err(
            "keychronctl not found. Install it and put it on PATH, or set the KEYCHRONCTL env var to its path."
                .to_string(),
        ),
    }
}

#[derive(Default)]
pub struct BackendRust {
    /// Monotonic request id counter for command_finished correlation.
    next_request_id: i32,
    /// QProperty backing fields (cxx-qt 0.10 stores properties on the Rust
    /// struct).
    pub busy: bool,
    pub binary_path: QString,
    pub last_error: QString,
}

#[cxx_qt::bridge(namespace = "keychron_launcher")]
mod backend {
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        /// Exit-code constants mirrored from the contract.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, busy)]
        #[qproperty(QString, binary_path)]
        #[qproperty(QString, last_error)]
        type Backend = super::BackendRust;

        /// Run `<binary> --json <command> [args...]` off-thread.
        /// Emits command_finished(request_id, ok, json, stderr_text, exit_code).
        #[qinvokable]
        fn run(self: Pin<&mut Backend>, command: QString, args: QStringList, request_id: i32);

        /// Allocate a fresh request id.
        #[qinvokable]
        fn new_request_id(self: Pin<&mut Backend>) -> i32;

        /// Re-resolve the backend binary (e.g. after install).
        #[qinvokable]
        fn refresh_binary_path(self: Pin<&mut Backend>);

        #[qsignal]
        fn command_finished(
            self: Pin<&mut Backend>,
            request_id: i32,
            ok: bool,
            json: QString,
            stderr_text: QString,
            exit_code: i32,
        );
    }

    impl cxx_qt::Threading for Backend {}
}

impl backend::Backend {
    fn new_request_id(mut self: Pin<&mut Self>) -> i32 {
        let next = self.as_mut().rust_mut().next_request_id + 1;
        self.as_mut().rust_mut().next_request_id = next;
        next
    }

    fn refresh_binary_path(mut self: Pin<&mut Self>) {
        match resolve_binary() {
            Ok(p) => self.as_mut().set_binary_path(QString::from(&p)),
            Err(e) => {
                self.as_mut().set_binary_path(QString::from(""));
                self.as_mut().set_last_error(QString::from(&e));
            }
        }
    }

    fn run(mut self: Pin<&mut Self>, command: QString, args: QStringList, request_id: i32) {
        let qt_thread = self.qt_thread();
        let command = command.to_string();
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();

        std::thread::spawn(move || {
            let (stdout, stderr, code) = match resolve_binary() {
                Ok(bin) => {
                    let mut cmd = Command::new(&bin);
                    cmd.arg("--json").arg(&command);
                    cmd.args(&args);
                    match cmd.output() {
                        Ok(out) => (
                            String::from_utf8_lossy(&out.stdout).to_string(),
                            String::from_utf8_lossy(&out.stderr).to_string(),
                            out.status.code().unwrap_or(-1),
                        ),
                        Err(e) => (String::new(), format!("error: cannot run backend: {e}"), -1),
                    }
                }
                Err(e) => (String::new(), format!("error: {e}"), -1),
            };

            // Exit code 0 with parseable stdout = ok; everything else is an
            // error even if stdout carries partial data.
            let ok = code == 0 && !stdout.trim().is_empty();
            qt_thread
                .queue(move |mut b| {
                    b.as_mut().set_busy(false);
                    b.as_mut().set_last_error(QString::from(&stderr));
                    b.command_finished(
                        request_id,
                        ok,
                        QString::from(&stdout),
                        QString::from(&stderr),
                        code,
                    );
                })
                .ok();
        });

        self.as_mut().set_busy(true);
    }
}

// Keep the atomic import referenced even if unused in some cfg.
#[allow(dead_code)]
static _UNUSED: AtomicI32 = AtomicI32::new(0);
