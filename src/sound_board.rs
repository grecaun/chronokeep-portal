use std::{io::Cursor, ops::Deref, sync::{Arc, Mutex}};

pub const EMILY_START: &'static [u8] = include_bytes!("sounds/emily-portal-started.mp3");
pub const EMILY_SHUTDOWN: &'static [u8] = include_bytes!("sounds/emily-portal-shutdown.mp3");
pub const EMILY_AC_FINISHED: &'static [u8] = include_bytes!("sounds/emily-startup-finished.mp3");
pub const EMILY_INTRODUCTION: &'static [u8] = include_bytes!("sounds/emily-introduction.mp3");
pub const EMILY_VOLUME_01: &'static [u8] = include_bytes!("sounds/emily-volume-01.mp3");
pub const EMILY_VOLUME_02: &'static [u8] = include_bytes!("sounds/emily-volume-02.mp3");
pub const EMILY_VOLUME_03: &'static [u8] = include_bytes!("sounds/emily-volume-03.mp3");
pub const EMILY_VOLUME_04: &'static [u8] = include_bytes!("sounds/emily-volume-04.mp3");
pub const EMILY_VOLUME_05: &'static [u8] = include_bytes!("sounds/emily-volume-05.mp3");
pub const EMILY_VOLUME_06: &'static [u8] = include_bytes!("sounds/emily-volume-06.mp3");
pub const EMILY_VOLUME_07: &'static [u8] = include_bytes!("sounds/emily-volume-07.mp3");
pub const EMILY_VOLUME_08: &'static [u8] = include_bytes!("sounds/emily-volume-08.mp3");
pub const EMILY_VOLUME_09: &'static [u8] = include_bytes!("sounds/emily-volume-09.mp3");
pub const EMILY_VOLUME_10: &'static [u8] = include_bytes!("sounds/emily-volume-10.mp3");
pub const MICHAEL_START: &'static [u8] = include_bytes!("sounds/michael-portal-started.mp3");
pub const MICHAEL_SHUTDOWN: &'static [u8] = include_bytes!("sounds/michael-portal-shutdown.mp3");
pub const MICHAEL_AC_FINISHED: &'static [u8] = include_bytes!("sounds/michael-startup-finished.mp3");
pub const MICHAEL_INTRODUCTION: &'static [u8] = include_bytes!("sounds/michael-introduction.mp3");
pub const MICHAEL_VOLUME_01: &'static [u8] = include_bytes!("sounds/michael-volume-01.mp3");
pub const MICHAEL_VOLUME_02: &'static [u8] = include_bytes!("sounds/michael-volume-02.mp3");
pub const MICHAEL_VOLUME_03: &'static [u8] = include_bytes!("sounds/michael-volume-03.mp3");
pub const MICHAEL_VOLUME_04: &'static [u8] = include_bytes!("sounds/michael-volume-04.mp3");
pub const MICHAEL_VOLUME_05: &'static [u8] = include_bytes!("sounds/michael-volume-05.mp3");
pub const MICHAEL_VOLUME_06: &'static [u8] = include_bytes!("sounds/michael-volume-06.mp3");
pub const MICHAEL_VOLUME_07: &'static [u8] = include_bytes!("sounds/michael-volume-07.mp3");
pub const MICHAEL_VOLUME_08: &'static [u8] = include_bytes!("sounds/michael-volume-08.mp3");
pub const MICHAEL_VOLUME_09: &'static [u8] = include_bytes!("sounds/michael-volume-09.mp3");
pub const MICHAEL_VOLUME_10: &'static [u8] = include_bytes!("sounds/michael-volume-10.mp3");

#[derive(Clone)]
pub struct SoundBoard {
    current_voice: Arc<Mutex<Voice>>,
}

impl SoundBoard {
    pub fn new(voice: Voice) -> SoundBoard {
        return SoundBoard{
            current_voice: Arc::new(Mutex::new(voice))
        }
    }

    pub fn change_voice(&mut self, new_voice: Voice) {
        if let Ok(mut voice) = self.current_voice.lock() {
            *voice = new_voice;
        }
    }

    pub fn get_voice(&self) -> Voice {
        let mut output = Voice::Emily;
        if let Ok(voice) = self.current_voice.lock() {
            match voice.deref() {
                Voice::Emily => output = Voice::Emily,
                Voice::Michael => output = Voice::Michael,
            }
        }
        return output
    }

    pub fn play_start_sound(&self, volume: f32) {
        if let Ok(voice) = self.current_voice.lock() {
            if let Ok((_source, source_handle)) = rodio::OutputStream::try_default() {
                if let Ok(sink) = rodio::Sink::try_new(&source_handle) {
                    sink.set_volume(volume);
                    let slice: Cursor<&[u8]>;
                    match voice.deref() {
                        Voice::Emily => {
                            slice = Cursor::new(EMILY_START);
                        }
                        Voice::Michael => {
                            slice = Cursor::new(MICHAEL_START);
                        }
                    }
                    let source = rodio::Decoder::new(slice).unwrap();
                    sink.append(source);
                    sink.sleep_until_end();
                }
            }
        }
    }

