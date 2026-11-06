use std::{fs, net::TcpListener, sync::{Arc, Mutex}};

use super::*;
use crate::database::sqlite::tests::setup_tests;

fn control(path: &str) -> Control {
    let mut sq = setup_tests(path);
    let control = Control::new(&mut sq).unwrap();
    drop(sq);
    _ = fs::remove_file(path);
    control
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn commands_for_each_action() {
    assert_eq!(ExitAction::Quit.command(None), Ok(None));
    assert_eq!(ExitAction::Shutdown.command(None), Ok(Some((String::from("sudo"), strings(&["shutdown", "-h", "now"])))));
    assert_eq!(ExitAction::Restart.command(None), Ok(Some((String::from("sudo"), strings(&["systemctl", "restart", "portal"])))));
    assert_eq!(ExitAction::Update.command(Some(String::from("/opt/portal/update.sh"))), Ok(Some((String::from("/opt/portal/update.sh"), Vec::new()))));
}

#[test]
fn update_needs_the_script() {
    assert!(ExitAction::Update.command(None).is_err());
    assert!(ExitAction::Update.command(Some(String::from("  "))).is_err());
}

#[test]
fn quit_runs_nothing() {
    // Returns without running anything on any platform.
    run(ExitAction::Quit);
}

#[test]
fn new_control_just_quits() {
    assert_eq!(control("./test_exit_default.sqlite").exit_action, ExitAction::Quit);
}

#[test]
fn first_exit_request_wins() {
    let mut c = control("./test_exit_first.sqlite");
    c.request_exit(ExitAction::Quit);
    assert_eq!(c.exit_action, ExitAction::Quit);
    c.request_exit(ExitAction::Shutdown);
    assert_eq!(c.exit_action, ExitAction::Shutdown);
    // A later restart, update or quit (e.g. from portal-quit) doesn't replace it.
    for later in [ExitAction::Restart, ExitAction::Update, ExitAction::Quit] {
        c.request_exit(later);
        assert_eq!(c.exit_action, ExitAction::Shutdown);
    }
}

#[test]
fn begin_records_the_action_and_stops_the_program() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let control = Arc::new(Mutex::new(control("./test_exit_begin.sqlite")));
    let keepalive = Arc::new(Mutex::new(true));
    begin(&control, &keepalive, ExitAction::Restart, port);
    assert_eq!(control.lock().unwrap().exit_action, ExitAction::Restart);
    assert!(!*keepalive.lock().unwrap(), "program told to stop");
    // The control loop's accept call is woken by a connection.
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_ok(), "control loop woken");
}

#[test]
fn check_quit_always_allowed() {
    assert_eq!(check(ExitAction::Quit, "windows", None), Ok(()));
    assert_eq!(check(ExitAction::Quit, "linux", None), Ok(()));
}

#[test]
fn check_refuses_other_platforms() {
    for action in [ExitAction::Shutdown, ExitAction::Restart, ExitAction::Update] {
        let err = check(action, "windows", Some(String::from("./Cargo.toml"))).unwrap_err();
        assert!(err.contains("windows"), "{action:?}: {err}");
    }
}

#[test]
fn check_shutdown_and_restart_on_linux() {
    assert_eq!(check(ExitAction::Shutdown, "linux", None), Ok(()));
    assert_eq!(check(ExitAction::Restart, "linux", None), Ok(()));
}

#[test]
fn check_update_needs_an_existing_script() {
    assert!(check(ExitAction::Update, "linux", None).unwrap_err().contains("not set"));
    assert!(check(ExitAction::Update, "linux", Some(String::from("./no-such-update-script.sh"))).unwrap_err().contains("not found"));
    // A directory isn't a script.
    assert!(check(ExitAction::Update, "linux", Some(String::from("./src"))).is_err());
    // Any existing file passes (Cargo.toml stands in for the script).
    assert_eq!(check(ExitAction::Update, "linux", Some(String::from("./Cargo.toml"))), Ok(()));
}
