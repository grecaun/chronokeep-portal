use std::sync::{Arc, Mutex};
#[cfg(target_os = "linux")]
use std::time::Duration;
#[cfg(target_os = "linux")]
use rppal::gpio::Gpio;

use crate::{control::Control, database::sqlite, screen::CharacterDisplay};

pub struct Buttons {
    _sqlite: Arc<Mutex<sqlite::SQLite>>,
    _control: Arc<Mutex<Control>>,
    _screen: Arc<Mutex<Option<CharacterDisplay>>>,
    keepalive: Arc<Mutex<bool>>,
    #[cfg(target_os = "linux")]
    up_button: u8,
    #[cfg(target_os = "linux")]
    down_button: u8,
    #[cfg(target_os = "linux")]
    left_button: u8,
    #[cfg(target_os = "linux")]
    right_button: u8,
}

impl Buttons {
    pub fn new(
        sqlite: Arc<Mutex<sqlite::SQLite>>,
        control: Arc<Mutex<Control>>,
        screen: Arc<Mutex<Option<CharacterDisplay>>>,
        keepalive: Arc<Mutex<bool>>,
        _up_button: u8,
        _down_button: u8,
        _left_button: u8,
        _right_button: u8
    ) -> Self {
        Self {
            _sqlite: sqlite,
            _control: control,
            _screen: screen,
            keepalive,
            #[cfg(target_os = "linux")]
            up_button: _up_button,
            #[cfg(target_os = "linux")]
            down_button: _down_button,
            #[cfg(target_os = "linux")]
            left_button: _left_button,
            #[cfg(target_os = "linux")]
            right_button: _right_button,
        }
    }

    pub fn run(&self) {
        #[cfg(target_os = "linux")]
        {
            let mut aGpio = Gpio::new()?;
            let mut btns = [
                aGpio.get(self.up_button)?.into_input(),
                aGpio.get(self.down_button)?.into_input(),
                aGpio.get(self.left_button)?.into_input(),
                aGpio.get(self.right_button)?.into_input(),
            ];
        }
        loop {
            if let Ok(keepalive) = self.keepalive.try_lock() {
                if *keepalive == false {
                    println!("Button thread stopping.");
                    break;
                }
            }
            #[cfg(target_os = "linux")]
            {
                if let Ok(result) = aGpio.poll_interrupts(&btns, false, Some(Duration::from_millis(250))) {
                    match result {
                        Some((pin, event)) => {
                            if pin.pin == self.up_button {
                                println!("Up button pressed.");
                            } else if pin.pin == self.down_button {
                                println!("Down button pressed.");
                            } else if pin.pin == self.left_button {
                                println!("Left button pressed.");
                            } else if pin.pin == self.right_button {
                                println!("Right button pressed.");
                            } else {
                                println!("Unknown button pressed. GPIO {}", pin.pin);
                            }
                        },
                        None => { }
                    }
                }
            }
        }
        println!("Button thread terminated.");
    }

    pub fn stop(&self) {
        if let Ok(mut keepalive) = self.keepalive.lock() {
            *keepalive = false;
        }
    }
}