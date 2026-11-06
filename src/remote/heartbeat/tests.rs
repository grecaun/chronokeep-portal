use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

/// One request the fake Remote API received.
struct Received {
    request_line: String,
    authorization: String,
    body: serde_json::Value,
}

/// A pretend Remote API that answers every request with `status` and `body`
/// and reports each request on the returned channel.
fn fake_remote(status: u16, body: &'static str) -> (String, mpsc::Receiver<Received>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let (mut length, mut authorization) = (0usize, String::new());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                if let Some(v) = lower.strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
                if lower.starts_with("authorization:") {
                    authorization = line["authorization:".len()..].trim().to_string();
                }
            }
            let mut raw = vec![0u8; length];
            reader.read_exact(&mut raw).unwrap();
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = tx.send(Received {
                request_line: request_line.trim().to_string(),
                authorization,
                body: serde_json::from_slice(&raw).unwrap_or(serde_json::Value::Null),
            });
        }
    });
    (url, rx)
}

fn api(kind: &str, url: &str, heartbeat: bool, device_name: &str) -> Api {
    Api::new(1, String::from("remote"), String::from(kind), String::from(" key-1 "), String::from(url), true, heartbeat, String::from(device_name))
}

fn beat() -> HeartbeatState {
    HeartbeatState {
        dev: String::from("Portal 1"),
        location: String::from("Portal 1"),
        now: 1,
        uptime: 2,
        battery: String::from("4.10"),
        charging: true,
        hash: String::from("Chronokeep Portal v0"),
        upload_errors: 0,
        readers_connected: 1,
        readers_total: 2,
        reading: true,
        chip_type: String::from("DEC"),
    }
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder().timeout(HTTP_TIMEOUT).build().unwrap()
}

#[test]
fn send_posts_heartbeat() {
    let (url, rx) = fake_remote(200, "");
    // No trailing slash on the saved URL; one is added.
    send(&client(), &api(api::API_TYPE_CHRONOKEEP_REMOTE, &url, true, ""), &beat()).unwrap();
    let got = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(got.request_line, "POST /heartbeat HTTP/1.1");
    assert_eq!(got.authorization, "Bearer key-1", "token is trimmed");
    assert_eq!(got.body["dev"], "Portal 1");
    assert_eq!(got.body["battery"], "4.10");
    assert_eq!(got.body["charging"], true);
    assert_eq!(got.body["readers_total"], 2);
}

#[test]
fn send_reports_server_message() {
    let (url, _rx) = fake_remote(401, r#"{"message":"Delete Keys Cannot Write"}"#);
    let err = send(&client(), &api(api::API_TYPE_CHRONOKEEP_REMOTE, &url, true, ""), &beat()).unwrap_err();
    assert!(err.contains("401") && err.contains("Delete Keys Cannot Write"), "{err}");
    let (url, _rx) = fake_remote(500, "not json");
    let err = send(&client(), &api(api::API_TYPE_CHRONOKEEP_REMOTE, &url, true, ""), &beat()).unwrap_err();
    assert!(err.contains("500"), "{err}");
}

#[test]
fn send_to_unreachable_api_fails() {
    let url = format!("http://{}/", TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap());
    assert!(send(&client(), &api(api::API_TYPE_CHRONOKEEP_REMOTE, &url, true, ""), &beat()).is_err());
}

#[test]
fn charging_uses_raw_battery_value() {
    assert!(!is_charging(0));
    assert!(!is_charging(100));
    assert!(!is_charging(110));
    assert!(is_charging(111));
    assert!(is_charging(255));
}

#[test]
fn only_remote_apis_with_heartbeat() {
    assert!(is_heartbeat_api(&api(api::API_TYPE_CHRONOKEEP_REMOTE, "u", true, "")));
    assert!(is_heartbeat_api(&api(api::API_TYPE_CHRONOKEEP_REMOTE_SELF, "u", true, "")));
    assert!(!is_heartbeat_api(&api(api::API_TYPE_CHRONOKEEP_REMOTE, "u", false, "")));
    assert!(!is_heartbeat_api(&api("SOMETHING_ELSE", "u", true, "")));
}

#[test]
fn device_name_per_api() {
    let apis = vec![
        api(api::API_TYPE_CHRONOKEEP_REMOTE, "a", true, "  Finish Line "),
        api(api::API_TYPE_CHRONOKEEP_REMOTE, "b", true, ""),
        api(api::API_TYPE_CHRONOKEEP_REMOTE, "c", true, "   "),
    ];
    let beats = with_device_names(beat(), apis);
    let names: Vec<_> = beats.iter().map(|(_, hb)| (hb.dev.as_str(), hb.location.as_str())).collect();
    assert_eq!(names, [("Finish Line", "Finish Line"), ("Portal 1", "Portal 1"), ("Portal 1", "Portal 1")]);
}

#[test]
fn sends_right_away_then_stops_promptly() {
    let signal = Arc::new(StopSignal::default());
    let (built_tx, built_rx) = mpsc::channel();
    let t_signal = signal.clone();
    let t = thread::spawn(move || run_loop(&t_signal, INTERVAL, move || {
        let _ = built_tx.send(());
        Vec::new()
    }));
    built_rx.recv_timeout(Duration::from_secs(1)).expect("first heartbeat built right away");
    let begin = Instant::now();
    signal.stop();
    t.join().unwrap();
    assert!(begin.elapsed() < Duration::from_secs(1), "stop took {:?}", begin.elapsed());
    assert!(built_rx.try_recv().is_err(), "only one heartbeat before the next interval");
}

#[test]
fn sends_every_interval_until_stopped() {
    let (url, rx) = fake_remote(200, "");
    let a = api(api::API_TYPE_CHRONOKEEP_REMOTE, &url, true, "");
    let signal = Arc::new(StopSignal::default());
    let t_signal = signal.clone();
    let t = thread::spawn(move || run_loop(&t_signal, Duration::from_millis(100), move || vec![(a.clone(), beat())]));
    for _ in 0..3 {
        let got = rx.recv_timeout(Duration::from_secs(5)).expect("heartbeat sent");
        assert_eq!(got.request_line, "POST /heartbeat HTTP/1.1");
    }
    signal.stop();
    t.join().unwrap();
    while rx.try_recv().is_ok() {}
    thread::sleep(Duration::from_millis(300));
    assert!(rx.try_recv().is_err(), "heartbeat sent after stop");
}

#[test]
fn stop_signal_wait() {
    let signal = StopSignal::default();
    let begin = Instant::now();
    assert!(!signal.wait_until(begin + Duration::from_millis(50)), "times out when not stopped");
    assert!(begin.elapsed() >= Duration::from_millis(50));
    assert!(!signal.wait_until(begin), "a past deadline returns at once");
    signal.stop();
    assert!(signal.is_stopped());
    assert!(signal.wait_until(Instant::now() + Duration::from_secs(60)), "returns at once once stopped");
}