    pub fn play_auto_connected_sound(&self, volume: f32) {
        if let Ok(voice) = self.current_voice.lock() {
            if let Ok((_source, source_handle)) = rodio::OutputStream::try_default() {
                if let Ok(sink) = rodio::Sink::try_new(&source_handle) {
                    sink.set_volume(volume);
                    let slice: Cursor<&[u8]>;
                    match voice.deref() {
                        Voice::Emily => {
                            slice = Cursor::new(EMILY_AC_FINISHED);
                        }
                        Voice::Michael => {
                            slice = Cursor::new(MICHAEL_AC_FINISHED);
                        }
                    }
                    let source = rodio::Decoder::new(slice).unwrap();
                    sink.append(source);
                    sink.sleep_until_end();
                }
            }
        }
    }

    pub fn play_close_sound(&self, volume: f32) {
        if let Ok(voice) = self.current_voice.lock() {
            if let Ok((_source, source_handle)) = rodio::OutputStream::try_default() {
                if let Ok(sink) = rodio::Sink::try_new(&source_handle) {
                    sink.set_volume(volume);
                    let slice: Cursor<&[u8]>;
                    match voice.deref() {
                        Voice::Emily => {
                            slice = Cursor::new(EMILY_SHUTDOWN);
                        }
                        Voice::Michael => {
                            slice = Cursor::new(MICHAEL_SHUTDOWN);
                        }
                    }
                    let source = rodio::Decoder::new(slice).unwrap();
                    sink.append(source);
                    sink.sleep_until_end();
                }
            }
        }
    }

    pub fn play_introduction(&self, volume: f32) {
        if let Ok(voice) = self.current_voice.lock() {
            if let Ok((_source, source_handle)) = rodio::OutputStream::try_default() {
                if let Ok(sink) = rodio::Sink::try_new(&source_handle) {
                    sink.set_volume(volume);
                    let slice: Cursor<&[u8]>;
                    match voice.deref() {
                        Voice::Emily => {
                            slice = Cursor::new(EMILY_INTRODUCTION);
                        }
                        Voice::Michael => {
                            slice = Cursor::new(MICHAEL_INTRODUCTION);
                        }
                    }
                    let source = rodio::Decoder::new(slice).unwrap();
                    sink.append(source);
                    sink.sleep_until_end();
                }
            }
        }
    }

    pub fn play_volume(&self, volume: f32) {
        let number: i32 = (volume * 10.0).trunc() as i32;
        if let Ok(voice) = self.current_voice.lock() {
            if let Ok((_source, source_handle)) = rodio::OutputStream::try_default() {
                if let Ok(sink) = rodio::Sink::try_new(&source_handle) {
                    sink.set_volume(volume);
                    let slice: Cursor<&[u8]>;
                    match voice.deref() {
                        Voice::Emily => {
                            match number {
                                1 => slice = Cursor::new(EMILY_VOLUME_01),
                                2 => slice = Cursor::new(EMILY_VOLUME_02),
                                3 => slice = Cursor::new(EMILY_VOLUME_03),
                                4 => slice = Cursor::new(EMILY_VOLUME_04),
                                5 => slice = Cursor::new(EMILY_VOLUME_05),
                                6 => slice = Cursor::new(EMILY_VOLUME_06),
                                7 => slice = Cursor::new(EMILY_VOLUME_07),
                                8 => slice = Cursor::new(EMILY_VOLUME_08),
                                9 => slice = Cursor::new(EMILY_VOLUME_09),
                                10 => slice = Cursor::new(EMILY_VOLUME_10),
                                _ => return
                            }
                        }
                        Voice::Michael => {
                            match number {
                                1 => slice = Cursor::new(MICHAEL_VOLUME_01),
                                2 => slice = Cursor::new(MICHAEL_VOLUME_02),
                                3 => slice = Cursor::new(MICHAEL_VOLUME_03),
                                4 => slice = Cursor::new(MICHAEL_VOLUME_04),
                                5 => slice = Cursor::new(MICHAEL_VOLUME_05),
                                6 => slice = Cursor::new(MICHAEL_VOLUME_06),
                                7 => slice = Cursor::new(MICHAEL_VOLUME_07),
                                8 => slice = Cursor::new(MICHAEL_VOLUME_08),
                                9 => slice = Cursor::new(MICHAEL_VOLUME_09),
                                10 => slice = Cursor::new(MICHAEL_VOLUME_10),
                                _ => return
                            }
                        }
                    }
                    let source = rodio::Decoder::new(slice).unwrap();
                    sink.append(source);
                    sink.sleep_until_end();
                }
            }
        }
    }

    pub fn play_sound(&self, volume: f32) {
        if let Ok(_voice) = self.current_voice.lock() {
            if let Ok((_source, source_handle)) = rodio::OutputStream::try_default() {
                if let Ok(sink) = rodio::Sink::try_new(&source_handle) {
                    sink.set_volume(volume);
                
                    // this should be a beep
                    let source = rodio::source::SineWave::new(800.0);
                    sink.append(source);
                    std::thread::sleep(std::time::Duration::from_millis(150));
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum Voice {
    Emily,
    Michael
}

impl Voice {
    pub fn as_str(&self) -> &'static str {
        match self {
            Voice::Emily => "emily",
            Voice::Michael => "michael"
        }
    }

    pub fn from_str(voice: &str) -> Voice {
        match voice {
            "emily" => Voice::Emily,
            "michael" => Voice::Michael,
            _ => Voice::Emily
        }
    }
}