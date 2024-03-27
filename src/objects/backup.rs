use std::{path::Path, fs::File, io::Read};

use serde::{Serialize, Deserialize};

use crate::{network::api, reader, sound_board::Voice};

pub const BACKUP_FILE_PATH: &str = "./portal_backup.json";

#[derive(Serialize,  Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Backup {
    pub name: String,
    pub sighting_period: u32,
    pub read_window: u8,
    pub chip_type: String,
    pub play_sound: bool,
    pub volume: f32,
    pub voice: Voice,
    pub auto_remote: bool,
    pub upload_interval: u64,

    pub readers: Vec<reader::Reader>,
    pub api: Vec<api::Api>,
}

pub fn restore_backup() -> Result<Backup, &'static str> {
    let path = Path::new(BACKUP_FILE_PATH);
    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(e) => {
            println!("Nothing to restore. {e}");
            return Err("nothing to restore")
        }
    };
    let mut s = String::new();
    match file.read_to_string(&mut s) {
        Ok(_) => {
            // process the string
            let output: Backup = match serde_json::from_str(s.as_str()) {
                Ok(it) => it,
                Err(e) => {
                    println!("Error deserializing backup. {e}");
                    return Err("unable to deserialize backed up settings")
                }
            };
            Ok(output)
        }
        Err(e) => {
            println!("Error reading file. {e}");
            Err("error reading the file")
        }
    }
}

pub fn save_backup(backup: &Backup, back_path: Option<&str>) {
    let mut path = Path::new(BACKUP_FILE_PATH);
    if let Some(b_path) = back_path {
        path = Path::new(b_path);
    }
    let file = match File::create(&path) {
        Ok(file) => file,
        Err(e) => {
            println!("error creating new file {e}");
            return
        }
    };
    match serde_json::to_writer_pretty(&file, backup) {
        Ok(_) => (),
        Err(e) => {
            println!("error writing backup {e}")
        }
    }
}