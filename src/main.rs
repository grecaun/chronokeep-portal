use core::panic;
use std::env;
use std::sync::Arc;
use std::sync::Mutex;
use dotenv::dotenv;
use crate::database::sqlite;
use crate::database::Database;
use crate::objects::backup;
use crate::objects::backup::Backup;
use crate::objects::setting;

pub mod control;
pub mod defaults;
pub mod database;
pub mod network;
pub mod objects;
pub mod reader;
pub mod types;
pub mod util;
pub mod llrp;
pub mod results;
pub mod remote;
pub mod processor;
pub mod sound_board;
pub mod screen;
pub mod buttons;
pub mod notifier;
pub mod battery;

const CONTROL_TYPE: &str = "socket";

fn main() {
    println!("Chronokeep Portal starting up...");
    if let Ok(_) = dotenv() {
        println!(".env file loaded successfully.")
    }
    let restore = sqlite::SQLite::already_exists() == false;
    let mut sqlite = sqlite::SQLite::new().unwrap();
    match sqlite.setup() {
        Ok(_) => println!("Database successfully setup."),
        Err(e) => {
            println!("Error setting up database: {e}");
            panic!()
        }
    }
    if restore {
        match backup::restore_backup() {
            Ok(val) => {
                for reader in val.readers {
                    match sqlite.save_reader(&reader) {
                        Ok(_) => {},
                        Err(e) => {
                            println!("error saving reader {e}");
                        }
                    }
                }
                for a in val.api {
                    match sqlite.save_api(&a) {
                        Ok(_) => {},
                        Err(e) => {
                            println!("error saving api {e}");
                        }
                    }
                }
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(control::SETTING_PORTAL_NAME),
                    val.name
                )) {
                    Ok(_) => {},
                    Err(e) => {
                        println!("error saving portal name {e}");
                    }
                }
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(control::SETTING_SIGHTING_PERIOD),
                    val.sighting_period.to_string()
                )) {
                    Ok(_) => {},
                    Err(e) => {
                        println!("error saving sighting period {e}");
                    }
                }
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(control::SETTING_READ_WINDOW),
                    val.read_window.to_string()
                )) {
                    Ok(_) => {},
                    Err(e) => {
                        println!("error saving read window {e}");
                    }
                }
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(control::SETTING_CHIP_TYPE),
                    val.chip_type
                )) {
                    Ok(_) => {},
                    Err(e) => {
                        println!("error saving chip type {e}");
                    }
                }
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(control::SETTING_PLAY_SOUND),
                    val.play_sound.to_string()
                )) {
                    Ok(_) => {},
                    Err(e) => {
                        println!("error saving chip type {e}");
                    }
                }
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(control::SETTING_VOLUME),
                    val.volume.to_string()
                )) {
                    Ok(_) => {},
                    Err(e) => {
                        println!("error saving chip type {e}");
                    }
                }
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(control::SETTING_VOICE),
                    String::from(val.voice.as_str())
                )) {
                    Ok(_) => {},
                    Err(e) => {
                        println!("error saving chip type {e}");
                    }
                }
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(control::SETTING_AUTO_REMOTE),
                    val.auto_remote.to_string()
                )) {
                    Ok(_) => {},
                    Err(e) => {
                        println!("error saving chip type {e}");
                    }
                }
                match sqlite.set_setting(&setting::Setting::new(
                    String::from(control::SETTING_UPLOAD_INTERVAL),
                    val.upload_interval.to_string()
                )) {
                    Ok(_) => {},
                    Err(e) => {
                        println!("error saving chip type {e}");
                    }
                }
            },
            Err(_) => (),
        };
    }
    let control = Arc::new(Mutex::new(control::Control::new(&sqlite).unwrap()));
    let sqlite = Arc::new(Mutex::new(sqlite));
    println!("Control values retrieved from database.");
    if let Ok(control) = control.lock() {
        println!("Portal is named '{}'.", control.name);
        println!("Portal version is '{}'", env!("CARGO_PKG_VERSION"));
        println!("Sightings will be ignored if received within {}", util::pretty_time(&u64::from(control.sighting_period)));
        println!("Play sound value set to {}.", control.play_sound);
    }
    else {
        println!("Unable to get control mutex for some reason.");
    }
    let keepalive: Arc<Mutex<bool>> = Arc::new(Mutex::new(true));
    // Check for command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() > 0 && args[0].as_str() == "daemon" {
        control::socket::control_loop(sqlite.clone(), &control, keepalive.clone())
    }  else {
        match CONTROL_TYPE {
            "socket" => {
                control::socket::control_loop(sqlite.clone(), &control, keepalive.clone())
            },
            "cli" => {
                control::cli::control_loop(sqlite.clone(), &control);
            },
            other => {
                println!("'{other}' is not a valid control type.");
            }
        }
    }
    if let Ok(sq) = sqlite.lock() {
        let control: control::Control = control::Control::new(&sq).unwrap();
        let readers = sq.get_readers().unwrap();
        let api = sq.get_apis().unwrap();
        let backup = Backup{
            name: control.name,
            sighting_period: control.sighting_period,
            read_window: control.read_window,
            chip_type: control.chip_type,
            play_sound: control.play_sound,
            volume: control.volume,
            voice: control.sound_board.get_voice(),
            auto_remote: control.auto_remote,
            upload_interval: control.upload_interval,
            readers,
            api
        };
        backup::save_backup(&backup, None);
    }
    println!("Goodbye!");
    if let Ok(control) = control.lock() {
        if control.play_sound {
            control.sound_board.play_shutdown(control.volume);
        }
    };
}