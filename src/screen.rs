use std::{net::TcpStream, sync::{Arc, Mutex}, thread::JoinHandle};
#[cfg(target_os = "linux")]
use std::{env, thread, time::Duration};

#[cfg(target_os = "linux")]
use std::time::SystemTime;
#[cfg(target_os = "linux")]
use std::fmt::Write;
#[cfg(target_os = "linux")]
use chrono::{DateTime, Datelike, Local, Timelike};
#[cfg(target_os = "linux")]
use i2c_character_display::{AdafruitLCDBackpack, LcdDisplayType, CharacterDisplayPCF8574T};
#[cfg(target_os = "linux")]
use rppal::{hal, i2c::I2c};

use crate::{control::{Control, socket::MAX_CONNECTED, sound::SoundNotifier}, database::sqlite, notifier, processor, reader::{self, auto_connect}, remote::uploader::{self, Status, Uploader}};
#[cfg(target_os = "linux")]
use crate::{control::{SETTING_AUTO_REMOTE, SETTING_CHIP_TYPE, SETTING_ENABLE_NTFY, SETTING_PLAY_SOUND, SETTING_READ_WINDOW, SETTING_UPLOAD_INTERVAL, SETTING_VOICE, SETTING_VOLUME, socket::{self, CONNECTION_CHANGE_PAUSE, UPDATE_SCRIPT_ENV}, sound::SoundType}, database::Database, network::api, objects::{read, setting::Setting}, reader::reconnector::Reconnector, remote::remote_util, sound_board::Voice, types};

mod ada;
mod pcf;

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
pub const SETTINGS_MANUAL_UPLOAD: u8 = 6;
pub const SETTINGS_UPLOAD_INTERVAL: u8 = 7;
pub const SETTINGS_ENABLE_NTFY: u8 = 8;
pub const SETTINGS_DELETE_CHIP_READS: u8 = 9;
pub const SETTINGS_SET_TIME_WEB: u8 = 10;
pub const SETTINGS_SET_TIME_MANUAL: u8 = 11;

pub const TIME_MENU_YEAR: u8 = 0;
pub const TIME_MENU_MONTH: u8 = 1;
pub const TIME_MENU_DAY: u8 = 2;
pub const TIME_MENU_HOUR: u8 = 3;
pub const TIME_MENU_MINUTE: u8 = 4;
pub const TIME_MENU_SECOND: u8 = 5;

#[allow(unused)]
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
    uploader: Option<Arc<Uploader>>,
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

#[allow(unused)]
pub struct DisplayInfo {
    title_bar: String,
    reader_info: Vec<String>,
    main_menu: Vec<String>,
    settings_menu: Vec<String>,
    upload_status: Status,
    upload_errors: usize,
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
                upload_status: Status::Unknown,
                upload_errors: 0,
            })),
            current_menu: [0, 0, 0],
            ac_state,
            read_saver,
            sound,
            uploader: None,
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

    pub fn set_uploader(&mut self, upload: Arc<Uploader>) {
        self.uploader = Some(upload);
    }

    pub fn update_upload_status(&mut self, status: uploader::Status, err_count: usize) {
        if let Ok(mut info) = self.info.lock() {
            info.upload_errors = err_count;
            info.upload_status = status;
        }
    }

    #[cfg(target_os = "linux")]
    fn update_menu(&mut self) {
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

    #[cfg(target_os = "linux")]
    fn update_settings(&mut self) {
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
                info.settings_menu.push(format!("   Manual Upload    "));
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
       
    #[allow(unused)]
    pub fn run(&mut self, bus: u8) {
        #[cfg(target_os = "linux")]
        {
            let mut adafruit = true;
            if let Ok(control) = self.control.lock() {
                if control.screen_type == types::TYPE_SCREEN_PCF8574T {
                    adafruit = false;
                }
            }
            if adafruit {
                self.ada_run(bus);
            } else {
                self.pcf_run(bus);
            }
        }
    }

    pub fn register_button(&self, button: ButtonPress) {
        if let Ok(mut presses) = self.button_presses.try_lock() {
            presses.push(button);
        }
    }
}