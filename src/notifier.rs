use std::{sync::{Arc, Condvar, Mutex}, time::Duration};

use reqwest::header::HeaderMap;

use crate::control::Control;

#[derive(Clone)]
pub enum Notification {
    Start,
    Stop,
    BatteryLow,
    BatteryCritical,
    BatteryUnknown,
    Location,
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
    }

    pub fn run(&mut self) {
        let http_client: reqwest::blocking::Client;
        match reqwest::blocking::ClientBuilder::new().timeout(Duration::from_secs(30))
                                    .connect_timeout(Duration::from_secs(30)).build() {
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
            while *waiting {
                waiting = cvar.wait(waiting).unwrap();
            }
            let mut work_list: Vec<Notification> = vec!();
            if let Ok(mut notifications) = self.notifications.lock() {
                work_list.append(&mut *notifications);
            }
            for note in work_list.iter() {
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
                        format!("{} has started.", name)
                    },
                    Notification::Stop => {
                        tag = String::from("red_square");
                        format!("{} is shutting down.", name)
                    },
                    Notification::BatteryLow => {
                        tag = String::from("battery");
                        priority = 4;
                        format!("Battery is low on {}.", name)
                    },
                    Notification::BatteryCritical => {
                        tag = String::from("battery");
                        priority = 5;
                        format!("Warning! Battery critical on {}.", name)
                    },
                    Notification::BatteryUnknown => {
                        tag = String::from("battery");
                        format!("{} is unable to detect the battery level.", name)
                    },
                    Notification::Location => {
                        tag = String::from("world_map");
                        format!("Location for {} is...", name)
                    },
                };
                if !url.is_empty() && !topic.is_empty() && !user.is_empty() && !pass.is_empty() {
                    match http_client.post(format!("{}{}", url, topic))
                    .headers(construct_headers(message, priority, tag))
                    .basic_auth(user, Some(pass))
                    .send() {
                        Ok(resp) => {
                            match resp.status() {
                                reqwest::StatusCode::OK | reqwest::StatusCode::NO_CONTENT => {}, // success
                                _ => {
                                    if let Ok(mut notifications) = self.notifications.lock() {
                                        notifications.push(note.clone());
                                    }
                                }
                            }
                        },
                        Err(_) => {
                            if let Ok(mut notifications) = self.notifications.lock() {
                                notifications.push(note.clone());
                            }
                        }
                    };
                }
            };
        }
    }
}

fn construct_headers(message: String, priority: u8, tag: String) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("Message", message.parse().unwrap());
    headers.insert("Priority", format!("{}", priority).parse().unwrap());
    headers.insert("Tags", tag.parse().unwrap());
    headers
}