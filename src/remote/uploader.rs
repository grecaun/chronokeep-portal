use std::{net::TcpStream, sync::{Arc, Mutex}, thread, time::Duration};

use serde::Serialize;

use crate::{control::{socket::{self, write_uploader_status, MAX_CONNECTED}, Control}, database::{sqlite, Database}, defaults, network::api, objects::read};

#[derive(Clone, PartialEq, Serialize, Debug)]
pub enum Status {
    Running,
    Stopping,
    Stopped,
    Unknown
}

pub struct Uploader {
    server_keepalive: Arc<Mutex<bool>>,
    local_keepalive: Arc<Mutex<bool>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    status: Arc<Mutex<Status>>,
    control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
    control: Arc<Mutex<Control>>
}

impl Uploader {
    pub fn new(
        keepalive: Arc<Mutex<bool>>,
        sqlite: Arc<Mutex<sqlite::SQLite>>,
        control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
        control: Arc<Mutex<Control>>
    ) -> Uploader {
        Uploader {
            server_keepalive: keepalive,
            local_keepalive: Arc::new(Mutex::new(false)),
            sqlite,
            status: Arc::new(Mutex::new(Status::Stopped)),
            control_sockets,
            control
        }
    }

    pub fn status(&self) -> Status {
        let mut output = Status::Unknown;
        if let Ok(stat) = self.status.lock() {
            output = stat.clone();
        }
        output
    }

    pub fn running(&self) -> bool {
        let mut output = false;
        if let Ok(r) = self.status.lock() {
            output = Status::Running == *r
        }
        output
    }

    pub fn stop(&self) {
        if let Ok(mut ka) = self.local_keepalive.lock() {
            *ka = false;
        }
        if let Ok(mut r) = self.status.lock() {
            *r = Status::Stopping
        }
        // let everyone know we're stopping
        self.update_control_socks();
    }

