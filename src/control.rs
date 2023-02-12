use crate::{database::{sqlite, self, Database, DBError}, defaults, objects::setting};
use rand::prelude::random;

pub mod cli;
pub mod socket;
pub mod zero_conf;
pub mod keepalive;

pub const SETTING_SIGHTING_PERIOD: &str = "SETTING_SIGHTING_PERIOD";
pub const SETTING_PORTAL_NAME: &str = "SETTING_PORTAL_NAME";
pub const SETTING_CHIP_TYPE: &str = "SETTING_CHIP_TYPE";
pub const SETTING_READ_WINDOW: &str = "SETTING_READ_WINDOW";

pub struct Control {
    pub sighting_period: u32,
    pub name: String,
    pub chip_type: String,
    pub read_window: u8,
}

impl Control {
    pub fn new(sqlite: &sqlite::SQLite) -> Result<Control, database::DBError> {
        let mut output = Control {
            sighting_period: defaults::DEFAULT_SIGHTING_PERIOD,
            name: String::from(""),
            chip_type: String::from(defaults::DEFAULT_CHIP_TYPE),
            read_window: defaults::DEFAULT_READ_WINDOW
        };
        match sqlite.get_setting(SETTING_SIGHTING_PERIOD) {
            Ok(s) => {
                let port: u32 = s.value().parse().unwrap();
                output.sighting_period = port;
            },
            // not found means we use the default
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_SIGHTING_PERIOD),
                    format!("{}", defaults::DEFAULT_SIGHTING_PERIOD)
                )) {
                    Ok(_) => {},
                    Err(e) => return Err(e)
                }
            },
            Err(e) => return Err(e)
        }
        match sqlite.get_setting(SETTING_PORTAL_NAME) {
            Ok(s) => {
                output.name = String::from(s.value());
            },
            Err(DBError::NotFound) => {
                let rval: u8 = random();
                let n = format!("Chrono Portal {}", rval);
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_PORTAL_NAME),
                    n,
                )) {
                    Ok(s) => {
                        output.name = String::from(s.value());
                        println!("Name successfully set to '{}'.", s.value());
                    }
                    Err(e) => return Err(e)
                }
            }
            Err(e) => {
                return Err(e)
            }
        }
        match sqlite.get_setting(SETTING_CHIP_TYPE) {
            Ok(s) => {
                output.chip_type = String::from(s.value());
            },
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_CHIP_TYPE),
                    String::from(defaults::DEFAULT_CHIP_TYPE),
                )) {
                    Ok(s) => {
                        output.chip_type = String::from(s.value());
                        println!("Chip type successfully set to '{}'.", s.value());
                    },
                    Err(e) => return Err(e)
                }
            }
            Err(e) => {
                return Err(e)
            }
        }
        match sqlite.get_setting(SETTING_READ_WINDOW) {
            Ok(s) => {
                let rw: u8 = s.value().parse().unwrap();
                output.read_window = rw;
            },
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_READ_WINDOW),
                    format!("{}", defaults::DEFAULT_READ_WINDOW),
                )) {
                    Ok(s) => {
                        let rw: u8 = s.value().parse().unwrap();
                        output.read_window = rw;
                        println!("Read window successfully set to '{}'.", s.value());
                    },
                    Err(e) => return Err(e)
                }
            }
            Err(e) => {
                return Err(e)
            }
        }
        Ok(output)
    }
}