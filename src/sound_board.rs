use std::{fs::File, io::{BufReader, Cursor}, ops::Deref, path::Path, sync::{Arc, Mutex}};

pub const EMILY_START:                  &'static [u8] = include_bytes!("sounds/emily-started.mp3");
pub const EMILY_SHUTDOWN:               &'static [u8] = include_bytes!("sounds/emily-shutdown.mp3");
pub const EMILY_VOLUME_01:              &'static [u8] = include_bytes!("sounds/emily-volume-01.mp3");
pub const EMILY_VOLUME_02:              &'static [u8] = include_bytes!("sounds/emily-volume-02.mp3");
pub const EMILY_VOLUME_03:              &'static [u8] = include_bytes!("sounds/emily-volume-03.mp3");
pub const EMILY_VOLUME_04:              &'static [u8] = include_bytes!("sounds/emily-volume-04.mp3");
pub const EMILY_VOLUME_05:              &'static [u8] = include_bytes!("sounds/emily-volume-05.mp3");
pub const EMILY_VOLUME_06:              &'static [u8] = include_bytes!("sounds/emily-volume-06.mp3");
pub const EMILY_VOLUME_07:              &'static [u8] = include_bytes!("sounds/emily-volume-07.mp3");
pub const EMILY_VOLUME_08:              &'static [u8] = include_bytes!("sounds/emily-volume-08.mp3");
pub const EMILY_VOLUME_09:              &'static [u8] = include_bytes!("sounds/emily-volume-09.mp3");
pub const EMILY_VOLUME_10:              &'static [u8] = include_bytes!("sounds/emily-volume-10.mp3");
pub const EMILY_INTRODUCTION:           &'static [u8] = include_bytes!("sounds/emily-introduction.mp3");
pub const EMILY_STARTUP_FINISHED:       &'static [u8] = include_bytes!("sounds/emily-startup-finished.mp3");
pub const EMILY_STARTUP_IN_PROGRESS:    &'static [u8] = include_bytes!("sounds/emily-startup-in-progress.mp3");
pub const EMILY_CUSTOM_NOT_AVAILABLE:   &'static [u8] = include_bytes!("sounds/emily-custom-not-available.mp3");

pub const MICHAEL_START:                &'static [u8] = include_bytes!("sounds/michael-started.mp3");
pub const MICHAEL_SHUTDOWN:             &'static [u8] = include_bytes!("sounds/michael-shutdown.mp3");
pub const MICHAEL_VOLUME_01:            &'static [u8] = include_bytes!("sounds/michael-volume-01.mp3");
pub const MICHAEL_VOLUME_02:            &'static [u8] = include_bytes!("sounds/michael-volume-02.mp3");
pub const MICHAEL_VOLUME_03:            &'static [u8] = include_bytes!("sounds/michael-volume-03.mp3");
pub const MICHAEL_VOLUME_04:            &'static [u8] = include_bytes!("sounds/michael-volume-04.mp3");
pub const MICHAEL_VOLUME_05:            &'static [u8] = include_bytes!("sounds/michael-volume-05.mp3");
pub const MICHAEL_VOLUME_06:            &'static [u8] = include_bytes!("sounds/michael-volume-06.mp3");
pub const MICHAEL_VOLUME_07:            &'static [u8] = include_bytes!("sounds/michael-volume-07.mp3");
pub const MICHAEL_VOLUME_08:            &'static [u8] = include_bytes!("sounds/michael-volume-08.mp3");
pub const MICHAEL_VOLUME_09:            &'static [u8] = include_bytes!("sounds/michael-volume-09.mp3");
pub const MICHAEL_VOLUME_10:            &'static [u8] = include_bytes!("sounds/michael-volume-10.mp3");
pub const MICHAEL_INTRODUCTION:         &'static [u8] = include_bytes!("sounds/michael-introduction.mp3");
pub const MICHAEL_STARTUP_FINISHED:     &'static [u8] = include_bytes!("sounds/michael-startup-finished.mp3");
pub const MICHAEL_STARTUP_IN_PROGRESS:  &'static [u8] = include_bytes!("sounds/michael-startup-in-progress.mp3");
pub const MICHAEL_CUSTOM_NOT_AVAILABLE: &'static [u8] = include_bytes!("sounds/michael-custom-not-available.mp3");

