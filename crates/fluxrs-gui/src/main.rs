#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
mod appview;
mod flux_extension;
mod gastype_extension;
mod keybinds;
mod ui;
mod utils;

use crate::ui::main_frame::FluxApp;
use fluxrs_core::cmd::cli::Cli;
use fluxrs_core::cmd::config::Config;
use fluxrs_core::fluxes_schema;

use clap::Parser;
use std::env;
use std::path::Path;
use std::process;
#[cfg(windows)]
fn attach_to_parent_console_if_present() {
    use std::fs::OpenOptions;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Foundation::WIN32_ERROR;
    use windows::Win32::System::Console::{
        AttachConsole, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE,
    };

    unsafe {
        // Try to attach to the parent's console.
        //
        // If this was launched from a terminal (PowerShell, cmd), this succeeds
        // and we can hook up stdout/stderr/stdin.
        //
        // If this was launched by double-click in Explorer, this returns an
        // error like ERROR_INVALID_HANDLE or ERROR_GEN_FAILURE.
        //
        // If we already HAVE a console (e.g. debug build without windows_subsystem = "windows"),
        // AttachConsole will often return ERROR_ACCESS_DENIED. That's fine, it just means
        // we don't need to do anything else.

        let result = AttachConsole(ATTACH_PARENT_PROCESS);

        // Only if AttachConsole succeeded do we need to redirect stdio.
        if result.is_ok() {
            // Re-open console output/error/input handles and assign them
            if let Ok(out_file) = OpenOptions::new().write(true).open("CONOUT$") {
                let _ = SetStdHandle(STD_OUTPUT_HANDLE, HANDLE(out_file.as_raw_handle() as *mut _));
            }
            if let Ok(err_file) = OpenOptions::new().write(true).open("CONOUT$") {
                let _ = SetStdHandle(STD_ERROR_HANDLE, HANDLE(err_file.as_raw_handle() as *mut _));
            }
            if let Ok(in_file) = OpenOptions::new().read(true).open("CONIN$") {
                let _ = SetStdHandle(STD_INPUT_HANDLE, HANDLE(in_file.as_raw_handle() as *mut _));
            }
        }
        // else: couldn't attach (probably GUI launch); just run GUI silently.
    }
}

#[cfg(not(windows))]
fn attach_to_parent_console_if_present() {
    // no-op on non-Windows
}

fn main() -> eframe::Result {
    if !Path::new("fluxrs.db").exists() {
        match fluxes_schema::initiate_tables() {
            Ok(_) => println!("Successfully initiated db tables"),
            Err(e) => {
                println!("Err:\n {}", e);
                process::exit(1)
            },
        }
    } else if let Err(e) = fluxes_schema::migrate_db() {
        eprintln!("Migration failed: {}", e);
        process::exit(1);
    }
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        // CLI mode: attach console on Windows so println!/eprintln! works.
        attach_to_parent_console_if_present();

        // Parse CLI flags
        let cli = Cli::parse();

        // Convert to runtime config and run pipeline
        let mut cfg: Config = cli.into_config();
        if let Err(e) = cfg.run() {
            eprintln!("{e}");
            process::exit(1);
        }

        process::exit(0);
    }

    let app = FluxApp::new();
    eframe::run_native("fluxrs", Default::default(), Box::new(|_cc| Ok(Box::new(app))))
}
