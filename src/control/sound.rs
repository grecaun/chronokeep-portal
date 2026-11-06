/*
Chronokeep Portal - RFID Tag Reading Software
Copyright (C) 2026 James Sentinella

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{collections::HashMap, sync::{Arc, Condvar, Mutex}, time::{Duration, Instant, SystemTime}};

use crate::{reader::zebra::TagData, sound_board};

pub struct Sounds {
    control: Arc<Mutex<super::Control>>,
    keepalive: Arc<Mutex<bool>>,
    sound_notifier: Arc<SoundNotifier>
}

/// Sounds waiting to be played.
#[derive(Default)]
struct Pending {
    beep: bool,
    sounds: Vec<SoundType>,
}

/// Tells the sound thread what to play. Requests are kept under the same lock
/// the sound thread waits on, so one made while a sound is playing is never missed.
pub struct SoundNotifier {
    pending: Mutex<Pending>,
    wake: Condvar,
    tag_list: Mutex<HashMap<u128, u64>>,
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
            pending: Mutex::new(Pending::default()),
            wake: Condvar::new(),
            tag_list: Mutex::new(HashMap::new()),
        }
    }

    pub fn notify_tags(&self, tags: &Vec<TagData>, ignore: u16) {
        let mut beep = false;
        if let Ok(cur_dur) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            let cur_time = cur_dur.as_secs(); // current time as unix timestamp
            if let Ok(mut t_list) = self.tag_list.lock() {
                for tag in tags {
                    // Play a sound if we haven't seen the tag ever, or not within the beep interval.
                    if !t_list.contains_key(&tag.tag()) || (t_list.get(&tag.tag()).unwrap() + (ignore as u64)) < cur_time {
                        beep = true;
                        // Update the time we've played a sound.
                        t_list.insert(tag.tag(), cur_time);
                    }
                }
            }
        }
        if beep {
            self.notify_one();
        }
    }

    pub fn notify_one(&self) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.beep = true;
        }
        self.wake.notify_all();
    }

    pub fn notify_custom(&self, sound: SoundType) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.sounds.push(sound);
        }
        self.wake.notify_all();
    }

    /// Waits until there's something to play and takes it (no polling).
    fn take(&self) -> Option<Pending> {
        let guard = self.pending.lock().ok()?;
        let mut pending = self.wake.wait_while(guard, |p| !p.beep && p.sounds.is_empty()).ok()?;
        Some(std::mem::take(&mut *pending))
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
            sound_notifier: Arc::new(SoundNotifier::new())
        }
    }

    pub fn get_notifier(&self) -> Arc<SoundNotifier> {
        self.sound_notifier.clone()
    }

    pub fn run(&mut self) {
        let mut last_sound = Instant::now();
        let mut separation: u64 = sound_board::BEEP_SEPARATION + rand::random_range(0..sound_board::BEEP_SEP_RAND_MAX);
        loop {
            if let Ok(ka) = self.keepalive.try_lock() {
                if *ka == false {
                    break
                }
            }
            let Some(pending) = self.sound_notifier.take() else {
                println!("Error waiting to play a sound.");
                break;
            };
            // Copy what's needed and let go of control before playing: each sound
            // plays to the end, and readers need control to process tags.
            let (play_sound, volume, board) = match self.control.lock() {
                Ok(control) => (control.play_sound, control.volume, control.sound_board.clone()),
                Err(_) => continue,
            };
            if !play_sound {
                continue;
            }
            // Beeps closer together than the separation are skipped.
            if pending.beep && last_sound.elapsed() >= Duration::from_millis(separation) {
                board.play_sound(volume);
                last_sound = Instant::now();
                separation = sound_board::BEEP_SEPARATION + rand::random_range(0..sound_board::BEEP_SEP_RAND_MAX);
            }
            for sound in pending.sounds {
                match sound {
                    SoundType::Volume => board.play_volume(volume),
                    SoundType::Introduction => board.play_introduction(volume),
                    SoundType::StartupFinished => board.play_startup_finished(volume),
                    SoundType::StartupInProgress => board.play_startup_in_progress(volume),
                    SoundType::CustomNotAvailable => board.play_custom_not_available(volume),
                    SoundType::Connected => board.play_connected(volume),
                    SoundType::Disconnected => board.play_disconnected(volume),
                    SoundType::Malfunction => board.play_malfunction(volume),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
