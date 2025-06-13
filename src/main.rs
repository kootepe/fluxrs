use std::env;
use std::path::Path;
use std::process;

use fluxrs::cmd::Config;
use fluxrs::myapp;

fn main() -> eframe::Result {
    if !Path::new("fluxrs.db").exists() {
        match fluxrs::query::initiate_tables() {
            Ok(_) => println!("Successfully initiated db tables"),
            Err(e) => println!("Err:\n {}", e),
        }
    } else {
        match fluxrs::query::migrate_db() {
            Ok(0) => println!("No migrations necessary."),
            Ok(1) => println!("Successfully migrated db tables."),
            Ok(2) => println!("Successfully migrated to db version 2"),
            Ok(3) => println!("Successfully migrated to db version 3"),
            Ok(4) => println!("Successfully migrated to db version 4"),
            Ok(5) => println!("Successfully migrated to db version 5"),
            Ok(6) => println!("Successfully migrated to db version 6"),
            Ok(_) => println!("Unknown success code."),
            Err(e) => println!("Err:\n {}", e),
        }
    }

    let inputs = env::args();
    if inputs.len() > 1 {
        let mut config = Config::build(inputs).unwrap_or_else(|err| {
            println!("Parsing problem {err}");
            process::exit(1)
        });
        config.run();
    }

    // NOTE: I dont think this error will ever happen since they are being handled in run?
    // if let Err(e) = fluxrs::run(config) {
    //     println!("App error: {e}.")
    // }

    // let mut data = fluxrs::run(config).unwrap();

    let app = myapp::MyApp::new();
    eframe::run_native("fluxrs", Default::default(), Box::new(|_cc| Ok(Box::new(app))))
}
