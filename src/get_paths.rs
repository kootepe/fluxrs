use glob::glob;
use std::error::Error;
use std::io;
use std::path::PathBuf;

// pub fn get_paths(args: Vec<String>) -> Result<Vec<PathBuf>, Box<dyn Error>> {
pub fn get_paths(parg: &str, string: &str) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let paths: Vec<PathBuf> = if !parg.is_empty() {
        glob(parg)
            .expect("Failed to read glob pattern")
            .filter_map(Result::ok)
            .collect()
    } else {
        println!("Give path pattern to {string} data (e.g., *.txt or 24*.DAT):");
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
    Ok(paths)
}

// pub fn get_time_paths(parg: &str) -> Result<Vec<PathBuf>, Box<dyn Error>> {
//     let paths: Vec<PathBuf> = if !parg.is_empty() {
//         glob(parg)
//             .expect("Failed to read glob pattern")
//             .filter_map(Result::ok)
//             .collect()
//     } else {
//         println!("Give path pattern to time data (e.g., *.txt):");
//         loop {
//             let mut input = String::new();
//             io::stdin()
//                 .read_line(&mut input)
//                 .expect("Failed to read line");
//
//             let value = input.trim().to_string();
//             println!("Pattern entered: {}", value);
//
//             let paths: Vec<PathBuf> = glob(&value)
//                 .expect("Failed to read glob pattern")
//                 .filter_map(Result::ok)
//                 .collect();
//
//             if !paths.is_empty() {
//                 break paths;
//             } else {
//                 println!("No files matched the pattern. Try again.");
//             }
//         }
//     };
//     Ok(paths)
// }
