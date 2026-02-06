use std::collections::HashMap;

use rand::random;

use crate::{control::{SETTING_AUTO_REMOTE, SETTING_CHIP_TYPE, SETTING_ENABLE_NTFY, SETTING_NTFY_PASS, SETTING_NTFY_TOPIC, SETTING_NTFY_URL, SETTING_NTFY_USER, SETTING_PLAY_SOUND, SETTING_PORTAL_NAME, SETTING_READ_WINDOW, SETTING_SCREEN_TYPE, SETTING_UPLOAD_INTERVAL, SETTING_VOICE, SETTING_VOLUME}, database::{DBError, sqlite::SQLite}, defaults::{DEFAULT_AUTO_REMOTE, DEFAULT_CHIP_TYPE, DEFAULT_ENABLE_NTFY, DEFAULT_PLAY_SOUND, DEFAULT_READ_WINDOW, DEFAULT_SCREEN_TYPE, DEFAULT_UPLOAD_INTERVAL, DEFAULT_VOLUME}, network::api, objects::{self, read, setting}, reader, sound_board::Voice};


#[cfg(test)]
mod tests;

pub struct MemStore {
    backend: SQLite,
    settings: HashMap<String, setting::Setting>,
    api: HashMap<i64, api::Api>,
    readers: HashMap<i64, reader::Reader>,
    reads: HashMap<u64, read::Read>,
}

impl MemStore {
    pub fn new() -> Result<MemStore, DBError> {
        match SQLite::new() {
            Ok(db) =>
                Ok(MemStore {
                    backend: db,
                    settings: HashMap::new(),
                    api: HashMap::new(),
                    readers: HashMap::new(),
                    reads: HashMap::new()
                }),
            Err(e) => Err(e)
        }
    }
}

