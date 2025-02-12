use chrono::offset::LocalResult;
use chrono::prelude::DateTime;
use chrono::{NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Helsinki;

use csv::StringRecord;
use std::error::Error;
use std::fs::File;
use std::path::Path;
use std::process;
use std::time::{Duration, UNIX_EPOCH};

const ERROR_INT: i64 = -9999;
const ERROR_FLOAT: f64 = -9999.;

pub trait EqualLen {
    fn validate_lengths(&self) -> bool;
}

#[derive(Debug)]
pub struct GasData {
    pub header: StringRecord,
    pub datetime: Vec<DateTime<Utc>>,
    pub secs: Vec<i64>,
    pub fsecs: Vec<f64>,
    pub nsecs: Vec<i64>,
    pub gas: Vec<f64>,
    pub diag: Vec<i64>,
}

impl EqualLen for GasData {
    fn validate_lengths(&self) -> bool {
        // check that all fields are equal length
        let lengths = [
            &self.header.len(),
            &self.datetime.len(),
            &self.secs.len(),
            &self.fsecs.len(),
            &self.nsecs.len(),
            &self.gas.len(),
            &self.diag.len(),
        ];
        let mut check: bool = true;

        for vec_len in lengths.iter() {
            let len = vec_len;
            if vec_len != len {
                check = false;
                break;
            } else {
                continue;
            };
        }
        check
    }
}

impl GasData {
    pub fn all_invalid(&self) -> bool {
        let invalids = [
            &self.secs.iter().all(|&x| x == ERROR_INT),
            &self.fsecs.iter().all(|&x| x == ERROR_FLOAT),
            &self.nsecs.iter().all(|&x| x == ERROR_INT),
            &self.gas.iter().all(|&x| x == ERROR_FLOAT),
            &self.diag.iter().all(|&x| x == ERROR_INT),
        ];
        invalids.iter().any(|&x| *x == true)
    }
}

#[derive(Debug)]
pub struct TimeData {
    pub chamber_id: Vec<String>,
    pub start_time: Vec<DateTime<Utc>>,
    pub close_offset: Vec<u64>,
    pub open_offset: Vec<u64>,
    pub end_offset: Vec<u64>,
}

impl EqualLen for TimeData {
    fn validate_lengths(&self) -> bool {
        let lengths = [
            &self.chamber_id.len(),
            &self.start_time.len(),
            &self.close_offset.len(),
            &self.open_offset.len(),
            &self.end_offset.len(),
        ];
        let mut check: bool = true;

        for vec_len in lengths.iter() {
            let len = vec_len;
            if vec_len != len {
                check = false;
                break;
            } else {
                continue;
            };
        }
        check
    }
}

impl TimeData {}

pub fn mk_rdr<P: AsRef<Path>>(filename: P) -> Result<csv::Reader<File>, Box<dyn Error>> {
    let file = File::open(filename)?;
    let rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .flexible(true)
        .from_reader(file);
    Ok(rdr)
}

pub fn read_gas_csv<P: AsRef<Path>>(filename: P) -> Result<GasData, Box<dyn Error>> {
    let mut rdr = mk_rdr(filename)?;
    let skip = 4;

    for _ in 0..skip {
        rdr.records().next();
    }

    let mut gas: Vec<f64> = Vec::new();
    let mut diag: Vec<i64> = Vec::new();
    let mut date: Vec<String> = Vec::new();
    let mut time: Vec<String> = Vec::new();
    //let mut ntime: Vec<f64> = Vec::new();
    let mut fsecs: Vec<f64> = Vec::new();
    let mut secs: Vec<i64> = Vec::new();
    let mut nsecs: Vec<i64> = Vec::new();
    let mut header = csv::StringRecord::new();

    for (i, r) in rdr.records().enumerate() {
        let record: &csv::StringRecord = &r?;
        if i == 0 {
            header = record.clone();
            continue;
        }
        if i == 1 {
            continue;
        }
        date.push(record[6].to_string());
        time.push(record[7].to_string());

        if let Ok(val) = record[10].parse::<f64>() {
            gas.push(val)
        } else {
            gas.push(ERROR_FLOAT)
        }
        if let Ok(val) = record[4].parse::<i64>() {
            diag.push(val)
        }

        if let Ok(val) = record[1].parse::<i64>() {
            secs.push(val)
        }
        if let Ok(val) = record[2].parse::<i64>() {
            nsecs.push(val)
        }
        if let Ok(val) = record[1].parse::<f64>() {
            fsecs.push(val)
        } else {
            println!("{}", &record[1]);
            fsecs.push(ERROR_FLOAT)
        }
    }

    let datetime: Vec<DateTime<Utc>> = secs
        .iter()
        .zip(nsecs.iter())
        .map(|(&sec, &nsec)| {
            let d =
                UNIX_EPOCH + Duration::from_secs(sec as u64) + Duration::from_nanos(nsec as u64);
            DateTime::<Utc>::from(d) // Convert to DateTime<Utc>
        })
        .collect();

    let df = GasData {
        header,
        datetime,
        secs,
        fsecs,
        nsecs,
        gas,
        diag,
    };
    Ok(df)
}

pub fn read_time_csv<P: AsRef<Path>>(filename: P) -> Result<TimeData, Box<dyn Error>> {
    let file = File::open(filename)?;
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(file);

    // chamber_id,start_time,close_offset,open_offset,end_offset
    let mut chamber_id: Vec<String> = Vec::new();
    let mut start_time: Vec<DateTime<Utc>> = Vec::new();
    let mut close_offset: Vec<u64> = Vec::new();
    let mut open_offset: Vec<u64> = Vec::new();
    let mut end_offset: Vec<u64> = Vec::new();

    for r in rdr.records() {
        let record: &csv::StringRecord = &r?;
        chamber_id.push(record[0].to_owned());

        match NaiveDateTime::parse_from_str(&record[1], "%Y-%m-%d %H:%M:%S") {
            Ok(naive_dt) => {
                let dt_utc = match Helsinki.from_local_datetime(&naive_dt) {
                    LocalResult::Single(dt) => dt.with_timezone(&Utc),
                    LocalResult::Ambiguous(dt1, _) => dt1.with_timezone(&Utc),
                    LocalResult::None => {
                        eprintln!("Impossible local time {}\nFix or remove.", naive_dt);
                        process::exit(1)
                    }
                };
                start_time.push(dt_utc)
            }
            Err(e) => println!("Failed to parse timestamp: {}", e),
        }
        if let Ok(val) = record[2].parse::<u64>() {
            close_offset.push(val)
        }
        if let Ok(val) = record[3].parse::<u64>() {
            open_offset.push(val)
        }
        if let Ok(val) = record[4].parse::<u64>() {
            end_offset.push(val)
        }
    }
    let df = TimeData {
        chamber_id,
        start_time,
        close_offset,
        open_offset,
        end_offset,
    };
    Ok(df)
}