    pub fn run(&self) {
        // check if we're already running, exit if so, otherwise set to running
        if let Ok(mut r) = self.status.lock() {
            if *r == Status::Running {
                return;
            }
            *r = Status::Running;
        }
        // let everyone know we're running
        self.update_control_socks();
        // set local keepalive to true to keep running until told to stop
        if let Ok(mut ka) = self.local_keepalive.lock() {
            *ka = true;
        }
        let http_client: reqwest::blocking::Client;
        match reqwest::blocking::ClientBuilder::new().timeout(Duration::from_secs(30))
                                    .connect_timeout(Duration::from_secs(30)).build() {
            Ok(client) => {
                http_client = client;
            },
            Err(_) => {
                if let Ok(mut r) = self.status.lock() {
                    *r = Status::Stopped;
                    // let everyone know we're stopped
                    self.update_control_socks();
                }
                println!("Unable to get our http client. Auto upload terminating.");
                return;
            },
        }
        // work loop
        loop {
            // exit our loop and terminate if local keep alive is done
            if let Ok(ka) = self.local_keepalive.lock() {
                if *ka == false {
                    break;
                }
            } else {
                println!("Unable to grab local keep alive mutex. Exiting.");
                break;
            }
            // exit our loop and terminate if server is shutting down
            if let Ok(ka) = self.server_keepalive.lock() {
                if *ka == false {
                    break;
                }
            } else {
                println!("Unable to grab server keep alive mutex. Exiting.");
                break;
            }
            // get our reads and then upload them
            if let Ok(mut sq) = self.sqlite.lock() {
                match sq.get_apis() {
                    Ok(apis) => {
                        let mut found = false;
                        for api in apis {
                            if api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF {
                                found = true;
                                match sq.get_not_uploaded_reads() {
                                    Ok(reads) => {
                                        // only upload in chunks of 50
                                        if reads.len() > 50 {
                                            println!("Attempting to upload {} reads.", reads.len());
                                            // get the total number of full 50 count loops to do
                                            let num_loops = reads.len() / 50;
                                            let mut loop_counter = 0;
                                            // counter starts at 0, num_loops is at minimum 1
                                            // after the first loop counter is 1 and should exit if only 50 items
                                            while loop_counter < num_loops {
                                                let start_ix = loop_counter * 50;
                                                let slice = &reads[start_ix..start_ix+50];
                                                match socket::upload_reads(&http_client, &api, &slice) {
                                                    Ok(count) => {
                                                        // if we uploaded the correct
                                                        if count == 50 {
                                                            let mut modified_reads: Vec<read::Read> = Vec::new();
                                                            for read in slice {
                                                                modified_reads.push(read::Read::new(
                                                                    read.id(),
                                                                    String::from(read.chip()),
                                                                    read.seconds(),
                                                                    read.milliseconds(),
                                                                    read.reader_seconds(),
                                                                    read.reader_milliseconds(),
                                                                    read.antenna(),
                                                                    String::from(read.reader()),
                                                                    String::from(read.rssi()),
                                                                    read.status(),
                                                                    read::READ_UPLOADED_TRUE
                                                                ));
                                                            }
                                                            match sq.update_reads_status(&modified_reads) {
                                                                Ok(_) => {},
                                                                Err(e) => {
                                                                    println!("Error updating uploaded reads: {e}");
                                                                }
                                                            }
                                                        } else {
                                                            println!("Error uploading reads. Count doesn't match. {} uploaded, expected {}", count, 50);
                                                        }
                                                    },
                                                    Err(e) => {
                                                        println!("Error uploading reads: {:?}", e);
                                                    }
                                                }
                                                loop_counter = loop_counter + 1;
                                            }
                                            let start_ix = loop_counter * 50;
                                            let slice = &reads[start_ix..reads.len()];
                                            match socket::upload_reads(&http_client, &api, &slice) {
                                                Ok(count) => {
                                                    // Need to calculate the count... for 75 items (0-74)
                                                    // only 1 loop, start_ix should be (1 * 50)
                                                    // 75 - 50 = 25
                                                    let amt = reads.len() - start_ix;
                                                    // check for correct amout
                                                    if count == amt {
                                                        let mut modified_reads: Vec<read::Read> = Vec::new();
                                                        for read in slice {
                                                            modified_reads.push(read::Read::new(
                                                                read.id(),
                                                                String::from(read.chip()),
                                                                read.seconds(),
                                                                read.milliseconds(),
                                                                read.reader_seconds(),
                                                                read.reader_milliseconds(),
                                                                read.antenna(),
                                                                String::from(read.reader()),
                                                                String::from(read.rssi()),
                                                                read.status(),
                                                                read::READ_UPLOADED_TRUE
                                                            ));
                                                        }
                                                        match sq.update_reads_status(&modified_reads) {
                                                            Ok(_) => { },
                                                            Err(e) => {
                                                                println!("Error updating uploaded reads: {e}");
                                                            }
                                                        }
                                                    } else {
                                                        println!("Error uploading reads. Count doesn't match. {} uploaded, expected {}", count, amt);
                                                    }
                                                },
                                                Err(e) => {
                                                    println!("Error uploading reads: {:?}", e);
                                                }
                                            }
                                        } else if reads.len() > 0 {
                                            println!("Attempting to upload {} reads.", reads.len());
                                            match socket::upload_reads(&http_client, &api, &reads) {
                                                Ok(count) => {
                                                    // if we uploaded the correct
                                                    if count == reads.len() {
                                                        let mut modified_reads: Vec<read::Read> = Vec::new();
                                                        for read in reads {
                                                            modified_reads.push(read::Read::new(
                                                                read.id(),
                                                                String::from(read.chip()),
                                                                read.seconds(),
                                                                read.milliseconds(),
                                                                read.reader_seconds(),
                                                                read.reader_milliseconds(),
                                                                read.antenna(),
                                                                String::from(read.reader()),
                                                                String::from(read.rssi()),
                                                                read.status(),
                                                                read::READ_UPLOADED_TRUE
                                                            ));
                                                        }
                                                        match sq.update_reads_status(&modified_reads) {
                                                            Ok(_) => { },
                                                            Err(e) => {
                                                                println!("Error updating uploaded reads: {e}");
                                                            }
                                                        }
                                                    } else {
                                                        println!("Error uploading reads. Count doesn't match. {} uploaded, expected {}", count, reads.len());
                                                    }
                                                },
                                                Err(e) => {
                                                    println!("Error uploading reads: {:?}", e);
                                                }
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        println!("Error uploading reads: {e}");
                                    }
                                }
                                // should only be one REMOTE or REMOTE_SELF type of API in the database
                                // so we can break
                                break;
                            }
                        }
                        if found == false {
                            println!("No remote API set up.");
                            break;
                        }
                    }
                    Err(e) => {
                        println!("Unable to get apis: {e}");
                    }
                }
            }
            let mut upload_pause: u64 = defaults::DEFAULT_UPLOAD_INTERVAL;
            if let Ok(control) = self.control.lock() {
                upload_pause = control.upload_interval;
            }
            // sleep for AUTO_UPLOAD_PAUSE seconds
            thread::sleep(Duration::from_secs(upload_pause));
        }
        if let Ok(mut r) = self.status.lock() {
            *r = Status::Stopped;
        }
        // let everyone know we're stopped
        self.update_control_socks();
        println!("Auto upload of reads finished.")
    }

    fn update_control_socks(&self) {
        // let all the control sockets know of our status
        if let Ok(c_socks) = self.control_sockets.lock() {
            let stat = self.status();
            for sock in c_socks.iter() {
                if let Some(sock) = sock {
                    _ = write_uploader_status(&sock, stat.clone());
                }
            }
        }
    }
}