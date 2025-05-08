use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use ina219::address::Address;
use ina219::SyncIna219;
#[cfg(target_os = "linux")]
use rppal::i2c::I2c;

use crate::control::Control;
use crate::screen::CharacterDisplay;

pub struct Checker {
    keepalive: Arc<Mutex<bool>>,
    control: Arc<Mutex<Control>>,
    screen: Arc<Mutex<Option<CharacterDisplay>>>,
}

impl Checker {
    pub fn new(
        keepalive: Arc<Mutex<bool>>,
        control: Arc<Mutex<Control>>,
        screen: Arc<Mutex<Option<CharacterDisplay>>>,
    ) -> Self {
        Self {
            keepalive,
            control,
            screen,
        }
    }

    pub fn run(&self) {
        println!("Starting battery checker thread.");
        #[cfg(target_os = "linux")]
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

    fn set_percentage(&self, voltage: u16) {
        // Voltage is in mV, charging is ~ 14600
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
        if let Ok(mut control) = self.control.lock() {
            control.battery = percentage;
        }
        #[cfg(target_os = "linux")]
        if let Ok(mut screen_opt) = self.screen.lock() {
            if let Some(screen) = &mut *screen_opt {
                screen.update_battery();
                screen.update();
            }
        }
    }
}