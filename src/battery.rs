use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ina219::address::Address;
use ina219::SyncIna219;
use rppal::i2c::I2c;
use chrono::Utc;
use std::net::TcpStream;
use chrono::{DateTime, Local};

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
                                    self.set_percentage(voltage.voltage_mv());
                                } else {
                                    println!("Error checking voltage.");
                                }
                            }
                            thread::sleep(Duration::from_secs(1));
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
        // Voltage is in mV
        // CHG  -- >  13800
        // 100% -- >  13550
        //  90% -- >  13180
        //  80% -- >  13170
        //  70% -- >  13160
        //  60% -- >  13150
        //  50% -- >  13100
        //  40% -- >  13050
        //  30% -- >  13030
        //  20% -- >  13010
        //  10% -- >  12990
        //   0% -- <= 12990
        // Discharge is (mostly) linear up to 20% then sharply declines.
        let percentage: u8 = if voltage > 13800 { 
            // charging will be considered anything above 110%
            150
        } else if voltage > 13550 {
            100
        } else if voltage > 13180 {
            90
        } else if voltage > 13170 {
            80
        } else if voltage > 13160 {
            70
        } else if voltage > 13150 {
            60
        } else if voltage > 13100 {
            50
        } else if voltage > 13050 {
            40
        } else if voltage > 13030 {
            30
        } else if voltage > 13010 {
            20
        } else if voltage > 12990 {
            10
        } else {
            0
        };
        let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(t) => { t.as_secs() }
            Err(_) => { 0 }
        };
        let mut batt = 0;
        if let Ok(mut control) = self.control.lock() {
            batt = control.battery;
            control.battery = percentage;
        }
        if batt > 30 && percentage <= 30 && now > self.last_low + 60 {
            let date_time: DateTime<Local> = SystemTime::now().into();
            self.notifier.send_notification(notifier::Notification::BatteryLow, format!("{}", date_time.format("%Y/%m/%d %T")));
            self.send_notification(APINotification::BatteryLow);
            self.last_low = now;
        } else if batt > 15 && percentage <= 15 && now > self.last_crit + 60 {
            let date_time: DateTime<Local> = SystemTime::now().into();
            self.notifier.send_notification(notifier::Notification::BatteryCritical, format!("{}", date_time.format("%Y/%m/%d %T")));
            self.send_notification(APINotification::BatteryCritical);
            self.last_crit = now;
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