use chrono::{LocalResult, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use egui::{Color32, RichText};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::str;

pub fn ensure_utf8<P: AsRef<Path>>(path: P) -> Result<String, Box<dyn Error>> {
    let bytes = fs::read(&path)?;
    match String::from_utf8(bytes) {
        Ok(s) => Ok(s),
        Err(e) => {
            Err(format!("Input file '{}' is not valid UTF-8: {}", path.as_ref().display(), e)
                .into())
        },
    }
}

pub fn parse_datetime(s: &str, tz: Tz) -> Result<i64, Box<dyn Error>> {
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d %H:%M",
        "%d-%m-%Y %H:%M:%S",
        "%d/%m/%Y %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.fZ",
    ];

    for fmt in &formats {
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(s, fmt) {
            let dt_utc = match tz.from_local_datetime(&naive_dt) {
                LocalResult::Single(dt) => dt.with_timezone(&Utc),
                LocalResult::Ambiguous(dt1, _) => dt1.with_timezone(&Utc),
                LocalResult::None => {
                    eprintln!("Impossible local time {}. Fix or remove.", naive_dt);
                    continue;
                },
            };
            return Ok(dt_utc.timestamp());
        }
    }
    Err(format!("Unrecognized datetime format: {}", s).into())
}

pub fn good_message(msg: &str) -> RichText {
    RichText::new(msg).color(Color32::GREEN)
}

pub fn bad_message(msg: &str) -> RichText {
    RichText::new(msg).color(Color32::RED)
}

pub fn warn_message(msg: &str) -> RichText {
    RichText::new(msg).color(Color32::YELLOW)
}
