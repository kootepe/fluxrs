use glob::glob;
use std::env;
use std::io;

use std::path::{Path, PathBuf};
mod csv_parse;
mod get_paths;
mod stats;

fn main() {
    let args: Vec<String> = env::args().collect();

    let gas_pat: &str = match args.get(1) {
        Some(str) => str,
        None => "", // returning empty string will prompt get paths to to ask
    };

    let time_pat: &str = match args.get(2) {
        Some(str) => str,
        None => "", // returning empty string will prompt get paths to to ask
    };

    let str: &str = "gas";
    let gaspaths: Vec<PathBuf> = match get_paths::get_paths(gas_pat, str) {
        Ok(vec) => vec,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };
    let str: &str = "time";
    let timepaths: Vec<PathBuf> = match get_paths::get_paths(time_pat, str) {
        Ok(vec) => vec,
        Err(e) => {
            println!("{}", e);
            return;
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
                return;
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
}
