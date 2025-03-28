use std::env;
use std::path::Path;
use std::process;

use fluxrs::myapp;
use fluxrs::Config;

fn main() -> eframe::Result {
    // fn main() -> Result<()> {
    let inputs = env::args();
    let config = Config::build(inputs).unwrap_or_else(|err| {
        println!("Parsing problem {err}");
        process::exit(1)
    });

    // NOTE: I dont think this error will ever happen since they are being handled in run?
    // if let Err(e) = fluxrs::run(config) {
    //     println!("App error: {e}.")
    // }

    if !Path::new("fluxrs.db").exists() {
        match fluxrs::query::initiate_tables() {
            Ok(_) => println!("Successfully initiated db tables"),
            Err(e) => println!("Err:\n {}", e),
        }
    } else {
        match fluxrs::query::migrate_db() {
            Ok(0) => println!("No migrations necessary."),
            Ok(1) => println!("Successfully migrated db tables."),
            Ok(_) => println!("ASD"),
            Err(e) => println!("Err:\n {}", e),
        }
    }

    // let mut data = fluxrs::run(config).unwrap();

    let app = myapp::MyApp::new();
    eframe::run_native("fluxrs", Default::default(), Box::new(|_cc| Ok(Box::new(app))))
}
