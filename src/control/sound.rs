use std::{sync::{Arc, Mutex, Condvar}, time::{Duration, Instant}};

use rand::Rng;

use crate::sound_board;

pub struct Sounds {
    control: Arc<Mutex<super::Control>>,
    keepalive: Arc<Mutex<bool>>,
    mtx: Arc<Mutex<bool>>,
    sound_notifier: Arc<SoundNotifier>
}

pub struct SoundNotifier {
    notifier: Arc<Condvar>,
    beep: Arc<Mutex<bool>>,
    sound_list: Arc<Mutex<Vec<SoundType>>>
}

pub enum SoundType {
    Volume,
    Introduction,
    StartupFinished,
    StartupInProgress,
    CustomNotAvailable
}

impl SoundNotifier {
    pub fn new() -> SoundNotifier {
        SoundNotifier {
            beep: Arc::new(Mutex::new(false)),
            sound_list: Arc::new(Mutex::new(Vec::new())),
            notifier: Arc::new(Condvar::new())
        }
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