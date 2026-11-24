//! The settings page on http://localhost:<web_port> (same page and API as host/webui.py).

use std::io::Read;
use std::path::PathBuf;
use std::thread;

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::config::{self, Channel, Config};
use crate::protocol::{self, KEY_IDS};
use crate::status::Shared;
use crate::{drive, padsettings};

/// The page is shared with the Python app and built into the executable.
const PAGE: &str = include_str!("../../host/web/index.html");
const LABEL_MAX: usize = 10;
const APPS_MAX: usize = 20;
const TARGET_MAX: usize = 500;
const BODY_MAX: usize = 200_000;

#[derive(Clone)]
pub struct WebState {
    pub config_path: PathBuf,
    pub status: Shared,
}

pub fn start(state: WebState, port: u16) -> Result<(thread::JoinHandle<()>, u16), String> {
    let server = Server::http(("127.0.0.1", port)).map_err(|e| format!("settings page on port {port}: {e}"))?;
    let actual = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(port);
    let handle = thread::Builder::new()
        .name("web".into())
        .spawn(move || {
            for request in server.incoming_requests() {
                let state = state.clone();
                // One short-lived thread per request keeps a slow client from blocking the page.
                let _ = thread::Builder::new().name("web-req".into()).spawn(move || handle(&state, request));
            }
        })
        .map_err(|e| e.to_string())?;
    Ok((handle, actual))
}

fn header(req: &Request, name: &str) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
}

fn respond(req: Request, code: u16, body: Vec<u8>, ctype: &str) {
    let mut resp = Response::from_data(body).with_status_code(code);
    for (k, v) in [("Content-Type", ctype), ("Cache-Control", "no-store"), ("X-Content-Type-Options", "nosniff")] {
        resp.add_header(Header::from_bytes(k.as_bytes(), v.as_bytes()).expect("valid header"));
    }
    let _ = req.respond(resp);
}

fn respond_json(req: Request, code: u16, data: &Value) {
    respond(req, code, data.to_string().into_bytes(), "application/json");
}

/// Only answer requests addressed to this machine (blocks DNS-rebinding tricks).
fn host_ok(req: &Request) -> bool {
    let host = header(req, "Host").unwrap_or_default();
    let name = match host.rsplit_once(':') {
        Some((n, port)) if port.chars().all(|c| c.is_ascii_digit()) => n.to_string(),
        _ => host,
    };
    matches!(name.as_str(), "localhost" | "127.0.0.1" | "[::1]")
}

fn status_json(state: &WebState, cfg: &Config) -> Value {
    let drive = drive::find(&cfg.pad_drive);
    json!({
        "status": state.status.lock().to_json(),
        "padDrive": drive.map(|d| d.display().to_string()),
    })
}

pub fn config_to_json(cfg: &Config) -> Value {
    let actions: serde_json::Map<String, Value> =
        KEY_IDS.iter().map(|k| (k.to_string(), json!(cfg.pc_action(k).unwrap_or("")))).collect();
    json!({
        "port": cfg.port,
        "stepPercent": cfg.step_percent,
        "channels": cfg.channels.iter().map(|c| json!({"label": c.label, "apps": c.apps})).collect::<Vec<_>>(),
        "pcActions": actions,
        "padDrive": cfg.pad_drive,
        "webPort": cfg.web_port,
    })
}

