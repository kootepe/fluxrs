mod cmd;

use crate::cmd::cli::Cli;
use crate::cmd::config::Config;

use fluxrs_core::db::fluxes_schema::initiate_tables;
use fluxrs_core::db::migrate::migrate_db;

use clap::Parser;
use std::path::Path;
use std::process;

fn main() {
    // DB init / migrations
    if !Path::new("fluxrs.db").exists() {
        match initiate_tables() {
            Ok(_) => println!("Successfully initiated db tables"),
            Err(e) => {
                eprintln!("Err:\n{}", e);
                process::exit(1);
            },
        }
    } else if let Err(e) = migrate_db() {
        eprintln!("Migration failed: {}", e);
        process::exit(1);
    }

    let cli = Cli::parse();
    let mut cfg: Config = cli.into_config();
    if let Err(e) = cfg.run() {
        eprintln!("{e}");
        process::exit(1);
    }
}
