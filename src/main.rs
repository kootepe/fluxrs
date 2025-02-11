use glob::glob;
use std::env;
use std::io;

use std::path::{Path, PathBuf};
mod csv_parse;
mod stats;

fn main() {
    let args: Vec<String> = env::args().collect();

    let fpaths: Vec<PathBuf> = if let Some(val) = args.get(1) {
        glob(val)
            .expect("Failed to read glob pattern")
            .filter_map(Result::ok)
            .collect()
    } else {
        println!("Give path pattern (e.g., *.txt):");
        loop {
            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .expect("Failed to read line");

            let value = input.trim().to_string();
            println!("Pattern entered: {}", value);

            let paths: Vec<PathBuf> = glob(&value)
                .expect("Failed to read glob pattern")
                .filter_map(Result::ok)
                .collect();

            if !paths.is_empty() {
                break paths;
            } else {
                println!("No files matched the pattern. Try again.");
            }
        }
    };

    // let mut dfvec: Vec<csv_parse::DataFrame> = Vec::new();
    // println!("{:?}", &fpaths);
    for path in &fpaths {
        let df = match csv_parse::read_csv(&path) {
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
