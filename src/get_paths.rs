use glob::glob;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::process;

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
    if parg.is_empty() {
        crate::exit_with_help();
    }
    let paths = glob_paths(&parg);
    if paths.is_empty() {
        return Err(format!("No {string} files found with pattern: {parg}").into());
    }
    Ok(paths)
}
