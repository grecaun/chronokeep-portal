use std::{net::TcpStream, sync::{Arc, Mutex}, thread, time::Duration};

use reqwest::StatusCode;
use serde::Serialize;

use crate::{control::{socket::{write_uploader_status, MAX_CONNECTED}, Control}, database::{sqlite, Database}, defaults, network::api, objects::read, screen::CharacterDisplay};
use crate::remote::remote_util;

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
    control: Arc<Mutex<Control>>,
    screen: Arc<Mutex<Option<CharacterDisplay>>>,
}

impl Uploader {
    pub fn new(
        keepalive: Arc<Mutex<bool>>,
        sqlite: Arc<Mutex<sqlite::SQLite>>,
        control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
        control: Arc<Mutex<Control>>,
        screen: Arc<Mutex<Option<CharacterDisplay>>>,
    ) -> Uploader {
        Uploader {
            server_keepalive: keepalive,
            local_keepalive: Arc::new(Mutex::new(false)),
            sqlite,
            status: Arc::new(Mutex::new(Status::Stopped)),
            control_sockets,
            control,
            screen,
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
        self.update_control_socks(0);
    }

    pub fn run(&self) {
        let mut err_count: usize = 0;
        // check if we're already running, exit if so, otherwise set to running
        if let Ok(mut r) = self.status.lock() {
            if *r == Status::Running {
                return;
            }
            *r = Status::Running;
        }
        // let everyone know we're running
        self.update_control_socks(err_count);
        // set local keepalive to true to keep running until told to stop
        if let Ok(mut ka) = self.local_keepalive.lock() {
            *ka = true;
        }
        let http_client: reqwest::blocking::Client;
        match reqwest::blocking::ClientBuilder::new().timeout(Duration::from_secs(5))
                                    .connect_timeout(Duration::from_secs(5)).build() {
            Ok(client) => {
                http_client = client;
            },
            Err(_) => {
                if let Ok(mut r) = self.status.lock() {
                    *r = Status::Stopped;
                    // let everyone know we're stopped
                }
                self.update_control_socks(0);
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
            let mut to_upload: Vec<read::Read> = Vec::new();
            let mut upload_api: Option<api::Api> = None;
            // get our reads and then upload them
            // check error count
            // if there are 2 or more errors then don't try to grab the database mutex, instead check net first
            // if the chronokeep results api health endpoint isn't reachable then the rest of the internet
            // is probably not reachable
            // one error could just be random, two means it's happened multiple times in a row
            // and a check to the health endpoint is a quick way to save some work
            let mut attempt_upload = true;
            if err_count > 1 {
                attempt_upload = false;
                match http_client.get("https://api.chronokeep.com/health").send() {
                    Ok(resp) => {
                        match resp.status() {
                            StatusCode::OK | StatusCode::NO_CONTENT => {
                                attempt_upload = true;
                            }
                            _ => {
                                err_count += 1;
                            }
                        }
                    },
                    Err(_) => {
                        err_count += 1;
                    }
                }
            }
            if attempt_upload {
                if let Ok(sq) = self.sqlite.lock() {
                    match sq.get_apis() {
                        Ok(apis) => {
                            let mut found = false;
                            for api in apis {
                                if api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF {
                                    found = true;
                                    upload_api = Some(api.clone());
                                    match sq.get_not_uploaded_reads() {
                                        Ok(mut reads) => {
                                            to_upload.append(&mut reads);
                                        },
                                        Err(e) => {
                                            println!("Error getting reads to upload: {e}");
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
            }
            // upload any reads we found in the database if we found our remote API
            if let Some(api) = upload_api {
                if to_upload.len() > 0 {
                    let (modified_reads, e_count) = remote_util::upload_all_reads(&http_client, &api, to_upload);
                    if let Ok(mut sq) = self.sqlite.lock() {
                        match sq.update_reads_status(&modified_reads) {
                            Ok(_) => {},
                            Err(e) => {
                                println!("Error updating uploaded reads: {e}");
                            }
                        }
                    }
                    if e_count > 0 {
                        err_count += e_count;
                        self.update_control_socks(err_count);
                    } else if err_count != 0 {
                        err_count = 0;
                        self.update_control_socks(err_count);
                    }
                }
            }

            let mut upload_pause: u64 = defaults::DEFAULT_UPLOAD_INTERVAL;
            if let Ok(control) = self.control.lock() {
                upload_pause = control.upload_interval;
            }
            // sleep for AUTO_UPLOAD_PAUSE seconds
            thread::sleep(Duration::from_secs(upload_pause));
        } // end auto upload loop
        if let Ok(mut r) = self.status.lock() {
            *r = Status::Stopped;
        }
        // let everyone know we're stopped
        self.update_control_socks(0);
        println!("Auto upload of reads finished.")
    }

    fn update_control_socks(&self, err_count: usize) {
        // let all the control sockets know of our status
        let stat = self.status();
        if let Ok(c_socks) = self.control_sockets.lock() {
            for sock in c_socks.iter() {
                if let Some(sock) = sock {
                    _ = write_uploader_status(&sock, stat.clone());
                }
            }
        }
        if let Ok(mut screen_opt) = self.screen.lock() {
            if let Some(screen) = &mut *screen_opt {
                screen.update_upload_status(stat, err_count);
            }
        }
    }
}