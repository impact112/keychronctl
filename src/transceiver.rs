//! Lockstep HID transceiver.
//!
//! Mirrors the launcher's "Serial" transceiver (docs/PROTOCOL.md §2.2):
//! one outstanding command; the ack is any input report whose payload
//! matches the expected echo; 1 s timeout, up to 3 retries.

use hidapi::HidDevice;
use std::time::{Duration, Instant};

pub const DEFAULT_TIMEOUT_MS: i32 = 1000;
pub const DEFAULT_RETRIES: u32 = 3;

#[derive(Debug)]
pub struct ProtocolError {
    pub message: String,
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

pub struct Transceiver {
    device: HidDevice,
    pub report_id: u8,
    pub payload_len: usize,
    pub timeout: Duration,
    pub retries: u32,
    pub verbose: bool,
}

impl Transceiver {
    pub fn new(device: HidDevice, report_id: u8, payload_len: usize) -> Self {
        Transceiver {
            device,
            report_id,
            payload_len,
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS as u64),
            retries: DEFAULT_RETRIES,
            verbose: false,
        }
    }

    /// Send `payload` and wait for an input report whose first payload byte
    /// equals `expect`. Returns the full response payload.
    pub fn request(&self, payload: &[u8], expect: u8) -> Result<Vec<u8>, ProtocolError> {
        let start = Instant::now();
        for attempt in 0..=self.retries {
            self.write(payload)?;
            match self.read_expect(expect, self.timeout) {
                Ok(resp) => return Ok(resp),
                Err(ProtocolError { message })
                    if message.contains("timeout") && attempt < self.retries =>
                {
                    if self.verbose {
                        eprintln!("[trace] timeout on 0x{expect:02X}, retry {}/{}", attempt + 1, self.retries);
                    }
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Err(ProtocolError {
            message: format!(
                "no response for command 0x{expect:02X} after {} retries ({} ms elapsed)",
                self.retries,
                start.elapsed().as_millis()
            ),
        })
    }

    /// Fire-and-forget write.
    pub fn write(&self, payload: &[u8]) -> Result<(), ProtocolError> {
        let mut buf = vec![0u8; 1 + self.payload_len];
        buf[0] = self.report_id;
        buf[1..1 + payload.len().min(self.payload_len)].copy_from_slice(payload);
        if self.verbose {
            eprintln!("[trace] -> report {} {}", self.report_id, hex(&buf[1..]));
        }
        match self.device.write(&buf) {
            Ok(n) if n == buf.len() => Ok(()),
            Ok(n) => Err(ProtocolError {
                message: format!("short write: {n} of {} bytes", buf.len()),
            }),
            Err(e) => Err(ProtocolError {
                message: format!("HID write failed: {e}"),
            }),
        }
    }

    /// 1k-receiver request path (docs/PROTOCOL.md §2.2, `_Y` transceiver):
    /// send a feature report, wait 200 ms, then read the feature report.
    /// The response payload has the report-ID byte stripped.
    pub fn feature_request(&self, payload: &[u8], expect: u8) -> Result<Vec<u8>, ProtocolError> {
        let mut buf = vec![0u8; 1 + self.payload_len];
        buf[0] = self.report_id;
        buf[1..1 + payload.len().min(self.payload_len)].copy_from_slice(payload);
        if self.verbose {
            eprintln!("[trace] -> feature {} {}", self.report_id, hex(&buf[1..]));
        }

        for attempt in 0..=self.retries {
            match self.device.send_feature_report(&buf) {
                Ok(_) => {}
                Err(e) => {
                    return Err(ProtocolError { message: format!("feature write failed: {e}") });
                }
            }
            std::thread::sleep(Duration::from_millis(200));
            let mut raw = vec![0u8; 1 + self.payload_len];
            raw[0] = self.report_id;
            let n = match self.device.get_feature_report(&mut raw) {
                Ok(n) => {
                    if n > 1 {
                        let resp = &raw[1..n];
                        if self.verbose {
                            eprintln!("[trace] <- feature {} {}", self.report_id, hex(resp));
                        }
                        if resp.first() == Some(&expect) {
                            return Ok(resp.to_vec());
                        }
                        if self.verbose {
                            eprintln!("[trace]   (unmatched feature response, waiting for 0x{expect:02X})");
                        }
                    }
                }
                Err(e) => {
                    if self.verbose {
                        eprintln!("[trace]   feature read failed: {e}");
                    }
                }
            };
            let _ = n;
            if attempt < self.retries && self.verbose {
                eprintln!("[trace] feature request retry {}/{}", attempt + 1, self.retries);
            }
        }
        Err(ProtocolError {
            message: format!("no feature-report response for command 0x{expect:02X}"),
        })
    }

    /// Read once, returning the payload (report-id byte stripped when numbered).
    pub fn read_expect(&self, expect: u8, timeout: Duration) -> Result<Vec<u8>, ProtocolError> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ProtocolError { message: "timeout".into() });
            }
            let mut raw = [0u8; 256];
            let n = self
                .device
                .read_timeout(&mut raw, remaining.as_millis() as i32)
                .map_err(|e| ProtocolError { message: format!("HID read failed: {e}") })?;
            if n == 0 {
                return Err(ProtocolError { message: "timeout".into() });
            }
            // Numbered-report devices (report_id != 0): hidraw read() includes the
            // IN report ID as byte 0, which may differ from our OUT report ID
            // (e.g. 8k mice: request on 181, response on 182). Strip it.
            let payload: &[u8] = if self.report_id != 0 {
                &raw[1..n]
            } else {
                &raw[..n]
            };
            if self.verbose {
                eprintln!("[trace] <- report {} {}", self.report_id, hex(payload));
            }
            if payload.first() == Some(&expect) {
                return Ok(payload.to_vec());
            }
            // Not the report we're waiting for (e.g. async notification); keep reading.
            if self.verbose {
                eprintln!("[trace]   (unmatched report, waiting for 0x{expect:02X})");
            }
        }
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}
