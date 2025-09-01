#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use std::env;
use std::path::Path;
use std::process;

use fluxrs::cmd::Config;
use fluxrs::fluxes_schema;
use fluxrs::ui::main_frame::MyApp;

fn main() -> eframe::Result {
    if !Path::new("fluxrs.db").exists() {
        match fluxes_schema::initiate_tables() {
            Ok(_) => println!("Successfully initiated db tables"),
            Err(e) => {
                println!("Err:\n {}", e);
                process::exit(1)
            },
        }
    } else {
        match fluxes_schema::migrate_db() {
            Ok(0) => println!("No migrations necessary."),
            Ok(1) => println!("Successfully migrated db tables."),
            Ok(2) => println!("Successfully migrated to db version 2"),
            Ok(3) => println!("Successfully migrated to db version 3"),
            Ok(4) => println!("Successfully migrated to db version 4"),
            Ok(5) => println!("Successfully migrated to db version 5"),
            Ok(6) => println!("Successfully migrated to db version 6"),
            Ok(7) => println!("Successfully migrated to db version 7"),
            Ok(_) => println!("Unknown success code."),
            Err(e) => {
                println!("Err:\n {}", e);
                process::exit(1)
            },
        }
    }

    let inputs = env::args();
    if inputs.len() > 1 {
        let config = Config::build(inputs).unwrap_or_else(|err| {
            println!("Parsing problem {err}");
            process::exit(1)
        });
        config.run();
    }

    let app = MyApp::new();
    eframe::run_native("fluxrs", Default::default(), Box::new(|_cc| Ok(Box::new(app))))
}
