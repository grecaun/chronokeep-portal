use chrono::{Utc, LocalResult, TimeZone, Local};

#[cfg(test)]
pub mod test;

pub struct Time {
    pub seconds: u64,
    pub milliseconds: u32,
}

pub fn pretty_time_full(t: &Time) -> Result<String, &'static str> {
    if t.milliseconds > 1000 {
        return Err("invalid time specified: milliseconds too large")
    }
    Ok(format!("{}:{:02}:{:02}.{:03}", t.seconds / 3600, (t.seconds % 3600) / 60, t.seconds % 60, t.milliseconds))
}

pub fn pretty_time(seconds: &u64) -> String {
    if seconds > &3600 {
        return format!("{}:{:02}:{:02}", seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    }
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub fn utc_seconds_to_local_string(seconds: i64) -> Result<String, &'static str> {
    match Utc.timestamp_millis_opt(seconds * 1000) {
        LocalResult::Single(time) => Ok(Local.from_utc_datetime(&time.naive_utc()).format("%Y-%m-%d %H:%M:%S").to_string()),
        _ => Err("seconds not a valid timestamp")
    }
}

pub fn utc_seconds_to_string(seconds: i64) -> Result<String, &'static str> {
    match Utc.timestamp_millis_opt(seconds * 1000) {
        LocalResult::Single(time) => Ok(Utc.from_utc_datetime(&time.naive_utc()).format("%Y-%m-%d %H:%M:%S").to_string()),
        _ => Err("seconds not a valid timestamp")
    }
}

impl Time {
    pub fn time_since(&self, other: &Time) -> Result<Time, &'static str> {
        if other.milliseconds > self.milliseconds {
            if self.seconds < 1 || self.seconds - 1 < other.seconds {
                return Err("invalid time")
            }
            return Ok(Time {
                seconds: self.seconds - other.seconds - 1,
                milliseconds: 1000 + self.milliseconds - other.milliseconds
            });
        }
        if self.seconds < other.seconds {
            return Err("invalid time")
        }
        Ok(Time {
            seconds: self.seconds - other.seconds,
            milliseconds: self.milliseconds - other.milliseconds
        })
    }
}

pub fn play_sound(volume: f32) {
    let (_source, source_handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&source_handle).unwrap();
    sink.set_volume(volume);

    // this should be a beep
    let source = rodio::source::SineWave::new(800.0);
    sink.append(source);

    std::thread::sleep(std::time::Duration::from_millis(200));
}