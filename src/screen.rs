use std::{net::TcpStream, sync::{Arc, Mutex}};

#[cfg(target_os = "linux")]
use std::fmt::Write;
#[cfg(target_os = "linux")]
use i2c_character_display::{AdafruitLCDBackpack, LcdDisplayType};
#[cfg(target_os = "linux")]
use rppal::{hal, i2c::I2c};
use std::sync::Condvar;

use crate::{control::{socket::{self, MAX_CONNECTED}, Control, SETTING_AUTO_REMOTE, SETTING_CHIP_TYPE, SETTING_PLAY_SOUND, SETTING_READ_WINDOW, SETTING_SIGHTING_PERIOD, SETTING_UPLOAD_INTERVAL, SETTING_VOICE, SETTING_VOLUME}, database::{sqlite, Database}, objects::setting::Setting, reader::{self, ANTENNA_STATUS_NONE}, remote::uploader::{self, Status}, sound_board::Voice, types::{TYPE_CHIP_DEC, TYPE_CHIP_HEX}};

pub const EMPTY_STRING: &str = "                    ";

#[derive(Clone)]
pub struct CharacterDisplay {
    keepalive: Arc<Mutex<bool>>,
    control: Arc<Mutex<Control>>,
    readers: Arc<Mutex<Vec<reader::Reader>>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    control_sockets: Option<Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>>,
    waiter: Arc<(Mutex<bool>, Condvar)>,
    button_presses: Arc<Mutex<Vec<ButtonPress>>>,
    title_bar: String,
    reader_info: Vec<String>,
    main_menu: Vec<String>,
    settings_menu: Vec<String>,
    current_menu: [u8; 3],
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
    ) -> Self {
        Self {
            keepalive,
            control,
            readers,
            sqlite,
            control_sockets: None,
            waiter: Arc::new((Mutex::new(true), Condvar::new())),
            button_presses: Arc::new(Mutex::new(Vec::new())),
            title_bar: String::from(""),
            reader_info: Vec::new(),
            main_menu: vec![
                " > Start Reading    ".to_string(),
                "   Settings         ".into(),
                "   About            ".into(),
                "   Shutdown         ".into(),
            ],
            settings_menu: Vec::new(),
            current_menu: [0, 0, 0],
        }
    }

    pub fn set_control_sockets(&mut self, sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>) {
        self.control_sockets = Some(sockets);
    }

    pub fn update_title_bar(&mut self, status: uploader::Status, err_count: isize) {
        if let Ok(control) = self.control.lock() {
            if err_count > 9 {
                self.title_bar = format!("{:.17} 9+", control.name)
            } else if err_count > 0 {
                self.title_bar = format!("{:.17}  {}", control.name, err_count)
            } else {
                let mut upload_status = " ";
                if status == Status::Running {
                    upload_status = "+";
                } else if status == Status::Stopped || status == Status::Stopping {
                    upload_status = "-";
                }
                self.title_bar = format!("{:.17}  {}", control.name, upload_status)
            }
        }
    }

    pub fn update_readers(&mut self) {
        let mut lines: Vec<[u8;9]> = Vec::new();
        self.reader_info.clear();
        // Collect all connected readers.
        if let Ok(readers) = self.readers.lock() {
            // Two readers per line, integer divison rounds towards zero but we want ceiling.
            let mut num = 0;
            for read in readers.iter() {
                num += 1;
                if let Some(is_con) = read.is_connected() {
                    if is_con {
                        let mut line = [ANTENNA_STATUS_NONE; 9];
                        line[0] = num;
                        if let Ok(ants) = read.antennas.lock() {
                            line[1] = ants[0];
                            line[2] = ants[1];
                            line[3] = ants[2];
                            line[4] = ants[3];
                            line[5] = ants[4];
                            line[6] = ants[5];
                            line[7] = ants[6];
                            line[8] = ants[7];
                        }
                        lines.push(line);
                    }
                }
            }
        }
        // Add all reader lines to the menu.
        for ix in 0..(lines.len() + 1)/2 {
            // Doing two lines at a time means the real index is ix*2.
            // If the number of lines is less than the real index + 1 then there is no second
            // reader in this set of lines
            if lines.len() < ix*2+1 {
                self.reader_info.push(
                    format!("{:.1}{}{}{}{}{}{}{}{}           ",
                        lines[ix*2][0],
                        reader::helpers::antenna_status_str(lines[ix*2][1]),
                        reader::helpers::antenna_status_str(lines[ix*2][2]),
                        reader::helpers::antenna_status_str(lines[ix*2][3]),
                        reader::helpers::antenna_status_str(lines[ix*2][4]),
                        reader::helpers::antenna_status_str(lines[ix*2][5]),
                        reader::helpers::antenna_status_str(lines[ix*2][6]),
                        reader::helpers::antenna_status_str(lines[ix*2][7]),
                        reader::helpers::antenna_status_str(lines[ix*2][8]),
                    ))
            } else {
                self.reader_info.push(
                    format!("{}{}{}{}{}{}{}{}{}  {}{}{}{}{}{}{}{}{}",
                    lines[ix*2][0],
                    reader::helpers::antenna_status_str(lines[ix*2][1]),
                    reader::helpers::antenna_status_str(lines[ix*2][2]),
                    reader::helpers::antenna_status_str(lines[ix*2][3]),
                    reader::helpers::antenna_status_str(lines[ix*2][4]),
                    reader::helpers::antenna_status_str(lines[ix*2][5]),
                    reader::helpers::antenna_status_str(lines[ix*2][6]),
                    reader::helpers::antenna_status_str(lines[ix*2][7]),
                    reader::helpers::antenna_status_str(lines[ix*2][8]),
                    lines[ix*2+1][0],
                    reader::helpers::antenna_status_str(lines[ix*2+1][1]),
                    reader::helpers::antenna_status_str(lines[ix*2+1][2]),
                    reader::helpers::antenna_status_str(lines[ix*2+1][3]),
                    reader::helpers::antenna_status_str(lines[ix*2+1][4]),
                    reader::helpers::antenna_status_str(lines[ix*2+1][5]),
                    reader::helpers::antenna_status_str(lines[ix*2+1][6]),
                    reader::helpers::antenna_status_str(lines[ix*2+1][7]),
                    reader::helpers::antenna_status_str(lines[ix*2+1][8]),
                    ))
            }
        }
    }

    pub fn update_menu(&mut self) {
        match self.current_menu[0] {
            0 => { // main menu, max ix 3
                for line in self.main_menu.iter_mut() {
                    line.replace_range(1..2, " ");
                }
                self.main_menu[self.current_menu[1] as usize].replace_range(1..2, ">");
            },
            1 => { // settings menu, max ix 7
                for line in self.settings_menu.iter_mut() {
                    line.replace_range(1..2, " ");
                }
                self.settings_menu[self.current_menu[1] as usize].replace_range(1..2, ">");
            }
            _ => {}
        }
    }

    pub fn update_settings(&mut self) {
        self.settings_menu.clear();
        if let Ok(control) = self.control.lock() {
            let mut play_sound = "no";
            if control.play_sound {
                play_sound = "yes";
            }
            let mut auto_upload = "no";
            if control.auto_remote {
                auto_upload = "yes";
            }
            self.settings_menu.push(format!("   Sightings   {:>4.4} ", control.sighting_period));
            self.settings_menu.push(format!("   Read Window {:>4.4} ", control.read_window));
            self.settings_menu.push(format!("   Chip Type   {:>4.4} ", control.chip_type));
            self.settings_menu.push(format!("   Play Sounds {:>4.4} ", play_sound));
            self.settings_menu.push(format!("   Volume      {:>4.4} ", (control.volume * 10.0) as usize));
            self.settings_menu.push(format!("   Voice    {:>7.7} ", control.sound_board.get_voice().as_str()));
            self.settings_menu.push(format!("   Auto Upload {:>4.4} ", auto_upload));
            self.settings_menu.push(format!("   Upload Int  {:>4.4} ", control.upload_interval));
        }
    }

    pub fn run(&mut self, bus: u8) {
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
            self.update_title_bar(uploader::Status::Unknown, 0);
            let mut messages: Vec<String> = vec!(self.title_bar.clone());
            messages.push(self.main_menu[1].clone());
            messages.push(self.main_menu[0].clone());
            messages.push(self.main_menu[2].clone());
            for msg in &*messages {
                let _ = write!(lcd, "{msg}");
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
                            println!("Up button registered.");
                            match self.current_menu[0] {
                                0 => {
                                    if self.current_menu[1] > 0 {
                                        self.current_menu[1] -= 1;
                                    } else {
                                        self.current_menu[1] = 4;
                                    }
                                }
                                1 => {
                                    if self.current_menu[1] > 0 {
                                        self.current_menu[1] -= 1;
                                    } else {
                                        self.current_menu[1] = 7;
                                    }
                                }
                                2 => {}, // current reading, do nothing
                                _ => { // 3 == about, 2 == ?
                                    self.current_menu[0] = 0;
                                    self.current_menu[1] = 0;
                                }
                            }
                            self.current_menu[2] = 0;
                            self.update_menu();
                        },
                        ButtonPress::Down => {
                            println!("Down button registered.");
                            match self.current_menu[0] {
                                0 => { // main menu, max ix 3
                                    if self.current_menu[1] < 3 {
                                        self.current_menu[1] += 1;
                                    } else { // wrap around to the start
                                        self.current_menu[1] = 0;
                                    }
                                },
                                1 => { // settings menu, max ix 7
                                    if self.current_menu[1] < 7 {
                                        self.current_menu[1] += 1;
                                    } else { // wrap around to 0
                                        self.current_menu[1] = 0;
                                    }
                                }
                                2 => {}, // current reading, do nothing
                                _ => { // 3 == about
                                    self.current_menu[0] = 0;
                                    self.current_menu[1] = 0;
                                }
                            }
                            self.current_menu[2] = 0;
                            self.update_menu();
                        },
                        ButtonPress::Left => {
                            println!("Left button registered.");
                            match self.current_menu[0] {
                                0 => {},
                                1 => {
                                    if let Ok(mut control) = self.control.lock() {
                                        match self.current_menu[1] {
                                            0 => {  // Sighting Period
                                                if control.sighting_period > 29 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.sighting_period -= 30;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_SIGHTING_PERIOD.to_string(), control.sighting_period.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                            1 => {  // Read Window
                                                if control.read_window > 5 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.read_window -= 1;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_READ_WINDOW.to_string(), control.read_window.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                            2 => {  // Chip Type
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
                                            3 => {  // Play Sound
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    control.play_sound = !control.play_sound;
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_PLAY_SOUND.to_string(), control.play_sound.to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            4 => {  // Volume
                                                if control.volume >= 0.1 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.volume -= 0.1;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_VOLUME.to_string(), control.volume.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                            5 => {  // Voice
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
                                            6 => {  // Auto Upload
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    control.auto_remote = !control.auto_remote;
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_AUTO_REMOTE.to_string(), control.auto_remote.to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            7 => {  // Upload Interval
                                                if control.upload_interval > 0{
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
                                },
                                2 => {}, // currently reading, do nothing
                                _ => { // 3 == about
                                    self.current_menu[0] = 0;
                                    self.current_menu[1] = 0;
                                    self.update_menu();
                                },
                            }
                            self.current_menu[2] = 0;
                        },
                        ButtonPress::Right => {
                            println!("Right button registered.");
                            match self.current_menu[0] {
                                0 => { // similar to enter function

                                },
                                1 => {
                                    if let Ok(mut control) = self.control.lock() {
                                        match self.current_menu[1] {
                                            0 => {  // Sighting Period
                                                if control.sighting_period < 99990 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.sighting_period += 30;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_SIGHTING_PERIOD.to_string(), control.sighting_period.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                            1 => {  // Read Window
                                                if control.read_window < 50 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.read_window += 1;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_READ_WINDOW.to_string(), control.read_window.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                            2 => {  // Chip Type
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
                                            3 => {  // Play Sound
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    control.play_sound = !control.play_sound;
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_PLAY_SOUND.to_string(), control.play_sound.to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            4 => {  // Volume
                                                if control.volume < 1.0 {
                                                    if let Ok(sq) = self.sqlite.lock() {
                                                        control.volume += 0.1;
                                                        if let Err(e) = sq.set_setting(&Setting::new(SETTING_VOLUME.to_string(), control.volume.to_string())) {
                                                            println!("Error saving setting: {e}");
                                                        }
                                                    }
                                                }
                                            }
                                            5 => {  // Voice
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
                                            6 => {  // Auto Upload
                                                if let Ok(sq) = self.sqlite.lock() {
                                                    control.auto_remote = !control.auto_remote;
                                                    if let Err(e) = sq.set_setting(&Setting::new(SETTING_AUTO_REMOTE.to_string(), control.auto_remote.to_string())) {
                                                        println!("Error saving setting: {e}");
                                                    }
                                                }
                                            }
                                            7 => {  // Upload Interval
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
                                },
                                2 => {
                                    if self.current_menu[2] == 1 {
                                        // TODO stop reading
                                    }
                                },
                                _ => { // 3 == about
                                    self.current_menu[0] = 0;
                                    self.current_menu[1] = 0;
                                    self.update_menu();
                                },
                            }
                            self.current_menu[2] = 0;
                        },
                        ButtonPress::Enter => {
                            println!("Enter button registered.");
                            match self.current_menu[0] {
                                0 => { // main
                                    match self.current_menu[1] {
                                        0 => { // Start Reading
                                        },
                                        1 => { // Settings
                                            self.current_menu[0] = 1;
                                            self.current_menu[1] = 0;
                                            self.update_settings();
                                        }
                                        2 => { // About
                                            self.current_menu[0] = 3;
                                            self.current_menu[1] = 0;
                                        },
                                        3 => { // Shutdown
                                            // TODO Deal with shutdown
                                        },
                                        _ => {}
                                    }
                                },
                                1 => { // settings
                                    self.current_menu[0] = 0;
                                    self.current_menu[1] = 0;
                                    // notify of settings changes
                                    if let Some(csock) = &self.control_sockets {
                                        if let Ok(sq) = self.sqlite.try_lock() {
                                            let settings = socket::get_settings(&sq);
                                            if let Ok(socks) = csock.try_lock() {
                                                for sock_opt in &*socks {
                                                    if let Some(sock) = sock_opt {
                                                        _ = socket::write_settings(&sock, &settings);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                2 => { // currently reading
                                    self.current_menu[2] = 1;
                                },
                                _ => { // 3 => about menu
                                    self.current_menu[0] = 0;
                                    self.current_menu[1] = 0;
                                    self.update_menu();
                                },
                            }
                        },
                    } 
                }
                presses.clear();
                #[cfg(target_os = "linux")]
                {
                    let mut messages: Vec<String> = vec!(self.title_bar.clone());
                    let _ = lcd.clear();
                    let _ = lcd.home();
                    match self.current_menu[0] {
                        0 => { // main menu, max ix 3
                            match self.current_menu[1] {
                                0 | 1 => {
                                    messages.push(self.main_menu[1].clone()); // Interface writes lines odd lines before even lines,
                                    messages.push(self.main_menu[0].clone()); // So order Vec as [Line 1, Line 3, Line 2, Line 4]
                                    messages.push(self.main_menu[2].clone());
                                },
                                _ => { // 2 | 3
                                    messages.push(self.main_menu[2].clone());
                                    messages.push(self.main_menu[1].clone());
                                    messages.push(self.main_menu[3].clone());
                                },
                            };
                        },
                        1 => { // settings menu, max ix 7
                            match self.current_menu[1] {
                                0 | 1 => {
                                    messages.push(self.settings_menu[1].clone());
                                    messages.push(self.settings_menu[0].clone());
                                    messages.push(self.settings_menu[2].clone());
                                },
                                2 => {
                                    messages.push(self.settings_menu[2].clone());
                                    messages.push(self.settings_menu[1].clone());
                                    messages.push(self.settings_menu[3].clone());
                                },
                                3 => {
                                    messages.push(self.settings_menu[3].clone());
                                    messages.push(self.settings_menu[2].clone());
                                    messages.push(self.settings_menu[4].clone());
                                },
                                4 => {
                                    messages.push(self.settings_menu[4].clone());
                                    messages.push(self.settings_menu[3].clone());
                                    messages.push(self.settings_menu[5].clone());
                                },
                                5 => {
                                    messages.push(self.settings_menu[5].clone());
                                    messages.push(self.settings_menu[4].clone());
                                    messages.push(self.settings_menu[6].clone());
                                },
                                _ => { // 6 | 7
                                    messages.push(self.settings_menu[6].clone());
                                    messages.push(self.settings_menu[5].clone());
                                    messages.push(self.settings_menu[7].clone());
                                },
                            };
                        }
                        2 => { // reader is reading
                            if self.reader_info.len() > 0 {
                                messages.push(self.reader_info[0].clone());
                            }
                            if self.reader_info.len() > 1 {
                                messages.push(self.reader_info[1].clone());
                            }
                        }
                        3 => { // about menu
                            messages.push(format!("{:^20.20}", ""));
                            messages.push(format!("{:^20.20}", env!("CARGO_PKG_VERSION")));
                            messages.push(format!("{:^20.20}", "Chronokeep Portal"));
                            messages.push(format!("{:^20.20}", ""));
                        }
                        _ => {}
                    }
                    for msg in &*messages {
                        let _ = write!(lcd, "{msg}");
                    }
                }
                // TODO Update the screen.
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
    pub fn stop(&self) {
        if let Ok(mut keepalive) = self.keepalive.lock() {
            *keepalive = false;
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