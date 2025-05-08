use std::{net::TcpStream, sync::{Arc, Mutex}, thread::{self, JoinHandle}, time::Duration};

#[cfg(target_os = "linux")]
use std::fmt::Write;
#[cfg(target_os = "linux")]
use i2c_character_display::{AdafruitLCDBackpack, LcdDisplayType};
#[cfg(target_os = "linux")]
use rppal::{hal, i2c::I2c};
use std::sync::Condvar;

use crate::{control::{socket::{self, CONNECTION_CHANGE_PAUSE, MAX_CONNECTED}, sound::{SoundNotifier, SoundType}, Control, SETTING_AUTO_REMOTE, SETTING_CHIP_TYPE, SETTING_PLAY_SOUND, SETTING_READ_WINDOW, SETTING_SIGHTING_PERIOD, SETTING_UPLOAD_INTERVAL, SETTING_VOICE, SETTING_VOLUME}, database::{sqlite, Database}, notifier, objects::setting::Setting, processor::{self, SightingsProcessor}, reader::{self, auto_connect, reconnector::Reconnector}, remote::uploader::{self, Status}, sound_board::Voice, types::{TYPE_CHIP_DEC, TYPE_CHIP_HEX}};

pub const EMPTY_STRING: &str = "                    ";

pub const MAIN_MENU: u8 = 0;
pub const SETTINGS_MENU: u8 = 1;
pub const READING_MENU: u8 = 2;
pub const ABOUT_MENU: u8 = 3;
pub const SHUTDOWN_MENU: u8 = 4;
pub const STARTUP_MENU: u8 = 5;
pub const SCREEN_OFF: u8 = 15;

pub const MAIN_START_READING: u8 = 0;
pub const MAIN_SETTINGS: u8 = 1;
pub const MAIN_ABOUT: u8 = 2;
pub const MAIN_SHUTDOWN: u8 = 3;

pub const SETTINGS_SIGHTING_PERIOD: u8 = 0;
pub const SETTINGS_READ_WINDOW: u8 = 1;
pub const SETTINGS_CHIP_TYPE: u8 = 2;
pub const SETTINGS_PLAY_SOUND: u8 = 3;
pub const SETTINGS_VOLUME: u8 = 4;
pub const SETTINGS_VOICE: u8 = 5;
pub const SETTINGS_AUTO_UPLOAD: u8 = 6;
pub const SETTINGS_UPLOAD_INTERVAL: u8 = 7;

