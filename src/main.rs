#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
use std::env;
use std::path::Path;
use std::process;

use clap::Parser;

use fluxrs::cmd::cli::Cli;
use fluxrs::cmd::config::Config;
use fluxrs::fluxes_schema;
use fluxrs::ui::main_frame::MyApp;

#[cfg(windows)]
fn attach_to_parent_console_if_present() {
    use std::fs::OpenOptions;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Console::{
        AttachConsole, GetConsoleMode, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS,
        STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    unsafe {
        // If we already have a console (e.g., built without GUI subsystem), do nothing.
        let stdout = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut _mode = 0;
        let already_has_console =
            windows::Win32::System::Console::GetConsoleMode(stdout, &mut _mode).is_ok();

        if !already_has_console {
            // Attach to the parent process's console (works when launched from a terminal)
            if AttachConsole(ATTACH_PARENT_PROCESS).is_ok() {
                // Reopen std handles to the console devices
                if let Ok(out) = OpenOptions::new().write(true).open("CONOUT$") {
                    let _ = SetStdHandle(STD_OUTPUT_HANDLE, HANDLE(out.as_raw_handle() as isize));
                }
                if let Ok(err) = OpenOptions::new().write(true).open("CONOUT$") {
                    let _ = SetStdHandle(STD_ERROR_HANDLE, HANDLE(err.as_raw_handle() as isize));
                }
                if let Ok(inp) = OpenOptions::new().read(true).open("CONIN$") {
                    let _ = SetStdHandle(STD_INPUT_HANDLE, HANDLE(inp.as_raw_handle() as isize));
                }
            }
            // If AttachConsole fails, we were likely double-clicked → keep running without a console.
        }
    }
}

fn main() -> eframe::Result {
    #[cfg(windows)]
    attach_to_parent_console_if_present();

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
        let mut cfg: crate::Config = cli.into_config();
        if let Err(e) = cfg.run() {
            eprintln!("{e}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    let app = MyApp::new();
    eframe::run_native("fluxrs", Default::default(), Box::new(|_cc| Ok(Box::new(app))))
}
