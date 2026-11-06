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

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::Value;

use super::{upload_all_reads, upload_saved_reads, NUMBER_READS_PER_UPLOAD};
use crate::database::{sqlite::tests::setup_tests, Database};
use crate::network::api::{self, Api};
use crate::objects::read::{self, Read as ChipRead};

#[derive(Clone, Copy)]
enum Mode {
    /// Accept every read.
    Ok,
    /// Say one fewer read was saved than was sent.
    ShortCount,
    /// Answer with a server error.
    Status500,
    /// Fail only the Nth request (1-based); accept the rest.
    FailRequest(usize),
}

/// A pretend Remote API. Returns its URL and the number of reads in each
/// request it got.
fn fake_remote(mode: Mode) -> (String, Arc<Mutex<Vec<usize>>>) {
    fake_remote_with(mode, Arc::new(|| {}))
}

/// Like fake_remote, but runs `on_request` while handling each request.
fn fake_remote_with(mode: Mode, on_request: Arc<dyn Fn() + Send + Sync>) -> (String, Arc<Mutex<Vec<usize>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let batches = Arc::new(Mutex::new(Vec::new()));
    let t_batches = batches.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let batches = t_batches.clone();
            let on_request = on_request.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut out = stream;
                loop {
                    let mut length = 0usize;
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    loop {
                        line.clear();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        let l = line.trim();
                        if l.is_empty() {
                            break;
                        }
                        if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
                            length = v.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0u8; length];
                    reader.read_exact(&mut body).unwrap();
                    on_request();
                    let v: Value = serde_json::from_slice(&body).unwrap();
                    let n = v["reads"].as_array().map(|a| a.len()).unwrap_or(0);
                    let request_no = {
                        let mut b = batches.lock().unwrap();
                        b.push(n);
                        b.len()
                    };
                    let (status, count) = match mode {
                        Mode::Ok => (200, n),
                        Mode::ShortCount => (200, n.saturating_sub(1)),
                        Mode::Status500 => (500, 0),
                        Mode::FailRequest(k) if k == request_no => (500, 0),
                        Mode::FailRequest(_) => (200, n),
                    };
                    let body = format!("{{\"count\":{count}}}");
                    let reason = if status == 200 { "OK" } else { "Internal Server Error" };
                    let resp = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
                    if out.write_all(resp.as_bytes()).is_err() {
                        return;
                    }
                }
            });
        }
    });
    (url, batches)
}

fn api_for(url: &str) -> Api {
    Api::new(1, String::from("Test"), String::from(api::API_TYPE_CHRONOKEEP_REMOTE_SELF), String::from("key"), String::from(url), true, false, String::from("pc"))
}

fn reads(n: usize) -> Vec<ChipRead> {
    (0..n).map(|i| ChipRead::new(
        i as i64 + 1, format!("{}", 1000 + i), 1_700_000_000 + i as i64, 0, 1_700_000_000 + i as i64, 0, 1,
        String::from("Finish"), String::from("-50"), read::READ_UPLOADED_FALSE,
    )).collect()
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::ClientBuilder::new().timeout(Duration::from_secs(5)).build().unwrap()
}

fn run(n: usize, mode: Mode) -> (Vec<ChipRead>, usize, Vec<usize>) {
    let (url, batches) = fake_remote(mode);
    let (done, errors) = upload_all_reads(&client(), &api_for(&url), reads(n));
    let b = batches.lock().unwrap().clone();
    (done, errors, b)
}

#[test]
fn nothing_to_upload_sends_nothing() {
    let (done, errors, batches) = run(0, Mode::Ok);
    assert!(done.is_empty());
    assert_eq!(errors, 0);
    assert!(batches.is_empty());
}

#[test]
fn batch_sizes() {
    let n = NUMBER_READS_PER_UPLOAD;
    for (count, expected) in [
        (1, vec![1]),
        (n - 1, vec![n - 1]),
        (n, vec![n]),
        (n + 1, vec![n, 1]),
        (2 * n, vec![n, n]),
        (2 * n + 50, vec![n, n, 50]),
    ] {
        let (done, errors, batches) = run(count, Mode::Ok);
        assert_eq!(batches, expected, "{count} reads");
        assert_eq!(errors, 0, "{count} reads");
        assert_eq!(done.len(), count, "{count} reads");
        assert!(done.iter().all(|r| r.uploaded() == read::READ_UPLOADED_TRUE), "{count} reads");
    }
}