impl super::Database for MemStore {
    fn setup(&mut self) -> Result<(), super::DBError> {
        if let Err(e) = self.backend.setup() {
            return Err(e);
        }
        self.settings.insert(String::from(SETTING_PORTAL_NAME), match self.backend.get_setting(SETTING_PORTAL_NAME) {
            Ok(s) => s,
            Err(_) => {
                let rval: u8 = random();
                let n = format!("Chrono Portal {}", rval);
                let set = setting::Setting::new(String::from(SETTING_PORTAL_NAME), n);
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_CHIP_TYPE), match self.backend.get_setting(SETTING_CHIP_TYPE) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_CHIP_TYPE), String::from(DEFAULT_CHIP_TYPE));
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_READ_WINDOW), match self.backend.get_setting(SETTING_READ_WINDOW) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_READ_WINDOW), format!("{}", DEFAULT_READ_WINDOW));
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_PLAY_SOUND), match self.backend.get_setting(SETTING_PLAY_SOUND) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_PLAY_SOUND), format!("{}", DEFAULT_PLAY_SOUND));
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_VOLUME), match self.backend.get_setting(SETTING_VOLUME) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_VOLUME), format!("{}", DEFAULT_VOLUME));
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_VOICE), match self.backend.get_setting(SETTING_VOICE) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_VOICE), String::from(Voice::Emily.as_str()));
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_AUTO_REMOTE), match self.backend.get_setting(SETTING_AUTO_REMOTE) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_AUTO_REMOTE), format!("{}", DEFAULT_AUTO_REMOTE));
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_UPLOAD_INTERVAL), match self.backend.get_setting(SETTING_UPLOAD_INTERVAL) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_UPLOAD_INTERVAL), format!("{}", DEFAULT_UPLOAD_INTERVAL));
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_NTFY_URL), match self.backend.get_setting(SETTING_NTFY_URL) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_NTFY_URL), String::new());
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_NTFY_USER), match self.backend.get_setting(SETTING_NTFY_USER) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_NTFY_USER), String::new());
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_NTFY_PASS), match self.backend.get_setting(SETTING_NTFY_PASS) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_NTFY_PASS), String::new());
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_NTFY_TOPIC), match self.backend.get_setting(SETTING_NTFY_TOPIC) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_NTFY_TOPIC), String::new());
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_ENABLE_NTFY), match self.backend.get_setting(SETTING_ENABLE_NTFY) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_ENABLE_NTFY), format!("{}", DEFAULT_ENABLE_NTFY));
                _ = self.backend.set_setting(&set);
                set
            },
        });
        self.settings.insert(String::from(SETTING_SCREEN_TYPE), match self.backend.get_setting(SETTING_SCREEN_TYPE) {
            Ok(s) => s,
            Err(_) => {
                let set = setting::Setting::new(String::from(SETTING_SCREEN_TYPE), String::from(DEFAULT_SCREEN_TYPE));
                _ = self.backend.set_setting(&set);
                set
            },
        });
        if let Ok(apis) = &mut self.backend.get_apis() {
            for api in apis {
                self.api.insert(api.id(), api.clone());
            }
        }
        if let Ok(readers) = &mut self.backend.get_readers() {
            for r in readers {
                self.readers.insert(r.id(), r.clone());
            }
        }
        if let Ok(reads) = self.backend.get_all_reads() {
            for r in reads {
                self.reads.insert(r.id(), r);
            }
        }
        Ok(())
    }

    fn set_setting(&mut self, setting: &setting::Setting) -> Result<setting::Setting, super::DBError> {
        match self.backend.set_setting(setting) {
            Ok(s) => {
                let t = s.clone();
                self.settings.insert(String::from(t.name()), t);
                Ok(s)
            },
            Err(e) => Err(e)
        }
    }

    fn get_setting(&self, name: &str) -> Result<setting::Setting, super::DBError> {
        match self.settings.get(&String::from(name)) {
            Some(s) => Ok(s.clone()),
            None => Err(DBError::NotFound),
        }
    }

    fn save_reader(&mut self, reader: &reader::Reader) -> Result<i64, super::DBError> {
        match self.backend.save_reader(reader) {
            Ok(i) => {
                let mut n_reader = reader.clone();
                n_reader.set_id(i);
                self.readers.insert(i, n_reader);
                Ok(i)
            },
            Err(e) => Err(e)
        }
    }

    fn get_reader(&self, id: &i64) -> Result<reader::Reader, super::DBError> {
        match self.readers.get(id) {
            Some(reader) => Ok(reader.clone()),
            None => Err(DBError::NotFound),
        }
    }

    fn get_readers(&self) -> Result<Vec<reader::Reader>, super::DBError> {
        let mut output: Vec<reader::Reader> = Vec::new();
        for reader in self.readers.values() {
            output.push(reader.clone());
        }
        Ok(output)
    }

    fn delete_reader(&mut self, id: &i64) -> Result<usize, super::DBError> {
        match self.backend.delete_reader(id) {
            Ok(num) => {
                self.readers.remove(id);
                Ok(num)
            }
            Err(e) => Err(e)
        }
    }

    fn save_api(&mut self, api: &api::Api) -> Result<i64, super::DBError> {
        todo!()
    }

    fn get_apis(&self) -> Result<Vec<api::Api>, super::DBError> {
        todo!()
    }

    fn delete_api(&mut self, id: &i64) -> Result<usize, super::DBError> {
        todo!()
    }

    fn save_reads(&mut self, reads: &Vec<objects::read::Read>) -> Result<usize, super::DBError> {
        todo!()
    }

    fn get_reads(&self, start: i64, end: i64) -> Result<Vec<objects::read::Read>, super::DBError> {
        todo!()
    }

    fn get_all_reads(&self) -> Result<Vec<objects::read::Read>, super::DBError> {
        todo!()
    }

    fn delete_reads(&mut self, start: i64, end: i64) -> Result<usize, super::DBError> {
        todo!()
    }

    fn delete_all_reads(&mut self) -> Result<usize, super::DBError> {
        todo!()
    }

    fn reset_reads_upload(&mut self) -> Result<usize, super::DBError> {
        todo!()
    }

    fn get_not_uploaded_reads(&self) -> Result<Vec<objects::read::Read>, super::DBError> {
        todo!()
    }

    fn update_reads_status(&mut self, reads: &Vec<objects::read::Read>) -> Result<usize, super::DBError> {
        todo!()
    }
}