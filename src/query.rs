use crate::instruments::GasType;
use crate::structs::{GasData, TimeData};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use csv::StringRecord;
use rusqlite::{params, Connection, Result};
use std::collections::HashMap;

pub fn query_cycles(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<TimeData> {
    println!("Querying cycles");
    println!("{}", start);
    println!("{}", end);
    let mut stmt = conn.prepare(
        "SELECT chamber_id, start_time, close_offset, open_offset, end_offset, site
         FROM cycles
         WHERE start_time BETWEEN ?1 AND ?2",
    )?;

    let mut times = TimeData::new();
    let cycle_iter = stmt.query_map(params![start.timestamp(), end.timestamp()], |row| {
        let chamber_id: String = row.get(0)?;
        let start_timestamp: i64 = row.get(1)?; // Start time as UNIX timestamp
        let close_offset: i64 = row.get(2)?;
        let open_offset: i64 = row.get(3)?;
        let end_offset: i64 = row.get(4)?;

        let start_time = DateTime::<Utc>::from_utc(
            NaiveDateTime::from_timestamp_opt(start_timestamp, 0).expect("Invalid timestamp"),
            Utc,
        );

        times.chamber_id.push(chamber_id);
        times.start_time.push(start_time);
        times.close_offset.push(close_offset);
        times.open_offset.push(open_offset);
        times.end_offset.push(end_offset);

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
) -> Result<HashMap<String, GasData>> {
    // let mut data = HashMap::new();
    println!("Querying gas data");
    let mut grouped_data: HashMap<String, GasData> = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT datetime, ch4, co2, h2o, n2o, diag FROM measurements
             WHERE datetime BETWEEN ?1 AND ?2
             ORDER BY datetime",
    )?;

    let rows = stmt.query_map(params![start.timestamp(), end.timestamp()], |row| {
        let datetime_unix: i64 = row.get(0)?;
        let ch4: Option<f64> = row.get(1)?; // Handle NULL values
        let co2: Option<f64> = row.get(2)?;
        let h2o: Option<f64> = row.get(3)?;
        let n2o: Option<f64> = row.get(4)?;
        let diag: i64 = row.get(5)?;

        // Convert UNIX timestamp to DateTime<Utc>
        let naive_datetime =
            NaiveDateTime::from_timestamp_opt(datetime_unix, 0).expect("Invalid timestamp");
        let datetime_utc = DateTime::<Utc>::from_utc(naive_datetime, Utc);

        Ok((datetime_utc, ch4, co2, h2o, n2o, diag))
    })?;

    for row in rows {
        let (datetime, ch4, co2, h2o, n2o, diag) = row?;

        // ✅ Extract YYYY-MM-DD for grouping
        let date_key = datetime.format("%Y-%m-%d").to_string();

        // ✅ Get or create a new GasData entry
        let entry = grouped_data
            .entry(date_key.clone())
            .or_insert_with(|| GasData {
                header: StringRecord::new(),
                datetime: Vec::new(),
                gas: HashMap::new(),
                diag: Vec::new(),
            });

        // ✅ Append values
        entry.datetime.push(datetime);
        entry.diag.push(diag);

        // ✅ Store each gas type in the `HashMap`
        if let Some(v) = ch4 {
            entry
                .gas
                .entry(GasType::CH4)
                .or_insert_with(Vec::new)
                .push(v);
        }
        if let Some(v) = co2 {
            entry
                .gas
                .entry(GasType::CO2)
                .or_insert_with(Vec::new)
                .push(v);
        }
        if let Some(v) = h2o {
            entry
                .gas
                .entry(GasType::H2O)
                .or_insert_with(Vec::new)
                .push(v);
        }
        if let Some(v) = n2o {
            entry
                .gas
                .entry(GasType::N2O)
                .or_insert_with(Vec::new)
                .push(v);
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
            site TEXT NOT NULL
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
