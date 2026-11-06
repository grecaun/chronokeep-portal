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

use std::sync::{Arc, Mutex, Condvar};

use crate::{database::{sqlite, Database}, objects::read};

/// What the saver thread is waiting for.
#[derive(Default)]
struct Wake {
    /// Reads were queued since the last save.
    pending: bool,
    /// `stop` was called; save what's left and exit.
    stopping: bool,
}

/// Saves reads to the database on its own thread so readers never wait on the
/// database. It keeps running until `stop` is called (not when the program
/// starts shutting down) so reads handed over by readers while they disconnect
/// are still saved; `start` saves everything queued before it returns.
pub struct ReadSaver {
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    reads: Arc<Mutex<Vec<read::Read>>>,

    running: Arc<Mutex<bool>>,
    wake: Arc<(Mutex<Wake>, Condvar)>
}

impl ReadSaver {
    pub fn new(
        sqlite: Arc<Mutex<sqlite::SQLite>>,
    ) -> ReadSaver {
        ReadSaver {
            sqlite,
            reads: Arc::new(Mutex::new(Vec::<read::Read>::new())),
            running: Arc::new(Mutex::new(false)),
            wake: Arc::new((Mutex::new(Wake::default()), Condvar::new()))
        }
    }

    pub fn save_reads(&self, in_reads: &Vec<read::Read>) -> Result<(), &str> {
        // After the saver has stopped nothing would pick these up, so save them now.
        if self.stopped() {
            return self.write(in_reads.clone()).map_err(|_| "error saving reads");
        }
        match self.reads.lock() {
            Ok(mut reads) => reads.append(&mut in_reads.clone()),
            Err(_) => return Err("error getting reads mutex"),
        }
        let (lock, cvar) = &*self.wake;
        if let Ok(mut wake) = lock.lock() {
            wake.pending = true;
        }
        cvar.notify_all();
        // The saver may have finished between the check above and queuing.
        if self.stopped() && !self.flush() {
            return Err("error saving reads");
        }
        Ok(())
    }

    /// Tells the saver to save what's queued and exit. Wakes it right away.
    pub fn stop(&self) {
        println!("Sending shutdown command to read saver.");
        let (lock, cvar) = &*self.wake;
        if let Ok(mut wake) = lock.lock() {
            wake.stopping = true;
        }
        cvar.notify_all();
    }

    pub fn running(&self) -> bool {
        if let Ok(run) = self.running.lock() {
            return *run
        }
        false
    }

    /// True once `stop` was called and the thread has finished.
    fn stopped(&self) -> bool {
        let stopping = self.wake.0.lock().map(|w| w.stopping).unwrap_or(false);
        stopping && !self.running()
    }

    /// Saves reads to the database. Returns the reads it couldn't save.
    fn write(&self, reads: Vec<read::Read>) -> Result<(), Vec<read::Read>> {
        if reads.is_empty() {
            return Ok(());
        }
        let Ok(mut db) = self.sqlite.lock() else { return Err(reads) };
        match db.save_reads(&reads) {
            Ok(_) => Ok(()),
            Err(e) => {
                println!("Error saving reads. {e}");
                Err(reads)
            }
        }
    }

    /// Saves everything queued. Reads that fail are queued again.
    fn flush(&self) -> bool {
        let queued = match self.reads.lock() {
            Ok(mut reads) => std::mem::take(&mut *reads),
            Err(_) => return false,
        };
        match self.write(queued) {
            Ok(()) => true,
            Err(mut failed) => {
                if let Ok(mut reads) = self.reads.lock() {
                    failed.append(&mut reads);
                    *reads = failed;
                }
                false
            }
        }
    }

    pub fn start(&self) {
        if let Ok(mut run) = self.running.lock() {
            *run = true
        } else {
            return
        }
        println!("Starting read saver.");
        let (lock, cvar) = &*self.wake;
        loop {
            // Sleep until reads are queued or we're told to stop (no polling).
            let stopping = match lock.lock() {
                Ok(guard) => match cvar.wait_while(guard, |w| !w.pending && !w.stopping) {
                    Ok(mut wake) => {
                        wake.pending = false;
                        wake.stopping
                    },
                    Err(e) => {
                        println!("unable to aquire semaphore: {e}");
                        true
                    }
                },
                Err(e) => {
                    println!("unable to aquire semaphore: {e}");
                    true
                }
            };
            self.flush();
            if stopping {
                println!("Read saver told to stop.");
                break;
            }
        }
        // Anything queued while the last save ran.
        if !self.flush() {
            let left = self.reads.lock().map(|r| r.len()).unwrap_or(0);
            println!("Unable to save {left} reads before stopping.");
        }
        if let Ok(mut run) = self.running.lock() {
            *run = false
        }
    }
}

#[cfg(test)]
mod tests;
