#[cfg(target_os = "linux")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "linux")]
use std::time::Duration;
#[cfg(target_os = "linux")]
use rppal::gpio::{Gpio, Trigger};

#[cfg(target_os = "linux")]
use crate::screen::ButtonPress;
#[cfg(target_os = "linux")]
use crate::screen::CharacterDisplay;

#[cfg(target_os = "linux")]
pub struct Buttons {
    _screen: Arc<Mutex<Option<CharacterDisplay>>>,
    keepalive: Arc<Mutex<bool>>,
    up_button: u8,
    down_button: u8,
    left_button: u8,
    right_button: u8,
    enter_button: u8,
}

#[cfg(target_os = "linux")]
impl Buttons {
    pub fn new(
        _screen: Arc<Mutex<Option<CharacterDisplay>>>,
        keepalive: Arc<Mutex<bool>>
    ) -> Self {
        println!("Checking if there are buttons we should be reading from.");
        let mut up_button: u8 = 0;
        if let Ok(btn) = std::env::var("PORTAL_UP_BUTTON") {
            up_button = btn.parse().unwrap_or(0);
        }
        let mut down_button: u8 = 0;
        if let Ok(btn) = std::env::var("PORTAL_DOWN_BUTTON") {
            down_button = btn.parse().unwrap_or(0);
        }
        let mut left_button: u8 = 0;
        if let Ok(btn) = std::env::var("PORTAL_LEFT_BUTTON") {
            left_button = btn.parse().unwrap_or(0);
        }
        let mut right_button: u8 = 0;
        if let Ok(btn) = std::env::var("PORTAL_RIGHT_BUTTON") {
            right_button = btn.parse().unwrap_or(0);
        }
        let mut enter_button: u8 = 0;
        if let Ok(btn) = std::env::var("PORTAL_ENTER_BUTTON") {
            enter_button = btn.parse().unwrap_or(0);
        }
        Self {
            _screen,
            keepalive,
            up_button,
            down_button,
            left_button,
            right_button,
            enter_button,
        }
    }

    pub fn run(&self) {
        let a_gpio = Gpio::new().unwrap();
        if let (Ok(up_btn),
                Ok(down_btn),
                Ok(left_btn),
                Ok(right_btn),
                Ok(enter_btn))
             = (a_gpio.get(self.up_button),
                a_gpio.get(self.down_button),
                a_gpio.get(self.left_button),
                a_gpio.get(self.right_button),
                a_gpio.get(self.enter_button)) {
            let mut up = up_btn.into_input_pullup();
            let _ = up.set_interrupt(Trigger::RisingEdge, Some(Duration::from_millis(100)));
            let mut down = down_btn.into_input_pullup();
            let _ = down.set_interrupt(Trigger::RisingEdge, Some(Duration::from_millis(100)));
            let mut left = left_btn.into_input_pullup();
            let _ = left.set_interrupt(Trigger::RisingEdge, Some(Duration::from_millis(100)));
            let mut right = right_btn.into_input_pullup();
            let _ = right.set_interrupt(Trigger::RisingEdge, Some(Duration::from_millis(100)));
            let mut enter = enter_btn.into_input_pullup();
            let _ = enter.set_interrupt(Trigger::RisingEdge, Some(Duration::from_millis(100)));
            let btns = [
                &up,
                &down,
                &left,
                &right,
                &enter,
            ];
            println!("Entering button thread loop.");
            loop {
                if let Ok(keepalive) = self.keepalive.try_lock() {
                    if *keepalive == false {
                        println!("Button thread stopping.");
                        break;
                    }
                }
                if let Ok(result) = a_gpio.poll_interrupts(&btns, false, Some(Duration::from_millis(1250))) {
                    match result {
                        Some((pin, _event)) => {
                            let p = pin.pin();
                            if let Ok(guarded_screen) = self._screen.try_lock() {
                                if let Some(screen) = &*guarded_screen {
                                    if p == self.up_button {
                                        screen.register_button(ButtonPress::Up);
                                    } else if p == self.down_button {
                                        screen.register_button(ButtonPress::Down);
                                    } else if p == self.left_button {
                                        screen.register_button(ButtonPress::Left);
                                    } else if p == self.right_button {
                                        screen.register_button(ButtonPress::Right);
                                    } else if p == self.enter_button {
                                        screen.register_button(ButtonPress::Enter);
                                    } else {
                                        println!("Unknown button pressed. GPIO {p}");
                                    }
                                }
                            }
                        },
                        None => {}
                    }
                }
            }
        } else {
            println!("Unable to get buttons.");
        }
        println!("Button thread terminated.");
    }

    pub fn stop(&self) {
        if let Ok(mut keepalive) = self.keepalive.lock() {
            *keepalive = false;
        }
    }
}