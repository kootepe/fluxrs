use crate::meteodata::MeteoData;
use crate::timedata::TimeData;
use crate::volumedata::VolumeData;
use chrono::offset::LocalResult;
use chrono::prelude::DateTime;
use chrono::{NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Helsinki;

use std::error::Error;
use std::fs::File;
use std::path::Path;
use std::process;

pub fn mk_rdr<P: AsRef<Path>>(filename: P) -> Result<csv::Reader<File>, Box<dyn Error>> {
    let file = File::open(filename)?;
    let rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .flexible(true)
        .from_reader(file);
    Ok(rdr)
}

pub fn parse_secnsec_to_dt(sec: i64, nsec: i64) -> DateTime<Utc> {
    match Helsinki.timestamp_opt(sec, nsec as u32) {
        LocalResult::Single(dt) => return dt.with_timezone(&Utc),
        LocalResult::Ambiguous(dt1, _) => return dt1.with_timezone(&Utc),
        LocalResult::None => {
            eprintln!("Impossible local time: sec={} nsec={}", sec, nsec);
        },
    };

    // Default fallback timestamp if parsing fails
    Utc.timestamp_opt(0, 0).single().unwrap() // Returns Unix epoch (1970-01-01 00:00:00 UTC)
}

// pub fn read_gas_csv<P: AsRef<Path>>(filename: P) -> Result<GasData, Box<dyn Error>> {
//     let mut rdr = mk_rdr(filename)?;
//     let skip = 4;
//
//     for _ in 0..skip {
//         rdr.records().next();
//     }
//
//     let mut ch4: Vec<f64> = Vec::new();
//     let mut co2: Vec<f64> = Vec::new();
//     let mut diag: Vec<i64> = Vec::new();
//     let mut datetime: Vec<DateTime<Utc>> = Vec::new();
//     let mut secs: Vec<i64> = Vec::new();
//     let mut nsecs: Vec<i64> = Vec::new();
//     let mut header = csv::StringRecord::new();
//
//     if let Some(result) = rdr.records().next() {
//         header = result?;
//     }
//     let ch4_col = "CH4";
//     let co2_col = "CO2";
//     let diag_col = "DIAG";
//     let secs_col = "SECONDS";
//     let nsecs_col = "NANOSECONDS";
//
//     let idx_ch4 = header
//         .iter()
//         .position(|h| h == ch4_col)
//         .ok_or("Column not found")?;
//     let idx_co2 = header
//         .iter()
//         .position(|h| h == co2_col)
//         .ok_or("Column not found")?;
//     let idx_diag = header
//         .iter()
//         .position(|h| h == diag_col)
//         .ok_or("Column not found")?;
//     let idx_secs = header
//         .iter()
//         .position(|h| h == secs_col)
//         .ok_or("Column not found")?;
//     let idx_nsecs = header
//         .iter()
//         .position(|h| h == nsecs_col)
//         .ok_or("Column not found")?;
//     for (i, r) in rdr.records().enumerate() {
//         let record: &csv::StringRecord = &r?;
//         if i == 0 {
//             header = record.clone();
//             continue;
//         }
//         if i == 1 {
//             continue;
//         }
//
//         if let Ok(val) = record[idx_ch4].parse::<f64>() {
//             ch4.push(val)
//         } else {
//             ch4.push(structs::ERROR_FLOAT)
//         }
//         if let Ok(val) = record[idx_co2].parse::<f64>() {
//             co2.push(val)
//         } else {
//             co2.push(structs::ERROR_FLOAT)
//         }
//         if let Ok(val) = record[idx_diag].parse::<i64>() {
//             diag.push(val)
//         }
//         let sec = record[idx_secs].parse::<i64>()?;
//         let nsec = record[idx_nsecs].parse::<i64>()?;
//         let dt_utc = parse_secnsec_to_dt(sec, nsec);
//         datetime.push(dt_utc);
//
//         if let Ok(val) = record[idx_secs].parse::<i64>() {
//             secs.push(val)
//         }
//         if let Ok(val) = record[idx_nsecs].parse::<i64>() {
//             nsecs.push(val)
//         }
//     }
//     let mut indices: Vec<usize> = (0..datetime.len()).collect();
//     indices.sort_by(|&i, &j| datetime[i].cmp(&datetime[j]));
//
//     let datetime: Vec<chrono::DateTime<Utc>> = indices.iter().map(|&i| datetime[i]).collect();
//     let gas: Vec<f64> = indices.iter().map(|&i| ch4[i]).collect();
//     let diag: Vec<i64> = indices.iter().map(|&i| diag[i]).collect();
//
//     let df = GasData {
//         header,
//         datetime,
//         gas,
//         diag,
//     };
//     Ok(df)
// }
pub fn read_meteo_csv<P: AsRef<Path>>(file_path: P) -> Result<MeteoData, Box<dyn Error>> {
    let file = File::open(file_path)?;

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true) // ✅ Ensure headers are read
        .from_reader(file);

    let mut datetime = Vec::new();
    let mut temperature = Vec::new();
    let mut pressure = Vec::new();

    for result in rdr.records() {
        let record = result?;

        let datetime_str = &record[0]; // Read datetime column
        let temp: f64 = record[1].parse()?; // Read air_temperature column
        let press: f64 = record[2].parse()?; // Read air_pressure column

        // Convert datetime string to Unix timestamp
        let dt = NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S")?;
        let timestamp = Utc.from_utc_datetime(&dt).timestamp();

        // Store values
        datetime.push(timestamp);
        temperature.push(temp);
        pressure.push(press);
    }

    Ok(MeteoData { datetime, temperature, pressure })
}
pub fn read_volume_csv<P: AsRef<Path>>(file_path: P) -> Result<VolumeData, Box<dyn Error>> {
    let file = File::open(file_path)?;

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true) // ✅ Ensure headers are read
        .from_reader(file);

    let mut datetime = Vec::new();
    let mut chamber_id = Vec::new();
    let mut volume = Vec::new();

    for result in rdr.records() {
        let record = result?;

        let datetime_str = &record[0]; // Read datetime column
        let ch = &record[1]; // Read datetime column
        let vol: f64 = record[2].parse()?; // Read air_pressure column

        // Convert datetime string to Unix timestamp
        let dt = NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S")?;
        let timestamp = Utc.from_utc_datetime(&dt).timestamp();

        // Store values
        datetime.push(timestamp);
        chamber_id.push(ch.to_owned());
        volume.push(vol);
    }

    Ok(VolumeData { datetime, chamber_id, volume })
}
pub fn read_time_csv<P: AsRef<Path>>(filename: P) -> Result<TimeData, Box<dyn Error>> {
    let file = File::open(filename)?;
    let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(file);

    // chamber_id,start_time,close_offset,open_offset,end_offset
    let mut chamber_id: Vec<String> = Vec::new();
    let mut start_time: Vec<DateTime<Utc>> = Vec::new();
    let mut close_offset: Vec<i64> = Vec::new();
    let mut open_offset: Vec<i64> = Vec::new();
    let mut end_offset: Vec<i64> = Vec::new();
    let mut project: Vec<String> = Vec::new();

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
                    },
                };
                start_time.push(dt_utc)
            },
            Err(e) => println!("Failed to parse timestamp: {}", e),
        }
        if let Ok(val) = record[2].parse::<i64>() {
            close_offset.push(val)
        }
        if let Ok(val) = record[3].parse::<i64>() {
            open_offset.push(val)
        }
        if let Ok(val) = record[4].parse::<i64>() {
            end_offset.push(val)
        }
    }
    let df = TimeData { chamber_id, start_time, close_offset, open_offset, end_offset, project };
    Ok(df)
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn test_any_col_invalid() {
//         let valid_data = GasData {
//             header: csv::StringRecord::new(),
//             datetime: vec![Utc::now(), Utc::now(), Utc::now()],
//             gas: vec![1.0, 2.0, 3.0],
//             diag: vec![1, 2, 3],
//         };
//         assert!(
//             !valid_data.any_col_invalid(),
//             "Expected valid data to return false"
//         )
//     }
//     #[test]
//     fn invalid_multiple() {
//         let invalid_multiple = GasData {
//             header: csv::StringRecord::new(),
//             datetime: vec![Utc::now(), Utc::now(), Utc::now()],
//             gas: vec![structs::ERROR_FLOAT; 3],
//             diag: vec![structs::ERROR_INT; 3],
//         };
//         assert!(
//             invalid_multiple.any_col_invalid(),
//             "Expected multiple invalid columns to return true"
//         )
//     }
// }
