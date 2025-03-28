use crate::instruments::get_instrument_by_model;
use crate::instruments::{GasType, InstrumentType};
use crate::stats;
use crate::structs::{Cycle, ErrorMask, GasData, MeteoData, TimeData, VolumeData};
use chrono::{DateTime, NaiveDateTime, TimeDelta, TimeZone, Utc};
use csv::StringRecord;
use rusqlite::{params, Connection, Result};
use std::collections::HashMap;
use std::hash::Hash;

const DB_VERSION: i32 = 1;

pub fn query_cycles(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
) -> Result<TimeData> {
    println!("Querying cycles");
    println!("{}", start);
    println!("{}", end);
    let mut stmt = conn.prepare(
        "SELECT chamber_id, start_time, close_offset, open_offset, end_offset, project_id
         FROM cycles
         WHERE start_time BETWEEN ?1 AND ?2
         AND project_id = ?3
         ORDER BY start_time",
    )?;

    let mut times = TimeData::new();
    let cycle_iter =
        stmt.query_map(params![start.timestamp(), end.timestamp(), project], |row| {
            let chamber_id: String = row.get(0)?;
            let start_timestamp: i64 = row.get(1)?; // Start time as UNIX timestamp
            let close_offset: i64 = row.get(2)?;
            let open_offset: i64 = row.get(3)?;
            let end_offset: i64 = row.get(4)?;
            let project_id: String = row.get(5)?;

            let start_time = DateTime::<Utc>::from_utc(
                NaiveDateTime::from_timestamp_opt(start_timestamp, 0).expect("Invalid timestamp"),
                Utc,
            );

            times.chamber_id.push(chamber_id);
            times.start_time.push(start_time);
            times.close_offset.push(close_offset);
            times.open_offset.push(open_offset);
            times.end_offset.push(end_offset);
            times.project.push(project_id);

            Ok(())
        })?;

    for row in cycle_iter {
        row?; // Ensure errors are propagated
    }
    Ok(times)
}

pub fn query_gas(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
    instrument_serial: String,
) -> Result<HashMap<String, GasData>> {
    // let mut data = HashMap::new();
    println!("Querying gas data");
    let mut grouped_data: HashMap<String, GasData> = HashMap::new();
    println!("{}", instrument_serial);
    println!("{}", project);

    let mut stmt = conn.prepare(
        "SELECT datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model
             FROM measurements
             WHERE datetime BETWEEN ?1 AND ?2
             and project_id = ?3 AND instrument_serial = ?4
             ORDER BY datetime",
    )?;

    let rows = stmt.query_map(
        params![start.timestamp(), end.timestamp(), project, instrument_serial],
        |row| {
            let datetime_unix: i64 = row.get(0)?;
            let ch4: Option<f64> = row.get(1)?; // Handle NULL values
            let co2: Option<f64> = row.get(2)?;
            let h2o: Option<f64> = row.get(3)?;
            let n2o: Option<f64> = row.get(4)?;
            let diag: i64 = row.get(5)?;
            let instrument_serial: Option<String> = row.get(6)?;
            let instrument_model: Option<String> = row.get(7)?;

            // Convert UNIX timestamp to DateTime<Utc>
            let naive_datetime =
                NaiveDateTime::from_timestamp_opt(datetime_unix, 0).expect("Invalid timestamp");
            let datetime_utc = DateTime::<Utc>::from_utc(naive_datetime, Utc);

            Ok((datetime_utc, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model))
        },
    )?;

    for row in rows {
        let (datetime, ch4, co2, h2o, n2o, diag, instrument_serial, instrument_model) = row?;

        // ✅ Extract YYYY-MM-DD for grouping
        let date_key = datetime.format("%Y-%m-%d").to_string();

        // ✅ Get or create a new GasData entry
        let entry = grouped_data.entry(date_key.clone()).or_insert_with(|| GasData {
            header: StringRecord::new(),
            instrument_model: String::new(),
            instrument_serial: String::new(),
            datetime: Vec::new(),
            gas: HashMap::new(),
            diag: Vec::new(),
        });

        // ✅ Append values
        entry.datetime.push(datetime);
        entry.diag.push(diag);
        entry.instrument_model = instrument_model.unwrap();
        entry.instrument_serial = instrument_serial.unwrap();

        // ✅ Store each gas type in the `HashMap`
        if let Some(v) = ch4 {
            entry.gas.entry(GasType::CH4).or_insert_with(Vec::new).push(v);
        }
        if let Some(v) = co2 {
            entry.gas.entry(GasType::CO2).or_insert_with(Vec::new).push(v);
        }
        if let Some(v) = h2o {
            entry.gas.entry(GasType::H2O).or_insert_with(Vec::new).push(v);
        }
        if let Some(v) = n2o {
            entry.gas.entry(GasType::N2O).or_insert_with(Vec::new).push(v);
        }
    }
    Ok(grouped_data)
}

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
    let mut conn = Connection::open("fluxrs.db")?;
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
    let mut conn = Connection::open("fluxrs.db")?;
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

    // insert_measurements(&mut conn, gases)?;
    // insert_cycles(&mut conn, times)?;
    Ok(())
}

