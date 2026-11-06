use std::{io::{BufRead, BufReader}, net::{TcpListener, TcpStream}, sync::{Arc, Mutex}, thread, time::{Duration, Instant}};

use super::*;
use crate::control::socket::MAX_CONNECTED;

/// A connected pair: (portal's side, the remote client's side).
fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let remote = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (portal, _) = listener.accept().unwrap();
    (portal, remote)
}

fn lines(remote: TcpStream, timeout: Duration) -> Vec<String> {
    remote.set_read_timeout(Some(timeout)).unwrap();
    BufReader::new(remote).lines().map_while(|l| l.ok()).collect()
}

#[test]
fn sends_each_message_as_one_json_line() {
    let (portal, remote) = pair();
    let client = Client::new(&portal).unwrap();
    assert!(client.send(&serde_json::json!({"command": "a", "n": 1})));
    assert!(client.send(&vec![1, 2, 3]));
    client.finish(Duration::from_secs(2));
    let got = lines(remote, Duration::from_secs(2));
    assert_eq!(got, [r#"{"command":"a","n":1}"#, "[1,2,3]"]);
}

#[test]
fn messages_from_many_threads_never_interleave() {
    // Before, two threads writing to one socket could mix their bytes together.
    let (portal, remote) = pair();
    let client = Client::new(&portal).unwrap();
    let big = "x".repeat(20_000);
    let senders: Vec<_> = (0..4).map(|t| {
        let (c, big) = (client.clone(), big.clone());
        thread::spawn(move || {
            for i in 0..25 {
                assert!(c.send(&serde_json::json!({"thread": t, "i": i, "pad": big})));
            }
        })
    }).collect();
    let reader = thread::spawn(move || lines(remote, Duration::from_secs(5)));
    for s in senders {
        s.join().unwrap();
    }
    client.finish(Duration::from_secs(5));
    let got = reader.join().unwrap();
    assert_eq!(got.len(), 100);
    for l in got {
        let v: serde_json::Value = serde_json::from_str(&l).expect("whole JSON message");
        assert_eq!(v["pad"].as_str().unwrap().len(), 20_000);
    }
}

#[test]
fn sending_never_blocks_on_a_client_that_stops_reading() {
    // The remote side never reads. Sends only queue, so they stay fast even while
    // a lock is held, and once the queue is full the client is disconnected.
    let (portal, _remote) = pair();
    let client = Client::with_queue_limit(&portal, 20).unwrap();
    let lock = Mutex::new(());
    let big = "x".repeat(200_000);
    let begin = Instant::now();
    let _held = lock.lock().unwrap();
    let mut accepted = 0;
    for _ in 0..200 {
        if !client.send(&big) {
            break;
        }
        accepted += 1;
    }
    assert!(begin.elapsed() < Duration::from_secs(2), "sending took {:?}", begin.elapsed());
    assert!(accepted < 200, "a stuck client was eventually dropped");
    assert!(!client.is_alive());
    assert!(!client.send(&"after"), "nothing more is sent to a dropped client");
}

#[test]
fn write_timeout_disconnects_a_stuck_client() {
    let (portal, _remote) = pair();
    portal.set_write_timeout(Some(Duration::from_millis(200))).unwrap();
    let client = Client::with_queue_limit(&portal, 1000).unwrap();
    let big = "x".repeat(200_000);
    // Fill the socket buffers so the writer thread's write blocks, then times out.
    for _ in 0..100 {
        if !client.send(&big) {
            break;
        }
    }
    let begin = Instant::now();
    while client.is_alive() && begin.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!client.is_alive(), "writer gave up after the write timeout");
}

#[test]
fn new_sets_the_write_timeout() {
    let (portal, _remote) = pair();
    let _client = Client::new(&portal).unwrap();
    assert_eq!(portal.write_timeout().unwrap(), Some(WRITE_TIMEOUT));
}

#[test]
fn end_closes_after_queued_messages() {
    let (portal, remote) = pair();
    let client = Client::new(&portal).unwrap();
    assert!(client.send(&"last"));
    client.end();
    // Everything queued arrives, then the connection closes (the read ends).
    assert_eq!(lines(remote, Duration::from_secs(2)), [r#""last""#]);
    let begin = Instant::now();
    while client.is_alive() && begin.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!client.is_alive());
}

#[test]
fn close_and_dead_peer() {
    let (portal, remote) = pair();
    let client = Client::new(&portal).unwrap();
    client.close();
    assert!(!client.is_alive());
    assert!(!client.send(&"x"));
    drop(remote);
}

#[test]
fn connected_copies_clients_with_their_slots() {
    let (a, _ra) = pair();
    let (b, _rb) = pair();
    let sockets: ControlSockets = Arc::new(Mutex::new(Default::default()));
    {
        let mut s = sockets.lock().unwrap();
        s[1] = Some(Client::new(&a).unwrap());
        s[MAX_CONNECTED] = Some(Client::new(&b).unwrap());
    }
    let got: Vec<usize> = connected(&sockets).into_iter().map(|(ix, _)| ix).collect();
    assert_eq!(got, [1, MAX_CONNECTED]);
    // The lock isn't held after copying.
    assert!(sockets.try_lock().is_ok());
}
