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

//! A connected control socket client (Chronokeep desktop, the web page, ...).
//!
//! Messages to a client are queued and written by that client's own writer
//! thread. Sending never blocks, so nothing waits on a slow or stuck client
//! and no lock is ever held while writing to a socket. Each message is written
//! whole, so messages sent from different threads never interleave. A client
//! whose writes time out, or that falls too far behind, is disconnected.

use std::{io::{self, Write}, net::{Shutdown, TcpStream}, sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc, Condvar, Mutex}, thread, time::Duration};

use serde::Serialize;

/// The connected clients, one slot per connection (the last slot is kept for
/// connections from the portal computer itself).
pub type ControlSockets = Arc<Mutex<[Option<Client>; crate::control::socket::MAX_CONNECTED + 1]>>;

/// How long a single write may take before the client is considered gone.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// Messages waiting to be written before a client is considered too slow.
pub const QUEUE_LIMIT: usize = 1000;

enum Outgoing {
    Message(Vec<u8>),
    /// Write everything before this, then close the connection.
    Close,
}

#[derive(Clone)]
pub struct Client {
    queue: mpsc::SyncSender<Outgoing>,
    alive: Arc<AtomicBool>,
    stream: Arc<TcpStream>,
    /// Set once the writer thread has finished.
    finished: Arc<(Mutex<bool>, Condvar)>,
}

impl Client {
    /// Starts the writer thread for `stream`. The caller keeps reading from its own copy.
    pub fn new(stream: &TcpStream) -> io::Result<Client> {
        stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
        Client::with_queue_limit(stream, QUEUE_LIMIT)
    }

    pub(crate) fn with_queue_limit(stream: &TcpStream, limit: usize) -> io::Result<Client> {
        let writer = stream.try_clone()?;
        let (queue, outgoing) = mpsc::sync_channel(limit);
        let client = Client {
            queue,
            alive: Arc::new(AtomicBool::new(true)),
            stream: Arc::new(stream.try_clone()?),
            finished: Arc::new((Mutex::new(false), Condvar::new())),
        };
        let (alive, finished) = (client.alive.clone(), client.finished.clone());
        thread::spawn(move || {
            write_loop(writer, outgoing, &alive);
            let (lock, cvar) = &*finished;
            if let Ok(mut done) = lock.lock() {
                *done = true;
            }
            cvar.notify_all();
        });
        Ok(client)
    }

    /// Queues one message (JSON and a newline). Returns false if the client is
    /// gone or too far behind (it's disconnected then).
    pub fn send<T: Serialize>(&self, message: &T) -> bool {
        if !self.is_alive() {
            return false;
        }
        let mut bytes = match serde_json::to_vec(message) {
            Ok(b) => b,
            Err(e) => {
                println!("Unable to encode message for client: {e}");
                return false;
            }
        };
        bytes.push(b'\n');
        match self.queue.try_send(Outgoing::Message(bytes)) {
            Ok(()) => true,
            Err(mpsc::TrySendError::Full(_)) => {
                println!("Client isn't keeping up with messages; disconnecting it.");
                self.close();
                false
            },
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.alive.store(false, Ordering::SeqCst);
                false
            },
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Disconnects now; anything still queued is dropped.
    pub fn close(&self) {
        self.alive.store(false, Ordering::SeqCst);
        _ = self.stream.shutdown(Shutdown::Both);
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        if how != Shutdown::Read {
            self.alive.store(false, Ordering::SeqCst);
        }
        self.stream.shutdown(how)
    }

    /// Closes the connection once everything already queued is written, without waiting.
    pub fn end(&self) {
        if self.is_alive() && self.queue.try_send(Outgoing::Close).is_err() {
            self.close();
        }
    }

    /// Writes everything already queued, then closes the connection, waiting up
    /// to `wait` for that to finish. Used when portal stops so clients get the
    /// last messages (shutting down, disconnect).
    pub fn finish(&self, wait: Duration) {
        self.end();
        let (lock, cvar) = &*self.finished;
        if let Ok(done) = lock.lock() {
            _ = cvar.wait_timeout_while(done, wait, |done| !*done);
        }
        self.close();
    }
}

fn write_loop(mut stream: TcpStream, outgoing: mpsc::Receiver<Outgoing>, alive: &AtomicBool) {
    // Ends when every Client copy is gone, on Close, or when a write fails.
    for message in outgoing {
        match message {
            Outgoing::Message(bytes) => {
                if let Err(e) = stream.write_all(&bytes) {
                    // Timed out or the connection broke. A partly written message
                    // can't be taken back, so the connection is done.
                    match e.kind() {
                        io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted => {},
                        _ => println!("Error writing to client, disconnecting it: {e}"),
                    }
                    break;
                }
            },
            Outgoing::Close => break,
        }
    }
    alive.store(false, Ordering::SeqCst);
    _ = stream.shutdown(Shutdown::Both);
}

/// The connected clients, copied out so they can be sent to without holding
/// the control sockets lock. Each comes with its slot number.
pub fn connected(sockets: &Mutex<[Option<Client>; crate::control::socket::MAX_CONNECTED + 1]>) -> Vec<(usize, Client)> {
    match sockets.lock() {
        Ok(s) => s.iter().enumerate().filter_map(|(ix, c)| c.as_ref().map(|c| (ix, c.clone()))).collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