pub fn insert_flux(conn: &Connection, cycle: &Cycle) -> Result<()> {
    // Compute/collect the values that are common to both queries.
    let start_timestamp = cycle.start_time.timestamp();
    let close_offset = cycle.close_offset;
    let open_offset = cycle.open_offset;
    let end_offset = cycle.end_offset;
    let lag_s = cycle.lag_s as i64;

    // Gas values (defaulting to 0.0 if absent).
    let ch4_flux = cycle.flux.get(&GasType::CH4).copied().unwrap_or(0.0);
    let ch4_r2 = cycle.calc_r2.get(&GasType::CH4).copied().unwrap_or(0.0);
    let ch4_measurement_r2 = cycle.measurement_r2.get(&GasType::CH4).copied().unwrap_or(0.0);
    let ch4_slope = cycle.slope.get(&GasType::CH4).copied().unwrap_or(0.0);

    let co2_flux = cycle.flux.get(&GasType::CO2).copied().unwrap_or(0.0);
    let co2_r2 = cycle.calc_r2.get(&GasType::CO2).copied().unwrap_or(0.0);
    let co2_measurement_r2 = cycle.measurement_r2.get(&GasType::CO2).copied().unwrap_or(0.0);
    let co2_slope = cycle.slope.get(&GasType::CO2).copied().unwrap_or(0.0);

    let h2o_flux = cycle.flux.get(&GasType::H2O).copied().unwrap_or(0.0);
    let h2o_r2 = cycle.calc_r2.get(&GasType::H2O).copied().unwrap_or(0.0);
    let h2o_measurement_r2 = cycle.measurement_r2.get(&GasType::H2O).copied().unwrap_or(0.0);
    let h2o_slope = cycle.slope.get(&GasType::H2O).copied().unwrap_or(0.0);

    let n2o_flux = cycle.flux.get(&GasType::N2O).copied().unwrap_or(0.0);
    let n2o_r2 = cycle.calc_r2.get(&GasType::N2O).copied().unwrap_or(0.0);
    let n2o_measurement_r2 = cycle.measurement_r2.get(&GasType::N2O).copied().unwrap_or(0.0);
    let n2o_slope = cycle.slope.get(&GasType::N2O).copied().unwrap_or(0.0);

    // r value for the main gas.
    let main_gas_r2 = cycle.measurement_r2.get(&cycle.main_gas).copied().unwrap_or(0.0);

    // Convert error code.
    let error_code = cycle.error_code.0;
    let chamber_volume = cycle.chamber_volume;

    // First, attempt an INSERT OR IGNORE.
    conn.execute(
        "INSERT OR IGNORE INTO fluxes (
            instrument_model, instrument_serial, chamber_id, start_time, close_offset,
            open_offset, end_offset, lag_s, air_pressure, air_temperature, error_code,
            is_valid, main_gas_r2, ch4_flux, ch4_r2, ch4_measurement_r2, ch4_slope, co2_flux, co2_r2, co2_measurement_r2, co2_slope,
            h2o_flux, h2o_r2, h2o_measurement_r2, h2o_slope, n2o_flux, n2o_r2, n2o_measurement_r2, n2o_slope,
            chamber_volume
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                  ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30)",
        params![
            cycle.instrument_model.to_string(),
            cycle.instrument_serial,
            cycle.chamber_id,
            start_timestamp,
            close_offset,
            open_offset,
            end_offset,
            lag_s,
            cycle.air_pressure,
            cycle.air_temperature,
            error_code,
            cycle.is_valid,
            main_gas_r2,
            ch4_flux,
            ch4_r2,
            ch4_measurement_r2,
            ch4_slope,
            co2_flux,
            co2_r2,
            co2_measurement_r2,
            co2_slope,
            h2o_flux,
            h2o_r2,
            h2o_measurement_r2,
            h2o_slope,
            n2o_flux,
            n2o_r2,
            n2o_measurement_r2,
            n2o_slope,
            chamber_volume,
        ],
    )?;

    // Then, update the row (even if it was just inserted) with the latest values.
    // We use instrument_serial and start_time as the unique keys to identify the record.
    conn.execute(
        "UPDATE fluxes SET
            instrument_model = ?1,
            chamber_id = ?2,
            close_offset = ?3,
            open_offset = ?4,
            end_offset = ?5,
            lag_s = ?6,
            air_pressure = ?7,
            air_temperature = ?8,
            error_code = ?9,
            is_valid = ?10,
            main_gas_r2 = ?11,
            ch4_flux = ?12,
            ch4_r2 = ?13,
            ch4_measurement_r2 = ?14,
            ch4_slope = ?15,
            co2_flux = ?16,
            co2_r2 = ?17,
            co2_measurement_r2 = ?18,
            co2_slope = ?19,
            h2o_flux = ?20,
            h2o_r2 = ?21,
            h2o_measurement_r2 = ?22,
            h2o_slope = ?23,
            n2o_flux = ?24,
            n2o_r2 = ?25,
            n2o_measurement_r2 = ?26,
            n2o_slope = ?27,
            chamber_volume = ?28
         WHERE instrument_serial = ?29 AND start_time = ?30",
        params![
            cycle.instrument_model.to_string(),
            cycle.chamber_id,
            close_offset,
            open_offset,
            end_offset,
            lag_s,
            cycle.air_pressure,
            cycle.air_temperature,
            error_code,
            cycle.is_valid,
            main_gas_r2,
            ch4_flux,
            ch4_r2,
            ch4_measurement_r2,
            ch4_slope,
            co2_flux,
            co2_r2,
            co2_measurement_r2,
            co2_slope,
            h2o_flux,
            h2o_r2,
            h2o_measurement_r2,
            h2o_slope,
            n2o_flux,
            n2o_r2,
            n2o_measurement_r2,
            n2o_slope,
            chamber_volume,
            cycle.instrument_serial,
            start_timestamp,
        ],
    )?;

    Ok(())
}

