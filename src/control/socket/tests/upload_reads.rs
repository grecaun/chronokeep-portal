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

//! Uploading reads to a Chronokeep Remote API.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

use crate::control::socket::upload_reads;
use crate::network::api::{self, Api};
use crate::objects::read;

/// One request the fake Remote API received.
struct Received {
    request_line: String,
    authorization: String,
    body: serde_json::Value,
}

/// A pretend Remote API that answers one request with `status` and `body`, and sends what
/// it received on the returned channel.
fn fake_remote(status: u16, body: &'static str) -> (String, mpsc::Receiver<Received>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else { return };
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
        let body_json = serde_json::from_slice(&raw).unwrap_or(serde_json::Value::Null);
        let reason = if status == 200 { "OK" } else { "Error" };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
        tx.send(Received { request_line: request_line.trim().to_string(), authorization, body: body_json }).unwrap();
    });
    (url, rx)
}

fn remote_api(url: &str, device_name: &str) -> Api {
    Api::new(
        1,
        String::from("remote"),
        String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
        String::from("write-key"),
        String::from(url),
        true,
        false,
        String::from(device_name),
    )
}

fn reads() -> Vec<read::Read> {
    vec![
        read::Read::new(0, String::from("1001"), 1_700_000_000, 250, 1_700_000_001, 500, 2, String::from("Finish"), String::from("-55"), read::READ_UPLOADED_FALSE),
        read::Read::new(0, String::from("1002"), 1_700_000_010, 0, 1_700_000_011, 0, 1, String::from("Finish"), String::from("-60"), read::READ_UPLOADED_FALSE),
    ]
}

#[test]
fn sends_the_device_name_with_the_reads() {
    let (url, rx) = fake_remote(200, r#"{"count":2}"#);
    let client = reqwest::blocking::Client::new();

    let count = upload_reads(&client, &remote_api(&url, "finish-line-1"), &reads()).unwrap();
    assert_eq!(count, 2);

    let received = rx.recv().unwrap();
    assert_eq!(received.request_line, "POST /reads/add HTTP/1.1");
    assert_eq!(received.authorization, "Bearer write-key");
    assert_eq!(received.body["dev"], "finish-line-1");
    let sent = received.body["reads"].as_array().unwrap();
    assert_eq!(sent.len(), 2);
    // the fields Chronokeep Remote reads from each read
    assert_eq!(sent[0]["identifier"], "1001");
    assert_eq!(sent[0]["seconds"], 1_700_000_000);
    assert_eq!(sent[0]["milliseconds"], 250);
    assert_eq!(sent[0]["antenna"], 2);
    assert_eq!(sent[0]["reader"], "Finish");
    assert_eq!(sent[0]["rssi"], "-55");
    assert!(sent[0]["ident_type"].is_string());
    assert!(sent[0]["type"].is_string());
    assert_eq!(sent[1]["identifier"], "1002");
}

#[test]
fn sends_an_empty_device_name_when_none_is_set() {
    let (url, rx) = fake_remote(200, r#"{"count":2}"#);
    let client = reqwest::blocking::Client::new();

    upload_reads(&client, &remote_api(&url, ""), &reads()).unwrap();
    assert_eq!(rx.recv().unwrap().body["dev"], "");
}

#[test]
fn reports_errors_from_the_api() {
    let (url, _rx) = fake_remote(400, r#"{"message":"Invalid Request Body"}"#);
    let client = reqwest::blocking::Client::new();

    assert!(upload_reads(&client, &remote_api(&url, "finish-line-1"), &reads()).is_err());
}

/// Prints the exact body sent, so the same JSON can be checked against Chronokeep Remote.
/// Run with `cargo test upload_reads::print_body -- --ignored --nocapture`.
#[test]
#[ignore]
fn print_body() {
    let (url, rx) = fake_remote(200, r#"{"count":2}"#);
    let client = reqwest::blocking::Client::new();
    upload_reads(&client, &remote_api(&url, "finish-line-1"), &reads()).unwrap();
    println!("{}", rx.recv().unwrap().body);
}
