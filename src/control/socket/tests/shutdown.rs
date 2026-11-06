//! Waiting for threads to stop during shutdown.

use std::{sync::{mpsc, Arc, Mutex}, thread, time::{Duration, Instant}};

use crate::control::socket::join_all;

#[test]
fn join_all_waits_for_every_thread() {
    let done = Arc::new(Mutex::new(0));
    let handles = (0..3).map(|i| {
        let done = done.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(20 * i));
            *done.lock().unwrap() += 1;
        })
    }).collect();
    let joiners = Arc::new(Mutex::new(handles));
    assert!(join_all(&joiners, Duration::from_secs(5)));
    assert_eq!(*done.lock().unwrap(), 3);
    assert!(joiners.lock().unwrap().is_empty());
}

#[test]
fn join_all_with_nothing_to_wait_for() {
    let joiners = Arc::new(Mutex::new(Vec::new()));
    let begin = Instant::now();
    assert!(join_all(&joiners, Duration::from_secs(5)));
    assert!(begin.elapsed() < Duration::from_secs(1));
}

#[test]
fn join_all_gives_up_after_the_limit() {
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let stuck = thread::spawn(move || { _ = release_rx.recv(); });
    let joiners = Arc::new(Mutex::new(vec![stuck]));
    let begin = Instant::now();
    assert!(!join_all(&joiners, Duration::from_millis(200)));
    let waited = begin.elapsed();
    assert!(waited >= Duration::from_millis(200) && waited < Duration::from_secs(2), "waited {waited:?}");
    drop(release_tx); // let the stuck thread finish
}

#[test]
fn join_all_includes_threads_added_while_waiting() {
    let joiners: Arc<Mutex<Vec<thread::JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
    let finished = Arc::new(Mutex::new(false));
    let (t_joiners, t_finished) = (joiners.clone(), finished.clone());
    // Like a reader reconnecting during shutdown: one thread starts another.
    let first = thread::spawn(move || {
        let late = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            *t_finished.lock().unwrap() = true;
        });
        t_joiners.lock().unwrap().push(late);
    });
    joiners.lock().unwrap().push(first);
    assert!(join_all(&joiners, Duration::from_secs(5)));
    assert!(*finished.lock().unwrap(), "thread added during the wait was joined");
}
