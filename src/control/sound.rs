use std::{sync::{Arc, Mutex, Condvar}, time::{Duration, Instant}};

pub struct Sounds {
    control: Arc<Mutex<super::Control>>,
    keepalive: Arc<Mutex<bool>>,
    mtx: Arc<Mutex<bool>>,
    sound_notifier: Arc<SoundNotifier>
}

pub struct SoundNotifier {
    notifier: Arc<Condvar>,
    beep: Arc<Mutex<bool>>
}

impl SoundNotifier {
    pub fn new() -> SoundNotifier {
        SoundNotifier {
            beep: Arc::new(Mutex::new(false)),
            notifier: Arc::new(Condvar::new())
        }
    }

    pub fn notify_one(&self) {
        if let Ok(mut val) = self.beep.lock() {
            *val = true;
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
                            if control.play_sound == true && last_sound.elapsed() >= Duration::from_millis(350) {
                                control.sound_board.play_sound(control.volume);
                                last_sound = Instant::now();
                            }
                            *beep = false;
                        }
                    }
                }
            } else {
                println!("Error waiting to play a sound.");
            }
        }
    }
}