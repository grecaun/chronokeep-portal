use std::{sync::{Arc, Condvar, Mutex, WaitTimeoutResult}, time::Duration};

use reqwest::header::HeaderMap;

use crate::control::Control;

#[derive(Clone, Debug)]
pub enum Notification {
    Start,
    Stop,
    BatteryLow,
    BatteryCritical,
    BatteryUnknown,
    StartReading,
    StopReading,
    UnableToStartReading,
    Location,
    Shutdown,
}

#[derive(Clone)]
pub struct Notifier {
    keepalive: Arc<Mutex<bool>>,
    control: Arc<Mutex<Control>>,
    notifications: Arc<Mutex<Vec<Notification>>>,
    waiter: Arc<(Mutex<bool>, Condvar)>,
}

impl Notifier {
    pub fn new(
        keepalive: Arc<Mutex<bool>>,
        control: Arc<Mutex<Control>>,
    ) -> Self {
        Self {
            keepalive,
            control,
            notifications: Arc::new(Mutex::new(vec!())),
            waiter: Arc::new((Mutex::new(true), Condvar::new())),
        }
    }

    pub fn send_notification(&self, note: Notification) {
        if let Ok(mut notifications) = self.notifications.lock() {
            notifications.push(note);
        }
        let (lock, cvar) = &*self.waiter;
        let mut waiting = lock.lock().unwrap();
        *waiting = false;
        cvar.notify_one();
    }

    pub fn run(&mut self) {
        let http_client: reqwest::blocking::Client;
        match reqwest::blocking::ClientBuilder::new().timeout(Duration::from_secs(10))
                                    .connect_timeout(Duration::from_secs(10)).build() {
            Ok(client) => {
                http_client = client;
            },
            Err(_) => {
                println!("Unable to get our http client. Cannot start notifier thread.");
                return;
            },
        }
        loop {
            if let Ok(keepalive) = self.keepalive.try_lock() {
                if *keepalive == false {
                    println!("Notifier thread stopping.");
                    break;
                }
            }
            let (lock, cvar) = &*self.waiter.clone();
            let mut waiting = lock.lock().unwrap();
            let mut result: WaitTimeoutResult;
            while *waiting {
                (waiting, result) = cvar.wait_timeout(waiting, Duration::from_secs(30)).unwrap();
                if result.timed_out() {
                    break;
                }
            }
            *waiting = true;
            drop(waiting);
            let mut work_list: Vec<Notification> = vec!();
            if let Ok(mut notifications) = self.notifications.lock() {
                work_list.append(&mut *notifications);
            }
            for note in work_list.iter() {
                println!("Notification received: {:?}", note);
                let mut name = String::from("Chronokeep Portal");
                let mut url = String::from("");
                let mut topic = String::from("");
                let mut user = String::from("");
                let mut pass = String::from("");
                if let Ok(control) = self.control.lock() {
                    name = control.name.clone();
                    url = control.ntfy_url.clone();
                    topic = control.ntfy_topic.clone();
                    user = control.ntfy_user.clone();
                    pass = control.ntfy_pass.clone();
                }
                let mut priority: u8 = 3;
                let tag: String;
                let message = match note {
                    Notification::Start => {
                        tag = String::from("green_circle");
                        format!("{name} has started.")
                    },
                    Notification::Stop => {
                        tag = String::from("red_square");
                        format!("{name} is shutting down.")
                    },
                    Notification::BatteryLow => {
                        tag = String::from("battery");
                        priority = 4;
                        format!("Battery is low on {name}.")
                    },
                    Notification::BatteryCritical => {
                        tag = String::from("battery");
                        priority = 5;
                        format!("Warning! Battery critical on {name}.")
                    },
                    Notification::BatteryUnknown => {
                        tag = String::from("battery");
                        format!("{name} is unable to detect the battery level.")
                    },
                    Notification::Location => {
                        tag = String::from("world_map");
                        format!("Location for {name} is...")
                    },
                    Notification::StartReading => { // used when Auto Start is set
                        tag = String::from("medal_sports");
                        format!("{name} has successfully connected to the reader.")
                    },
                    Notification::StopReading => {
                        tag = String::from("warning");
                        priority = 5;
                        format!("A reader on {name} has unexpectedly disconnected.")
                    },
                    Notification::UnableToStartReading => {
                        tag = String::from("warning");
                        priority = 5;
                        format!("Unable to connect to a reader on {name}.")
                    },
                    Notification::Shutdown => {
                        tag = String::from("stop_sign");
                        format!("{name} is shutting down.")
                    }
                };
                if !url.is_empty() && !topic.is_empty() && !user.is_empty() && !pass.is_empty() {
                    println!("Sending notification...");
                    match http_client.post(format!("{}{}", url, topic))
                        .headers(construct_headers(priority, tag))
                        .basic_auth(user, Some(pass))
                        .body(message)
                        .send() {
                            Ok(resp) => {
                                match resp.status() {
                                    reqwest::StatusCode::OK | reqwest::StatusCode::NO_CONTENT => {}, // success
                                    other => {
                                        println!("Unknown status code trying to send notification: {other}");
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error sending notification: {e}");
                                if let Ok(mut notifications) = self.notifications.lock() {
                                    notifications.push(note.clone());
                                }
                            }
                        };
                }
            };
        } // end loop
    }
}

fn construct_headers(priority: u8, tag: String) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("X-Priority", format!("{}", priority).parse().unwrap());
    headers.insert("X-Tags", tag.parse().unwrap());
    headers
}