#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
mod appview;
mod flux_extension;
mod gastype_extension;
mod keybinds;
mod ui;
mod utils;

use crate::ui::main_frame::FluxApp;

use std::env;
use std::path::Path;
use std::process::{self, Command};

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
    // CLI forwarding
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        // Build path to cli exe (next to this binary)
        // Works whether installed, built, or running via cargo
        let mut exe = env::current_exe().expect("Failed to get current exe path");

        #[cfg(windows)]
        exe.set_file_name("fluxrs-cli.exe");
        #[cfg(not(windows))]
        exe.set_file_name("fluxrs_cli");

        // Spawn the cli and forward arguments
        let status = Command::new(&exe)
    .args(&args[1..]) // Pass everything except the program name
    .status()
    .unwrap_or_else(|e| {
        eprintln!(
            "You tried to launch fluxrs with arguments, triggering the CLI mode.\n\
             But 'fluxrs_cli' (or 'fluxrs_cli.exe') could not be found or executed.\n\
             Attempted path: {}\n\
             Error: {}",
            exe.display(),
            e,
        );
        process::exit(1);
    });

        // Mirror cli exit code
        process::exit(status.code().unwrap_or(1));
    }

    // GUI mode
    let app = FluxApp::new();
    eframe::run_native("fluxrs", Default::default(), Box::new(|_cc| Ok(Box::new(app))))
}