/// Validates the page's host settings; Err is a message for the page.
pub fn json_to_config(d: &Value, base: &Config) -> Result<Config, String> {
    let d = d.as_object().ok_or("host settings missing")?;
    let step = match d.get("stepPercent") {
        None => base.step_percent,
        Some(v) => v.as_f64().ok_or("volume step must be a number")?,
    };
    if !(0.5..=20.0).contains(&step) {
        return Err("volume step must be between 0.5 and 20 %".into());
    }
    let chans = d.get("channels").and_then(Value::as_array).filter(|c| c.len() == 4).ok_or("expected 4 app knobs")?;
    let mut channels = Vec::new();
    for (i, c) in chans.iter().enumerate() {
        let label = c.get("label").and_then(Value::as_str).unwrap_or("").trim().to_string();
        let printable = label.chars().all(|ch| (' '..='~').contains(&ch));
        if label.is_empty() || label.chars().count() > LABEL_MAX || !printable {
            return Err(format!("knob {}: label must be 1-{LABEL_MAX} plain characters", i + 1));
        }
        let apps = c.get("apps").and_then(Value::as_array).cloned().unwrap_or_default();
        if apps.len() > APPS_MAX {
            return Err(format!("knob {}: too many apps", i + 1));
        }
        let apps: Vec<String> = apps
            .iter()
            .filter_map(|a| a.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect();
        channels.push(Channel { label, apps });
    }
    let mut pc_actions = Vec::new();
    if let Some(actions) = d.get("pcActions").and_then(Value::as_object) {
        for key in KEY_IDS {
            let target = actions.get(key).and_then(Value::as_str).unwrap_or("").trim();
            if !target.is_empty() {
                if target.chars().count() > TARGET_MAX {
                    return Err(format!("key {key}: the PC action is too long"));
                }
                pc_actions.push((key.to_string(), target.to_string()));
            }
        }
    }
    let text = |name: &str, fallback: &str| -> String {
        let v = d.get(name).and_then(Value::as_str).unwrap_or(fallback).trim();
        if v.is_empty() { "auto".into() } else { v.to_string() }
    };
    Ok(Config {
        port: text("port", &base.port),
        step_percent: step,
        channels,
        pc_actions,
        web_port: base.web_port,
        pad_drive: text("padDrive", &base.pad_drive),
    })
}

fn read_body(req: &mut Request) -> Result<Value, String> {
    if req.body_length().unwrap_or(0) > BODY_MAX {
        return Err("request too large".into());
    }
    let mut body = String::new();
    req.as_reader().take(BODY_MAX as u64 + 1).read_to_string(&mut body).map_err(|e| e.to_string())?;
    if body.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&body).map_err(|e| format!("bad JSON: {e}"))
}

