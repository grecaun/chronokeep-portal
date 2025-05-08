use std::sync::{Arc, Mutex};

use crate::control::Control;

pub enum Notification {
    Start,
    Stop,
    BatteryLow,
    BatteryCritical,
    BatteryUnknown,
    Location,
}

pub fn send_notification(control: Arc<Mutex<Control>>, note: Notification) {
    if let Ok(control) = control.lock() {
        let message = match note {
            Notification::Start => {
                format!("{} has started.", control.name)
            },
            Notification::Stop => {
                format!("{} is shutting down.", control.name)
            },
            Notification::BatteryLow => {
                format!("Battery is low on {}.", control.name)
            },
            Notification::BatteryCritical => {
                format!("Warning! Battery critical on {}.", control.name)
            },
            Notification::BatteryUnknown => {
                format!("{} is unable to detect the battery level.", control.name)
            },
            Notification::Location => {
                format!("Location for {} is...", control.name)
            },
        };
    }
}