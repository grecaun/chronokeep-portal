use std::{env, net::TcpStream, sync::{Arc, Mutex}, thread::{self, JoinHandle}, time::Duration};

#[cfg(target_os = "linux")]
use std::time::SystemTime;
#[cfg(target_os = "linux")]
use std::fmt::Write;
#[cfg(target_os = "linux")]
use chrono::{DateTime, Datelike, Local, Timelike};
#[cfg(target_os = "linux")]
use i2c_character_display::{AdafruitLCDBackpack, LcdDisplayType};
#[cfg(target_os = "linux")]
use rppal::{hal, i2c::I2c};

use crate::{control::{socket::{self, CONNECTION_CHANGE_PAUSE, MAX_CONNECTED, UPDATE_SCRIPT_ENV}, sound::{SoundNotifier, SoundType}, Control, SETTING_AUTO_REMOTE, SETTING_CHIP_TYPE, SETTING_ENABLE_NTFY, SETTING_PLAY_SOUND, SETTING_READ_WINDOW, SETTING_UPLOAD_INTERVAL, SETTING_VOICE, SETTING_VOLUME}, database::{sqlite, Database}, notifier, objects::setting::Setting, processor::{self}, reader::{self, auto_connect, reconnector::Reconnector}, remote::uploader::{self, Status}, sound_board::{Voice}, types::{TYPE_CHIP_DEC, TYPE_CHIP_HEX}};

pub const EMPTY_STRING: &str = "                    ";

pub const MAIN_MENU: u8 = 0;
pub const SETTINGS_MENU: u8 = 1;
pub const READING_MENU: u8 = 2;
pub const ABOUT_MENU: u8 = 3;
pub const SHUTDOWN_MENU: u8 = 4;
pub const STARTUP_MENU: u8 = 5;
pub const RESTART_MENU: u8 = 6;
pub const MANUAL_TIME_MENU: u8 = 7;
pub const UPDATE_MENU: u8 = 8;
pub const DELETE_READS_MENU: u8 = 9;
pub const DELETE_READS_MENU_TWO: u8 = 10;
pub const SCREEN_OFF: u8 = 15;

pub const MAIN_START_READING: u8 = 0;
pub const MAIN_SETTINGS: u8 = 1;
pub const MAIN_ABOUT: u8 = 2;
pub const MAIN_UPDATE: u8 = 3;
pub const MAIN_RESTART: u8 = 4;
pub const MAIN_SHUTDOWN: u8 = 5;

pub const SETTINGS_READ_WINDOW: u8 = 0;
pub const SETTINGS_CHIP_TYPE: u8 = 1;
pub const SETTINGS_PLAY_SOUND: u8 = 2;
pub const SETTINGS_VOLUME: u8 = 3;
pub const SETTINGS_VOICE: u8 = 4;
pub const SETTINGS_AUTO_UPLOAD: u8 = 5;
pub const SETTINGS_UPLOAD_INTERVAL: u8 = 6;
pub const SETTINGS_ENABLE_NTFY: u8 = 7;
pub const SETTINGS_DELETE_CHIP_READS: u8 = 8;
pub const SETTINGS_SET_TIME_WEB: u8 = 9;
pub const SETTINGS_SET_TIME_MANUAL: u8 = 10;

pub const TIME_MENU_YEAR: u8 = 0;
pub const TIME_MENU_MONTH: u8 = 1;
pub const TIME_MENU_DAY: u8 = 2;
pub const TIME_MENU_HOUR: u8 = 3;
pub const TIME_MENU_MINUTE: u8 = 4;
pub const TIME_MENU_SECOND: u8 = 5;

#[derive(Clone)]
pub struct CharacterDisplay {
    keepalive: Arc<Mutex<bool>>,
    control: Arc<Mutex<Control>>,
    readers: Arc<Mutex<Vec<reader::Reader>>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
    read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
    button_presses: Arc<Mutex<Vec<ButtonPress>>>,
    ac_state: Arc<Mutex<auto_connect::State>>,
    read_saver: Arc<processor::ReadSaver>,
    sound: Arc<SoundNotifier>,
    joiners: Arc<Mutex<Vec<JoinHandle<()>>>>,
    info: Arc<Mutex<DisplayInfo>>,
    control_port: u16,
    current_menu: [u8; 3],
    notifier: notifier::Notifier,
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    seconds: u8,
    volume: u8,
}

pub struct DisplayInfo {
    title_bar: String,
    reader_info: Vec<String>,
    main_menu: Vec<String>,
    settings_menu: Vec<String>,
}

pub enum ButtonPress {
    Up,
    Down,
    Left,
    Right,
    Enter
}

impl CharacterDisplay {
    pub fn new(
        keepalive: Arc<Mutex<bool>>,
        control: Arc<Mutex<Control>>,
        readers: Arc<Mutex<Vec<reader::Reader>>>,
        sqlite: Arc<Mutex<sqlite::SQLite>>,
        control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
        read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
        ac_state: Arc<Mutex<auto_connect::State>>,
        read_saver: Arc<processor::ReadSaver>,
        sound: Arc<SoundNotifier>,
        joiners: Arc<Mutex<Vec<JoinHandle<()>>>>,
        control_port: u16,
        notifier: notifier::Notifier,
    ) -> Self {
        Self {
            keepalive,
            control,
            readers,
            sqlite,
            control_sockets,
            read_repeaters,
            button_presses: Arc::new(Mutex::new(Vec::new())),
            info: Arc::new(Mutex::new(DisplayInfo {
                title_bar: format!("{:<20}", "Chronokeep"),
                reader_info: Vec::new(),
                main_menu: vec![
                    " > Start Reading    ".to_string(),
                    "   Settings         ".to_string(),
                    "   About            ".to_string(),
                    "   Update           ".to_string(),
                    "   Restart          ".to_string(),
                    "   Shutdown         ".to_string(),
                ],
                settings_menu: Vec::new(),
            })),
            current_menu: [0, 0, 0],
            ac_state,
            read_saver,
            sound,
            joiners,
            control_port,
            notifier,
            year: 0,
            month: 0,
            day: 0,
            hour: 0,
            minute: 0,
            seconds: 0,
            volume: 10,
        }
    }

    pub fn set_shutdown(&mut self) {
        self.current_menu[0] = SCREEN_OFF;
    }

    pub fn update_upload_status(&mut self, status: uploader::Status, err_count: usize) {
        if let Ok(mut info) = self.info.lock() {
            if err_count > 99 {
                info.title_bar.replace_range(14..16, "99");
            } else if err_count > 0 {
                info.title_bar.replace_range(14..16, format!("{:>2}", err_count).as_str());
            } else {
                let mut upload_status = " ?";
                if status == Status::Running {
                    upload_status = " +";
                } else if status == Status::Stopped || status == Status::Stopping {
                    upload_status = " -";
                }
                info.title_bar.replace_range(14..16, upload_status);
            }
        }
    }

    pub fn update_battery(&mut self) {
        if let Ok(mut info) = self.info.lock() {
            if let Ok(control) = self.control.lock() {
                if control.battery > 135 {
                    info.title_bar.replace_range(17..20, "chg");
                } else if control.battery >= 30 {
                    info.title_bar.replace_range(17..20, " ok");
                } else if control.battery >= 15 {
                    info.title_bar.replace_range(17..20, "low");
                } else {
                    info.title_bar.replace_range(17..20, "cri");
                }
            }
        }
    }