#[derive(Clone)]
pub struct CharacterDisplay {
    keepalive: Arc<Mutex<bool>>,
    control: Arc<Mutex<Control>>,
    readers: Arc<Mutex<Vec<reader::Reader>>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
    read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
    sight_processor: Arc<SightingsProcessor>,
    waiter: Arc<(Mutex<bool>, Condvar)>,
    button_presses: Arc<Mutex<Vec<ButtonPress>>>,
    ac_state: Arc<Mutex<auto_connect::State>>,
    read_saver: Arc<processor::ReadSaver>,
    sound: Arc<SoundNotifier>,
    joiners: Arc<Mutex<Vec<JoinHandle<()>>>>,
    info: Arc<Mutex<DisplayInfo>>,
    control_port: u16,
    current_menu: [u8; 3],
    notifier: notifier::Notifier,
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
        sight_processor: Arc<SightingsProcessor>,
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
            sight_processor,
            waiter: Arc::new((Mutex::new(true), Condvar::new())),
            button_presses: Arc::new(Mutex::new(Vec::new())),
            info: Arc::new(Mutex::new(DisplayInfo {
                title_bar: format!("{:<20}", "Chronokeep"),
                reader_info: Vec::new(),
                main_menu: vec![
                    " > Start Reading    ".to_string(),
                    "   Settings         ".to_string(),
                    "   About            ".to_string(),
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
            notifier
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
                let mut upload_status = "?";
                if status == Status::Running {
                    upload_status = "+";
                } else if status == Status::Stopped || status == Status::Stopping {
                    upload_status = "-";
                }
                info.title_bar.replace_range(15..16, upload_status);
            }
        }
    }

    pub fn update_battery(&mut self) {
        if let Ok(mut info) = self.info.lock() {
            if let Ok(control) = self.control.lock() {
                if control.battery > 150 {
                    info.title_bar.replace_range(17..20, "+++");
                } else if control.battery >= 100 {
                    info.title_bar.replace_range(17..20, "100");
                } else {
                    info.title_bar.replace_range(17..20, format!("{:>3}", control.battery).as_str());
                }
            }
        }
    }

    pub fn update_name(&mut self) {
        if let Ok(mut info) = self.info.lock() {
            if let Ok(control) = self.control.lock() {
                info.title_bar.replace_range(0..13, format!("{:<13}", control.name).as_str());
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
                MAIN_MENU => { // main menu, max ix 3
                    for line in info.main_menu.iter_mut() {
                        line.replace_range(1..2, " ");
                    }
                    info.main_menu[self.current_menu[1] as usize].replace_range(1..2, ">");
                },
                SETTINGS_MENU => { // settings menu, max ix 7
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
                info.settings_menu.push(format!("   Sightings   {:>4} ", control.sighting_period));
                info.settings_menu.push(format!("   Read Window {:>4} ", control.read_window));
                info.settings_menu.push(format!("   Chip Type   {:>4} ", control.chip_type));
                info.settings_menu.push(format!("   Play Sounds {:>4} ", play_sound));
                info.settings_menu.push(format!("   Volume      {:>4} ", (control.volume * 10.0) as usize));
                info.settings_menu.push(format!("   Voice    {:>7} ", control.sound_board.get_voice().as_str()));
                info.settings_menu.push(format!("   Auto Upload {:>4} ", auto_upload));
                info.settings_menu.push(format!("   Upload Int  {:>4} ", control.upload_interval));
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
            self.update_name();
            if let Ok(info) = self.info.lock() {
                let mut messages: Vec<String> = vec!(info.title_bar.clone());
                messages.push(info.main_menu[1].clone());
                messages.push(info.main_menu[0].clone());
                messages.push(info.main_menu[2].clone());
                for msg in &*messages {
                    let _ = write!(lcd, "{msg}");
                }
            }
        }
        loop {
            if let Ok(keepalive) = self.keepalive.try_lock() {
                if *keepalive == false {
                    println!("LCD thread stopping.");
                    break;
                }
            }
            let (lock, cvar) = &*self.waiter.clone();
            let mut waiting = lock.lock().unwrap();
            while *waiting {
                waiting = cvar.wait(waiting).unwrap();
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
                                    if self.current_menu[1] > SETTINGS_SIGHTING_PERIOD {
                                        self.current_menu[1] -= 1;
                                    } else {
                                        self.current_menu[1] = SETTINGS_UPLOAD_INTERVAL;
                                    }
                                }
                                ABOUT_MENU | STARTUP_MENU => {
                                    self.current_menu[0] = MAIN_MENU;
                                    self.current_menu[1] = MAIN_START_READING;
                                    self.update_menu();
                                }
                                SHUTDOWN_MENU => {
                                    self.current_menu[1] = (self.current_menu[1] + 1) % 2;
                                },
                                _ => {}, // 2 = currently reading, do nothing
                            }
                            self.current_menu[2] = 0; // current_menu[2] is only used for proper stop reading command
                            self.update_menu();
                        },
                        ButtonPress::Down => {
                            match self.current_menu[0] {
                                MAIN_MENU => { // main menu, max ix 3
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
                                SETTINGS_MENU => { // settings menu, max ix 7
                                    if self.current_menu[1] < SETTINGS_UPLOAD_INTERVAL {
                                        self.current_menu[1] += 1;
                                    } else { // wrap around to 0
                                        self.current_menu[1] = SETTINGS_SIGHTING_PERIOD;
                                    }
                                }
                                ABOUT_MENU | STARTUP_MENU => { // 3 == about
                                    self.current_menu[0] = MAIN_MENU;
                                    self.current_menu[1] = MAIN_START_READING;
                                    self.update_menu();
                                }
                                SHUTDOWN_MENU => {
                                    self.current_menu[1] = (self.current_menu[1] + 1) % 2;
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
                                            SETTINGS_SIGHTING_PERIOD => {  // Sighting Period
                                                if control.sighting_period > 29 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.sighting_period -= 30;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_SIGHTING_PERIOD.to_string(), control.sighting_period.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
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
                                                if control.volume >= 0.1 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.volume -= 0.1;
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
                                SHUTDOWN_MENU => {
                                    self.current_menu[1] = (self.current_menu[1] + 1) % 2;
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
                                                                    reader.set_read_repeaters(self.read_repeaters.clone());
                                                                    reader.set_sight_processor(self.sight_processor.clone());
                                                                    let reconnector = Reconnector::new(
                                                                        self.readers.clone(),
                                                                        self.joiners.clone(),
                                                                        self.control_sockets.clone(),
                                                                        self.read_repeaters.clone(),
                                                                        self.sight_processor.clone(),
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
                                                                        },
                                                                        Err(e) => {
                                                                            println!("Error connecting to reader: {e}");
                                                                        }
                                                                    }
                                                                }
                                                                u_readers.push(reader);
                                                            }
                                                            println!("Notifying control sockets about change in status. (Start2)");
                                                            thread::sleep(Duration::from_millis(CONNECTION_CHANGE_PAUSE));
                                                            if let Ok(c_socks) = self.control_sockets.lock() {
                                                                for sock in c_socks.iter() {
                                                                    if let Some(sock) = sock {
                                                                        _ = socket::write_reader_list(&sock, &u_readers);
                                                                    }
                                                                }
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
                                            self.current_menu[1] = SETTINGS_SIGHTING_PERIOD;
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
                                                        thread::sleep(Duration::from_millis(CONNECTION_CHANGE_PAUSE));
                                                        if let Ok(c_socks) = self.control_sockets.lock() {
                                                            for sock in c_socks.iter() {
                                                                if let Some(sock) = sock {
                                                                    _ = socket::write_reader_list(&sock, &u_readers);
                                                                }
                                                            }
                                                        }
                                                    }
                                                },
                                                _ => {
                                                    println!("Auto connect is working right now.");
                                                    self.sound.notify_custom(SoundType::StartupInProgress);
                                                    self.current_menu[0] = STARTUP_MENU;
                                                    self.current_menu[1] = 0;
                                                },
                                            }
                                        } else {
                                            println!("Auto connect is working right now.");
                                            self.sound.notify_custom(SoundType::StartupInProgress);
                                            self.current_menu[0] = STARTUP_MENU;
                                            self.current_menu[1] = 0;
                                        }
                                    }
                                },
                                SETTINGS_MENU => {
                                    if let Ok(mut control) = self.control.lock() {
                                        match self.current_menu[1] {
                                            SETTINGS_SIGHTING_PERIOD => {  // Sighting Period
                                                if control.sighting_period < 99990 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.sighting_period += 30;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_SIGHTING_PERIOD.to_string(), control.sighting_period.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
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
                                                if control.volume < 1.0 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.volume += 0.1;
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
                                            _ => {}
                                        }
                                    }
                                    self.update_settings();
                                },
                                SHUTDOWN_MENU => {
                                    self.current_menu[1] = (self.current_menu[1] + 1) % 2;
                                },
                                ABOUT_MENU | STARTUP_MENU => { // 3 == about, 5 == startup
                                    self.current_menu[0] = MAIN_MENU;
                                    self.current_menu[1] = MAIN_START_READING;
                                    self.update_menu();
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
                                                            for ix in (0..u_readers.len()).rev() {
                                                                let mut reader = u_readers.remove(ix);
                                                                if reader.is_connected() != Some(true) {
                                                                    reader.set_control_sockets(self.control_sockets.clone());
                                                                    reader.set_read_repeaters(self.read_repeaters.clone());
                                                                    reader.set_sight_processor(self.sight_processor.clone());
                                                                    let reconnector = Reconnector::new(
                                                                        self.readers.clone(),
                                                                        self.joiners.clone(),
                                                                        self.control_sockets.clone(),
                                                                        self.read_repeaters.clone(),
                                                                        self.sight_processor.clone(),
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
                                                                        },
                                                                        Err(e) => {
                                                                            println!("Error connecting to reader: {e}");
                                                                        }
                                                                    }
                                                                    thread::sleep(Duration::from_millis(CONNECTION_CHANGE_PAUSE));
                                                                }
                                                                u_readers.push(reader);
                                                            }
                                                            println!("Notifying control sockets about change in status. (Start)");
                                                            thread::sleep(Duration::from_millis(CONNECTION_CHANGE_PAUSE));
                                                            if let Ok(c_socks) = self.control_sockets.lock() {
                                                                for sock in c_socks.iter() {
                                                                    if let Some(sock) = sock {
                                                                        _ = socket::write_reader_list(&sock, &u_readers);
                                                                    }
                                                                }
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
                                        _ => {}
                                    }
                                },
                                SETTINGS_MENU => { // settings -> saves settings and goes back
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
                                            let _ = lcd.backlight(false);
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
                                    } else {
                                        self.current_menu[0] = MAIN_MENU;
                                        self.current_menu[1] = MAIN_START_READING;
                                        self.update_menu();
                                    }
                                },
                                _ => {},
                            }
                        },
                    } 
                }
                presses.clear();
            }
            if let Ok(u_readers) = self.readers.lock() {
                // make sure to iterate through the vec in reverse so we don't have some weird loop issues
                let mut connected = false;
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
                if self.current_menu[0] == READING_MENU && !connected {
                    self.current_menu[0] = MAIN_MENU;
                    self.current_menu[1] = MAIN_START_READING;
                }
            }
            #[cfg(target_os = "linux")]
            {
                let mut messages: Vec<String> = vec!();
                if let Ok(info) = self.info.lock() {
                    messages.push(info.title_bar.clone())
                }
                let _ = lcd.home();
                match self.current_menu[0] {
                    MAIN_MENU => { // main menu, max ix 3
                        if let Ok(info) = self.info.lock() {
                            match self.current_menu[1] {
                                0 | 1 => {
                                    messages.push(info.main_menu[1].clone()); // Interface writes lines odd lines before even lines,
                                    messages.push(info.main_menu[0].clone()); // So order Vec as [Line 1, Line 3, Line 2, Line 4]
                                    messages.push(info.main_menu[2].clone());
                                },
                                _ => { // 2 | 3
                                    messages.push(info.main_menu[2].clone());
                                    messages.push(info.main_menu[1].clone());
                                    messages.push(info.main_menu[3].clone());
                                },
                            };
                        }
                    },
                    SETTINGS_MENU => { // settings menu, max ix 7
                        if let Ok(info) = self.info.lock() {
                            match self.current_menu[1] {
                                0 | 1 => {
                                    messages.push(info.settings_menu[1].clone());
                                    messages.push(info.settings_menu[0].clone());
                                    messages.push(info.settings_menu[2].clone());
                                },
                                2 => {
                                    messages.push(info.settings_menu[2].clone());
                                    messages.push(info.settings_menu[1].clone());
                                    messages.push(info.settings_menu[3].clone());
                                },
                                3 => {
                                    messages.push(info.settings_menu[3].clone());
                                    messages.push(info.settings_menu[2].clone());
                                    messages.push(info.settings_menu[4].clone());
                                },
                                4 => {
                                    messages.push(info.settings_menu[4].clone());
                                    messages.push(info.settings_menu[3].clone());
                                    messages.push(info.settings_menu[5].clone());
                                },
                                5 => {
                                    messages.push(info.settings_menu[5].clone());
                                    messages.push(info.settings_menu[4].clone());
                                    messages.push(info.settings_menu[6].clone());
                                },
                                _ => { // 6 | 7
                                    messages.push(info.settings_menu[6].clone());
                                    messages.push(info.settings_menu[5].clone());
                                    messages.push(info.settings_menu[7].clone());
                                },
                            };
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
                        messages.push(format!("{:^20}", ""));
                        messages.push(format!("{:^20}", format!("Version {}", env!("CARGO_PKG_VERSION"))));
                        messages.push(format!("{:^20}", "Chronokeep Portal"));
                        messages.push(format!("{:^20}", ""));
                    },
                    STARTUP_MENU => {
                        messages.clear();
                        messages.push(format!("{:^20}", ""));
                        messages.push(format!("{:^20}", "Please wait."));
                        messages.push(format!("{:^20}", "System Initializing."));
                        messages.push(format!("{:^20}", ""));
                    }
                    SHUTDOWN_MENU => {
                        messages.clear();
                        messages.push(format!("{:^20}", ""));
                        if self.current_menu[1] == 0 {
                            messages.push(String::from("     YES    > NO    "));
                        } else {
                            messages.push(String::from("   > YES      NO    "));
                        }
                        messages.push(format!("{:^20}", "Shutdown System?"));
                        messages.push(format!("{:^20}", ""));
                    },
                    SCREEN_OFF => {
                        let _ = lcd.clear();
                        let _ = lcd.backlight(false);
                        let _ = lcd.show_display(false);
                    }
                    _ => {}
                }
                for msg in &*messages {
                    let _ = write!(lcd, "{msg}");
                }
            }
            *waiting = true;
        }
        #[cfg(target_os = "linux")]
        {
            let _ = lcd.clear();
            let _ = lcd.backlight(false);
            let _ = lcd.show_display(false);
        }
        println!("LCD thread terminated.");
    }

    pub fn register_button(&self, button: ButtonPress) {
        if let Ok(mut presses) = self.button_presses.try_lock() {
            presses.push(button);
        }
        let (lock, cvar) = &*self.waiter;
        let mut waiting = lock.lock().unwrap();
        *waiting = false;
        cvar.notify_one();
    }

    #[cfg(target_os = "linux")]
    pub fn update(&self) {
        {
            let (lock, cvar) = &*self.waiter;
            let mut waiting = lock.lock().unwrap();
            *waiting = false;
            cvar.notify_one();
        }
    }
}