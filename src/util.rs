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