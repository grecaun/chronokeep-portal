use crate::{database::{self, sqlite, DBError, Database}, defaults, objects::setting, sound_board::{SoundBoard, Voice}};
use rand::prelude::random;

pub mod socket;
pub mod zero_conf;
pub mod sound;

pub const SETTING_SIGHTING_PERIOD: &str = "SETTING_SIGHTING_PERIOD";
pub const SETTING_PORTAL_NAME: &str = "SETTING_PORTAL_NAME";
pub const SETTING_CHIP_TYPE: &str = "SETTING_CHIP_TYPE";
pub const SETTING_READ_WINDOW: &str = "SETTING_READ_WINDOW";
pub const SETTING_PLAY_SOUND: &str = "SETTING_PLAY_SOUND";
pub const SETTING_VOLUME: &str = "SETTING_VOLUME";
pub const SETTING_VOICE: &str = "SETTING_VOICE";
pub const SETTING_AUTO_REMOTE: &str = "SETTING_AUTO_REMOTE";
pub const SETTING_UPLOAD_INTERVAL: &str = "SETTING_UPLOAD_INTERVAL";
pub const SETTING_NTFY_URL: &str = "SETTING_NTFY_URL";
pub const SETTING_NTFY_USER: &str = "SETTING_NTFY_USER";
pub const SETTING_NTFY_PASS: &str = "SETTING_NTFY_PASS";
pub const SETTING_NTFY_TOPIC: &str = "SETTING_NTFY_TOPIC";
pub const SETTING_ENABLE_NTFY: &str = "SETTING_ENABLE_NTFY";

pub struct Control {
    pub name: String,
    pub sighting_period: u32,
    pub read_window: u8,
    pub chip_type: String,
    pub play_sound: bool,
    pub volume: f32,
    pub sound_board: SoundBoard,
    pub auto_remote: bool,
    pub upload_interval: u64,
    pub ntfy_url: String,
    pub ntfy_user: String,
    pub ntfy_pass: String,
    pub ntfy_topic: String,
    pub enable_ntfy: bool,
    pub battery: u8,
}

impl Control {
    pub fn update(&mut self, new_control: Control) -> Result<(), ()> {
        if self.name != new_control.name {
            self.name = new_control.name
        }
        if self.sighting_period != new_control.sighting_period {
            self.sighting_period = new_control.sighting_period
        }
        if self.read_window != new_control.read_window {
            self.read_window = new_control.read_window
        }
        if self.chip_type != new_control.chip_type {
            self.chip_type = new_control.chip_type
        }
        if self.play_sound != new_control.play_sound {
            self.play_sound = new_control.play_sound
        }
        if self.volume != new_control.volume {
            self.volume = new_control.volume
        }
        if self.auto_remote != new_control.auto_remote {
            self.auto_remote = new_control.auto_remote
        }
        if self.upload_interval != new_control.upload_interval {
            self.upload_interval = new_control.upload_interval   
        }
        if self.ntfy_url != new_control.ntfy_url {
            self.ntfy_url = new_control.ntfy_url   
        }
        if self.ntfy_user != new_control.ntfy_user {
            self.ntfy_user = new_control.ntfy_user   
        }
        if self.ntfy_pass != new_control.ntfy_pass {
            self.ntfy_pass = new_control.ntfy_pass   
        }
        if self.ntfy_topic != new_control.ntfy_topic {
            self.ntfy_topic = new_control.ntfy_topic   
        }
        if self.enable_ntfy != new_control.enable_ntfy {
            self.enable_ntfy = new_control.enable_ntfy
        }
        if self.sound_board.get_voice() != new_control.sound_board.get_voice() {
            return self.sound_board.change_voice(new_control.sound_board.get_voice())
        }
        Ok(())
    }

