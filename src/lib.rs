use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

mod csv_parse;
mod get_paths;
mod stats;

pub struct Config {
    pub gas_path: String,
    pub time_path: String,
}

impl Config {
    pub fn build(args: &[String]) -> Result<Config, &'static str> {
        let gas_path: String = match args.get(1) {
            Some(str) => str.clone(),
            None => String::new(), // returning empty string will prompt get paths to to ask
        };

        let time_path: String = match args.get(2) {
            Some(str) => str.clone(),
            None => String::new(), // returning empty string will prompt get paths to to ask
        };

        Ok(Config {
            gas_path,
            time_path,
        })
    }
}

pub fn run(config: Config) -> Result<(), Box<dyn Error>> {
    let str: &str = "gas";
    let gaspaths: Vec<PathBuf> = match get_paths::get_paths(config.gas_path, str) {
        Ok(vec) => vec,
        Err(e) => {
            println!("{}", e);
            return Err(e);
        }
    };
    let str: &str = "time";
    let timepaths: Vec<PathBuf> = match get_paths::get_paths(config.time_path, str) {
        Ok(vec) => vec,
        Err(e) => {
            println!("{}", e);
            return Err(e);
        }
    };
    let mut gasv: Vec<csv_parse::GasData> = Vec::new();
    for path in &gaspaths {
        match csv_parse::read_gas_csv(&path) {
            Ok(res) => {
                let r = stats::pearson_correlation(&res.fsecs, &res.gas).unwrap_or_else(|| {
                    println!("{:?}", &path);
                    0.0
                });
                println!("{:?}", &r);
                gasv.push(res);
            }
            Err(err) => {
                println!("Crashed with: {}, {:?}", err, &path);
                return Err(err);
            }
        };
    }
    for path in &timepaths {
        let times = match csv_parse::read_time_csv(&path) {
            Ok(res) => Some(res),
            Err(err) => {
                println!("Crashed with: {}, {:?}", err, &path);
                None
            }
        };
        if let Some(df) = times {
            println!("{:?}", df.start_time[0]);
        }
    }
    Ok(())
}
