use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ina219::address::Address;
use ina219::SyncIna219;
use rppal::i2c::I2c;
use chrono::Utc;
use std::net::TcpStream;
use std::fs::OpenOptions;
use std::io::Write;

use crate::{database::Database, control::{Control, socket::{self, notifications::APINotification, MAX_CONNECTED}}, sqlite, network::api, screen::CharacterDisplay, notifier};

pub struct Checker {
    keepalive: Arc<Mutex<bool>>,
    control: Arc<Mutex<Control>>,
    screen: Arc<Mutex<Option<CharacterDisplay>>>,
    notifier: notifier::Notifier,
    control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    last_low: u64,
    last_crit: u64,
}

impl Checker {
    pub fn new(
        keepalive: Arc<Mutex<bool>>,
        control: Arc<Mutex<Control>>,
        screen: Arc<Mutex<Option<CharacterDisplay>>>,
        notifier: notifier::Notifier,
        control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
        sqlite: Arc<Mutex<sqlite::SQLite>>,
    ) -> Self {
        Self {
            keepalive,
            control,
            screen,
            notifier,
            control_sockets,
            sqlite,
            last_low: 0,
            last_crit: 0,
        }
    }

    pub fn run(&mut self) {
        println!("Starting battery checker thread.");
        let start = SystemTime::now();
        let mut file = OpenOptions::new().append(true).create(true).open("/portal/logs/battery.txt").expect("Unable to open file.");
        if let Ok(device) = I2c::with_bus(1) {
            println!("I2C initialized.");
            if let Ok(mut ina) = SyncIna219::new(device, Address::from_byte(0x40).unwrap()) {
                println!("ina219 initiailized.");
                if let Ok(config) = ina.configuration() {
                    println!("Configuration pulled.");
                    if let Some(time) = config.conversion_time_us() {
                        println!("Conversion time gathered.");
                        let conversion_time = Duration::from_micros(time as u64);
                        thread::sleep(conversion_time);
                        println!("Getting measurement.");
                        if let Ok(Some(_)) = ina.next_measurement() {
                            if let Ok(voltage) = ina.bus_voltage() {
                                self.set_percentage(voltage.voltage_mv());
                            } else {
                                println!("Error checking voltage on startup.");
                            }
                        } else {
                            println!("Error checking for measurement on startup.");
                        }
                        
                        loop {
                            thread::sleep(conversion_time);
                            if let Ok(Some(_)) = ina.next_measurement() {
                                if let Ok(voltage) = ina.bus_voltage() {
                                    match start.elapsed() {
                                        Ok(t) => {
                                            match writeln!(&mut file, "{} - Voltage: {}", t.as_secs(), voltage) {
                                                Ok(_) => {}
                                                Err(e) => { println!("Error trying to write to file. {e}") }
                                            }
                                        },
                                        Err(_) => { }
                                    }
                                    self.set_percentage(voltage.voltage_mv());
                                } else {
                                    println!("Error checking voltage.");
                                }
                            }
                            thread::sleep(Duration::from_secs(5));
                            if let Ok(keepalive) = self.keepalive.lock() {
                                if *keepalive == false {
                                    break;
                                }
                            }
                        }
                    } else {
                        println!("Error getting conversion time for ina219 device.");
                    }
                } else {
                    println!("Error setting configuration for ina219 device.");
                }
            } else {
                println!("Error connecting to ina219 device.")
            }
        } else {
            println!("Error initializing i2c for ina219 device.")
        }
    }

    fn set_percentage(&mut self, voltage: u16) {
        // Voltage is in mV, charging is > 14000 ?
        // 100% - 13600
        //  90% - 13400
        //  80% - 13300
        //  70% - 13200
        //  60% - 13100
        //  50% - 13000
        //  40% - 13000
        //  30% - 12900
        //  20% - 12800
        //  10% - 12000
        //   0% - 10000
        // Discharge is (mostly) linear up to 20% then sharply declines.
        let percentage: u8 = if voltage > 12800 { // check if above 20%
            // this will be incorrect for values above 40% excepting the case of 100%
            // we will be under reporting the battery level at 50% and up
            // charging will be considered anything above 110%
            ((voltage - 12600) / 10) as u8 // 
        } else if voltage > 12000 { // 10% to 20%, each 80 mV is 1%, add to base 10
            (10 + ((voltage - 12000) / 80)) as u8
        } else { // 0% to 10%, each 200 mV is 1%
            ((voltage - 10000) / 200) as u8
        };
        let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(t) => { t.as_secs() }
            Err(_) => { 0 }
        };
        if let Ok(mut control) = self.control.lock() {
            if control.battery > 30 && percentage <= 30 && now > self.last_low + 60 {
                self.notifier.send_notification(notifier::Notification::BatteryLow);
                self.send_notification(APINotification::BatteryLow);
                self.last_low = now;
            } else if control.battery > 15 && percentage <= 15 && now > self.last_crit + 60 {
                self.notifier.send_notification(notifier::Notification::BatteryCritical);
                self.send_notification(APINotification::BatteryCritical);
                self.last_crit = now;
            }
            control.battery = percentage;
        }
        if let Ok(mut screen_opt) = self.screen.lock() {
            if let Some(screen) = &mut *screen_opt {
                screen.update_battery();
            }
        }
    }

    fn send_notification(&self, notification: APINotification) {
        let time = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();
        if let Ok(c_socks) = self.control_sockets.lock() {
            println!("notifying connected sockets");
            for sock in c_socks.iter() {
                if let Some(s) = sock {
                    _ = socket::write_notification(&s, &notification, &time);
                }
            }
        }
        if let Ok(control) = self.control.lock() {
            if control.auto_remote {
                if let Ok(sq) = self.sqlite.lock() {
                    match sq.get_apis() {
                        Ok(apis) => {
                            for api in apis {
                                if api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF {
                                    self.notifier.send_api_notification(&api, notification);
                                    break;
                                }
                            }
                        },
                        Err(e) => {
                            println!("Error trying to get apis: {e}");
                        }
                    }
                }
            }
        }
    }
}