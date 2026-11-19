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

use std::fs;
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};

use super::*;
use crate::control::client::{Client, ControlSockets};
use crate::database::{sqlite::{self, tests::setup_tests}, Database};
use crate::network::api;

fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let remote = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (portal, _) = listener.accept().unwrap();
    (portal, remote)
}

/// A portal database with two Chronokeep Remote APIs saved, one of each kind.
fn with_two_remote_apis(path: &str) -> Arc<Mutex<sqlite::SQLite>> {
    let mut sq = setup_tests(path);
    for (id, kind) in [(1, api::API_TYPE_CHRONOKEEP_REMOTE), (2, api::API_TYPE_CHRONOKEEP_REMOTE_SELF)] {
        sq.save_api(&Api::new(
            0,
            format!("remote-{id}"),
            String::from(kind),
            format!("key-{id}"),
            format!("http://remote-{id}.test/"),
            true,
            true,
            String::new(),
        )).unwrap();
    }
    Arc::new(Mutex::new(sq))
}

fn notifier_for(sqlite: &Arc<Mutex<sqlite::SQLite>>, auto_remote: bool) -> Notifier {
    let mut control = Control::new(&mut sqlite.lock().unwrap()).unwrap();
    control.auto_remote = auto_remote;
    Notifier::new(Arc::new(Mutex::new(true)), Arc::new(Mutex::new(control)))
}

/// The Remote APIs (by token) a notification has been queued for.
fn queued_for(notifier: &Notifier) -> Vec<String> {
    let mut tokens: Vec<String> = notifier.api_notifications.lock().unwrap().iter()
        .map(|(api, note)| {
            assert!(matches!(note, APINotification::HighTemp), "unexpected notification {note:?}");
            String::from(api.token())
        })
        .collect();
    tokens.sort();
    tokens
}

/// Connects two clients to the control socket, runs `send`, and returns what each received.
fn clients_receive(send: impl FnOnce(&ControlSockets)) -> Vec<Vec<String>> {
    let sockets: ControlSockets = Arc::new(Mutex::new(Default::default()));
    let (a, ra) = pair();
    let (b, rb) = pair();
    let (ca, cb) = (Client::new(&a).unwrap(), Client::new(&b).unwrap());
    {
        let mut s = sockets.lock().unwrap();
        s[0] = Some(ca.clone());
        s[1] = Some(cb.clone());
    }
    send(&sockets);
    ca.finish(Duration::from_secs(2));
    cb.finish(Duration::from_secs(2));
    [ra, rb].into_iter().map(|r| {
        r.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        BufReader::new(r).lines().map_while(|l| l.ok()).collect()
    }).collect()
}

#[test]
fn remote_apis_are_every_chronokeep_remote_api() {
    let path = "./test_notifier_remote_apis.sqlite";
    let sqlite = with_two_remote_apis(path);
    let mut tokens: Vec<String> = remote_apis(&sqlite).iter().map(|a| String::from(a.token())).collect();
    tokens.sort();
    assert_eq!(tokens, ["key-1", "key-2"]);
    drop(sqlite);
    _ = fs::remove_file(path);
}

#[test]
fn device_notifications_reach_every_client_and_every_remote_api() {
    let path = "./test_notifier_device_all.sqlite";
    let sqlite = with_two_remote_apis(path);
    let notifier = notifier_for(&sqlite, true);

    let received = clients_receive(|sockets| notifier.send_device_notification(APINotification::HighTemp, sockets, &sqlite));
    for (i, lines) in received.iter().enumerate() {
        assert!(lines.iter().any(|l| l.contains("HIGH_TEMP")), "client {i} got {lines:?}");
    }
    assert_eq!(queued_for(&notifier), ["key-1", "key-2"]);
    drop(sqlite);
    _ = fs::remove_file(path);
}

#[test]
fn without_auto_upload_only_clients_are_told() {
    let path = "./test_notifier_device_local.sqlite";
    let sqlite = with_two_remote_apis(path);
    let notifier = notifier_for(&sqlite, false);

    let received = clients_receive(|sockets| notifier.send_device_notification(APINotification::HighTemp, sockets, &sqlite));
    for (i, lines) in received.iter().enumerate() {
        assert!(lines.iter().any(|l| l.contains("HIGH_TEMP")), "client {i} got {lines:?}");
    }
    assert!(queued_for(&notifier).is_empty());
    drop(sqlite);
    _ = fs::remove_file(path);
}
