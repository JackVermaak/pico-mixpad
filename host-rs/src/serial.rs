//! Finding the macropad's data port and talking to it.

use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

use serialport::{SerialPort, SerialPortType};

use crate::bridge::Link;
use crate::protocol::{self, DeviceMessage};

const KNOWN_VIDS: [u16; 2] = [0x239A, 0x2E8A]; // Adafruit (CircuitPython), Raspberry Pi
const BAUD: u32 = 115_200; // ignored by USB CDC, required by the API
pub const READ_TIMEOUT: Duration = Duration::from_millis(200);

pub struct SerialLink {
    port: Box<dyn SerialPort>,
    buf: Vec<u8>,
}

impl SerialLink {
    pub fn open(name: &str, timeout: Duration) -> serialport::Result<Self> {
        // DTR must be on: CircuitPython only counts the data port as connected (and only sends on
        // it) while DTR is set. pyserial sets it by default; serialport on Windows turns it off.
        let port = serialport::new(name, BAUD).timeout(timeout).dtr_on_open(true).open()?;
        Ok(SerialLink { port, buf: Vec::new() })
    }

    fn take_line(&mut self) -> Option<String> {
        let pos = self.buf.iter().position(|&b| b == b'\n')?;
        let line: Vec<u8> = self.buf.drain(..=pos).collect();
        Some(String::from_utf8_lossy(&line).trim().to_string())
    }
}

impl Link for SerialLink {
    fn read_line(&mut self) -> io::Result<Option<String>> {
        if let Some(line) = self.take_line() {
            return Ok(Some(line));
        }
        let mut chunk = [0u8; 256];
        match self.port.read(&mut chunk) {
            Ok(0) => Ok(None),
            Ok(n) => {
                self.buf.extend_from_slice(&chunk[..n]);
                if self.buf.len() > 4096 {
                    self.buf.clear(); // garbage without newlines; start over
                }
                Ok(self.take_line())
            }
            Err(e) if e.kind() == io::ErrorKind::TimedOut => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.port.write_all(data)?;
        self.port.flush()
    }
}

/// The firmware version if the port answers the handshake like a Pico Mixpad.
fn probe(name: &str) -> Option<String> {
    let mut link = SerialLink::open(name, Duration::from_millis(300)).ok()?;
    let _ = link.port.clear(serialport::ClearBuffer::Input);
    link.write(protocol::HANDSHAKE.as_bytes()).ok()?;
    let deadline = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < deadline {
        if let Ok(Some(line)) = link.read_line() {
            if let Some(DeviceMessage::Hello(version)) = protocol::parse_device_line(&line) {
                log::info!("found macropad firmware {version} on {name}");
                return Some(version);
            }
        }
    }
    None
}

fn describe(p: &serialport::SerialPortInfo) -> String {
    match &p.port_type {
        SerialPortType::UsbPort(u) => format!("{} (USB {:04X}:{:04X})", p.port_name, u.vid, u.pid),
        _ => p.port_name.clone(),
    }
}

/// (port, firmware version) of the macropad; Err says what was tried, for the log.
pub fn find_port(configured: &str) -> Result<(String, Option<String>), String> {
    if !configured.eq_ignore_ascii_case("auto") {
        return Ok((configured.to_string(), None));
    }
    let ports = serialport::available_ports().map_err(|e| format!("can't list serial ports: {e}"))?;
    if ports.is_empty() {
        return Err("no serial ports found: is the pad plugged in?".into());
    }
    let likely: Vec<&serialport::SerialPortInfo> = ports
        .iter()
        .filter(|p| matches!(&p.port_type, SerialPortType::UsbPort(u) if KNOWN_VIDS.contains(&u.vid)))
        .collect();
    let candidates: Vec<&serialport::SerialPortInfo> = if likely.is_empty() { ports.iter().collect() } else { likely };
    for p in &candidates {
        if let Some(v) = probe(&p.port_name) {
            return Ok((p.port_name.clone(), Some(v)));
        }
    }
    let tried: Vec<String> = candidates.iter().map(|p| describe(p)).collect();
    Err(format!("no macropad answered on {}", tried.join(", ")))
}