#[test]
fn uploaded_reads_keep_their_ids() {
    let (done, _, _) = run(3, Mode::Ok);
    let mut ids: Vec<i64> = done.iter().map(|r| r.id()).collect();
    ids.sort();
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn short_count_marks_nothing_in_that_batch() {
    let (done, errors, _) = run(5, Mode::ShortCount);
    assert!(done.is_empty());
    assert_eq!(errors, 1);
    let n = NUMBER_READS_PER_UPLOAD;
    let (done, errors, _) = run(n + 5, Mode::ShortCount);
    assert!(done.is_empty());
    assert_eq!(errors, 2);
}

#[test]
fn server_errors_are_counted() {
    let (done, errors, _) = run(5, Mode::Status500);
    assert!(done.is_empty());
    assert_eq!(errors, 1);
}

#[test]
fn one_failed_batch_does_not_stop_the_rest() {
    let n = NUMBER_READS_PER_UPLOAD;
    let (done, errors, batches) = run(2 * n + 10, Mode::FailRequest(2));
    assert_eq!(batches.len(), 3);
    assert_eq!(errors, 1);
    assert_eq!(done.len(), n + 10);
}

#[test]
fn unreachable_server() {
    let url = format!("http://{}/", TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap());
    let (done, errors) = upload_all_reads(&client(), &api_for(&url), reads(3));
    assert!(done.is_empty());
    assert_eq!(errors, 1);
}

#[test]
fn saved_reads_upload_without_holding_the_database() {
    let path = "./test_upload_saved_reads.sqlite";
    let sqlite = Arc::new(Mutex::new(setup_tests(path)));
    // While each upload request is being handled, check whether the database is locked.
    let locked = Arc::new(Mutex::new(Vec::new()));
    // Weak, so the fake server doesn't keep the database open after the test.
    let (t_sqlite, t_locked) = (Arc::downgrade(&sqlite), locked.clone());
    let (url, batches) = fake_remote_with(Mode::Ok, Arc::new(move || {
        if let Some(sq) = t_sqlite.upgrade() {
            t_locked.lock().unwrap().push(sq.try_lock().is_err());
        }
    }));
    {
        let mut sq = sqlite.lock().unwrap();
        // Id 0 adds a new API (any other id updates an existing one).
        sq.save_api(&Api::new(0, String::from("Test"), String::from(api::API_TYPE_CHRONOKEEP_REMOTE_SELF), String::from("key"), String::from(&url), true, false, String::from("pc"))).unwrap();
        // A disabled API gets nothing.
        sq.save_api(&Api::new(0, String::from("Off"), String::from(api::API_TYPE_CHRONOKEEP_REMOTE), String::from("k2"), String::from(&url), false, false, String::new())).unwrap();
        sq.save_reads(&reads(150)).unwrap();
    }
    let errors = upload_saved_reads(&client(), &sqlite);
    assert_eq!(errors, 0);
    assert_eq!(*batches.lock().unwrap(), vec![100, 50], "one enabled Remote API, in batches of 100");
    let locked = locked.lock().unwrap().clone();
    assert_eq!(locked.len(), 2);
    assert!(locked.iter().all(|l| !l), "database was locked during an upload");
    let saved = sqlite.lock().unwrap().get_all_reads().unwrap();
    assert_eq!(saved.len(), 150);
    assert!(saved.iter().all(|r| r.uploaded() == read::READ_UPLOADED_TRUE), "all reads marked uploaded");
    drop(sqlite);
    _ = std::fs::remove_file(path);
}

#[test]
fn saved_reads_with_no_remote_api_sends_nothing() {
    let path = "./test_upload_saved_reads_none.sqlite";
    let sqlite = Arc::new(Mutex::new(setup_tests(path)));
    sqlite.lock().unwrap().save_reads(&reads(5)).unwrap();
    assert_eq!(upload_saved_reads(&client(), &sqlite), 0);
    drop(sqlite);
    _ = std::fs::remove_file(path);
}
