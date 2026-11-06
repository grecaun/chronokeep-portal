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

//! What happens after the portal program stops. Shutdown, restart and update
//! requests (from the control socket, the web page or the screen) only record
//! the action here and stop the program; `main` runs the OS command once every
//! thread has stopped and all reads are saved.

use std::{env, net::TcpStream, path::Path, process::Command, sync::{Arc, Mutex}};

use crate::control::{socket::UPDATE_SCRIPT_ENV, Control};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExitAction {
    /// Just stop the program.
    #[default]
    Quit,
    /// Shut the computer down.
    Shutdown,
    /// Restart the portal service.
    Restart,
    /// Run the update script.
    Update,
}

impl ExitAction {
    /// The OS command to run for this action on Linux, or None when there's
    /// nothing to run. `update_script` is the PORTAL_UPDATE_SCRIPT value.
    pub fn command(self, update_script: Option<String>) -> Result<Option<(String, Vec<String>)>, String> {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        match self {
            ExitAction::Quit => Ok(None),
            ExitAction::Shutdown => Ok(Some((String::from("sudo"), args(&["shutdown", "-h", "now"])))),
            ExitAction::Restart => Ok(Some((String::from("sudo"), args(&["systemctl", "restart", "portal"])))),
            ExitAction::Update => match update_script {
                Some(path) if !path.trim().is_empty() => Ok(Some((path, Vec::new()))),
                _ => Err(format!("update script environment variable ({UPDATE_SCRIPT_ENV}) not set")),
            },
        }
    }
}

/// Checks that `action` can run on this computer, so a request can be refused
/// while portal is still running instead of stopping for something that won't
/// happen. `os` is the platform name; `update_script` is the PORTAL_UPDATE_SCRIPT value.
pub fn check(action: ExitAction, os: &str, update_script: Option<String>) -> Result<(), String> {
    if action == ExitAction::Quit {
        return Ok(());
    }
    if os != "linux" {
        return Err(format!("not supported on this platform ({os})"));
    }
    match action.command(update_script)? {
        Some((program, _)) if action == ExitAction::Update && !Path::new(&program).is_file() => {
            Err(format!("update script not found ({program})"))
        },
        _ => Ok(()),
    }
}

/// Records `action` and stops the program. The control loop then stops every
/// thread and saves all reads before `main` runs the action.
pub fn begin(control: &Arc<Mutex<Control>>, keepalive: &Arc<Mutex<bool>>, action: ExitAction, control_port: u16) {
    if let Ok(mut c) = control.lock() {
        c.request_exit(action);
    }
    if let Ok(mut ka) = keepalive.lock() {
        println!("Starting program stop sequence ({action:?}).");
        *ka = false;
    }
    // connect to ensure the control loop exits its accept call
    _ = TcpStream::connect(format!("127.0.0.1:{control_port}"));
}

/// Runs the OS command for `action`. Called by `main` after everything has
/// stopped. Waits for the command so it isn't cut off when the program exits.
pub fn run(action: ExitAction) {
    if action == ExitAction::Quit {
        return;
    }
    let update_script = env::var(UPDATE_SCRIPT_ENV).ok();
    if let Err(e) = check(action, env::consts::OS, update_script.clone()) {
        println!("Unable to {action:?}: {e}");
        return;
    }
    let (program, args) = match action.command(update_script) {
        Ok(Some(c)) => c,
        Ok(None) => return,
        Err(e) => {
            println!("Unable to {action:?}: {e}");
            return;
        }
    };
    println!("Sending {action:?} command to the OS.");
    match Command::new(&program).args(&args).status() {
        Ok(status) if status.success() => println!("{action:?} command sent to OS successfully."),
        Ok(status) => println!("{action:?} command failed: {status}"),
        Err(e) => println!("Error sending {action:?} command: {e}"),
    }
}

#[cfg(test)]
mod tests;
