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
