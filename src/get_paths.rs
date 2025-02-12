use glob::glob;
use std::error::Error;
use std::io;
use std::path::PathBuf;

fn glob_paths(str: &str) -> Vec<PathBuf> {
    glob(str)
        .expect("Failed to read glob pattern")
        .filter_map(Result::ok)
        .collect()
}

fn ask_paths() -> String {
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read line");

    let value = input.trim().to_string();
    println!("Pattern entered: {}", value);
    input
}

pub fn get_paths(parg: String, string: &str) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let paths: Vec<PathBuf> = if !parg.is_empty() {
        let vec = glob_paths(&parg);
        if vec.is_empty() {
            eprintln!("No {string} files found with {parg}");
        }
        vec
    } else {
        println!("Give path pattern to {string} data (e.g., *.txt or 24*.DAT):");
        loop {
            let pattern = ask_paths();
            let paths = glob_paths(&pattern);

            if !paths.is_empty() {
                break paths;
            } else {
                println!("No files matched the pattern. Try again.");
            }
        }
    };
    Ok(paths)
}
