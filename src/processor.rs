use std::sync::{Arc, Mutex, Condvar};

use crate::{database::{sqlite, Database}, objects::read};

pub struct ReadSaver {
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    reads: Arc<Mutex<Vec<read::Read>>>,

    keepalive: Arc<Mutex<bool>>,
    running: Arc<Mutex<bool>>,
    semaphore: Arc<(Mutex<bool>, Condvar)>
}

impl ReadSaver {
    pub fn new(
        sqlite: Arc<Mutex<sqlite::SQLite>>,
        keepalive: Arc<Mutex<bool>>
    ) -> ReadSaver {
        ReadSaver {
            sqlite,
            reads: Arc::new(Mutex::new(Vec::<read::Read>::new())),
            keepalive,
            running: Arc::new(Mutex::new(false)),
            semaphore: Arc::new((Mutex::new(false), Condvar::new()))
        }
    }

    pub fn save_reads(&self, in_reads: &Vec<read::Read>) -> Result<(), &str> {
        if let Ok(mut reads) = self.reads.try_lock() {
            reads.append(&mut in_reads.clone());
        } else {
            return Err("error getting reads mutext")
        }
        let (lock, cvar) = &*self.semaphore;
        let mut notify = lock.lock().unwrap();
        *notify = true;
        cvar.notify_all();
        drop(notify);
        Ok(())
    }


    pub fn stop(&self) {
        println!("Sending shutdown command to read saver.");
        if let Ok(mut run) = self.running.lock() {
            *run = false;
        }
    }

    pub fn running(&self) -> bool {
        if let Ok(run) = self.running.lock() {
            return *run
        }
        false
    }

    pub fn start(&self) {
        if let Ok(mut run) = self.running.lock() {
            *run = true
        } else {
            return
        }
        println!("Starting read saver.");
        loop {
            if let Ok(ka) = self.keepalive.lock() {
                if *ka == false {
                    println!("Sightings processor told to quit. /1/");
                    break;
                }
            } else {
                println!("Error getting keep alive mutex. Exiting.");
                break;
            }
            if let Ok(run) = self.running.lock() {
                if *run == false {
                    println!("Sightings processor told to quit. /2/");
                    break;
                }
            }
            let (lock, cvar) = &*self.semaphore;
            match cvar.wait_while(
                lock.lock().unwrap(),
                |notify| *notify == false
            ) {
                Ok(mut notify) => {
                    *notify = false; // we've been notified, reset semaphore to waiting state
                    drop(notify);    // drop the semaphore so we don't block other threads that may have tried to save while we're working
                    // save reads if they exist
                    let mut tmp_reads = Vec::<read::Read>::new();
                    if let Ok(mut reads) = self.reads.lock() {
                        tmp_reads.append(&mut reads);
                    }
                    if tmp_reads.len() > 0 {
                        if let Ok(mut db) = self.sqlite.lock() {
                            match db.save_reads(&tmp_reads) {
                                Ok(_num) => { },
                                Err(e) => {
                                    println!("Error saving reads. {e}");
                                    if let Ok(mut reads) = self.reads.lock() {
                                        reads.append(&mut tmp_reads);
                                    }
                                },
                            }
                        }
                    }
                },
                Err(e) => {
                    println!("unable to aquire semaphore: {e}");
                    break;
                }
            }
        }
        if let Ok(mut run) = self.running.lock() {
            *run = false
        }
        // save reads if they exist when closing
        if let Ok(mut reads) = self.reads.lock() {
            if reads.len() > 0 {
                if let Ok(mut db) = self.sqlite.lock() {
                    match db.save_reads(&reads) {
                        Ok(_num) => {
                            //println!("Saved {num} reads.");
                            reads.clear();
                        },
                        Err(e) => println!("Error saving reads. {e}"),
                    }
                }
            }
        }
    }
}