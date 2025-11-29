use crate::db::fluxes_schema::DB_VERSION;
use rusqlite::{Connection, OptionalExtension, Result};

pub fn migrate_db() -> Result<()> {
    let conn = Connection::open("fluxrs.db")?;

    // user_version is 0 by default in SQLite
    let mut version: i32 = conn.query_row("PRAGMA user_version;", [], |row| row.get(0))?;
    let mut migrated_steps = 0;

    // --- Migration 2: add *_source columns ---
    if version < 2 {
        // fluxes.pressure_source
        let has_pressure_src_fluxes = column_exists(&conn, "fluxes", "pressure_source")?;
        if !has_pressure_src_fluxes {
            println!("Applying migration v2: add fluxes.pressure_source");
            conn.execute("ALTER TABLE fluxes ADD COLUMN pressure_source INTEGER;", [])?;
        }

        // flux_history.pressure_source
        let has_pressure_src_hist = column_exists(&conn, "flux_history", "pressure_source")?;
        if !has_pressure_src_hist {
            println!("Applying migration v2: add flux_history.pressure_source");
            conn.execute("ALTER TABLE flux_history ADD COLUMN pressure_source INTEGER;", [])?;
        }

        // fluxes.temperature_source
        let has_temp_src_fluxes = column_exists(&conn, "fluxes", "temperature_source")?;
        if !has_temp_src_fluxes {
            println!("Applying migration v2: add fluxes.temperature_source");
            conn.execute("ALTER TABLE fluxes ADD COLUMN temperature_source INTEGER;", [])?;
        }

        // flux_history.temperature_source
        let has_temp_src_hist = column_exists(&conn, "flux_history", "temperature_source")?;
        if !has_temp_src_hist {
            println!("Applying migration v2: add flux_history.temperature_source");
            conn.execute("ALTER TABLE flux_history ADD COLUMN temperature_source INTEGER;", [])?;
        }

        // Backfill pressure_source:
        // Default = 1  if value == 980.0
        // Raw     = 0  otherwise
        conn.execute(
            "UPDATE fluxes
             SET pressure_source = CASE
                 WHEN air_pressure = 980.0 THEN 1
                 ELSE 0
             END;",
            [],
        )?;
        conn.execute(
            "UPDATE flux_history
             SET pressure_source = CASE
                 WHEN air_pressure = 980.0 THEN 1
                 ELSE 0
             END;",
            [],
        )?;

        // Backfill temperature_source:
        // Default = 1  if value == 10.0
        // Raw     = 0  otherwise
        conn.execute(
            "UPDATE fluxes
             SET temperature_source = CASE
                 WHEN air_temperature = 10.0 THEN 1
                 ELSE 0
             END;",
            [],
        )?;
        conn.execute(
            "UPDATE flux_history
             SET temperature_source = CASE
                 WHEN air_temperature = 10.0 THEN 1
                 ELSE 0
             END;",
            [],
        )?;

        version = 2;
        migrated_steps += 1;
    }

    // --- Migration 3: add *_dist columns ---
    if version < 3 {
        // fluxes.pressure_dist
        let has_pressure_dist_fluxes = column_exists(&conn, "fluxes", "pressure_dist")?;
        if !has_pressure_dist_fluxes {
            println!("Applying migration v3: add fluxes.pressure_dist");
            conn.execute("ALTER TABLE fluxes ADD COLUMN pressure_dist INTEGER;", [])?;
        }

        // flux_history.pressure_dist
        let has_pressure_dist_hist = column_exists(&conn, "flux_history", "pressure_dist")?;
        if !has_pressure_dist_hist {
            println!("Applying migration v3: add flux_history.pressure_dist");
            conn.execute("ALTER TABLE flux_history ADD COLUMN pressure_dist INTEGER;", [])?;
        }

        // fluxes.temperature_dist
        let has_temp_dist_fluxes = column_exists(&conn, "fluxes", "temperature_dist")?;
        if !has_temp_dist_fluxes {
            println!("Applying migration v3: add fluxes.temperature_dist");
            conn.execute("ALTER TABLE fluxes ADD COLUMN temperature_dist INTEGER;", [])?;
        }

        // flux_history.temperature_dist
        let has_temp_dist_hist = column_exists(&conn, "flux_history", "temperature_dist")?;
        if !has_temp_dist_hist {
            println!("Applying migration v3: add flux_history.temperature_dist");
            conn.execute("ALTER TABLE flux_history ADD COLUMN temperature_dist INTEGER;", [])?;
        }

        // Backfill pressure_dist:
        // NULL if old default 980.0
        // 0    otherwise (treated as "0 seconds away")
        conn.execute(
            "UPDATE fluxes
             SET pressure_dist = CASE
                 WHEN air_pressure = 980.0 THEN NULL
                 ELSE 0
             END;",
            [],
        )?;
        conn.execute(
            "UPDATE flux_history
             SET pressure_dist = CASE
                 WHEN air_pressure = 980.0 THEN NULL
                 ELSE 0
             END;",
            [],
        )?;

        // Backfill temperature_dist:
        // NULL if old default 10.0
        // 0    otherwise
        conn.execute(
            "UPDATE fluxes
             SET temperature_dist = CASE
                 WHEN air_temperature = 10.0 THEN NULL
                 ELSE 0
             END;",
            [],
        )?;
        conn.execute(
            "UPDATE flux_history
             SET temperature_dist = CASE
                 WHEN air_temperature = 10.0 THEN NULL
                 ELSE 0
             END;",
            [],
        )?;

        version = 3;
        migrated_steps += 1;
    }
    if version < 4 {
        // 1. Create indexes on measurements
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_measurements_project_datetime
         ON measurements (project_link, datetime, instrument_link);",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_measurements_instrument_link
         ON measurements (instrument_link);",
            [],
        )?;

        // 2. Remove old offset columns from fluxes
        println!("Migration v4: removing close_offset, open_offset, end_offset from fluxes");
        conn.execute("ALTER TABLE fluxes DROP COLUMN close_offset;", [])?;
        conn.execute("ALTER TABLE fluxes DROP COLUMN open_offset;", [])?;
        conn.execute("ALTER TABLE fluxes DROP COLUMN end_offset;", [])?;

        // 3. Remove old offset columns from flux_history
        println!("Migration v4: removing close_offset, open_offset, end_offset from flux_history");
        conn.execute("ALTER TABLE flux_history DROP COLUMN close_offset;", [])?;
        conn.execute("ALTER TABLE flux_history DROP COLUMN open_offset;", [])?;
        conn.execute("ALTER TABLE flux_history DROP COLUMN end_offset;", [])?;

        version = 4;
        migrated_steps += 1;
    }

    // Only bump user_version once, at the end, to the *latest* schema version
    if migrated_steps > 0 {
        println!("Setting PRAGMA user_version = {}", DB_VERSION);
        conn.execute(&format!("PRAGMA user_version = {};", DB_VERSION), [])?;
    }

    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    // NOTE: the table name has to be a literal in the pragma call.
    // We safely embed it by escaping single quotes.
    fn esc(s: &str) -> String {
        s.replace('\'', "''")
    }

    let sql = format!("SELECT 1 FROM pragma_table_info('{}') WHERE name = ?1 LIMIT 1;", esc(table));
    conn.query_row(&sql, [column], |_| Ok(true)).optional().map(|opt| opt.unwrap_or(false))
}
