use std::env;
use std::process;

use fluxrs::Config;

fn main() {
    let args: Vec<String> = env::args().collect();

    let config = Config::build(&args).unwrap_or_else(|err| {
        println!("Parsing problem {err}");
        process::exit(1)
    });

    // NOTE: I dont think this error will ever happen since they are being handled in run?
    if let Err(e) = fluxrs::run(config) {
        println!("App error: {e}.")
    }
}
