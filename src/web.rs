/*
Chronokeep Portal - RFID Tag Reading Software
Copyright (C) 2026 James Sentinella

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Built-in web page for controlling the portal from a browser.
//!
//! * `GET /`               the page (one self-contained HTML file, works offline)
//! * `GET /api/info`       portal name, version, platform and uptime
//! * `GET /api/time`       this computer's clock (Unix milliseconds)
//! * `GET /api/events`     Server-Sent Events: every control socket message
//!                         (`readers`, `settings`, `api_list`, `reads`, ...) as it happens
//! * `GET /api/reads`      most recently saved reads (`?limit=N`, default 200)
//! * `POST /api/command`   a control socket request as JSON; replies with JSON
//!
//! The page talks to the portal through its own control socket over the
//! loopback address, the same way Chronokeep desktop does. Changes made here
//! go through the same code and are sent to every connected desktop client.
//!
//! Environment variables:
//! * `PORTAL_WEB_PORT`     port to serve the page on (default 8484, `0` or `off` to disable)
//! * `PORTAL_WEB_ADDRESS`  address to listen on (default 0.0.0.0, any address)

use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::control;
use crate::database::{sqlite, Database};

pub const DEFAULT_PORT: u16 = 8484;
pub const WEB_PORT_ENV: &str = "PORTAL_WEB_PORT";
pub const WEB_ADDRESS_ENV: &str = "PORTAL_WEB_ADDRESS";

const PAGE: &str = include_str!("web/index.html");
const MAX_BODY: u64 = 1024 * 1024;
const EVENT_PING: Duration = Duration::from_secs(15);
/// How long a command may take before the page is told it's still running.
const COMMAND_DEADLINE: Duration = Duration::from_secs(30);
/// Commands sent to the control socket at the same time.
const MAX_COMMANDS: usize = 2;

/// Requests the page may send. Connection management (`connect`, `subscribe`, ...)
/// is handled here, and `set_noficiation` is only meant for the portal itself.
const ALLOWED_COMMANDS: &[&str] = &[
    "api_list", "api_save", "api_remove", "api_remote_manual_upload", "api_remote_auto_upload",
    "reader_list", "reader_get_all", "reader_add", "reader_remove", "reader_connect", "reader_disconnect",
    "reader_start", "reader_stop", "reader_start_all", "reader_stop_all",
    "reads_delete_all",
    "settings_get", "settings_get_all", "settings_set",
    "time_get", "time_set",
    "quit", "shutdown", "restart", "update",
];

struct Hub {
    control_port: u16,
    keepalive: Arc<Mutex<bool>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    started: Instant,
    subscribers: Mutex<Vec<(u64, mpsc::Sender<String>)>>,
    next_id: AtomicU64,
    bridge_connected: AtomicBool,
    running_commands: (Mutex<usize>, Condvar),
}

fn running(keepalive: &Arc<Mutex<bool>>) -> bool {
    keepalive.lock().map(|ka| *ka).unwrap_or(false)
}

fn now_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// Starts the web page if it's enabled. Called once the control socket is listening.
pub fn start(control_port: u16, keepalive: Arc<Mutex<bool>>, sqlite: Arc<Mutex<sqlite::SQLite>>) {
    let port = match std::env::var(WEB_PORT_ENV) {
        Ok(v) if v.trim().eq_ignore_ascii_case("off") || v.trim() == "0" => {
            println!("Web page disabled ({WEB_PORT_ENV}={v}).");
            return;
        }
        Ok(v) => match v.trim().parse::<u16>() {
            Ok(p) => p,
            Err(_) => {
                println!("Invalid {WEB_PORT_ENV} value '{v}', using {DEFAULT_PORT}.");
                DEFAULT_PORT
            }
        },
        Err(_) => DEFAULT_PORT,
    };
    let address: IpAddr = match std::env::var(WEB_ADDRESS_ENV) {
        Ok(v) => match v.trim().parse() {
            Ok(a) => a,
            Err(_) => {
                println!("Invalid {WEB_ADDRESS_ENV} value '{v}', using 0.0.0.0.");
                IpAddr::V4(Ipv4Addr::UNSPECIFIED)
            }
        },
        Err(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
    };
    let server = match Server::http((address, port)) {
        Ok(s) => s,
        Err(e) => {
            println!("Unable to start web page on {address}:{port}: {e}");
            return;
        }
    };
    println!("Web page available at http://{address}:{port}/");
    let hub = Arc::new(Hub {
        control_port,
        keepalive,
        sqlite,
        started: Instant::now(),
        subscribers: Mutex::new(Vec::new()),
        next_id: AtomicU64::new(1),
        bridge_connected: AtomicBool::new(false),
        running_commands: (Mutex::new(0), Condvar::new()),
    });
    let b_hub = hub.clone();
    thread::spawn(move || bridge_loop(b_hub));
    thread::spawn(move || {
        for req in server.incoming_requests() {
            let hub = hub.clone();
            thread::spawn(move || handle(req, &hub));
        }
    });
}

// ------------------------------------------------------------------ events

impl Hub {
    fn control_address(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.control_port)
    }

    fn subscribe(&self) -> (u64, mpsc::Receiver<String>) {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        // Tell the new page whether the portal connection is up.
        _ = tx.send(bridge_message(self.bridge_connected.load(Ordering::SeqCst)));
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.push((id, tx));
        }
        (id, rx)
    }

    fn unsubscribe(&self, id: u64) {
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.retain(|(i, _)| *i != id);
        }
    }

    fn broadcast(&self, msg: &str) {
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.retain(|(_, tx)| tx.send(msg.to_string()).is_ok());
        }
    }

    fn set_bridge(&self, connected: bool) {
        if self.bridge_connected.swap(connected, Ordering::SeqCst) != connected {
            self.broadcast(&bridge_message(connected));
        }
    }
}

fn bridge_message(connected: bool) -> String {
    json!({ "command": "web_bridge", "connected": connected }).to_string()
}

/// Keeps one connection open to the control socket, subscribed to reads, and
/// passes everything it receives on to the open pages.
fn bridge_loop(hub: Arc<Hub>) {
    while running(&hub.keepalive) {
        match TcpStream::connect_timeout(&hub.control_address(), Duration::from_secs(3)) {
            Ok(stream) => {
                run_bridge(&hub, stream);
                hub.set_bridge(false);
            }
            Err(e) => println!("Web: unable to connect to the control socket: {e}"),
        }
        for _ in 0..4 {
            if !running(&hub.keepalive) {
                return;
            }
            thread::sleep(Duration::from_millis(500));
        }
    }
}

fn run_bridge(hub: &Hub, mut stream: TcpStream) {
    _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    if stream.write_all(b"{\"command\":\"connect\",\"reads\":true}\n").is_err() {
        return;
    }
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 16384];
    loop {
        if !running(&hub.keepalive) {
            _ = stream.write_all(b"{\"command\":\"disconnect\"}\n");
            break;
        }
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(_) => break,
        }
        for (text, value) in take_messages(&mut buffer) {
            match value.get("command").and_then(Value::as_str) {
                Some("keepalive") => {
                    if stream.write_all(b"{\"command\":\"keepalive_ack\"}\n").is_err() {
                        return;
                    }
                }
                Some("disconnect") => return,
                Some(_) => {
                    hub.set_bridge(true);
                    hub.broadcast(&text);
                }
                None => {}
            }
        }
    }
    _ = stream.shutdown(Shutdown::Both);
}

// ---------------------------------------------------------------- commands

fn error_reply(kind: &str, message: &str) -> String {
    json!({ "command": "error", "error": { "error_type": kind, "message": message } }).to_string()
}

/// Checks setting values before they're saved. The portal reads some of them
/// back as small numbers and won't start if they don't fit.
fn validate_settings(cmd: &Value) -> Result<(), String> {
    let Some(list) = cmd.get("settings").and_then(Value::as_array) else {
        return Err(String::from("settings missing"));
    };
    for s in list {
        let name = s.get("name").and_then(Value::as_str).unwrap_or("");
        let value = s.get("value").and_then(Value::as_str).unwrap_or("");
        let ok = match name {
            control::SETTING_PORTAL_NAME => !value.trim().is_empty(),
            control::SETTING_READ_WINDOW => value.parse::<u8>().map(|v| v >= 1).unwrap_or(false),
            // Any number; the portal snaps it into 0 to 600 seconds.
            control::SETTING_BEEP_IGNORE => value.trim().parse::<f64>().map(|v| v.is_finite()).unwrap_or(false),
            control::SETTING_UPLOAD_INTERVAL => value.parse::<u64>().map(|v| v >= 1).unwrap_or(false),
            control::SETTING_VOLUME => value.parse::<f32>().map(|v| (0.0..=1.0).contains(&v)).unwrap_or(false),
            control::SETTING_PLAY_SOUND | control::SETTING_ENABLE_NTFY => value == "true" || value == "false",
            control::SETTING_CHIP_TYPE => value == "DEC" || value == "HEX",
            control::SETTING_VOICE => matches!(value, "emily" | "michael" | "custom"),
            control::SETTING_SCREEN_TYPE => matches!(value, "ADAFRUIT" | "PCF8574T"),
            control::SETTING_NTFY_URL | control::SETTING_NTFY_USER | control::SETTING_NTFY_PASS
            | control::SETTING_NTFY_TOPIC => true,
            _ => return Err(format!("'{name}' is not a setting the web page can change")),
        };
        if !ok {
            return Err(format!("'{value}' is not a valid value for {name}"));
        }
    }
    Ok(())
}

/// Which reply carries the answer to a command (others are just passing through).
fn expected_reply(command: &str) -> &'static [&'static str] {
    match command {
        "settings_get_all" => &["settings_all"],
        "settings_get" | "settings_set" => &["settings"],
        "api_list" | "api_save" | "api_remove" => &["api_list"],
        "reader_list" | "reader_get_all" | "reader_add" | "reader_remove" | "reader_disconnect"
        | "reader_stop" | "reader_start_all" | "reader_stop_all" => &["readers"],
        "reads_delete_all" | "update" | "api_remote_manual_upload" => &["success"],
        "time_get" | "time_set" => &["time"],
        "api_remote_auto_upload" => &["read_auto_upload"],
        _ => &[],
    }
}

/// Runs one control socket request on its own short connection and returns
/// the reply. The connection gets a `disconnect` right after the request, and
/// the control socket handles requests in order, so everything it says before
/// closing the connection is about this request (plus any updates it sends to
/// every client, which are passed over).
fn web_command(hub: &Hub, body: &[u8]) -> String {
    let (command, cmd) = match check_command(body) {
        Ok(c) => c,
        Err(reply) => return reply,
    };
    // Limit how many control connections the page uses at once.
    let (lock, cvar) = &hub.running_commands;
    {
        let Ok(mut count) = lock.lock() else {
            return error_reply("SERVER_ERROR", "web page lock failed");
        };
        let wait_until = Instant::now() + Duration::from_secs(10);
        while *count >= MAX_COMMANDS {
            let left = wait_until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return error_reply("SERVER_ERROR", "the portal is busy with other requests; try again");
            }
            count = match cvar.wait_timeout(count, left) {
                Ok((c, _)) => c,
                Err(_) => return error_reply("SERVER_ERROR", "web page lock failed"),
            };
        }
        *count += 1;
    }
    let reply = run_command(hub.control_address(), &command, &cmd, COMMAND_DEADLINE);
    if let Ok(mut count) = lock.lock() {
        *count -= 1;
    }
    cvar.notify_one();
    reply
}

/// Checks a request from the page. Returns the command name and request, or the
/// error reply to send back.
fn check_command(body: &[u8]) -> Result<(String, Value), String> {
    let cmd: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => return Err(error_reply("INVALID_REQUEST", &format!("not valid JSON: {e}"))),
    };
    if !cmd.is_object() {
        return Err(error_reply("INVALID_REQUEST", "expected a JSON object"));
    }
    let Some(command) = cmd.get("command").and_then(Value::as_str).map(String::from) else {
        return Err(error_reply("INVALID_REQUEST", "command missing"));
    };
    if !ALLOWED_COMMANDS.contains(&command.as_str()) {
        return Err(error_reply("UNKNOWN_COMMAND", &format!("'{command}' can't be sent from the web page")));
    }
    if command == "settings_set" {
        if let Err(e) = validate_settings(&cmd) {
            return Err(error_reply("INVALID_SETTING", &e));
        }
    }
    Ok((command, cmd))
}

/// Takes every complete line out of `buffer` and parses it as JSON. Blank and
/// unreadable lines are skipped; a partial last line stays in the buffer.
fn take_messages(buffer: &mut Vec<u8>) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    while let Some(pos) = buffer.iter().position(|b| *b == b'\n') {
        let line: Vec<u8> = buffer.drain(..=pos).collect();
        if let Some(m) = parse_line(&line) {
            out.push(m);
        }
    }
    out
}

fn parse_line(line: &[u8]) -> Option<(String, Value)> {
    let text = String::from_utf8_lossy(line).trim().to_string();
    if text.is_empty() {
        return None;
    }
    let value = serde_json::from_str::<Value>(&text).ok()?;
    Some((text, value))
}

fn run_command(address: SocketAddr, command: &str, cmd: &Value, deadline: Duration) -> String {
    let mut stream = match TcpStream::connect_timeout(&address, Duration::from_secs(3)) {
        Ok(s) => s,
        Err(e) => return error_reply("SERVER_ERROR", &format!("unable to reach the control socket: {e}")),
    };
    _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let out = format!("{}\n{{\"command\":\"disconnect\"}}\n", cmd);
    if let Err(e) = stream.write_all(out.as_bytes()) {
        return error_reply("SERVER_ERROR", &format!("unable to send request: {e}"));
    }
    let deadline = Instant::now() + deadline;
    let expected = expected_reply(command);
    let mut answer: Option<String> = None;
    let mut error: Option<String> = None;
    let mut finished = false;
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 16384];
    while !finished && Instant::now() < deadline {
        let mut messages = Vec::new();
        match stream.read(&mut chunk) {
            Ok(0) => finished = true,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(_) => finished = true,
        }
        messages.extend(take_messages(&mut buffer));
        // The connection closed; a last message without a newline still counts.
        if finished {
            messages.extend(parse_line(&std::mem::take(&mut buffer)));
        }
        for (text, value) in messages {
            match value.get("command").and_then(Value::as_str) {
                Some("disconnect") => finished = true,
                Some("error") => {
                    if error.is_none() {
                        error = Some(text);
                    }
                }
                Some(c) if expected.contains(&c) => answer = Some(text),
                _ => {}
            }
        }
    }
    _ = stream.shutdown(Shutdown::Both);
    if let Some(e) = error {
        return e;
    }
    if let Some(a) = answer {
        return a;
    }
    if !finished {
        return json!({ "command": "pending" }).to_string();
    }
    json!({ "command": "ok" }).to_string()
}

// ------------------------------------------------------------------- reads

fn recent_reads(hub: &Hub, limit: usize) -> String {
    let reads = match hub.sqlite.lock() {
        Ok(sq) => sq.get_recent_reads(limit),
        Err(_) => return error_reply("DATABASE_ERROR", "unable to get the database"),
    };
    match reads {
        Ok(list) => {
            let list: Vec<Value> = list.iter().map(|r| json!({
                "id": r.id(),
                "identifier": r.chip(),
                "seconds": r.seconds(),
                "milliseconds": r.milliseconds(),
                "reader_seconds": r.reader_seconds(),
                "reader_milliseconds": r.reader_milliseconds(),
                "antenna": r.antenna(),
                "reader": r.reader(),
                "rssi": r.rssi(),
                "uploaded": r.uploaded() != 0,
            })).collect();
            json!({ "command": "reads", "list": list }).to_string()
        }
        Err(e) => error_reply("DATABASE_ERROR", &format!("error getting reads: {e}")),
    }
}

fn info(hub: &Hub) -> String {
    let name = hub.sqlite.lock().ok()
        .and_then(|sq| sq.get_setting(control::SETTING_PORTAL_NAME).ok())
        .map(|s| String::from(s.value()))
        .unwrap_or_default();
    json!({
        "name": name,
        "version": env!("CARGO_PKG_VERSION"),
        "platform": std::env::consts::OS,
        "can_set_time": std::env::consts::OS == "linux",
        "uptime_seconds": hub.started.elapsed().as_secs(),
        "time_ms": now_millis() as u64,
        "connected": hub.bridge_connected.load(Ordering::SeqCst),
    }).to_string()
}

// -------------------------------------------------------------------- http

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn json_response(body: String) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body)
        .with_header(header("Content-Type", "application/json"))
        .with_header(header("Cache-Control", "no-store"))
}

fn handle(mut req: Request, hub: &Hub) {
    let url = req.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let result = match (req.method(), path) {
        (Method::Get, "/" | "/index.html") => req.respond(
            Response::from_string(PAGE)
                .with_header(header("Content-Type", "text/html; charset=utf-8"))
                .with_header(header("Cache-Control", "no-store")),
        ),
        (Method::Get, "/api/info") => req.respond(json_response(info(hub))),
        (Method::Get, "/api/time") => {
            req.respond(json_response(json!({ "time_ms": now_millis() as u64 }).to_string()))
        }
        (Method::Get, "/api/reads") => {
            let limit = query
                .split('&')
                .find_map(|kv| kv.strip_prefix("limit="))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(200)
                .clamp(1, 2000);
            req.respond(json_response(recent_reads(hub, limit)))
        }
        (Method::Get, "/api/events") => return event_stream(req, hub),
        (Method::Post, "/api/command") => {
            // Requiring a JSON content type means another website open in the same
            // browser can't send commands (the browser would need a CORS preflight,
            // which is never approved).
            let is_json = req.headers().iter().any(|h| {
                h.field.equiv("Content-Type") && h.value.as_str().to_ascii_lowercase().starts_with("application/json")
            });
            if !is_json {
                req.respond(Response::from_string("expected application/json").with_status_code(415))
            } else {
                let mut body = Vec::new();
                match req.as_reader().take(MAX_BODY).read_to_end(&mut body) {
                    Ok(_) => {
                        let reply = web_command(hub, &body);
                        req.respond(json_response(reply))
                    }
                    Err(e) => req.respond(Response::from_string(format!("unable to read request: {e}")).with_status_code(400)),
                }
            }
        }
        _ => req.respond(Response::from_string("not found").with_status_code(404)),
    };
    if let Err(e) = result {
        println!("Web: unable to send response for {path}: {e}");
    }
}

/// Streams messages to the browser until it goes away.
fn event_stream(req: Request, hub: &Hub) {
    let (id, rx) = hub.subscribe();
    let mut out = req.into_writer();
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
                Connection: close\r\nX-Accel-Buffering: no\r\n\r\n";
    let mut ok = out.write_all(head.as_bytes()).and_then(|_| out.flush()).is_ok();
    while ok {
        if !running(&hub.keepalive) {
            break;
        }
        let chunk = match rx.recv_timeout(EVENT_PING) {
            Ok(msg) => format!("data: {msg}\n\n"),
            // A comment line keeps proxies from closing the connection and
            // shows us when the browser has gone.
            Err(RecvTimeoutError::Timeout) => ": ping\n\n".to_string(),
            Err(RecvTimeoutError::Disconnected) => break,
        };
        ok = out.write_all(chunk.as_bytes()).and_then(|_| out.flush()).is_ok();
    }
    hub.unsubscribe(id);
}

#[cfg(test)]
mod tests;
