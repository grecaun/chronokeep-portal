/*
Chronokeep Portal - RFID Tag Reading Software
Copyright (C) 2026 James Sentinella

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{fmt, sync::{Arc, Mutex}, thread, time::{Duration, Instant}};
use i2c_character_display::{AdafruitLCDBackpack, CharacterDisplayPCF8574T, LcdDisplayType};
use rppal::{hal, i2c::I2c};

// Time between attempts to open the bus or initialize the display.
const RETRY_DELAY: Duration = Duration::from_millis(500);
// Number of failed init attempts at startup before continuing without a working display.
// Re-initialization keeps being attempted from the display loop after this.
const STARTUP_INIT_ATTEMPTS: u32 = 20;
// Minimum time between re-initialization attempts after an error while running.
const REINIT_INTERVAL: Duration = Duration::from_secs(1);

enum Device {
    Pcf(CharacterDisplayPCF8574T<I2c, hal::Delay>),
    Ada(AdafruitLCDBackpack<I2c, hal::Delay>),
}

// Calls the same method on whichever display type is in use and turns the error into a String.
macro_rules! on_device {
    ($self:ident, $lcd:ident => $call:expr) => {
        match &mut $self.device {
            Device::Pcf($lcd) => $call.map(|_| ()).map_err(|e| e.to_string()),
            Device::Ada($lcd) => $call.map(|_| ()).map_err(|e| e.to_string()),
        }
    };
}

/// A 20x4 character LCD on either a PCF8574T or Adafruit backpack.
///
/// Any failed operation marks the display as needing re-initialization, since a failed
/// write can leave the controller in the wrong mode or out of step, which shows up as garbage
/// characters. Call `recover_if_needed` regularly to fix that.
pub struct Lcd {
    device: Device,
    failed: bool,
    last_init: Instant,
}

impl Lcd {
    /// Opens the I2C bus and initializes the display, retrying until it works.
    ///
    /// Opening the bus is retried for as long as `keepalive` is true, since the screen can't work
    /// without it. If the bus opens but the display can't be initialized after several attempts,
    /// the display is returned anyway and `recover_if_needed` keeps trying from the display loop.
    /// Returns None only if `keepalive` turns false first.
    pub fn connect(adafruit: bool, bus: u8, keepalive: &Arc<Mutex<bool>>) -> Option<Lcd> {
        let mut attempt: u32 = 0;
        let mut lcd = loop {
            if !is_alive(keepalive) {
                return None;
            }
            match I2c::with_bus(bus) {
                Ok(i2c) => {
                    let delay = hal::Delay::new();
                    let device = if adafruit {
                        Device::Ada(AdafruitLCDBackpack::new(i2c, LcdDisplayType::Lcd20x4, delay))
                    } else {
                        Device::Pcf(CharacterDisplayPCF8574T::new(i2c, LcdDisplayType::Lcd20x4, delay))
                    };
                    break Lcd { device, failed: true, last_init: Instant::now() };
                }
                Err(e) => {
                    attempt += 1;
                    // Don't fill the log if the bus never shows up.
                    if attempt <= 5 || attempt % 60 == 0 {
                        println!("Error connecting to screen on bus {bus} (attempt {attempt}). {e}");
                    }
                    thread::sleep(RETRY_DELAY);
                }
            }
        };
        for attempt in 1..=STARTUP_INIT_ATTEMPTS {
            if !is_alive(keepalive) {
                return None;
            }
            println!("Initializing the lcd (attempt {attempt}).");
            match lcd.initialize() {
                Ok(()) => {
                    println!("LCD initialized.");
                    return Some(lcd);
                }
                Err(e) => println!("Error initializing lcd. {e}"),
            }
            thread::sleep(RETRY_DELAY);
        }
        println!("Unable to initialize the lcd after {STARTUP_INIT_ATTEMPTS} attempts, will keep retrying in the background.");
        Some(lcd)
    }

    /// Runs the display's setup sequence, which also clears it, homes the cursor and turns on
    /// the backlight. The display stays marked as failed unless it succeeds.
    fn initialize(&mut self) -> Result<(), String> {
        self.last_init = Instant::now();
        on_device!(self, d => d.init())?;
        self.failed = false;
        Ok(())
    }

    /// Re-initializes the display if an earlier operation failed. Returns true if it did,
    /// in which case the display has been cleared and needs to be redrawn.
    pub fn recover_if_needed(&mut self) -> bool {
        if !self.failed || self.last_init.elapsed() < REINIT_INTERVAL {
            return false;
        }
        match self.initialize() {
            Ok(()) => {
                println!("LCD re-initialized after an error.");
                true
            }
            Err(e) => {
                println!("Error re-initializing lcd. {e}");
                false
            }
        }
    }

    fn track(&mut self, res: Result<(), String>) -> Result<(), String> {
        if res.is_err() {
            self.failed = true;
        }
        res
    }

    pub fn clear(&mut self) -> Result<(), String> {
        let res = on_device!(self, d => d.clear());
        self.track(res)
    }

    pub fn home(&mut self) -> Result<(), String> {
        let res = on_device!(self, d => d.home());
        self.track(res)
    }

    pub fn backlight(&mut self, on: bool) -> Result<(), String> {
        let res = on_device!(self, d => d.backlight(on));
        self.track(res)
    }

    pub fn show_display(&mut self, on: bool) -> Result<(), String> {
        let res = on_device!(self, d => d.show_display(on));
        self.track(res)
    }
}

impl fmt::Write for Lcd {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let res = match &mut self.device {
            Device::Pcf(d) => d.write_str(s),
            Device::Ada(d) => d.write_str(s),
        };
        if res.is_err() {
            self.failed = true;
        }
        res
    }
}

fn is_alive(keepalive: &Arc<Mutex<bool>>) -> bool {
    match keepalive.lock() {
        Ok(k) => *k,
        Err(_) => false,
    }
}