pub const CUSTOM_START:                 &'static str = "./sound/started.mp3";
pub const CUSTOM_SHUTDOWN:              &'static str = "./sound/shutdown.mp3";
pub const CUSTOM_VOLUME_01:             &'static str = "./sound/volume-01.mp3";
pub const CUSTOM_VOLUME_02:             &'static str = "./sound/volume-02.mp3";
pub const CUSTOM_VOLUME_03:             &'static str = "./sound/volume-03.mp3";
pub const CUSTOM_VOLUME_04:             &'static str = "./sound/volume-04.mp3";
pub const CUSTOM_VOLUME_05:             &'static str = "./sound/volume-05.mp3";
pub const CUSTOM_VOLUME_06:             &'static str = "./sound/volume-06.mp3";
pub const CUSTOM_VOLUME_07:             &'static str = "./sound/volume-07.mp3";
pub const CUSTOM_VOLUME_08:             &'static str = "./sound/volume-08.mp3";
pub const CUSTOM_VOLUME_09:             &'static str = "./sound/volume-09.mp3";
pub const CUSTOM_VOLUME_10:             &'static str = "./sound/volume-10.mp3";
pub const CUSTOM_INTRODUCTION:          &'static str = "./sound/introduction.mp3";
pub const CUSTOM_STARTUP_FINISHED:      &'static str = "./sound/startup-finished.mp3";
pub const CUSTOM_STARTUP_IN_PROGRESS:   &'static str = "./sound/startup-in-progress.mp3";

pub const BEEP_FREQUENCY: f32 = 1000.0;
pub const BEEP_DURATION: u64 = 150;
pub const BEEP_SEPARATION: u64 = 250;

trait SliceType: std::io::Read + std::io::Seek {}

#[derive(Clone)]
pub struct SoundBoard {
    current_voice: Arc<Mutex<Voice>>,
    custom_available: bool,
}

impl SoundBoard {
    pub fn new(voice: Voice) -> SoundBoard {
        return SoundBoard{
            custom_available:
                Path::new(CUSTOM_START).exists() &&
                Path::new(CUSTOM_SHUTDOWN).exists() &&
                Path::new(CUSTOM_VOLUME_01).exists() &&
                Path::new(CUSTOM_VOLUME_02).exists() &&
                Path::new(CUSTOM_VOLUME_03).exists() &&
                Path::new(CUSTOM_VOLUME_04).exists() &&
                Path::new(CUSTOM_VOLUME_05).exists() &&
                Path::new(CUSTOM_VOLUME_06).exists() &&
                Path::new(CUSTOM_VOLUME_07).exists() &&
                Path::new(CUSTOM_VOLUME_08).exists() &&
                Path::new(CUSTOM_VOLUME_09).exists() &&
                Path::new(CUSTOM_VOLUME_10).exists() &&
                Path::new(CUSTOM_INTRODUCTION).exists() &&
                Path::new(CUSTOM_STARTUP_FINISHED).exists() &&
                Path::new(CUSTOM_STARTUP_IN_PROGRESS).exists(),
            current_voice: Arc::new(Mutex::new(voice))
        }
    }

    pub fn custom_available(&self) -> bool {
        return self.custom_available;
    }

    pub fn change_voice(&mut self, new_voice: Voice) -> Result<(), ()> {
        if new_voice == Voice::Custom && !self.custom_available {
            println!("Custom voice is not available at this time.");
            return Err(())
        }
        if let Ok(mut voice) = self.current_voice.lock() {
            *voice = new_voice;
        }
        Ok(())
    }

    pub fn get_voice(&self) -> Voice {
        let mut output = Voice::Emily;
        if let Ok(voice) = self.current_voice.lock() {
            match voice.deref() {
                Voice::Emily => output = Voice::Emily,
                Voice::Michael => output = Voice::Michael,
                Voice::Custom => output = Voice::Custom,
            }
        }
        return output
    }