fn handle(state: &WebState, mut req: Request) {
    if !host_ok(&req) {
        return respond(req, 403, b"forbidden".to_vec(), "text/plain");
    }
    let path = req.url().split('?').next().unwrap_or("").to_string();
    let cfg = match config::load(&state.config_path) {
        Ok(c) => c,
        Err(e) => return respond_json(req, 500, &json!({"error": format!("config.toml: {e}")})),
    };
    match (req.method().clone(), path.as_str()) {
        (Method::Get, "/" | "/index.html") => respond(req, 200, PAGE.as_bytes().to_vec(), "text/html; charset=utf-8"),
        (Method::Get, "/api/status") => {
            let data = status_json(state, &cfg);
            respond_json(req, 200, &data)
        }
        (Method::Get, "/api/state") => {
            let mut data = status_json(state, &cfg);
            let drive = drive::find(&cfg.pad_drive);
            data["host"] = config_to_json(&cfg);
            data["pad"] = drive::read_settings(drive.as_deref());
            data["defaults"] = padsettings::defaults();
            data["options"] = padsettings::options_for_ui();
            data["keyIds"] = json!(KEY_IDS);
            respond_json(req, 200, &data)
        }
        (Method::Post, _) if header(&req, "X-Mixpad").as_deref() != Some("1") => {
            // A custom header can't be sent cross-site without a CORS preflight, which is never allowed.
            respond(req, 403, b"forbidden".to_vec(), "text/plain")
        }
        (Method::Post, "/api/identify") => match read_body(&mut req) {
            Err(e) => respond_json(req, 400, &json!({"error": e})),
            Ok(body) => {
                let on = body.get("on").and_then(Value::as_bool).unwrap_or(false);
                {
                    let mut st = state.status.lock();
                    st.set_identify(on);
                    st.outbox.push(protocol::quiet_line(on));
                }
                respond_json(req, 200, &json!({"ok": true, "identifying": on}))
            }
        },
        (Method::Post, "/api/save") => {
            let parsed = read_body(&mut req).and_then(|body| {
                let new_cfg = json_to_config(body.get("host").unwrap_or(&Value::Null), &cfg)?;
                let pad = body.get("pad").filter(|p| p.is_object()).cloned().ok_or("pad settings missing")?;
                Ok((new_cfg, pad))
            });
            let (new_cfg, pad) = match parsed {
                Ok(v) => v,
                Err(e) => return respond_json(req, 400, &json!({"error": e})),
            };
            if let Err(e) = config::save(&state.config_path, &new_cfg) {
                return respond_json(req, 500, &json!({"error": format!("could not save config.toml: {e}")}));
            }
            let drive = drive::find(&new_cfg.pad_drive);
            let (written, pad_error) = match &drive {
                None => (false, None),
                Some(d) => match drive::write_settings(d, &pad) {
                    Ok(()) => (true, None),
                    Err(e) => (false, Some(format!("could not write to {}: {e}", d.display()))),
                },
            };
            respond_json(req, 200, &json!({
                "ok": true,
                "padWritten": written,
                "padError": pad_error,
                "padDrive": drive.map(|d| d.display().to_string()),
                "pad": padsettings::normalise(&pad),
            }))
        }
        _ => respond(req, 404, b"not found".to_vec(), "text/plain"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;

    fn http(port: u16, method: &str, path: &str, body: Option<&Value>, extra: &[(&str, &str)]) -> (u16, String) {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let body = body.map(|b| b.to_string()).unwrap_or_default();
        let mut req = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\nContent-Length: {}\r\n", body.len());
        if !extra.iter().any(|(k, _)| *k == "Host") {
            req += &format!("Host: localhost:{port}\r\n");
        }
        for (k, v) in extra {
            req += &format!("{k}: {v}\r\n");
        }
        req += "\r\n";
        req += &body;
        s.write_all(req.as_bytes()).unwrap();
        let mut reader = BufReader::new(s);
        let mut first = String::new();
        reader.read_line(&mut first).unwrap();
        let code: u16 = first.split_whitespace().nth(1).unwrap().parse().unwrap();
        let mut rest = String::new();
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap() > 0 {
            rest.push_str(&line);
            line.clear();
        }
        let body = rest.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
        (code, body)
    }

    #[test]
    fn state_save_identify_and_guards() {
        let dir = std::env::temp_dir().join(format!("mixpad-web-{}", std::process::id()));
        let pad = dir.join("CIRCUITPY");
        std::fs::create_dir_all(&pad).unwrap();
        std::fs::write(pad.join("code.py"), "").unwrap();
        let cfg_path = dir.join("config.toml");
        let mut cfg = Config::default();
        cfg.pad_drive = pad.display().to_string();
        config::save(&cfg_path, &cfg).unwrap();

        let status = Shared::default();
        let (_h, port) = start(WebState { config_path: cfg_path.clone(), status: status.clone() }, 0).unwrap();

        let (code, page) = http(port, "GET", "/", None, &[]);
        assert_eq!(code, 200);
        assert!(page.contains("Pico Mixpad"));

        let (code, body) = http(port, "GET", "/api/state", None, &[]);
        assert_eq!(code, 200);
        let mut state: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(state["padDrive"], json!(pad.display().to_string()));
        assert_eq!(state["pad"], padsettings::defaults());

        state["host"]["channels"][0] = json!({"label": "Game", "apps": ["cs2.exe"]});
        state["host"]["pcActions"]["2"] = json!("notepad.exe");
        state["pad"]["keys"][1] = json!({"type": "pc"});
        state["pad"]["screen"]["rotation"] = json!(0);
        let save = json!({"host": state["host"], "pad": state["pad"]});
        let auth = [("X-Mixpad", "1"), ("Content-Type", "application/json")];
        let (code, body) = http(port, "POST", "/api/save", Some(&save), &auth);
        assert_eq!(code, 200, "{body}");
        let res: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(res["padWritten"], true);
        let written: Value = serde_json::from_str(&std::fs::read_to_string(pad.join("settings.json")).unwrap()).unwrap();
        assert_eq!(written["keys"][1], json!({"type": "pc"}));
        assert_eq!(written["screen"]["rotation"], 0);
        let saved = config::load(&cfg_path).unwrap();
        assert_eq!(saved.channels[0], Channel { label: "Game".into(), apps: vec!["cs2.exe".into()] });
        assert_eq!(saved.pc_action("2"), Some("notepad.exe"));

        let mut bad = save.clone();
        bad["host"]["channels"].as_array_mut().unwrap().pop();
        let (code, body) = http(port, "POST", "/api/save", Some(&bad), &auth);
        assert_eq!(code, 400);
        assert!(body.contains("4 app knobs"));

        let (code, _) = http(port, "POST", "/api/identify", Some(&json!({"on": true})), &auth);
        assert_eq!(code, 200);
        assert_eq!(status.lock().outbox, vec!["QUIET 1\n".to_string()]);
        let (_, body) = http(port, "GET", "/api/status", None, &[]);
        assert!(body.contains("\"identifying\":true"));

        let (code, _) = http(port, "POST", "/api/save", Some(&save), &[("Content-Type", "application/json")]);
        assert_eq!(code, 403, "no X-Mixpad header");
        let (code, _) = http(port, "GET", "/api/state", None, &[("Host", "evil.example")]);
        assert_eq!(code, 403, "not addressed to localhost");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
