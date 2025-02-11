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

            Ok(res) => Some(res),
            Err(err) => {
                println!("Crashed with: {}, {:?}", err, &path);
                None
            }
        };
        if let Some(df) = df {
            // println!("{:?}", &path);
            // let r = stats::pearson_correlation(&df.fsecs, &df.gas);
            // let s = df.fsecs.clone();
            // let g = df.gas.clone();
            // let calcvec: Vec<(f64, f64)> = s.into_iter().zip(g.into_iter()).collect();
            let r = stats::pearson_correlation(&df.fsecs, &df.gas).unwrap_or_else(|| {
                // let r = stats::pearson_correlation(&calcvec).unwrap_or_else(|| {
                println!("{:?}", &path);
                // println!("{:?}", &df.secs);
                // println!("{:?}", &df.gas);
                // panic!("Whats wrong?");
                0.0
            });
            // dfvec.push(df);
            println!("{:?}", r);
        }
    }
}
