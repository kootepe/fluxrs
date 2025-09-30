#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use std::env;
use std::path::Path;
use std::process;

use clap::Parser;

use fluxrs::cmd::cli::Cli;
use fluxrs::cmd::config::Config;
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
            Ok(0) => (),
            Ok(1) => println!("Successfully migrated db tables."),
            Ok(_) => println!("Unknown success code."),
            Err(e) => {
                println!("Err:\n {}", e);
                process::exit(1)
            },
        }
    }

    let inputs = env::args();
    if inputs.len() > 1 {
        let cli = Cli::parse();

        // Convert into your existing Config and run the current pipeline
        let cfg: crate::Config = cli.into_config();
        if let Err(e) = cfg.run() {
            eprintln!("{e}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    let app = MyApp::new();
    eframe::run_native("fluxrs", Default::default(), Box::new(|_cc| Ok(Box::new(app))))
}