pub fn insert_fluxes_ignore_duplicates(
    conn: &mut Connection,
    cycles: &[Cycle],
    project: String,
) -> Result<()> {
    let tx = conn.transaction()?; // Start transaction for bulk insertion

    {
        let mut insert_stmt = tx.prepare(
            "INSERT OR IGNORE INTO fluxes (
            instrument_model, instrument_serial, chamber_id, main_gas, start_time,
            close_offset, open_offset, end_offset, lag_s, air_pressure, air_temperature,
            error_code, is_valid, main_gas_r2, ch4_flux, ch4_r2, ch4_measurement_r2, ch4_slope,
            ch4_calc_range_start, ch4_calc_range_end, co2_flux, co2_r2, co2_measurement_r2, co2_slope,
            co2_calc_range_start, co2_calc_range_end, h2o_flux, h2o_r2, h2o_measurement_r2, h2o_slope,
            h2o_calc_range_start, h2o_calc_range_end, n2o_flux, n2o_r2, n2o_measurement_r2, n2o_slope,
            n2o_calc_range_start, n2o_calc_range_end, project_id, manual_adjusted, manual_valid, chamber_volume
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                  ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24,
                  ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42)",
        )?;
        for cycle in cycles {
            execute_insert(&mut insert_stmt, cycle, &project)?;
        }
    }

    tx.commit()?;
    Ok(())
}
pub fn update_fluxes(
    conn: &mut Connection,
    cycles: &[Cycle],
    project: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tx = conn.transaction()?; // Start transaction for consistency
    {
        let mut update_stmt = tx.prepare(
            "UPDATE fluxes SET
            instrument_model = ?1, chamber_id = ?2, main_gas = ?3, close_offset = ?4,
            open_offset = ?5, end_offset = ?6, lag_s = ?7, air_pressure = ?8,
            air_temperature = ?9, error_code = ?10, is_valid = ?11, main_gas_r2 = ?12,
            ch4_flux = ?13, ch4_r2 = ?14, ch4_measurement_r2 = ?15, ch4_slope = ?16,
            ch4_calc_range_start = ?17, ch4_calc_range_end = ?18, co2_flux = ?19,
            co2_r2 = ?20, co2_measurement_r2 = ?21, co2_slope = ?22, co2_calc_range_start = ?23,
            co2_calc_range_end = ?24, h2o_flux = ?25, h2o_r2 = ?26, h2o_measurement_r2 = ?27,
            h2o_slope = ?28, h2o_calc_range_start = ?29, h2o_calc_range_end = ?30, n2o_flux = ?31,
            n2o_r2 = ?32, n2o_measurement_r2 = ?33, n2o_slope = ?34, n2o_calc_range_start = ?35,
            n2o_calc_range_end = ?36, project_id = ?37, manual_adjusted = ?38, manual_valid = ?39, chamber_volume = ?40
         WHERE instrument_serial = ?41 AND start_time = ?42",
        )?;

        for cycle in cycles {
            match execute_update(&mut update_stmt, cycle, &project)
            // execute_update(
            //     &mut conn,
            //     &[self.cycles[*self.index].clone()],
            //     self.selected_project.as_ref().unwrap().clone(),
             {
                Ok(_) => println!("Fluxes updated successfully!"),
                Err(e) => eprintln!("Error inserting fluxes: {}", e),
            }
        }
    }
    tx.commit()?;
    Ok(())
}
// pub fn insert_or_update_fluxes(
//     conn: &mut Connection,
//     cycles: &[Cycle],
//     project: String,
// ) -> Result<()> {
//     let tx = conn.transaction()?; // Start transaction for consistency
//     {
//         let mut insert_stmt = tx.prepare(
//             "INSERT OR IGNORE INTO fluxes (
//             instrument_model, instrument_serial, chamber_id, main_gas, start_time,
//             close_offset, open_offset, end_offset, lag_s, air_pressure, air_temperature,
//             error_code, is_valid, main_gas_r2, ch4_flux, ch4_r2, ch4_measurement_r2, ch4_slope,
//             ch4_calc_range_start, ch4_calc_range_end, co2_flux, co2_r2, co2_measurement_r2, co2_slope,
//             co2_calc_range_start, co2_calc_range_end, h2o_flux, h2o_r2, h2o_measurement_r2, h2o_slope,
//             h2o_calc_range_start, h2o_calc_range_end, n2o_flux, n2o_r2, n2o_measurement_r2, n2o_slope,
//             n2o_calc_range_start, n2o_calc_range_end, project_id, manual_adjusted
//         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
//                   ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24,
//                   ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40)",
//         )?;
//
//         let mut update_stmt = tx.prepare(
//             "UPDATE fluxes SET
//             instrument_model = ?1, chamber_id = ?2, main_gas = ?3, close_offset = ?4,
//             open_offset = ?5, end_offset = ?6, lag_s = ?7, air_pressure = ?8,
//             air_temperature = ?9, error_code = ?10, is_valid = ?11, main_gas_r2 = ?12,
//             ch4_flux = ?13, ch4_r2 = ?14, ch4_measurement_r2 = ?15, ch4_slope = ?16,
//             ch4_calc_range_start = ?17, ch4_calc_range_end = ?18, co2_flux = ?19,
//             co2_r2 = ?20, co2_measurement_r2 = ?21, co2_slope = ?22, co2_calc_range_start = ?23,
//             co2_calc_range_end = ?24, h2o_flux = ?25, h2o_r2 = ?26, h2o_measurement_r2 = ?27,
//             h2o_slope = ?28, h2o_calc_range_start = ?29, h2o_calc_range_end = ?30, n2o_flux = ?31,
//             n2o_r2 = ?32, n2o_measurement_r2 = ?33, n2o_slope = ?34, n2o_calc_range_start = ?35,
//             n2o_calc_range_end = ?36, project_id = ?37, manual_adjusted = ?38
//          WHERE instrument_serial = ?39 AND start_time = ?40",
//         )?;
//
//         for cycle in cycles {
//             execute_insert(&mut insert_stmt, cycle, &project)?;
//             execute_update(&mut update_stmt, cycle, &project)?;
//         }
//     }
//     tx.commit()?;
//     Ok(())
// }

