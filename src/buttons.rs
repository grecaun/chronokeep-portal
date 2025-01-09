#[cfg(target_os = "linux")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "linux")]
use std::time::Duration;
#[cfg(target_os = "linux")]
use rppal::gpio::{Gpio, Trigger};

#[cfg(target_os = "linux")]
use crate::{control::Control, database::sqlite, screen::CharacterDisplay};

#[cfg(target_os = "linux")]
pub struct Buttons {
    _sqlite: Arc<Mutex<sqlite::SQLite>>,
    _control: Arc<Mutex<Control>>,
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
        _sqlite: Arc<Mutex<sqlite::SQLite>>,
        _control: Arc<Mutex<Control>>,
        _screen: Arc<Mutex<Option<CharacterDisplay>>>,
        keepalive: Arc<Mutex<bool>>,
        up_button: u8,
        down_button: u8,
        left_button: u8,
        right_button: u8,
        enter_button: u8
    ) -> Self {
        Self {
            _sqlite,
            _control,
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
                            if p == self.up_button {
                                println!("Up button pressed.");
                            } else if p == self.down_button {
                                println!("Down button pressed.");
                            } else if p == self.left_button {
                                println!("Left button pressed.");
                            } else if p == self.right_button {
                                println!("Right button pressed.");
                            } else if p == self.enter_button {
                                println!("Enter button pressed.");
                            } else {
                                println!("Unknown button pressed. GPIO {p}");
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