    pub fn update_readers(&mut self) {
        if let Ok(mut info) = self.info.lock() {
            info.reader_info.clear();
            // Collect all connected readers.
            if let Ok(readers) = self.readers.lock() {
                for read in readers.iter() {
                    if let Some(is_con) = read.is_connected() {
                        if is_con {
                            if let Ok(ants) = read.antennas.lock() {
                                info.reader_info.push(
                                    format!("{} {}{}{}{}{}{}{}{}",
                                        read.nickname(),
                                        reader::helpers::antenna_status_str(ants[0]),
                                        reader::helpers::antenna_status_str(ants[1]),
                                        reader::helpers::antenna_status_str(ants[2]),
                                        reader::helpers::antenna_status_str(ants[3]),
                                        reader::helpers::antenna_status_str(ants[4]),
                                        reader::helpers::antenna_status_str(ants[5]),
                                        reader::helpers::antenna_status_str(ants[6]),
                                        reader::helpers::antenna_status_str(ants[7]),
                                    ));
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn update_menu(&mut self) {
        if let Ok(mut info) = self.info.lock() {
            match self.current_menu[0] {
                MAIN_MENU => { // main menu, max ix 4
                    for line in info.main_menu.iter_mut() {
                        line.replace_range(1..2, " ");
                    }
                    info.main_menu[self.current_menu[1] as usize].replace_range(1..2, ">");
                },
                SETTINGS_MENU => { // settings menu, max ix 8
                    for line in info.settings_menu.iter_mut() {
                        line.replace_range(1..2, " ");
                    }
                    info.settings_menu[self.current_menu[1] as usize].replace_range(1..2, ">");
                }
                _ => {}
            }
        }
    }

    pub fn update_settings(&mut self) {
        if let Ok(mut info) = self.info.lock() {
            info.settings_menu.clear();
            if let Ok(control) = self.control.lock() {
                let mut play_sound = "no";
                if control.play_sound {
                    play_sound = "yes";
                }
                let mut auto_upload = "no";
                if control.auto_remote {
                    auto_upload = "yes";
                }
                let mut enable_ntfy = "no";
                if control.enable_ntfy {
                    enable_ntfy = "yes";
                }
                self.volume = (control.volume * 10.0) as u8;
                info.settings_menu.push(format!("   Read Window {:>4} ", control.read_window));
                info.settings_menu.push(format!("   Chip Type   {:>4} ", control.chip_type));
                info.settings_menu.push(format!("   Play Sounds {:>4} ", play_sound));
                info.settings_menu.push(format!("   Volume      {:>4} ", self.volume));
                info.settings_menu.push(format!("   Voice    {:>7} ", control.sound_board.get_voice().as_str()));
                info.settings_menu.push(format!("   Auto Upload {:>4} ", auto_upload));
                info.settings_menu.push(format!("   Upload Int  {:>4} ", control.upload_interval));
                info.settings_menu.push(format!("   Enable NTFY {:>4} ", enable_ntfy));
                info.settings_menu.push(String::from("   Delete Reads     "));
                info.settings_menu.push(String::from("   Set Time (Web)   "));
                info.settings_menu.push(String::from("   Set Time (Manual)"));
            }
            for line in info.settings_menu.iter_mut() {
                line.replace_range(1..2, " ");
            }
            info.settings_menu[self.current_menu[1] as usize].replace_range(1..2, ">");
        }
    }

    pub fn run(&mut self, bus: u8) {
        println!("Screen bus set to {bus}");
        #[cfg(target_os = "linux")]
        println!("Attempting to connect to screen on i2c{bus}.");
        #[cfg(target_os = "linux")]
        let i2c_res = I2c::with_bus(bus);
        #[cfg(target_os = "linux")]
        if let Err(ref e) = i2c_res {
            println!("Error connecting to screen on bus {bus}. {e}");
        }
        #[cfg(target_os = "linux")]
        let i2c = i2c_res.unwrap();
        #[cfg(target_os = "linux")]
        let delay = hal::Delay::new();
        #[cfg(target_os = "linux")]
        let mut lcd = AdafruitLCDBackpack::new(i2c, LcdDisplayType::Lcd20x4, delay);
        #[cfg(target_os = "linux")]
        {
            println!("Initializing the lcd.");
            if let Err(e) = lcd.init() {
                println!("Error initializing lcd. {e}");
            }
            if let Err(e) = lcd.backlight(true) {
                println!("Error setting lcd backlight. {e}");
            }
            if let Err(e) = lcd.clear() {
                println!("Error clearing lcd. {e}");
            }
            if let Err(e) = lcd.home() {
                println!("Error homing cursor. {e}");
            }
            let sys_time = SystemTime::now();
            let date_time: DateTime<Local> = sys_time.into();
            if let Ok(mut info) = self.info.lock() {
                info.title_bar.replace_range(0..14, format!("{:<14}", date_time.format("%m-%d %H:%M:%S")).as_str());
                let mut messages: Vec<String> = vec!(info.title_bar.clone());
                messages.push(info.main_menu[1].clone());
                messages.push(info.main_menu[0].clone());
                messages.push(info.main_menu[2].clone());
                for msg in &*messages {
                    let _ = write!(lcd, "{msg}");
                }
            }
            self.year = date_time.year() as u16;
            self.month = date_time.month() as u8;
            self.day = date_time.day() as u8;
            self.hour = date_time.hour() as u8;
            self.minute = date_time.minute() as u8;
            self.seconds = date_time.second() as u8;
        }
        loop {
            if let Ok(keepalive) = self.keepalive.try_lock() {
                if *keepalive == false {
                    println!("LCD thread stopping.");
                    break;
                }
            }
            if let Ok(mut presses) = self.button_presses.clone().try_lock() {
                for press in &*presses {
                    match press {
                        ButtonPress::Up => {
                            match self.current_menu[0] {
                                MAIN_MENU => {
                                    if self.current_menu[1] > MAIN_START_READING {
                                        self.current_menu[1] -= 1;
                                    } else {
                                        self.current_menu[1] = MAIN_SHUTDOWN;
                                    }
                                }
                                READING_MENU => {
                                    if let Ok(info) = self.info.lock() {
                                        if info.reader_info.len() > 3 {
                                            if self.current_menu[1] > 0 {
                                                self.current_menu[1] -= 1;
                                            } else {
                                                self.current_menu[1] = (info.reader_info.len() - 1) as u8;
                                            }
                                        }
                                    }
                                }
                                SETTINGS_MENU => {
                                    if self.current_menu[1] > SETTINGS_READ_WINDOW {
                                        self.current_menu[1] -= 1;
                                    } else {
                                        self.current_menu[1] = SETTINGS_SET_TIME_MANUAL;
                                    }
                                }
                                ABOUT_MENU | STARTUP_MENU => {
                                    self.current_menu[0] = MAIN_MENU;
                                    self.current_menu[1] = MAIN_START_READING;
                                    self.update_menu();
                                }
                                SHUTDOWN_MENU | RESTART_MENU | UPDATE_MENU | DELETE_READS_MENU | DELETE_READS_MENU_TWO => {
                                    self.current_menu[1] = (self.current_menu[1] + 1) % 2;
                                },
                                MANUAL_TIME_MENU => {
                                    match self.current_menu[1] {
                                        TIME_MENU_YEAR => {
                                            if self.year < 2200 { // max of 175 years from the day this is typed
                                                self.year += 1;
                                            }
                                        },
                                        TIME_MENU_MONTH => {
                                            if self.month < 12 {
                                                self.month += 1;
                                            } else { // overflow to JAN
                                                self.month = 1;
                                            }
                                            // lower day if day value isn't valid for the month
                                            if (self.month == 4 || self.month == 6 || self.month == 9 || self.month == 11) && self.day > 30 {
                                                self.day = 30;
                                            } else if self.month == 2 && self.day > 28 {
                                                // Check for leap year and set to 29 if leap year, otherwise 28. Stays 29 if it was already 29.
                                                if self.year % 400 == 0 || (self.year % 4 == 0 && self.year % 100 != 0) {
                                                    self.day = 29;
                                                } else {
                                                    self.day = 28;
                                                }
                                            }
                                        },
                                        TIME_MENU_DAY => {
                                            if ((self.month == 1            // January, March, May, July, August, October, and December have 31 days
                                                || self.month == 3
                                                || self.month == 5
                                                || self.month == 7
                                                || self.month == 8
                                                || self.month == 10
                                                || self.month == 12)
                                                && self.day < 31)           // JAN, MAR, MAY, JUL, AUG, OCT, DEC
                                                || ((self.month == 4        // April, June, September, and November have 30 days
                                                || self.month == 6
                                                || self.month == 9
                                                || self.month == 11)
                                                && self.day < 30)           // APR, JUN, SEP, NOV
                                                || ((self.year % 400 == 0   // it is a leap year when divisible by 400
                                                || (self.year % 4 == 0 && self.year % 100 != 0)) // or divisible 4 but not 100
                                                && self.day < 29)           // FEB - Leap year
                                                || self.day < 28 {          // FEB - Non leap year
                                                self.day += 1;
                                            } else { // overflow to the first of the month
                                                self.day = 1;
                                            }
                                        },
                                        TIME_MENU_HOUR => {
                                            if self.hour < 23 { // max 23, 24 would be 00
                                                self.hour += 1;
                                            } else { // overflow to midnight
                                                self.hour = 0;
                                            }
                                        },
                                        TIME_MENU_MINUTE => {
                                            if self.minute < 59 { // max 59, 60 would be 00
                                                self.minute += 1;
                                            } else { // overflow to 0
                                                self.minute = 0;
                                            }
                                        },
                                        TIME_MENU_SECOND => {
                                            if self.seconds < 59 { // max 59, 60 would be 00
                                                self.seconds += 1;
                                            } else { // overflow to 0
                                                self.seconds = 0;
                                            }
                                        },
                                        _ => {}
                                    }
                                },
                                _ => {}, // 2 = currently reading, do nothing
                            }
                            self.current_menu[2] = 0; // current_menu[2] is only used for proper stop reading command
                            self.update_menu();
                        },
                        ButtonPress::Down => {
                            match self.current_menu[0] {
                                MAIN_MENU => { // main menu, max ix MAIN_SHUTDOWN
                                    if self.current_menu[1] < MAIN_SHUTDOWN {
                                        self.current_menu[1] += 1;
                                    } else { // wrap around to the start
                                        self.current_menu[1] = MAIN_START_READING;
                                    }
                                },
                                READING_MENU => {
                                    if let Ok(info) = self.info.lock() {
                                        if info.reader_info.len() > 3 {
                                            if self.current_menu[1] < (info.reader_info.len() - 1) as u8 {
                                                self.current_menu[1] += 1;
                                            } else {
                                                self.current_menu[1] = 0;
                                            }
                                        }
                                    }
                                }
                                SETTINGS_MENU => { // settings menu, max ix SETTINGS_SET_TIME_MANUAL
                                    if self.current_menu[1] < SETTINGS_SET_TIME_MANUAL {
                                        self.current_menu[1] += 1;
                                    } else { // wrap around to 0
                                        self.current_menu[1] = SETTINGS_READ_WINDOW;
                                    }
                                }
                                ABOUT_MENU | STARTUP_MENU => { // 3 == about
                                    self.current_menu[0] = MAIN_MENU;
                                    self.current_menu[1] = MAIN_START_READING;
                                    self.update_menu();
                                }
                                SHUTDOWN_MENU | RESTART_MENU | UPDATE_MENU | DELETE_READS_MENU | DELETE_READS_MENU_TWO => {
                                    self.current_menu[1] = (self.current_menu[1] + 1) % 2;
                                },
                                MANUAL_TIME_MENU => {
                                    match self.current_menu[1] {
                                        TIME_MENU_YEAR => {
                                            if self.year > 2020 { // min year is 2020
                                                self.year -= 1;
                                            } // No rollover here
                                        },
                                        TIME_MENU_MONTH => {
                                            if self.month > 1 { // min month is january (1)
                                                self.month -= 1;
                                            } else {
                                                self.month = 12; // roll down to december
                                            }
                                            // lower day if day value isn't valid for the month
                                            if (self.month == 4 || self.month == 6 || self.month == 9 || self.month == 11) && self.day > 30 {
                                                self.day = 30;
                                            } else if self.month == 2 && self.day > 28 {
                                                // Check for leap year and set to 29 if leap year, otherwise 28. Stays 29 if it was already 29.
                                                if self.year % 400 == 0 || (self.year % 4 == 0 && self.year % 100 != 0) {
                                                    self.day = 29;
                                                } else {
                                                    self.day = 28;
                                                }
                                            }
                                        },
                                        TIME_MENU_DAY => {  // min day is 1
                                            if self.day > 1 {
                                                self.day -= 1;
                                            } else {
                                                // Roll day to max day of the year - JAN, MAR, MAY, JUL, AUG, OCT, DEC have 31
                                                if self.month == 1 || self.month == 3 || self.month == 5 || self.month == 7
                                                    || self.month == 8 || self.month == 10 || self.month == 12 {
                                                    self.day = 31;
                                                // APR, JUN, SEP, NOV have 31
                                                } else if self.month == 4 || self.month == 6 || self.month == 9 || self.month == 11 {
                                                    self.day = 30;
                                                // Leaving FEB - Leap years have 29 and non-leap years 28.
                                                } else if self.year % 400 == 0 || (self.year % 4 == 0 && self.year % 100 != 0) {
                                                    self.day = 29;
                                                } else {
                                                    self.day = 28;
                                                }
                                            }
                                        },
                                        TIME_MENU_HOUR => {
                                            if self.hour > 0 { // min hour is 0 (midnight)
                                                self.hour -= 1;
                                            } else { // roll down to the last hour of the day
                                                self.hour = 23;
                                            }
                                        },
                                        TIME_MENU_MINUTE => { // 0 min
                                            if self.minute > 0 {
                                                self.minute -= 1;
                                            } else {
                                                self.minute = 59;
                                            }
                                        },
                                        TIME_MENU_SECOND => { // 0 min
                                            if self.seconds > 0 {
                                                self.seconds -= 1;
                                            } else {
                                                self.seconds = 59;
                                            }
                                        },
                                        _ => {}
                                    }
                                },
                                _ => {}, // 2 = currently reading, do nothing
                            }
                            self.current_menu[2] = 0;
                            self.update_menu();
                        },
                        ButtonPress::Left => {
                            match self.current_menu[0] {
                                SETTINGS_MENU => {
                                    if let Ok(mut control) = self.control.lock() {
                                        match self.current_menu[1] {
                                            SETTINGS_READ_WINDOW => {  // Read Window
                                                if control.read_window > 5 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.read_window -= 1;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_READ_WINDOW.to_string(), control.read_window.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                            SETTINGS_CHIP_TYPE => {  // Chip Type
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    if control.chip_type == TYPE_CHIP_HEX {
                                                        control.chip_type = TYPE_CHIP_DEC.to_string();
                                                    } else {
                                                        control.chip_type = TYPE_CHIP_HEX.to_string();
                                                    }
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_CHIP_TYPE.to_string(), control.chip_type.to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            SETTINGS_PLAY_SOUND => {  // Play Sound
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    control.play_sound = !control.play_sound;
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_PLAY_SOUND.to_string(), control.play_sound.to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            SETTINGS_VOLUME => {  // Volume
                                                if self.volume > 10 {
                                                    self.volume = 10;
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.volume = (self.volume as f32) / 10.0;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_VOLUME.to_string(), control.volume.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                } else if self.volume > 0 {
                                                    self.volume -= 1;
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.volume = (self.volume as f32) / 10.0;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_VOLUME.to_string(), control.volume.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                            SETTINGS_VOICE => {  // Voice
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    match control.sound_board.get_voice() {
                                                        Voice::Emily => {
                                                            if let Err(_) = control.sound_board.change_voice(Voice::Custom) {
                                                                println!("Error changing voice to Custom");
                                                                control.sound_board.play_custom_not_available(control.volume);
                                                            } else {
                                                                control.sound_board.play_introduction(control.volume);
                                                            }
                                                        },
                                                        Voice::Michael => {
                                                            if let Err(_) = control.sound_board.change_voice(Voice::Emily) {
                                                                println!("Error changing voice to Emily");
                                                            } else {
                                                                control.sound_board.play_introduction(control.volume);
                                                            }
                                                        },
                                                        Voice::Custom => {
                                                            if let Err(_) = control.sound_board.change_voice(Voice::Michael) {
                                                                println!("Error changing voice to Michael");
                                                            } else {
                                                                control.sound_board.play_introduction(control.volume);
                                                            }
                                                        },
                                                    }
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_VOICE.to_string(), control.sound_board.get_voice().as_str().to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            SETTINGS_AUTO_UPLOAD => {  // Auto Upload
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    control.auto_remote = !control.auto_remote;
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_AUTO_REMOTE.to_string(), control.auto_remote.to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            SETTINGS_UPLOAD_INTERVAL => {  // Upload Interval
                                                if control.upload_interval > 0 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.upload_interval -= 1;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_UPLOAD_INTERVAL.to_string(), control.upload_interval.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                            SETTINGS_ENABLE_NTFY => {  // Enable NTFY
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    control.enable_ntfy = !control.enable_ntfy;
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_ENABLE_NTFY.to_string(), control.enable_ntfy.to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    self.update_settings();
                                },
                                ABOUT_MENU | STARTUP_MENU => { // 3 == about
                                    self.current_menu[0] = MAIN_MENU;
                                    self.current_menu[1] = MAIN_START_READING;
                                    self.update_menu();
                                },
                                SHUTDOWN_MENU | RESTART_MENU | UPDATE_MENU | DELETE_READS_MENU | DELETE_READS_MENU_TWO => {
                                    self.current_menu[1] = (self.current_menu[1] + 1) % 2;
                                },
                                MANUAL_TIME_MENU => {
                                    if self.current_menu[1] == TIME_MENU_YEAR {
                                        self.current_menu[1] = TIME_MENU_SECOND + 1;
                                    } else {
                                        self.current_menu[1] -= 1;
                                    }
                                },
                                _ => {}, // main menu, reading menu, and shutdown menu
                            }
                            self.current_menu[2] = 0;
                        },
                        ButtonPress::Right => {
                            match self.current_menu[0] {
                                MAIN_MENU => { // similar to enter function
                                    match self.current_menu[1] {
                                        MAIN_START_READING => { // Start Reading
                                            #[cfg(target_os = "linux")]
                                            {
                                                let _ = lcd.clear();
                                                let _ = lcd.home();
                                                let _ = write!(lcd, "{:<20}", "");
                                                let _ = write!(lcd, "{:<20}", "");
                                                let _ = write!(lcd, "{:^20}", "Starting . . .");
                                                let _ = write!(lcd, "{:<20}", "");
                                            }
                                            if let Ok(ac) = self.ac_state.lock() {
                                                match *ac {
                                                    auto_connect::State::Finished |
                                                    auto_connect::State::Unknown => {
                                                        if let Ok(mut u_readers) = self.readers.lock() {
                                                            // make sure to iterate through the vec in reverse so we don't have some weird loop issues
                                                            for ix in (0..u_readers.len()).rev() {
                                                                let mut reader = u_readers.remove(ix);
                                                                if reader.is_connected() != Some(true) {
                                                                    reader.set_control_sockets(self.control_sockets.clone());
                                                                    reader.set_readers(self.readers.clone());
                                                                    reader.set_read_repeaters(self.read_repeaters.clone());
                                                                    let reconnector = Reconnector::new(
                                                                        self.readers.clone(),
                                                                        self.joiners.clone(),
                                                                        self.control_sockets.clone(),
                                                                        self.read_repeaters.clone(),
                                                                        self.control.clone(),
                                                                        self.sqlite.clone(),
                                                                        self.read_saver.clone(),
                                                                        self.sound.clone(),
                                                                        reader.id(),
                                                                        1,
                                                                        self.notifier.clone(),
                                                                    );
                                                                    match reader.connect(
                                                                            &self.sqlite.clone(),
                                                                            &self.control.clone(),
                                                                            &self.read_saver.clone(),
                                                                            self.sound.clone(),
                                                                            Some(reconnector),
                                                                            self.notifier.clone(),
                                                                        ) {
                                                                        Ok(j) => {
                                                                            if let Ok(mut join) = self.joiners.lock() {
                                                                                join.push(j);
                                                                            }
                                                                            self.sound.notify_custom(SoundType::Connected);
                                                                        },
                                                                        Err(e) => {
                                                                            println!("Error connecting to reader: {e}");
                                                                        }
                                                                    }
                                                                }
                                                                u_readers.push(reader);
                                                            }
                                                        }
                                                    }
                                                    _ => {
                                                        println!("Auto connect is working right now.");
                                                        self.sound.notify_custom(SoundType::StartupInProgress);
                                                        self.current_menu[0] = STARTUP_MENU;
                                                        self.current_menu[1] = 0;
                                                    }
                                                }
                                            } else {
                                                println!("Auto connect is working right now.");
                                                self.sound.notify_custom(SoundType::StartupInProgress);
                                                self.current_menu[0] = STARTUP_MENU;
                                                self.current_menu[1] = 0;
                                            }
                                        },
                                        MAIN_SETTINGS => { // Settings
                                            self.current_menu[0] = SETTINGS_MENU;
                                            self.current_menu[1] = SETTINGS_READ_WINDOW;
                                            self.update_settings();
                                            self.update_menu();
                                        }
                                        MAIN_ABOUT => { // About
                                            self.current_menu[0] = ABOUT_MENU;
                                            self.current_menu[1] = 0;
                                        },
                                        MAIN_SHUTDOWN => { // Shutdown
                                            self.current_menu[0] = SHUTDOWN_MENU;
                                            self.current_menu[1] = 0;
                                        },
                                        _ => {}
                                    }
                                },
                                READING_MENU => {
                                    if self.current_menu[2] == 1 {
                                        if let Ok(ac) = self.ac_state.lock() {
                                            match *ac {
                                                auto_connect::State::Finished |
                                                auto_connect::State::Unknown => {
                                                    #[cfg(target_os = "linux")]
                                                    {
                                                        let _ = lcd.clear();
                                                        let _ = lcd.home();
                                                        let _ = write!(lcd, "{:<20}", "");
                                                        let _ = write!(lcd, "{:<20}", "");
                                                        let _ = write!(lcd, "{:^20}", "Stopping . . .");
                                                        let _ = write!(lcd, "{:<20}", "");
                                                    }
                                                    if let Ok(mut u_readers) = self.readers.lock() {
                                                        for ix in (0..u_readers.len()).rev() {
                                                            let mut reader = u_readers.remove(ix);
                                                            if reader.is_reading() == Some(true) {
                                                                match reader.stop() {
                                                                    Ok(_) => {},
                                                                    Err(e) => {
                                                                        println!("Error stopping reader: {e}");
                                                                    }
                                                                }
                                                            }
                                                            if reader.is_connected() == Some(true) {
                                                                match reader.disconnect() {
                                                                    Ok(_) => {},
                                                                    Err(e) => {
                                                                        println!("Error disconnecting reader: {e}");
                                                                    }
                                                                }
                                                            }
                                                            u_readers.push(reader);
                                                        }
                                                    }
                                                },
                                                _ => {
                                                    println!("Auto connect is working right now.");
                                                    self.sound.notify_custom(SoundType::StartupInProgress);
                                                    self.current_menu[0] = STARTUP_MENU;
                                                    self.current_menu[1] = MAIN_START_READING;
                                                },
                                            }
                                        } else {
                                            println!("Auto connect is working right now.");
                                            self.sound.notify_custom(SoundType::StartupInProgress);
                                            self.current_menu[0] = STARTUP_MENU;
                                            self.current_menu[1] = MAIN_START_READING;
                                        }
                                        self.update_menu();
                                    }
                                },
                                SETTINGS_MENU => {
                                    if let Ok(mut control) = self.control.lock() {
                                        match self.current_menu[1] {
                                            SETTINGS_READ_WINDOW => {  // Read Window
                                                if control.read_window < 50 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.read_window += 1;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_READ_WINDOW.to_string(), control.read_window.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                            SETTINGS_CHIP_TYPE => {  // Chip Type
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    if control.chip_type == TYPE_CHIP_HEX {
                                                        control.chip_type = TYPE_CHIP_DEC.to_string();
                                                    } else {
                                                        control.chip_type = TYPE_CHIP_HEX.to_string();
                                                    }
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_CHIP_TYPE.to_string(), control.chip_type.to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            SETTINGS_PLAY_SOUND => {  // Play Sound
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    control.play_sound = !control.play_sound;
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_PLAY_SOUND.to_string(), control.play_sound.to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            SETTINGS_VOLUME => {  // Volume
                                                if self.volume < 10 {
                                                    self.volume += 1;
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.volume = (self.volume as f32) / 10.0;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_VOLUME.to_string(), control.volume.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                } else if self.volume > 10 {
                                                    self.volume = 10;
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.volume = (self.volume as f32) / 10.0;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_VOLUME.to_string(), control.volume.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                            SETTINGS_VOICE => {  // Voice
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    match control.sound_board.get_voice() {
                                                        Voice::Emily => {
                                                            if let Err(_) = control.sound_board.change_voice(Voice::Michael) {
                                                                println!("Error changing voice to Michael");
                                                            } else {
                                                                control.sound_board.play_introduction(control.volume);
                                                            }
                                                        },
                                                        Voice::Michael => {
                                                            if let Err(_) = control.sound_board.change_voice(Voice::Custom) {
                                                                println!("Error changing voice to Custom");
                                                                control.sound_board.play_custom_not_available(control.volume);
                                                            } else {
                                                                control.sound_board.play_introduction(control.volume);
                                                            }
                                                        },
                                                        Voice::Custom => {
                                                            if let Err(_) = control.sound_board.change_voice(Voice::Emily) {
                                                                println!("Error changing voice to Emily");
                                                            } else {
                                                                control.sound_board.play_introduction(control.volume);
                                                            }
                                                        },
                                                    }
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_VOICE.to_string(), control.sound_board.get_voice().as_str().to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            SETTINGS_AUTO_UPLOAD => {  // Auto Upload
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    control.auto_remote = !control.auto_remote;
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_AUTO_REMOTE.to_string(), control.auto_remote.to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            SETTINGS_UPLOAD_INTERVAL => {  // Upload Interval
                                                if control.upload_interval < 180 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.upload_interval += 1;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_UPLOAD_INTERVAL.to_string(), control.upload_interval.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                            SETTINGS_ENABLE_NTFY => {  // Enable NTFY
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    control.enable_ntfy = !control.enable_ntfy;
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_ENABLE_NTFY.to_string(), control.enable_ntfy.to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    self.update_settings();
                                },
                                SHUTDOWN_MENU | RESTART_MENU | UPDATE_MENU | DELETE_READS_MENU | DELETE_READS_MENU_TWO => {
                                    self.current_menu[1] = (self.current_menu[1] + 1) % 2;
                                },
                                ABOUT_MENU | STARTUP_MENU => { // 3 == about, 5 == startup
                                    self.current_menu[0] = MAIN_MENU;
                                    self.current_menu[1] = MAIN_START_READING;
                                    self.update_menu();
                                },
                                MANUAL_TIME_MENU => {
                                    if self.current_menu[1] > TIME_MENU_SECOND {
                                        self.current_menu[1] = TIME_MENU_YEAR;
                                    } else {
                                        self.current_menu[1] += 1;
                                    }
                                },
                                _ => {},
                            }
                            self.current_menu[2] = 0;
                        },
                        ButtonPress::Enter => {
                            match self.current_menu[0] {
                                MAIN_MENU => { // main
                                    match self.current_menu[1] {
                                        MAIN_START_READING => { // Start Reading
                                            #[cfg(target_os = "linux")]
                                            {
                                                let _ = lcd.clear();
                                                let _ = lcd.home();
                                                let _ = write!(lcd, "{:<20}", "");
                                                let _ = write!(lcd, "{:<20}", "");
                                                let _ = write!(lcd, "{:^20}", "Starting . . .");
                                                let _ = write!(lcd, "{:<20}", "");
                                            }
                                            if let Ok(ac) = self.ac_state.lock() {
                                                match *ac {
                                                    auto_connect::State::Finished |
                                                    auto_connect::State::Unknown => {
                                                        if let Ok(mut u_readers) = self.readers.lock() {
                                                            // make sure to iterate through the vec in reverse so we don't have some weird loop issues
                                                            let num_readers = u_readers.len();
                                                            let mut connected_readers: usize = 0;
                                                            for ix in (0..num_readers).rev() {
                                                                let mut reader = u_readers.remove(ix);
                                                                if reader.is_connected() != Some(true) {
                                                                    reader.set_control_sockets(self.control_sockets.clone());
                                                                    reader.set_readers(self.readers.clone());
                                                                    reader.set_read_repeaters(self.read_repeaters.clone());
                                                                    let reconnector = Reconnector::new(
                                                                        self.readers.clone(),
                                                                        self.joiners.clone(),
                                                                        self.control_sockets.clone(),
                                                                        self.read_repeaters.clone(),
                                                                        self.control.clone(),
                                                                        self.sqlite.clone(),
                                                                        self.read_saver.clone(),
                                                                        self.sound.clone(),
                                                                        reader.id(),
                                                                        1,
                                                                        self.notifier.clone(),
                                                                    );
                                                                    match reader.connect(
                                                                            &self.sqlite.clone(),
                                                                            &self.control.clone(),
                                                                            &self.read_saver.clone(),
                                                                            self.sound.clone(),
                                                                            Some(reconnector),
                                                                            self.notifier.clone(),
                                                                        ) {
                                                                        Ok(j) => {
                                                                            if let Ok(mut join) = self.joiners.lock() {
                                                                                join.push(j);
                                                                            }
                                                                            connected_readers += 1;
                                                                        },
                                                                        Err(e) => {
                                                                            println!("Error connecting to reader: {e}");
                                                                        }
                                                                    }
                                                                    thread::sleep(Duration::from_millis(CONNECTION_CHANGE_PAUSE));
                                                                }
                                                                u_readers.push(reader);
                                                            }
                                                            if connected_readers == num_readers {
                                                                self.sound.notify_custom(SoundType::Connected);
                                                            }
                                                        }
                                                    }
                                                    _ => {
                                                        println!("Auto connect is working right now.");
                                                        self.sound.notify_custom(SoundType::StartupInProgress);
                                                        self.current_menu[0] = STARTUP_MENU;
                                                        self.current_menu[1] = 0;
                                                    }
                                                }
                                            } else {
                                                println!("Auto connect is working right now.");
                                                self.sound.notify_custom(SoundType::StartupInProgress);
                                                self.current_menu[0] = STARTUP_MENU;
                                                self.current_menu[1] = 0;
                                            }

                                        },
                                        MAIN_SETTINGS => { // Settings
                                            self.current_menu[0] = SETTINGS_MENU;
                                            self.current_menu[1] = 0;
                                            self.update_settings();
                                            self.update_menu();
                                        }
                                        MAIN_ABOUT => { // About
                                            self.current_menu[0] = ABOUT_MENU;
                                            self.current_menu[1] = 0;
                                        },
                                        MAIN_SHUTDOWN => { // Shutdown
                                            self.current_menu[0] = SHUTDOWN_MENU;
                                            self.current_menu[1] = 0;
                                        },
                                        MAIN_RESTART => { // Restart
                                            self.current_menu[0] = RESTART_MENU;
                                            self.current_menu[1] = 0;
                                        },
                                        MAIN_UPDATE => { // Update
                                            self.current_menu[0] = UPDATE_MENU;
                                            self.current_menu[1] = 0;
                                        },
                                        _ => {}
                                    }
                                },
                                SETTINGS_MENU => { // settings -> saves settings and goes back (only if not on set time)
                                    match self.current_menu[1] {
                                        SETTINGS_SET_TIME_WEB => {
                                            #[cfg(target_os = "linux")]
                                            {
                                                let _ = lcd.clear();
                                                let _ = lcd.home();
                                                let _ = write!(lcd, "{:<20}", "");
                                                let _ = write!(lcd, "{:<20}", "");
                                                let _ = write!(lcd, "{:^20}", "Setting Time . . .");
                                                let _ = write!(lcd, "{:<20}", "");
                                            }
                                            match std::process::Command::new("sudo").arg("systemctl").arg("stop").arg("ntpd").status() {
                                                Ok(_) => {
                                                    match std::process::Command::new("sudo").arg("ntpd").arg(format!("-q")).arg(format!("-g")).arg(format!("--configfile=/etc/ntpsec/ntp-get.conf")).status() {
                                                        Ok(_) => {
                                                            match std::process::Command::new("sudo").arg("hwclock").arg("-w").status() {
                                                                Ok(_) => {
                                                                    self.current_menu[0] = MAIN_MENU;
                                                                    self.current_menu[1] = MAIN_START_READING;
                                                                    self.update_menu();
                                                                    // notify of settings changes
                                                                    if let Ok(sq) = self.sqlite.try_lock() {
                                                                        let settings = socket::get_settings(&sq);
                                                                        if let Ok(socks) = self.control_sockets.try_lock() {
                                                                            for sock_opt in &*socks {
                                                                                if let Some(sock) = sock_opt {
                                                                                    _ = socket::write_settings(&sock, &settings);
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                },
                                                                Err(e) => {
                                                                    println!("error setting hardware clock: {e}");
                                                                }
                                                            }
                                                        },
                                                        Err(e) => {
                                                            println!("error setting time: {e}");
                                                        }
                                                    }
                                                },
                                                Err(e) => {
                                                    println!("error stopping ntpd: {e}");
                                                }
                                            }
                                            match std::process::Command::new("sudo").arg("systemctl").arg("start").arg("ntpd").status() {
                                                Ok(_) => {},
                                                Err(e) => {
                                                    println!("error starting ntpd: {e}");
                                                }
                                            }
                                        },
                                        SETTINGS_SET_TIME_MANUAL => {
                                            self.current_menu[0] = MANUAL_TIME_MENU;
                                            self.current_menu[1] = TIME_MENU_YEAR;
                                        },
                                        SETTINGS_DELETE_CHIP_READS => {
                                            self.current_menu[0] = DELETE_READS_MENU;
                                            self.current_menu[1] = 0;
                                        }
                                        _ => {
                                            self.current_menu[0] = MAIN_MENU;
                                            self.current_menu[1] = MAIN_START_READING;
                                            self.update_menu();
                                            // notify of settings changes
                                            if let Ok(sq) = self.sqlite.try_lock() {
                                                let settings = socket::get_settings(&sq);
                                                if let Ok(socks) = self.control_sockets.try_lock() {
                                                    for sock_opt in &*socks {
                                                        if let Some(sock) = sock_opt {
                                                            _ = socket::write_settings(&sock, &settings);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                READING_MENU => { // currently reading
                                    self.current_menu[2] = 1; // used to allow readers to stop
                                },
                                ABOUT_MENU | STARTUP_MENU => {
                                    self.current_menu[0] = MAIN_MENU;
                                    self.current_menu[1] = MAIN_START_READING;
                                    self.update_menu();
                                },
                                SHUTDOWN_MENU => {
                                    if self.current_menu[1] == 1 {
                                        #[cfg(target_os = "linux")]
                                        {
                                            let _ = lcd.clear();
                                            //let _ = lcd.backlight(false);
                                            let _ = lcd.show_display(false);
                                        }
                                        if let Ok(mut ka) = self.keepalive.lock() {
                                            println!("Starting program stop sequence.");
                                            *ka = false;
                                        }
                                        // play a shutdown command since the shutdown 
                                        if let Ok(control) = self.control.lock() {
                                            if control.play_sound {
                                                control.sound_board.play_shutdown(control.volume);
                                            }
                                        }
                                        // send shutdown command to the OS
                                        println!("Sending OS shutdown command if on Linux.");
                                        match std::env::consts::OS {
                                            "linux" => {
                                                match std::process::Command::new("sudo").arg("shutdown").arg("-h").arg("now").spawn() {
                                                    Ok(_) => {
                                                        println!("Shutdown command sent to OS successfully.");
                                                    },
                                                    Err(e) => {
                                                        println!("Error sending shutdown command: {e}");
                                                    }
                                                }
                                            },
                                            other => {
                                                println!("Shutdown not supported on this platform ({other})");
                                            }
                                        }
                                        // connect to ensure the spawning thread will exit the accept call
                                        _ = TcpStream::connect(format!("127.0.0.1:{}", self.control_port));
                                    }
                                    self.current_menu[0] = MAIN_MENU;
                                    self.current_menu[1] = MAIN_START_READING;
                                    self.update_menu();
                                },
                                RESTART_MENU => {
                                    if self.current_menu[1] == 1 {
                                        #[cfg(target_os = "linux")]
                                        {
                                            let _ = lcd.clear();
                                            //let _ = lcd.backlight(false);
                                            let _ = lcd.show_display(false);
                                        }
                                        if let Ok(mut ka) = self.keepalive.lock() {
                                            println!("Starting program restart sequence.");
                                            *ka = false;
                                        }
                                        // send shutdown command to the OS
                                        println!("Sending program restart command if on Linux.");
                                        match std::env::consts::OS {
                                            "linux" => {
                                                match std::process::Command::new("sudo").arg("systemctl").arg("restart").arg("portal").spawn() {
                                                    Ok(_) => {
                                                        println!("Restart command sent to OS successfully.");
                                                    },
                                                    Err(e) => {
                                                        println!("Error sending restart command: {e}");
                                                    }
                                                }
                                            },
                                            other => {
                                                println!("Rerstart not supported on this platform ({other})");
                                            }
                                        }
                                    }
                                    self.current_menu[0] = MAIN_MENU;
                                    self.current_menu[1] = MAIN_START_READING;
                                    self.update_menu();
                                },
                                UPDATE_MENU => {
                                    if self.current_menu[1] == 1 {
                                        #[cfg(target_os = "linux")]
                                        {
                                            let _ = lcd.clear();
                                            //let _ = lcd.backlight(false);
                                            let _ = lcd.show_display(false);
                                        }
                                        if let Ok(mut ka) = self.keepalive.lock() {
                                            println!("Starting program update sequence.");
                                            *ka = false;
                                        }
                                        // send shutdown command to the OS
                                        println!("Sending program update command if on Linux.");
                                        match std::env::consts::OS {
                                            "linux" => {
                                                if let Ok(update_path) = env::var(UPDATE_SCRIPT_ENV) {
                                                    match std::process::Command::new(update_path).spawn() {
                                                        Ok(_) => {},
                                                        Err(e) => {
                                                            println!("error updating: {e}");
                                                        }
                                                    }
                                                } else {
                                                    println!("update script environment variable not set");
                                                }
                                            },
                                            other => {
                                                println!("not supported on this platform ({other})");
                                            }
                                        }
                                    }
                                    self.current_menu[0] = MAIN_MENU;
                                    self.current_menu[1] = MAIN_START_READING;
                                    self.update_menu();
                                },
                                DELETE_READS_MENU => {
                                    if self.current_menu[1] == 1 {
                                        self.current_menu[0] = DELETE_READS_MENU_TWO;
                                        self.current_menu[1] = 0;
                                    } else {
                                        self.current_menu[0] = SETTINGS_MENU;
                                        self.current_menu[1] = SETTINGS_DELETE_CHIP_READS;
                                        self.update_menu();
                                    }
                                },
                                DELETE_READS_MENU_TWO => {
                                    if self.current_menu[1] == 1 {
                                        if let Ok(sq) = self.sqlite.lock() {
                                            if let Err(e) = sq.delete_all_reads() {
                                                println!("error deleting reads: {e}")
                                            }
                                        }
                                    }
                                    self.current_menu[0] = SETTINGS_MENU;
                                    self.current_menu[1] = SETTINGS_DELETE_CHIP_READS;
                                    self.update_menu();
                                },
                                MANUAL_TIME_MENU => {
                                    if self.current_menu[1] == TIME_MENU_SECOND + 1 {
                                        self.current_menu[0] = SETTINGS_MENU;
                                        self.current_menu[1] = SETTINGS_SET_TIME_MANUAL;
                                    } else {
                                        #[cfg(target_os = "linux")]
                                        {
                                            let _ = lcd.clear();
                                            let _ = lcd.home();
                                            let _ = write!(lcd, "{:<20}", "");
                                            let _ = write!(lcd, "{:<20}", "");
                                            let _ = write!(lcd, "{:^20}", "Setting Time . . .");
                                            let _ = write!(lcd, "{:<20}", "");
                                        }
                                        match std::env::consts::OS {
                                            "linux" => {
                                                match std::process::Command::new("sudo").arg("date")
                                                    .arg(format!("--set={:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                                                        self.year, self.month, self.day, self.hour, self.minute, self.seconds)).status()
                                                {
                                                    Ok(_) => {
                                                        match std::process::Command::new("sudo").arg("hwclock").arg("-w").status() {
                                                            Ok(_) => {
                                                                self.current_menu[0] = SETTINGS_MENU;
                                                                self.current_menu[1] = SETTINGS_SET_TIME_MANUAL;
                                                                self.update_menu();
                                                            },
                                                            Err(e) => {
                                                                println!("error setting time: {e}");
                                                            }
                                                        }
                                                    },
                                                    Err(e) => {
                                                        println!("error setting time: {e}");
                                                    },
                                                }
                                            },
                                            other => {
                                                println!("not supported on this platform ({other})");
                                            },
                                        }
                                    }
                                },
                                _ => {},
                            }
                        },
                    } 
                }
                presses.clear();
            }
            let mut connected = false;
            if let Ok(u_readers) = self.readers.lock() {
                for reader in u_readers.iter() {
                    if let Some(reading) = reader.is_reading() {
                        if reading {
                            self.current_menu[0] = READING_MENU;
                            self.current_menu[1] = 0;
                            connected = true;
                            break;
                        }
                    }
                }
            }
            if self.current_menu[0] == READING_MENU && !connected {
                self.current_menu[0] = MAIN_MENU;
                self.current_menu[1] = MAIN_START_READING;
                self.update_menu();
            }
            #[cfg(target_os = "linux")]
            {
                let sys_time = SystemTime::now();
                let date_time: DateTime<Local> = sys_time.into();
                if let Ok(mut info) = self.info.lock() {
                    info.title_bar.replace_range(0..14, format!("{:<14}", date_time.format("%m-%d %H:%M:%S")).as_str());
                }
                let mut messages: Vec<String> = vec!();
                if let Ok(info) = self.info.lock() {
                    messages.push(info.title_bar.clone())
                }
                let _ = lcd.home();
                match self.current_menu[0] {
                    MAIN_MENU => { // main menu, max ix 3
                        if let Ok(info) = self.info.lock() {
                            let max_ix: u8 = (info.main_menu.len() - 1).try_into().unwrap();
                            let mut disp_ix = self.current_menu[1] as usize;
                            if self.current_menu[1] == 0 {
                                disp_ix += 1; // index 0 needs to display 0, 1, 2
                            } else if self.current_menu[1] == max_ix {
                                disp_ix -= 1; // last value needs to display last-2, last-1, last
                            }
                            messages.push(info.main_menu[disp_ix].clone());     // Interface writes lines odd lines before even lines,
                            messages.push(info.main_menu[disp_ix - 1].clone()); // So order Vec as [Line 1, Line 3, Line 2, Line 4]
                            messages.push(info.main_menu[disp_ix + 1].clone()); // Messages comes pre-loaded with Line 1.
                        }
                    },
                    SETTINGS_MENU => { // settings menu, max ix 9
                        if let Ok(info) = self.info.lock() {
                            let max_ix: u8 = (info.settings_menu.len() - 1).try_into().unwrap();
                            let mut disp_ix = self.current_menu[1] as usize;
                            if self.current_menu[1] == 0 {
                                disp_ix += 1; // index 0 needs to display 0, 1, 2
                            } else if self.current_menu[1] == max_ix {
                                disp_ix -= 1; // last value needs to display last-2, last-1, last
                            }
                            messages.push(info.settings_menu[disp_ix].clone());     // Interface writes lines odd lines before even lines,
                            messages.push(info.settings_menu[disp_ix - 1].clone()); // So order Vec as [Line 1, Line 3, Line 2, Line 4]
                            messages.push(info.settings_menu[disp_ix + 1].clone()); // Messages comes pre-loaded with Line 1.
                        }
                    },
                    READING_MENU => { // reader is reading
                        self.update_readers();
                        if let Ok(info) = self.info.lock() {
                            match info.reader_info.len() {
                                1 => {
                                    messages.push(format!("{:^20}", info.reader_info[0]));
                                    messages.push(format!("{:^20}", ""));
                                    messages.push(format!("{:^20}", ""));
                                },
                                2 => {
                                    messages.push(format!("{:^20}", info.reader_info[0]));
                                    messages.push(format!("{:^20}", ""));
                                    messages.push(format!("{:^20}", info.reader_info[1]));
                                },
                                3 => {
                                    messages.push(format!("{:^20}", info.reader_info[1]));
                                    messages.push(format!("{:^20}", info.reader_info[0]));
                                    messages.push(format!("{:^20}", info.reader_info[2]));
                                },
                                _ => {
                                    let mut first = 0;
                                    let mut second = 1;
                                    let mut third = 2;
                                    let current_selection = self.current_menu[1] as usize;
                                    let max_ix = info.reader_info.len() - 1;
                                    if current_selection > 1 {
                                        if current_selection >= max_ix {
                                            first = max_ix - 2;
                                            second = max_ix - 1;
                                            third = max_ix;
                                        } else {
                                            first = current_selection - 1;
                                            second = current_selection;
                                            third = current_selection + 1;
                                        }
                                    }
                                    messages.push(format!("{:^20}", info.reader_info[second]));
                                    messages.push(format!("{:^20}", info.reader_info[first]));
                                    messages.push(format!("{:^20}", info.reader_info[third]));
                                }
                            }
                        }
                    },
                    ABOUT_MENU => { // about menu
                        messages.clear();
                        if let Ok(control) = self.control.try_lock() {
                            messages.push(format!("{:^20}", "Chronokeep Portal"));
                            messages.push(format!("{:^20}", "Device Name:"));
                            messages.push(format!("{:^20}", format!("Version {}", env!("CARGO_PKG_VERSION"))));
                            messages.push(format!("{:^20}", control.name));
                        } else {
                            messages.push(format!("{:^20}", ""));
                            messages.push(format!("{:^20}", format!("Version {}", env!("CARGO_PKG_VERSION"))));
                            messages.push(format!("{:^20}", "Chronokeep Portal"));
                            messages.push(format!("{:^20}", ""));
                        }
                    },
                    STARTUP_MENU => {
                        messages.clear();
                        messages.push(format!("{:^20}", ""));
                        messages.push(format!("{:^20}", "Please wait."));
                        messages.push(format!("{:^20}", "System Initializing."));
                        messages.push(format!("{:^20}", ""));
                    },
                    SHUTDOWN_MENU => {
                        messages.clear();
                        messages.push(format!("{:^20}", ""));
                        if self.current_menu[1] == 0 {
                            messages.push(String::from("     YES    > NO    "));
                        } else {
                            messages.push(String::from("   > YES      NO    "));
                        }
                        messages.push(format!("{:^20}", "Shutdown Portal?"));
                        messages.push(format!("{:^20}", ""));
                    },
                    UPDATE_MENU => {
                        messages.clear();
                        messages.push(format!("{:^20}", ""));
                        if self.current_menu[1] == 0 {
                            messages.push(String::from("     YES    > NO    "));
                        } else {
                            messages.push(String::from("   > YES      NO    "));
                        }
                        messages.push(format!("{:^20}", "Update Portal?"));
                        messages.push(format!("{:^20}", ""));
                    },
                    RESTART_MENU => {
                        messages.clear();
                        messages.push(format!("{:^20}", ""));
                        if self.current_menu[1] == 0 {
                            messages.push(String::from("     YES    > NO    "));
                        } else {
                            messages.push(String::from("   > YES      NO    "));
                        }
                        messages.push(format!("{:^20}", "Restart Portal?"));
                        messages.push(format!("{:^20}", ""));
                    },
                    DELETE_READS_MENU => {
                        messages.clear();
                        messages.push(format!("{:^20}", ""));
                        if self.current_menu[1] == 0 {
                            messages.push(String::from("     YES    > NO    "));
                        } else {
                            messages.push(String::from("   > YES      NO    "));
                        }
                        messages.push(format!("{:^20}", "Delete all reads?"));
                        messages.push(format!("{:^20}", ""));
                    },
                    DELETE_READS_MENU_TWO => {
                        messages.clear();
                        messages.push(format!("{:^20}", ""));
                        if self.current_menu[1] == 0 {
                            messages.push(String::from("     YES    > NO    "));
                        } else {
                            messages.push(String::from("   > YES      NO    "));
                        }
                        messages.push(format!("{:^20}", "Are you sure?"));
                        messages.push(format!("{:^20}", ""));
                    },
                    MANUAL_TIME_MENU => {
                        messages.clear();
                        messages.push(format!("{:<20}", ""));
                        messages.push(format!("{:04}-{:02}-{:02}  {:02}:{:02}:{:02}", //yyyy-MM-dd HH:mm:ss
                            self.year,
                            self.month,
                            self.day,
                            self.hour,
                            self.minute,
                            self.seconds));
                        match self.current_menu[1] {
                            TIME_MENU_YEAR => {
                                messages.push(String::from(" vv                 "));
                                messages.push(String::from(" ^^                 "));
                            },
                            TIME_MENU_MONTH => {
                                messages.push(String::from("     vv             "));
                                messages.push(String::from("     ^^             "));
                            },
                            TIME_MENU_DAY => {
                                messages.push(String::from("        vv          "));
                                messages.push(String::from("        ^^          "));
                            },
                            TIME_MENU_HOUR => {
                                messages.push(String::from("            vv      "));
                                messages.push(String::from("            ^^      "));
                            },
                            TIME_MENU_MINUTE => {
                                messages.push(String::from("               vv   "));
                                messages.push(String::from("               ^^   "));
                            },
                            TIME_MENU_SECOND => {
                                messages.push(String::from("                  vv"));
                                messages.push(String::from("                  ^^"));
                            },
                            _ => {
                                messages.push(format!("{:^20}", ""));
                                messages.push(format!("{:^20}", "Cancel"));
                            },
                        }
                    }
                    SCREEN_OFF => {
                        let _ = lcd.clear();
                        //let _ = lcd.backlight(false);
                        let _ = lcd.show_display(false);
                    }
                    _ => {}
                }
                for msg in &*messages {
                    let _ = write!(lcd, "{msg}");
                }
            }
            thread::sleep(Duration::from_millis(100));
        } // End processing loop
        #[cfg(target_os = "linux")]
        {
            let _ = lcd.clear();
            //let _ = lcd.backlight(false);
            let _ = lcd.show_display(false);
        }
        println!("LCD thread terminated.");
    }

    pub fn register_button(&self, button: ButtonPress) {
        if let Ok(mut presses) = self.button_presses.try_lock() {
            presses.push(button);
        }
    }
}