fn execute_insert(stmt: &mut rusqlite::Statement, cycle: &Cycle, project: &String) -> Result<()> {
    stmt.execute(params![
        cycle.instrument_model.to_string(),
        cycle.instrument_serial,
        cycle.chamber_id,
        cycle.main_gas.column_name(),
        cycle.start_time.timestamp(),
        cycle.close_offset,
        cycle.open_offset,
        cycle.end_offset,
        cycle.lag_s as i64,
        cycle.air_pressure,
        cycle.air_temperature,
        cycle.error_code.0,
        cycle.is_valid,
        cycle.measurement_r2.get(&cycle.main_gas).copied().unwrap_or(1.0),
        cycle.flux.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::N2O).copied().unwrap_or(0.0),
        project,
        cycle.manual_adjusted,
        cycle.manual_valid,
        cycle.chamber_volume,
    ])?;
    Ok(())
}

/// ✅ Helper function to execute UPDATE statement
fn execute_update(stmt: &mut rusqlite::Statement, cycle: &Cycle, project: &String) -> Result<()> {
    println!("{}", cycle.lag_s);
    stmt.execute(params![
        cycle.instrument_model.to_string(),
        cycle.chamber_id,
        cycle.main_gas.column_name(),
        cycle.close_offset,
        cycle.open_offset,
        cycle.end_offset,
        cycle.lag_s as i64,
        cycle.air_pressure,
        cycle.air_temperature,
        cycle.error_code.0,
        cycle.is_valid,
        cycle.measurement_r2.get(&cycle.main_gas).copied().unwrap_or(1.0),
        cycle.flux.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CH4).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CH4).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::CO2).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::CO2).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::H2O).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::H2O).copied().unwrap_or(0.0),
        cycle.flux.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.measurement_r2.get(&GasType::N2O).copied().unwrap_or(1.0),
        cycle.slope.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_range_start.get(&GasType::N2O).copied().unwrap_or(0.0),
        cycle.calc_range_end.get(&GasType::N2O).copied().unwrap_or(0.0),
        project,
        cycle.manual_adjusted,
        cycle.manual_valid,
        cycle.chamber_volume,
        cycle.instrument_serial,
        cycle.start_time.timestamp(),
    ])?;
    Ok(())
}