    pub fn new(sqlite: &sqlite::SQLite) -> Result<Control, database::DBError> {
        let mut output = Control {
            sighting_period: defaults::DEFAULT_SIGHTING_PERIOD,
            name: String::from(""),
            chip_type: String::from(defaults::DEFAULT_CHIP_TYPE),
            read_window: defaults::DEFAULT_READ_WINDOW,
            play_sound: defaults::DEFAULT_PLAY_SOUND,
            volume: defaults::DEFAULT_VOLUME,
            sound_board: SoundBoard::new(defaults::DEFAULT_VOICE),
            auto_remote: defaults::DEFAULT_AUTO_REMOTE,
            upload_interval: defaults::DEFAULT_UPLOAD_INTERVAL,
            ntfy_url: String::from(""),
            ntfy_user: String::from(""),
            ntfy_pass: String::from(""),
            ntfy_topic: String::from(""),
            enable_ntfy: defaults::DEFAULT_ENABLE_NTFY,
            battery: 0
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
        match sqlite.get_setting(SETTING_PLAY_SOUND) {
            Ok(s) => {
                let ps: bool = s.value().eq_ignore_ascii_case("true");
                output.play_sound = ps;
            },
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_PLAY_SOUND),
                    format!("{}", defaults::DEFAULT_PLAY_SOUND),
                )) {
                    Ok(s) => {
                        let ps: bool = s.value().eq_ignore_ascii_case("true");
                        output.play_sound = ps;
                        println!("Play sound successfully set to '{}'.", s.value());
                    },
                    Err(e) => return Err(e)
                }
            },
            Err(e) => {
                return Err(e)
            }
        }
        match sqlite.get_setting(SETTING_VOLUME) {
            Ok(s) => {
                let vol: f32 = s.value().parse().unwrap();
                output.volume = vol;
            },
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_VOLUME),
                    format!("{}", defaults::DEFAULT_VOLUME)
                )) {
                    Ok(s) => {
                        let vol: f32 = s.value().parse().unwrap();
                        output.volume = vol;
                        println!("Volume successfully set to '{}'.", s.value());
                    },
                    Err(e) => return Err(e)
                }
            },
            Err(e) => {
                return Err(e)
            }
        }
        match sqlite.get_setting(SETTING_VOICE) {
            Ok(s) => {
                _ = output.sound_board.change_voice(Voice::from_str(s.value()));
            },
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_VOICE),
                    String::from(Voice::Emily.as_str())
                )) {
                    Ok(s) => {
                        if let Ok(_) = output.sound_board.change_voice(Voice::from_str(s.value())){
                            println!("Voice successfully set as '{}'.", s.value());
                        }
                    },
                    Err(e) => return Err(e)
                }
            },
            Err(e) => {
                return Err(e)
            }
        }
        match sqlite.get_setting(SETTING_AUTO_REMOTE) {
            Ok(s) => {
                let ar: bool = s.value().eq_ignore_ascii_case("true");
                output.auto_remote = ar;
            },
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_AUTO_REMOTE),
                    format!("{}", defaults::DEFAULT_AUTO_REMOTE),
                )) {
                    Ok(s) => {
                        let ar: bool = s.value().eq_ignore_ascii_case("true");
                        output.auto_remote = ar;
                        println!("Auto remote successfully set to '{}'.", s.value());
                    },
                    Err(e) => return Err(e)
                }
            },
            Err(e) => {
                return Err(e)
            }
        }
        match sqlite.get_setting(SETTING_UPLOAD_INTERVAL) {
            Ok(s) => {
                let up: u64 = s.value().parse().unwrap();
                output.upload_interval = up;
            },
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_UPLOAD_INTERVAL),
                    format!("{}", defaults::DEFAULT_UPLOAD_INTERVAL))) {
                        Ok(s) => {
                            let up: u64 = s.value().parse().unwrap();
                            output.upload_interval = up;
                            println!("Upload interval successfully set to '{}'.", s.value());
                        },
                        Err(e) => return Err(e)
                    }
            }
            Err(e) => {
                return Err(e)
            }
        }
        match sqlite.get_setting(SETTING_NTFY_URL) {
            Ok(s) => {
                output.ntfy_url = String::from(s.value());
            },
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_NTFY_URL),
                    String::new(),
                )) {
                    Ok(s) => {
                        output.ntfy_url = String::from(s.value());
                        println!("NTFY url successfully set to '{}'.", s.value());
                    },
                    Err(e) => return Err(e)
                }
            }
            Err(e) => {
                return Err(e)
            }
        }
        match sqlite.get_setting(SETTING_NTFY_USER) {
            Ok(s) => {
                output.ntfy_user = String::from(s.value());
            },
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_NTFY_USER),
                    String::new(),
                )) {
                    Ok(s) => {
                        output.ntfy_user = String::from(s.value());
                        println!("NTFY user successfully set to '{}'.", s.value());
                    },
                    Err(e) => return Err(e)
                }
            }
            Err(e) => {
                return Err(e)
            }
        }
        match sqlite.get_setting(SETTING_NTFY_PASS) {
            Ok(s) => {
                output.ntfy_pass = String::from(s.value());
            },
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_NTFY_PASS),
                    String::new(),
                )) {
                    Ok(s) => {
                        output.ntfy_pass = String::from(s.value());
                        println!("NTFY pass successfully set to '{}'.", s.value());
                    },
                    Err(e) => return Err(e)
                }
            }
            Err(e) => {
                return Err(e)
            }
        }
        match sqlite.get_setting(SETTING_NTFY_TOPIC) {
            Ok(s) => {
                output.ntfy_topic = String::from(s.value());
            },
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_NTFY_TOPIC),
                    String::new(),
                )) {
                    Ok(s) => {
                        output.ntfy_topic = String::from(s.value());
                        println!("NTFY topic successfully set to '{}'.", s.value());
                    },
                    Err(e) => return Err(e)
                }
            }
            Err(e) => {
                return Err(e)
            }
        }
        match sqlite.get_setting(SETTING_ENABLE_NTFY) {
            Ok(s) => {
                let e_ntfy: bool = s.value().eq_ignore_ascii_case("true");
                output.enable_ntfy = e_ntfy;
            },
            Err(DBError::NotFound) => {
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(SETTING_ENABLE_NTFY),
                    format!("{}", defaults::DEFAULT_ENABLE_NTFY),
                )) {
                    Ok(s) => {
                        let e_ntfy: bool = s.value().eq_ignore_ascii_case("true");
                        output.enable_ntfy = e_ntfy;
                        println!("Enable ntfy successfully set to '{}'.", s.value());
                    },
                    Err(e) => return Err(e)
                }
            },
            Err(e) => {
                return Err(e)
            }
        }
        Ok(output)
    }
}