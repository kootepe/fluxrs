use chrono::prelude::DateTime;
use chrono::Utc;
use csv::StringRecord;
use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct DataFrame {
    pub header: StringRecord,
    pub datetime: Vec<DateTime<Utc>>,
    pub secs: Vec<u64>,
    pub fsecs: Vec<f64>,
    pub nsecs: Vec<u64>,
    pub gas: Vec<f64>,
    pub diag: Vec<u32>,
}

pub fn read_csv<P: AsRef<Path>>(filename: P) -> Result<DataFrame, Box<dyn Error>> {
    let file = File::open(filename)?;
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(false)
        .flexible(true)
        .from_reader(file);

    let skip = 4;

    for _ in 0..skip {
        rdr.records().next();
    }

    let mut gas: Vec<f64> = Vec::new();
    let mut diag: Vec<u32> = Vec::new();
    let mut date: Vec<String> = Vec::new();
    let mut time: Vec<String> = Vec::new();
    //let mut ntime: Vec<f64> = Vec::new();
    let mut fsecs: Vec<f64> = Vec::new();
    let mut secs: Vec<u64> = Vec::new();
    let mut nsecs: Vec<u64> = Vec::new();
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
        // date.push(record[6].to_string());
        // time.push(record[7].to_string());

        if let Ok(val) = record[10].parse::<f64>() {
            gas.push(val)
        } else {
            gas.push(9999.)
        }
        if let Ok(val) = record[4].parse::<u32>() {
            diag.push(val)
        }

        if let Ok(val) = record[1].parse::<u64>() {
            secs.push(val)
        }
        if let Ok(val) = record[2].parse::<u64>() {
            nsecs.push(val)
        }
        if let Ok(val) = record[1].parse::<f64>() {
            fsecs.push(val)
        } else {
            fsecs.push(9999.)
        }
    }

    let datetime: Vec<DateTime<Utc>> = secs
        .iter()
        .zip(nsecs.iter())
        .map(|(&sec, &nsec)| {
            let d = UNIX_EPOCH + Duration::from_secs(sec) + Duration::from_nanos(nsec);
            DateTime::<Utc>::from(d) // Convert to DateTime<Utc>
        })
        .collect();

    let df = DataFrame {
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
