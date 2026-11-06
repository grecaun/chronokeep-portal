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

use super::*;
use std::thread;

fn waiter() -> Arc<Waiter> {
    Arc::new((Mutex::new(WaitState::default()), std::sync::Condvar::new()))
}

#[test]
fn waits_for_the_interval_without_reads() {
    let w = waiter();
    let start = Instant::now();
    wait_for_next_upload(&w, Duration::from_millis(300));
    assert!(start.elapsed() >= Duration::from_millis(300));
}

#[test]
fn uploads_early_after_threshold_reads() {
    let w = waiter();
    let t = w.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        add_new_reads(&t, 10);
        add_new_reads(&t, UPLOAD_READ_THRESHOLD - 10);
    });
    let start = Instant::now();
    wait_for_next_upload(&w, Duration::from_secs(10));
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn fewer_reads_wait_for_the_interval() {
    let w = waiter();
    let t = w.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        add_new_reads(&t, UPLOAD_READ_THRESHOLD - 1);
    });
    let start = Instant::now();
    wait_for_next_upload(&w, Duration::from_millis(400));
    assert!(start.elapsed() >= Duration::from_millis(400));
}

#[test]
fn wake_stops_waiting() {
    let w = waiter();
    let t = w.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        let (lock, cvar) = &*t;
        lock.lock().unwrap().wake = true;
        cvar.notify_one();
    });
    let start = Instant::now();
    wait_for_next_upload(&w, Duration::from_secs(10));
    assert!(start.elapsed() < Duration::from_secs(2));
}

/// An uploader with one Remote API and no reads, so a run just waits out its upload interval.
fn uploader(path: &str) -> (Arc<Uploader>, Arc<Mutex<sqlite::SQLite>>) {
    let mut sq = crate::database::sqlite::tests::setup_tests(path);
    // The uploader stops if there's no Remote API; with no reads it's never contacted.
    sq.save_api(&api::Api::new(0, String::from("Test"), String::from(api::API_TYPE_CHRONOKEEP_REMOTE_SELF), String::from("key"), String::from("http://127.0.0.1:9/"), true, false, String::from("pc"))).unwrap();
    let control = Arc::new(Mutex::new(Control::new(&mut sq).unwrap()));
    let sqlite = Arc::new(Mutex::new(sq));
    let up = Arc::new(Uploader::new(
        Arc::new(Mutex::new(true)),
        sqlite.clone(),
        Arc::new(Mutex::new(Default::default())),
        control,
        Arc::new(Mutex::new(UploadInfo::new(Status::Unknown, 0, 0))),
    ));
    (up, sqlite)
}

/// Runs the uploader on its own thread; the receiver gets a message when that run returns.
fn start(up: &Arc<Uploader>) -> std::sync::mpsc::Receiver<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let u = up.clone();
    thread::spawn(move || {
        u.run();
        _ = tx.send(());
    });
    rx
}

fn wait_for(up: &Uploader, status: Status) {
    let begin = Instant::now();
    while up.status() != status {
        assert!(begin.elapsed() < Duration::from_secs(5), "status stuck at {:?}", up.status());
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn stop_then_quick_start_leaves_one_uploader() {
    // Before, a start right after a stop could revive the old run, leaving two.
    let path = "./test_uploader_restart.sqlite";
    let (up, sqlite) = uploader(path);
    let first = start(&up);
    wait_for(&up, Status::Running);
    up.stop();
    let second = start(&up);
    first.recv_timeout(Duration::from_secs(5)).expect("the stopped run finished");
    wait_for(&up, Status::Running);
    assert!(second.recv_timeout(Duration::from_millis(200)).is_err(), "the new run keeps going");
    up.stop();
    second.recv_timeout(Duration::from_secs(5)).expect("the new run stops");
    wait_for(&up, Status::Stopped);
    drop(up);
    drop(sqlite);
    _ = std::fs::remove_file(path);
}

#[test]
fn a_second_start_replaces_the_first() {
    let path = "./test_uploader_double_start.sqlite";
    let (up, sqlite) = uploader(path);
    let first = start(&up);
    wait_for(&up, Status::Running);
    let second = start(&up);
    first.recv_timeout(Duration::from_secs(5)).expect("the earlier run handed over");
    wait_for(&up, Status::Running);
    up.stop();
    second.recv_timeout(Duration::from_secs(5)).expect("stopped");
    wait_for(&up, Status::Stopped);
    drop(up);
    drop(sqlite);
    _ = std::fs::remove_file(path);
}

#[test]
fn stop_right_after_a_start_stops_everything() {
    // The new run may be cancelled while it waits for the old one, or start and then
    // stop; either way nothing is left running.
    let path = "./test_uploader_cancel.sqlite";
    let (up, sqlite) = uploader(path);
    let first = start(&up);
    wait_for(&up, Status::Running);
    let before = up.generation.load(Ordering::SeqCst);
    let second = start(&up);
    // Stop only once the new run has registered, so the stop comes after the start.
    let begin = Instant::now();
    while up.generation.load(Ordering::SeqCst) == before {
        assert!(begin.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(1));
    }
    up.stop();
    first.recv_timeout(Duration::from_secs(5)).expect("first finished");
    second.recv_timeout(Duration::from_secs(5)).expect("second finished");
    wait_for(&up, Status::Stopped);
    drop(up);
    drop(sqlite);
    _ = std::fs::remove_file(path);
}

#[test]
fn stop_while_a_start_is_waiting_ends_stopped() {
    // The test holds the run lock in place of an earlier run that has already
    // given up; the waiting run is then stopped and must leave the status Stopped.
    let path = "./test_uploader_waiting_stop.sqlite";
    let (up, sqlite) = uploader(path);
    let held = up.run_lock.lock().unwrap();
    let before = up.generation.load(Ordering::SeqCst);
    let waiting = start(&up);
    let begin = Instant::now();
    while up.generation.load(Ordering::SeqCst) == before {
        assert!(begin.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(1));
    }
    up.stop();
    assert_eq!(up.status(), Status::Stopping);
    drop(held);
    waiting.recv_timeout(Duration::from_secs(5)).expect("cancelled run returned");
    assert_eq!(up.status(), Status::Stopped);
    drop(up);
    drop(sqlite);
    _ = std::fs::remove_file(path);
}