/// Loads cycles from the fluxes table. The SELECT order must match the INSERT order:
/// 0: instrument_model
/// 1: instrument_serial
/// 2: chamber_id
/// 3: main_gas (stored as a string)
/// 4: start_time (Unix timestamp)
/// 5: close_offset
/// 6: open_offset
/// 7: end_offset
/// 8: lag_s (stored as integer)
/// 9: air_pressure
/// 10: air_temperature
/// 11: error_code (u16)
/// 12: is_valid (bool)
/// 13: main_gas_r2
/// 14: ch4_flux
/// 15: ch4_r
/// 16: ch4_slope
/// 17: ch4_calc_range_start
/// 18: ch4_calc_range_end
/// 19: co2_flux
/// 20: co2_r
/// 21: co2_slope
/// 22: co2_calc_range_start
/// 23: co2_calc_range_end
/// 24: h2o_flux
/// 25: h2o_r
/// 26: h2o_slope
/// 27: h2o_calc_range_start
/// 28: h2o_calc_range_end
/// 29: n2o_flux
/// 30: n2o_r
/// 31: n2o_slope
/// 32: n2o_calc_range_start
/// 33: n2o_calc_range_end
pub fn load_fluxes(
    conn: &mut Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
    instrument_serial: String,
) -> Result<Vec<Cycle>> {
    let mut stmt = conn.prepare(
        "SELECT instrument_model,
                instrument_serial,
                chamber_id,
                main_gas,
                start_time,
                close_offset,
                open_offset,
                end_offset,
                lag_s,
                air_pressure,
                air_temperature,
                error_code,
                is_valid,
                main_gas_r2,
                ch4_flux,
                ch4_r2,
                ch4_measurement_r2,
                ch4_slope,
                ch4_calc_range_start,
                ch4_calc_range_end,
                co2_flux,
                co2_r2,
                co2_measurement_r2,
                co2_slope,
                co2_calc_range_start,
                co2_calc_range_end,
                h2o_flux,
                h2o_r2,
                h2o_measurement_r2,
                h2o_slope,
                h2o_calc_range_start,
                h2o_calc_range_end,
                n2o_flux,
                n2o_r2,
                n2o_measurement_r2,
                n2o_slope,
                n2o_calc_range_start,
                n2o_calc_range_end,
                manual_adjusted,
                project_id,
                manual_valid,
                chamber_volume
         FROM fluxes WHERE start_time BETWEEN ?1 AND ?2 and project_id = ?3",
    )?;
    let gas_data = query_gas(conn, start, end, project.clone(), instrument_serial)?;

    let s = start.timestamp();
    let e = end.timestamp();
    let cycle_iter = stmt.query_map(params![s, e, project.clone()], |row| {
        // Basic fields
        let instrument_model: String = row.get(0)?;
        let instrument_serial: String = row.get(1)?;
        let chamber_id: String = row.get(2)?;

        let gases = get_instrument_by_model(&instrument_model).unwrap().base.gas_cols;
        let gastypes: Vec<GasType> =
            gases.iter().filter_map(|name| GasType::from_str(name)).collect();

        let main_gas_str: String = row.get(3)?;
        let main_gas = GasType::from_str(&main_gas_str).unwrap_or(GasType::CH4);

        let start_timestamp: i64 = row.get(4)?;
        let naive = NaiveDateTime::from_timestamp(start_timestamp, 0);
        let start_time = DateTime::<Utc>::from_utc(naive, Utc);
        let day = start_time.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD

        let close_offset: i64 = row.get(5)?;
        let open_offset: i64 = row.get(6)?;
        let end_offset: i64 = row.get(7)?;
        let lag_s: i64 = row.get(8)?;
        let lag_s = lag_s as f64; // convert back to f64

        let air_pressure: f64 = row.get(9)?;
        let air_temperature: f64 = row.get(10)?;

        let error_code_u16: u16 = row.get(11)?;
        let error_code = ErrorMask::from_u16(error_code_u16);

        let is_valid: bool = row.get(12)?;
        let main_gas_r2: f64 = row.get(13)?;

        // Compute derived times from start_time and offsets.
        let close_time = start_time + TimeDelta::seconds(close_offset);
        let open_time = start_time + TimeDelta::seconds(open_offset);
        let end_time = start_time + TimeDelta::seconds(end_offset);

        let gas_columns =
            vec![(GasType::CH4, 14), (GasType::CO2, 20), (GasType::H2O, 26), (GasType::N2O, 32)];

        let manual_adjusted = row.get(38)?;
        let project_name = row.get(39)?;
        let manual_valid: bool = row.get(40)?;
        let chamber_volume: f64 = row.get(41)?;
        let mut override_valid = None;
        if manual_valid {
            override_valid = Some(is_valid);
        }
        let filtered: Vec<(GasType, usize)> =
            gas_columns.into_iter().filter(|(gas, _)| gastypes.contains(gas)).collect();
        // Initialize the HashMaps.
        let mut flux = HashMap::new();
        let mut calc_r2 = HashMap::new();
        // let mut measurement_r2 = HashMap::new();
        let mut slope_map = HashMap::new();
        let mut calc_range_start_map = HashMap::new();
        let mut calc_range_end_map = HashMap::new();
        let mut calc_gas_v = HashMap::new();
        let mut calc_dt_v = HashMap::new();
        let mut measurement_dt_v = Vec::new();
        let mut measurement_diag_v = Vec::new();
        let mut measurement_gas_v = HashMap::new();
        let measurement_range_start = close_time + TimeDelta::seconds(lag_s as i64);
        let measurement_range_end = open_time + TimeDelta::seconds(lag_s as i64);
        let mut dt_v = Vec::new();
        let mut diag_v = Vec::new();
        let mut gas_v = HashMap::new();
        let mut min_y = HashMap::new();
        let mut max_y = HashMap::new();
        let mut measurement_r2 = HashMap::new();

        if let Some(gas_data_day) = gas_data.get(&day) {
            // --- Calculation & Measurement Filtering for Each Gas ---
            for (gas, base_idx) in filtered {
                if let Some(g_values) = gas_data_day.gas.get(&gas) {
                    // Here you extract per-gas values from the flux row.
                    // (We assume that part of the code remains the same.)
                    let gas_flux: f64 = row.get(base_idx)?;
                    let gas_r2: f64 = row.get(base_idx + 1)?;
                    let gas_measurement_r2: f64 = row.get(base_idx + 2)?;
                    let gas_slope: f64 = row.get(base_idx + 3)?;
                    let gas_calc_range_start: f64 = row.get(base_idx + 4)?;
                    let gas_calc_range_end: f64 = row.get(base_idx + 5)?;

                    flux.insert(gas, gas_flux);
                    calc_r2.insert(gas, gas_r2);
                    measurement_r2.insert(gas, gas_measurement_r2);
                    slope_map.insert(gas, gas_slope);
                    calc_range_start_map.insert(gas, gas_calc_range_start);
                    calc_range_end_map.insert(gas, gas_calc_range_end);

                    // Filter for calculation range using the per-gas calc range.
                    let (calc_dt, calc_vals) = filter_data_in_range(
                        &gas_data_day.datetime,
                        g_values,
                        calc_range_start_map.get(&gas).copied().unwrap_or(0.0),
                        calc_range_end_map.get(&gas).copied().unwrap_or(0.0),
                    );

                    calc_dt_v.insert(gas, calc_dt);
                    calc_gas_v.insert(gas, calc_vals);

                    // Filter for measurement range using the cycle's measurement range.
                    let (meas_dt, meas_vals) = filter_data_in_range(
                        &gas_data_day.datetime,
                        g_values,
                        measurement_range_start.timestamp() as f64,
                        measurement_range_end.timestamp() as f64,
                    );
                    // let timestamps: Vec<f64> =
                    //     meas_dt.iter().map(|x| x.timestamp() as f64).collect();
                    // measurement_r2.insert(
                    //     gas,
                    //     stats::pearson_correlation(&timestamps[..], &meas_vals).unwrap_or(0.0),
                    // );
                    // measurement_r2.insert(
                    //     &gas,
                    //     stats::pearson_correlation(
                    //         &meas_dt.iter().map(|x| x.timestamp() as f64).collect(),
                    //         &meas_vals,
                    //     )
                    //     .unwrap_or(0.0),
                    // );
                    if gas == main_gas {
                        // For the main gas, assign the filtered datetime vector.
                        measurement_dt_v = meas_dt;
                    }
                    measurement_gas_v.insert(gas, meas_vals);
                }
            }

            // --- Overall Cycle Data Filtering ---
            // Filter diagnostic data (diag_v) and full datetime (dt_v) for the entire cycle:
            let (dt_v_full, diag_v_full) = filter_diag_data(
                &gas_data_day.datetime,
                &gas_data_day.diag,
                start_time.timestamp() as f64,
                end_time.timestamp() as f64,
            );
            dt_v = dt_v_full; // Assign overall datetime vector.
            diag_v = diag_v_full; // Assign overall diagnostic vector.
            if dt_v.is_empty() {
                return Ok(None); // Use `None` to skip cycle
            }
            for &gas in &gastypes {
                if let Some(g_values) = gas_data_day.gas.get(&gas) {
                    let (full_dt, full_vals) = filter_data_in_range(
                        &gas_data_day.datetime,
                        g_values,
                        start_time.timestamp() as f64,
                        end_time.timestamp() as f64,
                    );
                    max_y.insert(gas, calculate_max_y_from_vec(&full_vals));
                    min_y.insert(gas, calculate_min_y_from_vec(&full_vals));
                    gas_v.insert(gas, full_vals);
                    // Optionally, store full_dt in a dedicated dt map if needed.
                }
            }
        }
        Ok(Some(Cycle {
            instrument_model: InstrumentType::from_str(&instrument_model),
            instrument_serial,
            project_name,
            manual_adjusted,
            chamber_id,
            start_time,
            calc_dt_v,
            calc_gas_v,
            diag_v,
            dt_v,
            gas_v,
            max_y,
            min_y,
            measurement_dt_v,
            measurement_gas_v,
            measurement_diag_v,
            close_time,
            open_time,
            end_time,
            air_temperature,
            air_pressure,
            chamber_volume,
            error_code,
            is_valid,
            override_valid,
            manual_valid,
            main_gas,
            close_offset,
            open_offset,
            end_offset,
            lag_s,
            max_idx: 0.0, // Default value.
            gases: gastypes,
            calc_range_start: calc_range_start_map,
            calc_range_end: calc_range_end_map,
            // The following fields were not stored; use defaults.
            measurement_range_start: (start_time
                + TimeDelta::seconds(close_offset)
                + TimeDelta::seconds(lag_s as i64))
            .timestamp() as f64,
            measurement_range_end: (start_time
                + TimeDelta::seconds(close_offset)
                + TimeDelta::seconds(lag_s as i64))
            .timestamp() as f64,
            slope: slope_map,
            flux,
            measurement_r2,
            calc_r2,
            // Other fields (dt_v, calc_dt_v, etc.) can be initialized as needed.
        }))
    })?;

    // let cycles: Vec<Cycle> = cycle_iter.filter_map(|res| res.ok().flatten()).collect();
    // let cycles: Result<Vec<Cycle>> = cycle_iter.collect();
    // let cycles: Vec<Cycle> =
    //     cycle_iter.collect::<Result<Vec<_>, _>>()?.into_iter().flatten().collect();
    let cycles: Vec<Cycle> =
        cycle_iter.collect::<Result<Vec<_>, _>>()?.into_iter().flatten().collect();
    if cycles.is_empty() {
        // return Err("No cycles found".into());
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(cycles)
}

fn filter_diag_data(
    datetimes: &[DateTime<Utc>],
    diag: &[i64],
    range_start: f64,
    range_end: f64,
) -> (Vec<DateTime<Utc>>, Vec<i64>) {
    datetimes
        .iter()
        .zip(diag.iter())
        .filter(|(dt, _)| {
            let t = dt.timestamp() as f64;
            t >= range_start && t <= range_end
        })
        .map(|(dt, &d)| (dt.clone(), d))
        .unzip()
}
fn filter_data_in_range(
    datetimes: &[DateTime<Utc>],
    values: &[f64],
    range_start: f64,
    range_end: f64,
) -> (Vec<DateTime<Utc>>, Vec<f64>) {
    // Zip the datetimes and values, filter by comparing each datetime's timestamp
    // to the given range, and then unzip the filtered pairs.
    datetimes
        .iter()
        .zip(values.iter())
        .filter(|(dt, _)| {
            let t = dt.timestamp() as f64;
            t >= range_start && t <= range_end
        })
        .map(|(dt, &v)| (dt.clone(), v))
        .unzip()
}

pub fn calculate_max_y_from_vec(values: &[f64]) -> f64 {
    values.iter().copied().filter(|v| !v.is_nan()).fold(f64::NEG_INFINITY, f64::max)
}

pub fn calculate_min_y_from_vec(values: &[f64]) -> f64 {
    values.iter().copied().filter(|v| !v.is_nan()).fold(f64::INFINITY, f64::min)
}

pub fn insert_meteo_data(
    conn: &mut Connection,
    project_id: &str,
    meteo_data: &MeteoData,
) -> Result<()> {
    if meteo_data.datetime.len() != meteo_data.temperature.len()
        || meteo_data.datetime.len() != meteo_data.pressure.len()
    {
        return Err(rusqlite::Error::InvalidQuery); // Ensure all arrays have the same length
    }

    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO meteo (project_id, datetime, temperature, pressure)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(datetime, project_id)
             DO UPDATE SET temperature = excluded.temperature, pressure = excluded.pressure",
        )?;

        for i in 0..meteo_data.datetime.len() {
            let datetime = meteo_data.datetime[i];
            let temperature = meteo_data.temperature[i];
            let pressure = meteo_data.pressure[i];

            stmt.execute(params![project_id, datetime, temperature, pressure])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn insert_volume_data(
    conn: &mut Connection,
    project_id: &str,
    volume_data: &VolumeData,
) -> Result<()> {
    if volume_data.datetime.len() != volume_data.chamber_id.len()
        || volume_data.datetime.len() != volume_data.volume.len()
    {
        return Err(rusqlite::Error::InvalidQuery); // Ensure all vectors have the same length
    }

    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO volume (chamber_id, project_id, datetime, volume)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(chamber_id, project_id, datetime)
             DO UPDATE SET volume = excluded.volume",
        )?;

        for i in 0..volume_data.datetime.len() {
            stmt.execute(params![
                &volume_data.chamber_id[i],
                project_id,
                volume_data.datetime[i],
                volume_data.volume[i]
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn get_nearest_meteo_data(conn: &Connection, project: String, time: i64) -> Result<(f64, f64)> {
    let mut stmt = conn.prepare(
        "SELECT temperature, pressure
             FROM meteo
             WHERE project_id = ?1
             ORDER BY ABS(datetime - ?2)
             LIMIT 1",
    )?;

    let result = stmt.query_row(params![&project, time], |row| Ok((row.get(0)?, row.get(1)?)));

    match result {
        Ok((temperature, pressure)) => Ok((temperature, pressure)),
        Err(_) => Ok((0.0, 0.0)), // Return defaults if no data is found
    }
}
pub fn query_meteo(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: String,
) -> Result<MeteoData> {
    // let mut data = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT datetime, temperature, pressure
             FROM meteo
             WHERE datetime BETWEEN ?1 AND ?2
             and project_id = ?3
             ORDER BY datetime",
    )?;

    let rows = stmt.query_map(
        params![start.timestamp() - 86400, end.timestamp() + 86400, project],
        |row| {
            let datetime_unix: i64 = row.get(0)?;
            let temp: f64 = row.get(1)?;
            let press: f64 = row.get(2)?;

            Ok((datetime_unix, temp, press))
        },
    )?;

    let mut meteos = MeteoData::default();
    for row in rows {
        let (time, temp, press) = row?;
        meteos.datetime.push(time);
        meteos.temperature.push(temp);
        meteos.pressure.push(press);
    }
    Ok(meteos)
}
