use std::sync::{Arc, Mutex};

#[cfg(target_os = "linux")]
use std::fmt::Write;
#[cfg(target_os = "linux")]
use std::sync::Condvar;
#[cfg(target_os = "linux")]
use i2c_character_display::{AdafruitLCDBackpack, LcdDisplayType};
#[cfg(target_os = "linux")]
use rppal::{hal, i2c::I2c};

#[derive(Clone)]
pub struct CharacterDisplay {
    _keepalive: Arc<Mutex<bool>>,
    #[cfg(target_os = "linux")]
    _waiter: Arc<(Mutex<bool>, Condvar)>,
    #[cfg(target_os = "linux")]
    _messages: Arc<Mutex<Vec<String>>>,
}

impl CharacterDisplay {
    pub fn new(
        keepalive: Arc<Mutex<bool>>
    ) -> Self {
        Self {
            _keepalive: keepalive,
            #[cfg(target_os = "linux")]
            _waiter: Arc::new((Mutex::new(true), Condvar::new())),
            #[cfg(target_os = "linux")]
            _messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[cfg(target_os = "linux")]
    pub fn run(&self, bus: u8) {
        println!("Attempting to connect to screen on i2c{bus}.");
        let i2c_res = I2c::with_bus(bus);
        if let Err(ref e) = i2c_res {
            println!("Error connecting to screen on bus {bus}. {e}");
            return;
        }
        let i2c = i2c_res.unwrap();
        let delay = hal::Delay::new();
        let mut lcd = AdafruitLCDBackpack::new(i2c, LcdDisplayType::Lcd20x4, delay);
        println!("Initializing the lcd.");
        if let Err(e) = lcd.init() {
            println!("Error initializing lcd. {e}");
            return;
        }
        if let Err(e) = lcd.backlight(true) {
            println!("Error setting lcd backlight. {e}");
            return;
        }
        if let Err(e) = lcd.clear() {
            println!("Error clearing lcd. {e}");
            return;
        }
        if let Err(e) = lcd.home() {
            println!("Error homing cursor. {e}");
            return;
        }
        if let Err(e) = lcd.print("01234567890123456789012345678901234567890123456789012345678901234567890123456789") {
            println!("Error printing to lcd screen. {e}");
            return;
        }
        loop {
            if let Ok(keepalive) = self._keepalive.try_lock() {
                if *keepalive == false {
                    println!("LCD thread stopping.");
                    break;
                }
            }
            let (lock, cvar) = &*self._waiter;
            let mut waiting = lock.lock().unwrap();
            while *waiting {
                waiting = cvar.wait(waiting).unwrap();
            }
            if let Ok(mut messages) = self._messages.try_lock() {
                let _ = lcd.clear();
                let _ = lcd.home();
                if messages.len() > 0 && messages.len() <= 4 {
                    for msg in &*messages {
                        let _ = writeln!(lcd, "{msg}");
                    }
                }
                messages.clear();
            }
            *waiting = true;
        }
        let _ = lcd.clear();
        let _ = lcd.backlight(false);
        let _ = lcd.show_display(false);
        println!("LCD thread terminated.");
    }

    #[cfg(target_os = "linux")]
    pub fn stop(&self) {
        if let Ok(mut keepalive) = self._keepalive.lock() {
            *keepalive = false;
        }
        let (lock, cvar) = &*self._waiter;
        let mut waiting = lock.lock().unwrap();
        *waiting = false;
        cvar.notify_one();
    }

    pub fn print(&self, mut _new_messages: Vec<String>) {
        #[cfg(target_os = "linux")]
        {
            if let Ok(mut messages) = self._messages.try_lock() {
                messages.append(&mut _new_messages);
            }
            let (lock, cvar) = &*self._waiter;
            let mut waiting = lock.lock().unwrap();
            *waiting = false;
            cvar.notify_one();
        }
    }
}