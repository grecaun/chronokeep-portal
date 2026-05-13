use std::{collections::HashMap, sync::{Arc, Condvar, Mutex}, time::{Duration, Instant, SystemTime}};

use rand::Rng;

use crate::{reader::zebra::TagData, sound_board};

pub struct Sounds {
    control: Arc<Mutex<super::Control>>,
    keepalive: Arc<Mutex<bool>>,
    mtx: Arc<Mutex<bool>>,
    sound_notifier: Arc<SoundNotifier>
}

pub struct SoundNotifier {
    notifier: Arc<Condvar>,
    beep: Arc<Mutex<bool>>,
    tag_list: Arc<Mutex<HashMap<u128, u64>>>,
    sound_list: Arc<Mutex<Vec<SoundType>>>
}

pub enum SoundType {
    Volume,
    Introduction,
    StartupFinished,
    StartupInProgress,
    CustomNotAvailable,
    Connected,
    Disconnected,
    Malfunction,
}

impl SoundNotifier {
    pub fn new() -> SoundNotifier {
        SoundNotifier {
            beep: Arc::new(Mutex::new(false)),
            notifier: Arc::new(Condvar::new()),
            tag_list: Arc::new(Mutex::new(HashMap::new())),
            sound_list: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn notify_tags(&self, tags: &Vec<TagData>) {
        if let Ok(cur_dur) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            let cur_time = cur_dur.as_secs(); // current time as unix timestamp
            if let Ok(mut t_list) = self.tag_list.lock() {
                for tag in tags {
                    // Play a sound if we haven't seen the tag ever, or if we've not seen it in the past 60 seconds.
                    if !t_list.contains_key(&tag.tag()) || (t_list.get(&tag.tag()).unwrap() + 60) > cur_time {
                        if let Ok(mut val) = self.beep.lock() {
                            *val = true;
                        }
                        // Update the time we've played a sound.
                        t_list.insert(tag.tag(), cur_time);
                    }
                }
            }
        }
        self.notifier.notify_one()
    }

    pub fn notify_one(&self) {
        if let Ok(mut val) = self.beep.lock() {
            *val = true;
        }
        self.notifier.notify_one()
    }

    pub fn notify_custom(&self, sound: SoundType) {
        if let Ok(mut sounds) = self.sound_list.lock() {
            sounds.push(sound);
        }
        self.notifier.notify_one()
    }
}

impl Sounds {
    pub fn new(
        control: Arc<Mutex<super::Control>>,
        keepalive: Arc<Mutex<bool>>
    ) -> Sounds {
        Sounds {
            control,
            keepalive,
            mtx: Arc::new(Mutex::new(true)),
            sound_notifier: Arc::new(SoundNotifier::new())
        }
    }

    pub fn get_notifier(&self) -> Arc<SoundNotifier> {
        self.sound_notifier.clone()
    }

    pub fn run(&mut self) {
        let mut last_sound = Instant::now();
        let mut separation: u64 = sound_board::BEEP_SEPARATION + rand::thread_rng().gen_range(0..sound_board::BEEP_SEP_RAND_MAX);
        loop {
            if let Ok(ka) = self.keepalive.try_lock() {
                match *ka {
                    false => {
                        break
                    },
                    true => {},
                }
            }
            let notifier = self.mtx.lock().unwrap();
            if let Ok(_) = self.sound_notifier.notifier.wait(notifier) {
                if let Ok(control) = self.control.lock() {
                    if let Ok(mut beep) = self.sound_notifier.beep.lock() {
                        if *beep == true {
                            // always change beep back to false, even if we don't play the sound
                            // if this doesn't happen then it may beep at some point we don't want it to beep
                            if control.play_sound == true && last_sound.elapsed() >= Duration::from_millis(separation) {
                                control.sound_board.play_sound(control.volume);
                                last_sound = Instant::now();
                                separation = sound_board::BEEP_SEPARATION + rand::thread_rng().gen_range(0..sound_board::BEEP_SEP_RAND_MAX);
                            }
                            *beep = false;
                        }
                    }
                    if let Ok(mut sounds) = self.sound_notifier.sound_list.lock() {
                        // go through all the custom values, then clear the vec
                        if control.play_sound == true {
                            for sound in &*sounds {
                                match sound {
                                    SoundType::Volume => control.sound_board.play_volume(control.volume),
                                    SoundType::Introduction => control.sound_board.play_introduction(control.volume),
                                    SoundType::StartupFinished => control.sound_board.play_startup_finished(control.volume),
                                    SoundType::StartupInProgress => control.sound_board.play_startup_in_progress(control.volume),
                                    SoundType::CustomNotAvailable => control.sound_board.play_custom_not_available(control.volume),
                                    SoundType::Connected => control.sound_board.play_connected(control.volume),
                                    SoundType::Disconnected => control.sound_board.play_disconnected(control.volume),
                                    SoundType::Malfunction => control.sound_board.play_malfunction(control.volume),
                                }
                            }
                        };
                        let list = &mut *sounds;
                        list.clear();
                    }
                }
            } else {
                println!("Error waiting to play a sound.");
            }
        }
    }
}