use chrono::prelude::DateTime;
use chrono::Utc;
use std::env;
use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};
mod stats;

use csv::StringRecord;

#[derive(Debug)]
struct DataFrame {
    header: StringRecord,
    datetime: Vec<DateTime<Utc>>,
    secs: Vec<u64>,
    fsecs: Vec<f64>,
    nsecs: Vec<u64>,
    gas: Vec<f64>,
    diag: Vec<u32>,
}

fn read_csv<P: AsRef<Path>>(filename: P) -> Result<DataFrame, Box<dyn Error>> {
    let file = File::open(filename)?;
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
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
        date.push(record[6].to_string());
        time.push(record[7].to_string());

        if let Ok(val) = record[10].parse::<f64>() {
            gas.push(val)
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
        if let Ok(val) = record[2].parse::<f64>() {
            fsecs.push(val)
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
    //let datetime: Vec<SystemTime> = secs
    //    .iter()
    //    .zip(nsecs.iter())
    //    .map(|(&sec, &nsec)| UNIX_EPOCH + Duration::from_secs(sec) + Duration::from_nanos(nsec))
    //    .collect();
    //let datetime = DateTime::<Utc>::from(d);
    //Formats the combined date and time with the specified format string.
    //let timestamp_str = datetime.format("%Y-%m-%d %H:%M:%S.%f").to_string();
    //let tm = secs[0] + (nsecs[0] / 1_000_000_000.);
    //println!("{}", tm);
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

fn main() {
    let args: Vec<String> = env::args().collect();

    let fpath: PathBuf = if let Some(val) = args.get(1) {
        PathBuf::from(val)
    } else {
        println!("Give path to file:");
        loop {
            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .expect("Failed to read line");

            let value = input.trim().to_string();
            if !value.is_empty() {
                break PathBuf::from(value);
            } else {
                println!("Give valid file path.")
            }
        }
    };

    println!("{:?}", &fpath);
    let df = match read_csv(&fpath) {
        Ok(res) => Some(res),
        Err(err) => {
            println!("Crashed with: {}, {:?}", err, &fpath);
            None
        }
    };

    if let Some(df) = &df {
        let s = df.fsecs.clone();
        let gas = df.gas.clone();
        let calcvec: Vec<(f64, f64)> = s.into_iter().zip(gas.into_iter()).collect();
        let lr = stats::LinReg::train(&calcvec);
        println!("{:?}", lr.slope);
        println!("{:?}", lr.intercept);

        let d = UNIX_EPOCH + Duration::from_secs(df.secs[0]) + Duration::from_nanos(df.nsecs[0]);
        // Create DateTime from SystemTime
        let datetime = DateTime::<Utc>::from(d);
        // Formats the combined date and time with the specified format string.
        let timestamp_str = datetime.format("%Y-%m-%d %H:%M:%S.%f").to_string();

        let r = stats::pearson_correlation(&df.fsecs, &df.gas);
        match r {
            None => println!("asd"),
            Some(val) => {
                println!("r: {}", val);
                println!("r2: {}", f64::powf(val, 2.));
            }
        }

        println!("{:?}", df.header);
        println!("{:?}", df.datetime[0]);
        println!("{:?}", df.secs[0]);
        println!("{:?}", df.gas[0]);
        println!("{:?}", df.diag[0]);
    }
}
