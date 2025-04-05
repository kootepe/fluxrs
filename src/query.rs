use rusqlite::{Connection, Result};

const DB_VERSION: i32 = 1;

pub fn init_cycle_db(conn: &Connection) {
    match conn.execute(
        "CREATE TABLE IF NOT EXISTS cycles (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            chamber_id TEXT NOT NULL,
            start_time integer NOT NULL,
            close_offset integer NOT NULL,
            open_offset integer NOT NULL,
            end_offset integer NOT NULL,
            project_id TEXT NOT NULL
        )",
        [],
    ) {
        Ok(_) => println!("Cycle table initialized successfully."),
        Err(e) => eprintln!("Error initializing cycle table: {}", e),
    }
}
pub fn init_measurement_db(conn: &Connection) {
    match conn.execute(
        "CREATE TABLE IF NOT EXISTS measurements (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            instrument_model TEXT NOT NULL,
            instrument_serial TEXT NOT NULL,
            datetime integer NOT NULL,
            ch4 float,
            co2 float,
            h2o float,
            n2o float,
            diag integer
        )",
        [],
    ) {
        Ok(_) => println!("Measurement table initialized successfully."),
        Err(e) => eprintln!("Error initializing measurement table: {}", e),
    }
}

pub fn migrate_db() -> Result<i32> {
    let conn = Connection::open("fluxrs.db")?;
    let current_version: i32 = conn.query_row("PRAGMA user_version;", [], |row| row.get(0))?;
    let mut migrated = 0;
    println!("Current db version: {current_version}");

    if current_version < 1 {
        println!("Applying migration 1: Setting PRAGMA to 1");
        conn.execute(&format!("PRAGMA user_version = {};", DB_VERSION), [])?;
        migrated = 1;
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
    conn.execute(
        // id INTEGER PRIMARY KEY AUTOINCREMENT,
        "CREATE TABLE IF NOT EXISTS fluxes (
            instrument_model TEXT NOT NULL,
            instrument_serial TEXT NOT NULL,
            chamber_id TEXT NOT NULL,
            project_id TEXT NOT NULL,
            manual_adjusted BOOL NOT NULL,
            manual_valid bool NOT NULL,

            start_time INTEGER NOT NULL,
            close_offset INTEGER NOT NULL,
            open_offset INTEGER NOT NULL,
            end_offset INTEGER NOT NULL,
            lag_s INTEGER NOT NULL,

            air_pressure FLOAT,
            air_temperature FLOAT,
            chamber_volume FLOAT,

            error_code INTEGER,
            is_valid BOOL,
            main_gas TEXT NOT NULL,
            main_gas_r2 FLOAT,

            ch4_flux FLOAT,
            ch4_r2 FLOAT,
            ch4_measurement_r2 FLOAT,
            ch4_slope FLOAT,
            ch4_calc_range_start FLOAT,
            ch4_calc_range_end FLOAT,

            co2_flux FLOAT,
            co2_r2 FLOAT,
            co2_measurement_r2 FLOAT,
            co2_slope FLOAT,
            co2_calc_range_start FLOAT,
            co2_calc_range_end FLOAT,

            h2o_flux FLOAT,
            h2o_r2 FLOAT,
            h2o_measurement_r2 FLOAT,
            h2o_slope FLOAT,
            h2o_calc_range_start FLOAT,
            h2o_calc_range_end FLOAT,

            n2o_flux FLOAT,
            n2o_r2 FLOAT,
            n2o_measurement_r2 FLOAT,
            n2o_slope FLOAT,
            n2o_calc_range_start FLOAT,
            n2o_calc_range_end FLOAT,
            PRIMARY KEY (instrument_serial, start_time, project_id)
        )",
        [],
    )?;

    Ok(())
}

pub fn calculate_max_y_from_vec(values: &[f64]) -> f64 {
    values.iter().copied().filter(|v| !v.is_nan()).fold(f64::NEG_INFINITY, f64::max)
}

pub fn calculate_min_y_from_vec(values: &[f64]) -> f64 {
    values.iter().copied().filter(|v| !v.is_nan()).fold(f64::INFINITY, f64::min)
}
