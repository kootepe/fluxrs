use crate::fluxes_schema::{create_flux_history_table, create_flux_table};
use rusqlite::{Connection, Result};

const DB_VERSION: i32 = 6;

pub fn migrate_db() -> Result<i32> {
    let conn = Connection::open("fluxrs.db")?;
    // user_version is 0 by default
    let current_version: i32 = conn.query_row("PRAGMA user_version;", [], |row| row.get(0))?;
    let mut migrated = 0;

    if current_version < 1 {
        println!("Applying migration 1: Setting PRAGMA to 1");
        conn.execute(&format!("PRAGMA user_version = {};", DB_VERSION), [])?;
        migrated = 1;
    }
    if current_version < 2 {
        println!("Applying migration 2: Adding flux_history table");
        conn.execute(&format!("PRAGMA user_version = {};", DB_VERSION), [])?;
        conn.execute(&create_flux_history_table(), [])?;
        migrated = 2;
    }
    if current_version < 3 {
        println!("Applying migration 3: New lag variables");
        conn.execute(&format!("PRAGMA user_version = {};", DB_VERSION), [])?;
        conn.execute("ALTER TABLE fluxes RENAME COLUMN lag_s TO open_lag_s;", [])?;
        conn.execute("ALTER TABLE fluxes ADD COLUMN close_lag_s INTEGER NOT NULL DEFAULT 0;", [])?;
        migrated = 3;
    }
    if current_version < 4 {
        println!("Applying migration 4: More lag variables");
        conn.execute(&format!("PRAGMA user_version = {};", DB_VERSION), [])?;
        conn.execute("ALTER TABLE fluxes ADD COLUMN end_lag_s INTEGER NOT NULL DEFAULT 0;", [])?;
        conn.execute("ALTER TABLE fluxes ADD COLUMN start_lag_s INTEGER NOT NULL DEFAULT 0;", [])?;
        migrated = 4;
    }
    if current_version < 5 {
        println!("Applying migration 5: Added linfit intercept");
        conn.execute(&format!("PRAGMA user_version = {};", DB_VERSION), [])?;
        conn.execute("ALTER TABLE fluxes ADD COLUMN ch4_intercept FLOAT NOT NULL DEFAULT 0;", [])?;
        conn.execute("ALTER TABLE fluxes ADD COLUMN co2_intercept FLOAT NOT NULL DEFAULT 0;", [])?;
        conn.execute("ALTER TABLE fluxes ADD COLUMN h2o_intercept FLOAT NOT NULL DEFAULT 0;", [])?;
        conn.execute("ALTER TABLE fluxes ADD COLUMN n2o_intercept FLOAT NOT NULL DEFAULT 0;", [])?;
        migrated = 5;
    }
    if current_version < 6 {
        println!("Applying migration 6: Add t0 concentration");
        conn.execute(&format!("PRAGMA user_version = {};", DB_VERSION), [])?;
        conn.execute(
            "ALTER TABLE fluxes ADD COLUMN ch4_t0_concentration FLOAT NOT NULL DEFAULT 0;",
            [],
        )?;
        conn.execute(
            "ALTER TABLE fluxes ADD COLUMN co2_t0_concentration FLOAT NOT NULL DEFAULT 0;",
            [],
        )?;
        conn.execute(
            "ALTER TABLE fluxes ADD COLUMN h2o_t0_concentration FLOAT NOT NULL DEFAULT 0;",
            [],
        )?;
        conn.execute(
            "ALTER TABLE fluxes ADD COLUMN n2o_t0_concentration FLOAT NOT NULL DEFAULT 0;",
            [],
        )?;
        conn.execute(
            "ALTER TABLE flux_history ADD COLUMN ch4_t0_concentration FLOAT NOT NULL DEFAULT 0;",
            [],
        )?;
        conn.execute(
            "ALTER TABLE flux_history ADD COLUMN co2_t0_concentration FLOAT NOT NULL DEFAULT 0;",
            [],
        )?;
        conn.execute(
            "ALTER TABLE flux_history ADD COLUMN h2o_t0_concentration FLOAT NOT NULL DEFAULT 0;",
            [],
        )?;
        conn.execute(
            "ALTER TABLE flux_history ADD COLUMN n2o_t0_concentration FLOAT NOT NULL DEFAULT 0;",
            [],
        )?;
        migrated = 6;
    }

    Ok(migrated)
}
pub fn initiate_tables() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open("fluxrs.db")?;
    // conn.execute("PRAGMA journal_mode=WAL;", [])?;
    // let wal_mode: String = conn.query_row("PRAGMA journal_mode=WAL;", [], |row| row.get(0))?;

    conn.execute(&format!("PRAGMA user_version = {};", DB_VERSION), [])?;
    // conn.execute("PRAGMA journal_mode = WAL;", [])?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS volume (
            chamber_id TEXT,
            project_id TEXT,
            datetime INTEGER,
            volume FLOAT,
            PRIMARY KEY (chamber_id, project_id, datetime)
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS meteo (
            project_id TEXT NOT NULL,
            datetime INTEGER,
            temperature FLOAT,
            pressure FLOAT,
            PRIMARY KEY (datetime, project_id)
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cycles (
            chamber_id TEXT NOT NULL,
            start_time INTEGER NOT NULL,
            close_offset INTEGER NOT NULL,
            open_offset INTEGER NOT NULL,
            end_offset INTEGER NOT NULL,
            project_id TEXT NOT NULL,
            PRIMARY KEY ( start_time, chamber_id, project_id)
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS projects (
            project_id TEXT PRIMARY KEY,
            main_gas TEXT NOT NULL,
            instrument_model TEXT NOT NULL,
            instrument_serial TEXT NOT NULL,
            current INTEGER NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS measurements (
            instrument_model TEXT NOT NULL,
            instrument_serial TEXT NOT NULL,
            datetime INTEGER,
            ch4 FLOAT,
            co2 FLOAT,
            h2o FLOAT,
            n2o FLOAT,
            diag INTEGER,
            project_id TEXT NOT NULL,
            PRIMARY KEY (datetime, instrument_serial, project_id)
        )",
        [],
    )?;
    conn.execute(&create_flux_table(), [])?;
    conn.execute(&create_flux_history_table(), [])?;

    Ok(())
}
