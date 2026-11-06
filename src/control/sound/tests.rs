use std::{sync::mpsc, thread};

use super::*;

/// Calls take() on another thread; the receiver gets what it returned.
fn take_later(n: &Arc<SoundNotifier>) -> mpsc::Receiver<Pending> {
    let (tx, rx) = mpsc::channel();
    let n = n.clone();
    thread::spawn(move || {
        if let Some(p) = n.take() {
            _ = tx.send(p);
        }
    });
    rx
}

#[test]
fn requests_made_before_waiting_are_kept() {
    // Before, a request made while a sound was playing (not waiting) was missed.
    let n = Arc::new(SoundNotifier::new());
    n.notify_custom(SoundType::Connected);
    n.notify_one();
    n.notify_custom(SoundType::Disconnected);
    let p = take_later(&n).recv_timeout(Duration::from_secs(1)).expect("returns at once");
    assert!(p.beep);
    assert_eq!(p.sounds.len(), 2);
}

#[test]
fn waits_until_something_is_requested() {
    let n = Arc::new(SoundNotifier::new());
    let rx = take_later(&n);
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err(), "nothing to play yet");
    n.notify_custom(SoundType::Volume);
    let p = rx.recv_timeout(Duration::from_secs(1)).expect("woken by the request");
    assert!(!p.beep);
    assert_eq!(p.sounds.len(), 1);
}

#[test]
fn take_clears_what_it_returns() {
    let n = Arc::new(SoundNotifier::new());
    n.notify_one();
    assert!(take_later(&n).recv_timeout(Duration::from_secs(1)).unwrap().beep);
    // Nothing left, so the next take waits.
    assert!(take_later(&n).recv_timeout(Duration::from_millis(100)).is_err());
}
