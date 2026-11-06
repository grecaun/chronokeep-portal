//! Sending to connected clients.

use std::{io::{BufRead, BufReader}, net::{TcpListener, TcpStream}, sync::{Arc, Mutex}, time::Duration};

use crate::control::{client::{Client, ControlSockets}, socket::{broadcast_reads, MAX_CONNECTED}};
use crate::objects::read;

fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let remote = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (portal, _) = listener.accept().unwrap();
    (portal, remote)
}

fn reads() -> Vec<read::Read> {
    vec![read::Read::new(1, String::from("1001"), 100, 0, 100, 0, 1, String::from("Finish"), String::from("-50"), read::READ_UPLOADED_FALSE)]
}

#[test]
fn reads_go_only_to_subscribed_clients() {
    let (a, ra) = pair();
    let (b, rb) = pair();
    let sockets: ControlSockets = Arc::new(Mutex::new(Default::default()));
    let repeaters = Arc::new(Mutex::new([false; MAX_CONNECTED]));
    let (ca, cb) = (Client::new(&a).unwrap(), Client::new(&b).unwrap());
    {
        let mut s = sockets.lock().unwrap();
        s[0] = Some(ca.clone());
        s[1] = Some(cb.clone());
    }
    repeaters.lock().unwrap()[0] = true;
    assert!(broadcast_reads(&sockets, &repeaters, &reads()));
    ca.finish(Duration::from_secs(2));
    cb.finish(Duration::from_secs(2));
    ra.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    rb.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let got_a: Vec<String> = BufReader::new(ra).lines().map_while(|l| l.ok()).collect();
    let got_b: Vec<String> = BufReader::new(rb).lines().map_while(|l| l.ok()).collect();
    assert_eq!(got_a.len(), 1);
    assert!(got_a[0].contains("1001"));
    assert!(got_b.is_empty(), "not subscribed");
}

#[test]
fn a_gone_client_is_unsubscribed_and_others_still_get_reads() {
    let (a, _ra) = pair();
    let (b, rb) = pair();
    let sockets: ControlSockets = Arc::new(Mutex::new(Default::default()));
    let repeaters = Arc::new(Mutex::new([false; MAX_CONNECTED]));
    let (ca, cb) = (Client::new(&a).unwrap(), Client::new(&b).unwrap());
    ca.close();
    {
        let mut s = sockets.lock().unwrap();
        s[0] = Some(ca);
        s[1] = Some(cb.clone());
    }
    *repeaters.lock().unwrap() = [true; MAX_CONNECTED];
    assert!(!broadcast_reads(&sockets, &repeaters, &reads()));
    assert!(!repeaters.lock().unwrap()[0], "gone client unsubscribed");
    assert!(repeaters.lock().unwrap()[1]);
    cb.finish(Duration::from_secs(2));
    rb.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    assert_eq!(BufReader::new(rb).lines().map_while(|l| l.ok()).count(), 1);
}