    pub fn play_started(&self, volume: f32) {
        if let Ok(voice) = self.current_voice.lock() {
            if let Ok((_source, source_handle)) = rodio::OutputStream::try_default() {
                if let Ok(sink) = rodio::Sink::try_new(&source_handle) {
                    sink.set_volume(volume);
                    match voice.deref() {
                        Voice::Emily => {
                            let slice = Cursor::new(EMILY_START);
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        Voice::Michael => {
                            let slice = Cursor::new(MICHAEL_START);
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        Voice::Custom => {
                            let slice = BufReader::new(File::open(CUSTOM_START).unwrap());
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                    }
                    sink.sleep_until_end();
                }
            }
        }
    }

    pub fn play_startup_finished(&self, volume: f32) {
        if let Ok(voice) = self.current_voice.lock() {
            if let Ok((_source, source_handle)) = rodio::OutputStream::try_default() {
                if let Ok(sink) = rodio::Sink::try_new(&source_handle) {
                    sink.set_volume(volume);
                    match voice.deref() {
                        Voice::Emily => {
                            let slice = Cursor::new(EMILY_STARTUP_FINISHED);
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        Voice::Michael => {
                            let slice = Cursor::new(MICHAEL_STARTUP_FINISHED);
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        Voice::Custom => {
                            let slice = BufReader::new(File::open(CUSTOM_STARTUP_FINISHED).unwrap());
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                    }
                    sink.sleep_until_end();
                }
            }
        }
    }

    pub fn play_shutdown(&self, volume: f32) {
        if let Ok(voice) = self.current_voice.lock() {
            if let Ok((_source, source_handle)) = rodio::OutputStream::try_default() {
                if let Ok(sink) = rodio::Sink::try_new(&source_handle) {
                    sink.set_volume(volume);
                    match voice.deref() {
                        Voice::Emily => {
                            let slice = Cursor::new(EMILY_SHUTDOWN);
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        Voice::Michael => {
                            let slice = Cursor::new(MICHAEL_SHUTDOWN);
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        Voice::Custom => {
                            let slice = BufReader::new(File::open(CUSTOM_SHUTDOWN).unwrap());
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                    }
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
                    match voice.deref() {
                        Voice::Emily => {
                            let slice = Cursor::new(EMILY_INTRODUCTION);
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        Voice::Michael => {
                            let slice = Cursor::new(MICHAEL_INTRODUCTION);
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        Voice::Custom => {
                            let slice = BufReader::new(File::open(CUSTOM_INTRODUCTION).unwrap());
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                    }
                    sink.sleep_until_end();
                }
            }
        }
    }

    pub fn play_custom_not_available(&self, volume: f32) {
        if let Ok(voice) = self.current_voice.lock() {
            if let Ok((_source, source_handle)) = rodio::OutputStream::try_default() {
                if let Ok(sink) = rodio::Sink::try_new(&source_handle) {
                    sink.set_volume(volume);
                    match voice.deref() {
                        Voice::Michael => {
                            let slice = Cursor::new(MICHAEL_CUSTOM_NOT_AVAILABLE);
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        _ => {
                            let slice = Cursor::new(EMILY_CUSTOM_NOT_AVAILABLE);
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                    }
                    sink.sleep_until_end();
                }
            }
        }
    }

    pub fn play_startup_in_progress(&self, volume: f32) {
        if let Ok(voice) = self.current_voice.lock() {
            if let Ok((_source, source_handle)) = rodio::OutputStream::try_default() {
                if let Ok(sink) = rodio::Sink::try_new(&source_handle) {
                    sink.set_volume(volume);
                    match voice.deref() {
                        Voice::Emily => {
                            let slice = Cursor::new(EMILY_STARTUP_IN_PROGRESS);
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        Voice::Michael => {
                            let slice = Cursor::new(MICHAEL_STARTUP_IN_PROGRESS);
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        Voice::Custom => {
                            let slice = BufReader::new(File::open(CUSTOM_STARTUP_IN_PROGRESS).unwrap());
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                    }
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
                    match voice.deref() {
                        Voice::Emily => {
                            let slice: Cursor<&[u8]>;
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
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        Voice::Michael => {
                            let slice: Cursor<&[u8]>;
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
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                        Voice::Custom => {
                            let slice: BufReader<File>;
                            match number {
                                1 => slice = BufReader::new(File::open(CUSTOM_VOLUME_01).unwrap()),
                                2 => slice = BufReader::new(File::open(CUSTOM_VOLUME_02).unwrap()),
                                3 => slice = BufReader::new(File::open(CUSTOM_VOLUME_03).unwrap()),
                                4 => slice = BufReader::new(File::open(CUSTOM_VOLUME_04).unwrap()),
                                5 => slice = BufReader::new(File::open(CUSTOM_VOLUME_05).unwrap()),
                                6 => slice = BufReader::new(File::open(CUSTOM_VOLUME_06).unwrap()),
                                7 => slice = BufReader::new(File::open(CUSTOM_VOLUME_07).unwrap()),
                                8 => slice = BufReader::new(File::open(CUSTOM_VOLUME_08).unwrap()),
                                9 => slice = BufReader::new(File::open(CUSTOM_VOLUME_09).unwrap()),
                                10 => slice = BufReader::new(File::open(CUSTOM_VOLUME_10).unwrap()),
                                _ => return
                            }
                            let source = rodio::Decoder::new(slice).unwrap();
                            sink.append(source);
                        }
                    }
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
                    let source = rodio::source::SineWave::new(BEEP_FREQUENCY);
                    sink.append(source);
                    std::thread::sleep(std::time::Duration::from_millis(BEEP_DURATION));
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Voice {
    Emily,
    Michael,
    Custom,
}

impl Voice {
    pub fn as_str(&self) -> &'static str {
        match self {
            Voice::Emily => "emily",
            Voice::Michael => "michael",
            Voice::Custom => "custom",
        }
    }

    pub fn from_str(voice: &str) -> Voice {
        match voice {
            "emily" => Voice::Emily,
            "michael" => Voice::Michael,
            "custom" => Voice::Custom,
            _ => Voice::Emily,
        }
    }
}