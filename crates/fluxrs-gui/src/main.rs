#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
mod appview;
mod cycle_navigator;
mod flux_extension;
mod gastype_extension;
mod keybinds;
mod ui;
mod utils;

use crate::ui::main_frame::FluxApp;

use std::path::Path;
use std::process;

fn main() -> eframe::Result {
    if !Path::new("fluxrs.db").exists() {
        match fluxrs_core::db::fluxes_schema::initiate_tables() {
            Ok(_) => println!("Successfully initiated db tables"),
            Err(e) => {
                eprintln!("Err:\n{}", e);
                process::exit(1);
            },
        }
    } else if let Err(e) = fluxrs_core::db::migrate::migrate_db() {
        eprintln!("Migration failed: {}", e);
        process::exit(1);
    }

    // GUI mode
    let app = FluxApp::new();
    eframe::run_native("fluxrs", Default::default(), Box::new(|_cc| Ok(Box::new(app))))
}
