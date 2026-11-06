use std::{fs, sync::mpsc, thread, time::Duration};

use super::*;
use crate::database::sqlite::tests::setup_tests;

fn saver(path: &str) -> (Arc<ReadSaver>, Arc<Mutex<sqlite::SQLite>>) {
    let sqlite = Arc::new(Mutex::new(setup_tests(path)));
    (Arc::new(ReadSaver::new(sqlite.clone())), sqlite)
}

fn reads(from: usize, count: usize) -> Vec<read::Read> {
    (from..from + count).map(|i| read::Read::new(
        0,
        format!("{}", 1000 + i),
        100 + i as i64,
        0,
        100 + i as i64,
        0,
        1,
        String::from("reader1"),
        String::from("-50"),
        read::READ_UPLOADED_FALSE,
    )).collect()
}

fn saved(sqlite: &Arc<Mutex<sqlite::SQLite>>) -> usize {
    sqlite.lock().unwrap().get_all_reads().unwrap().len()
}

/// Runs the saver on its own thread; the receiver gets a message when it returns.
fn run(saver: &Arc<ReadSaver>) -> mpsc::Receiver<()> {
    let (done_tx, done_rx) = mpsc::channel();
    let s = saver.clone();
    thread::spawn(move || {
        s.start();
        drop(s); // release the database before saying we're done
        _ = done_tx.send(());
    });
    done_rx
}

#[test]
fn saves_queued_reads() {
    let path = "./test_read_saver_saves.sqlite";
    let (saver, sqlite) = saver(path);
    let done = run(&saver);
    saver.save_reads(&reads(0, 3)).unwrap();
    saver.save_reads(&reads(3, 2)).unwrap();
    saver.stop();
    done.recv_timeout(Duration::from_secs(5)).expect("saver stopped");
    assert_eq!(saved(&sqlite), 5);
    assert!(!saver.running());
    // Both hold the database open; release it so the file can be deleted.
    drop(saver);
    drop(sqlite);
    _ = fs::remove_file(path);
}

#[test]
fn stop_wakes_an_idle_saver() {
    // Before, an idle saver only woke for new reads, so it never stopped.
    let path = "./test_read_saver_idle.sqlite";
    let (saver, sqlite) = saver(path);
    let done = run(&saver);
    thread::sleep(Duration::from_millis(50));
    assert!(saver.running());
    saver.stop();
    done.recv_timeout(Duration::from_secs(1)).expect("idle saver stopped promptly");
    // Both hold the database open; release it so the file can be deleted.
    drop(saver);
    drop(sqlite);
    _ = fs::remove_file(path);
}

#[test]
fn stop_before_start_still_saves_and_returns() {
    let path = "./test_read_saver_early_stop.sqlite";
    let (saver, sqlite) = saver(path);
    saver.save_reads(&reads(0, 4)).unwrap();
    saver.stop();
    let done = run(&saver);
    done.recv_timeout(Duration::from_secs(1)).expect("saver returned");
    assert_eq!(saved(&sqlite), 4);
    // Both hold the database open; release it so the file can be deleted.
    drop(saver);
    drop(sqlite);
    _ = fs::remove_file(path);
}

#[test]
fn reads_after_stopping_are_saved_directly() {
    let path = "./test_read_saver_late.sqlite";
    let (saver, sqlite) = saver(path);
    let done = run(&saver);
    saver.stop();
    done.recv_timeout(Duration::from_secs(5)).expect("saver stopped");
    saver.save_reads(&reads(0, 2)).unwrap();
    assert_eq!(saved(&sqlite), 2, "nothing is left waiting for a stopped saver");
    // Both hold the database open; release it so the file can be deleted.
    drop(saver);
    drop(sqlite);
    _ = fs::remove_file(path);
}

#[test]
fn shutdown_sound_claimed_once() {
    // The guard play_shutdown uses; only this test touches it.
    assert!(crate::sound_board::claim_shutdown_sound());
    assert!(!crate::sound_board::claim_shutdown_sound());
    assert!(!crate::sound_board::claim_shutdown_sound());
}
