use rusqlite::{Connection, OptionalExtension, Result};

const DB_VERSION: i32 = 1;

pub mod fluxes_col {
    pub const START_TIME: usize = 0;
    pub const CHAMBER_ID: usize = 1;
    pub const MAIN_INSTRUMENT_LINK: usize = 2;
    pub const INSTRUMENT_LINK: usize = 3;
    pub const MAIN_GAS: usize = 4;
    pub const GAS: usize = 5;
    pub const PROJECT_LINK: usize = 6;
    pub const CLOSE_OFFSET: usize = 7;
    pub const OPEN_OFFSET: usize = 8;
    pub const END_OFFSET: usize = 9;
    pub const OPEN_LAG_S: usize = 10;
    pub const CLOSE_LAG_S: usize = 11;
    pub const END_LAG_S: usize = 12;
    pub const START_LAG_S: usize = 13;
    pub const MIN_CALC_LEN: usize = 14;
    pub const AIR_PRESSURE: usize = 15;
    pub const AIR_TEMPERATURE: usize = 16;
    pub const CHAMBER_HEIGHT: usize = 17;
    pub const ERROR_CODE: usize = 18;
    pub const IS_VALID: usize = 19;
    pub const MANUAL_ADJUSTED: usize = 20;
    pub const MANUAL_VALID: usize = 21;
    pub const T0_CONC: usize = 22;
    pub const MEASUREMENT_R2: usize = 23;
    pub const FLUX: usize = 24;
    pub const R2: usize = 25;
    pub const INTERCEPT: usize = 26;
    pub const SLOPE: usize = 27;
    pub const CALC_START: usize = 28;
    pub const CALC_END: usize = 29;
}

pub const OTHER_COLS: &[&str] = &[
    "chamber_id",
    "main_gas",
    "project_link",
    "start_time",
    "close_offset",
    "open_offset",
    "end_offset",
    "open_lag_s",
    "close_lag_s",
    "end_lag_s",
    "start_lag_s",
    "air_pressure",
    "air_temperature",
    "chamber_height",
    "error_code",
    "is_valid",
    "main_gas_r2",
    "manual_adjusted",
    "manual_valid",
];

pub const FLUXES_COLUMNS: &[&str] = &[
    "start_time",
    "chamber_id",
    "main_instrument_link",
    "instrument_link",
    "main_gas",
    "gas",
    "project_link",
    "cycle_link",
    "close_offset",
    "open_offset",
    "end_offset",
    "open_lag_s",
    "close_lag_s",
    "end_lag_s",
    "start_lag_s",
    "min_calc_len",
    "air_pressure",
    "air_temperature",
    "chamber_height",
    "snow_depth_m",
    "error_code",
    "measurement_is_valid",
    "gas_is_valid",
    "manual_adjusted",
    "manual_valid",
    "deadband",
    "t0_concentration",
    "measurement_r2",
    "lin_flux",
    "lin_r2",
    "lin_adj_r2",
    "lin_intercept",
    "lin_slope",
    "lin_sigma",
    "lin_p_value",
    "lin_aic",
    "lin_rmse",
    "lin_range_start",
    "lin_range_end",
    "poly_flux",
    "poly_r2",
    "poly_adj_r2",
    "poly_sigma",
    "poly_aic",
    "poly_rmse",
    "poly_a0",
    "poly_a1",
    "poly_a2",
    "poly_range_start",
    "poly_range_end",
    "roblin_flux",
    "roblin_r2",
    "roblin_adj_r2",
    "roblin_intercept",
    "roblin_slope",
    "roblin_sigma",
    "roblin_aic",
    "roblin_rmse",
    "roblin_range_start",
    "roblin_range_end",
];
pub const FLUXES_COLUMNS_NO_LINK: &[&str] = &[
    "start_time",
    "chamber_id",
    "main_gas",
    "gas",
    "close_offset",
    "open_offset",
    "end_offset",
    "open_lag_s",
    "close_lag_s",
    "end_lag_s",
    "start_lag_s",
    "min_calc_len",
    "air_pressure",
    "air_temperature",
    "chamber_height",
    "snow_depth_m",
    "error_code",
    "measurement_is_valid",
    "gas_is_valid",
    "manual_adjusted",
    "manual_valid",
    "deadband",
    "t0_concentration",
    "measurement_r2",
    "lin_flux",
    "lin_r2",
    "lin_adj_r2",
    "lin_intercept",
    "lin_slope",
    "lin_sigma",
    "lin_p_value",
    "lin_aic",
    "lin_rmse",
    "lin_range_start",
    "lin_range_end",
    "poly_flux",
    "poly_r2",
    "poly_adj_r2",
    "poly_sigma",
    "poly_aic",
    "poly_rmse",
    "poly_a0",
    "poly_a1",
    "poly_a2",
    "poly_range_start",
    "poly_range_end",
    "roblin_flux",
    "roblin_r2",
    "roblin_adj_r2",
    "roblin_intercept",
    "roblin_slope",
    "roblin_sigma",
    "roblin_aic",
    "roblin_rmse",
    "roblin_range_start",
    "roblin_range_end",
];
pub fn make_select_all_fluxes() -> String {
    format!(
        "SELECT {},
        main_i.instrument_model     AS main_instrument_model,
        main_i.instrument_serial    AS main_instrument_serial,
        i.instrument_model          AS instrument_model,
        i.instrument_serial         AS instrument_serial

        FROM fluxes f

        LEFT JOIN instruments main_i ON f.main_instrument_link = main_i.id
        LEFT JOIN instruments i ON f.instrument_link = i.id

        WHERE f.project_link = ?1
        ORDER BY start_time",
        FLUXES_COLUMNS_NO_LINK.join(", ")
    )
}

pub fn make_select_fluxes() -> String {
    format!(
        "SELECT {}
        FROM fluxes
        WHERE start_time BETWEEN ?1 AND ?2
        AND project_link = ?3
        ORDER BY start_time",
        FLUXES_COLUMNS.join(", ")
    )
}

pub fn make_insert_or_ignore_fluxes() -> String {
    let columns = FLUXES_COLUMNS.join(", ");
    let placeholders =
        (1..=FLUXES_COLUMNS.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
    format!("INSERT OR IGNORE INTO fluxes ({columns}) VALUES ({placeholders})")
}

pub fn make_insert_fluxes() -> String {
    let placeholders: Vec<String> = (1..=FLUXES_COLUMNS.len()).map(|i| format!("?{}", i)).collect();
    format!(
        "INSERT INTO fluxes ({}) VALUES ({})",
        FLUXES_COLUMNS.join(", "),
        placeholders.join(", ")
    )
}

pub fn make_insert_flux_history() -> String {
    // Total columns = archived_at + flux columns
    let mut columns = vec!["archived_at"];
    columns.extend(FLUXES_COLUMNS);

    let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("?{}", i)).collect();

    format!(
        "INSERT INTO flux_history ({}) VALUES ({})",
        columns.join(", "),
        placeholders.join(", ")
    )
}

pub fn make_update_fluxes() -> String {
    let set_clause: Vec<String> =
        FLUXES_COLUMNS.iter().enumerate().map(|(i, col)| format!("{col} = ?{}", i + 1)).collect();

    // Add WHERE clause for identifying row
    let where_clause = format!(
        "instrument_link = ?{} AND start_time = ?{} AND gas = ?{} AND project_link = ?{}",
        fluxes_col::INSTRUMENT_LINK + 1,
        fluxes_col::START_TIME + 1,
        fluxes_col::GAS + 1,
        fluxes_col::PROJECT_LINK + 1,
    );

    format!("UPDATE fluxes SET {} WHERE {}", set_clause.join(", "), where_clause)
}

pub fn make_insert_flux_results() -> String {
    "INSERT INTO flux_results (
        cycle_id, fit_id, gas_type,
        flux, r2, intercept, slope,
        range_start, range_end
    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        .to_owned()
}

pub fn create_flux_results_table() -> String {
    "CREATE TABLE flux_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cycle_id INTEGER NOT NULL, -- FK to fluxes.id
    gas_type TEXT NOT NULL,
    fit_id TEXT NOT NULL, -- e.g., \"linear\", \"robust\", etc.
    flux FLOAT NOT NULL,
    r2 FLOAT,
    intercept FLOAT,
    slope FLOAT,
    range_start INTEGER,
    range_end INTEGER,


    FOREIGN KEY (cycle_id) REFERENCES fluxes(id) ON DELETE CASCADE,
    UNIQUE (cycle_id, gas_type, fit_id)
);"
    .to_owned()
}
pub fn create_flux_table() -> String {
    "CREATE TABLE IF NOT EXISTS fluxes (
            id						INTEGER PRIMARY KEY,
            start_time				INTEGER NOT NULL,
            chamber_id				TEXT NOT NULL,
            main_instrument_link	INTEGER NOT NULL,
            instrument_link			INTEGER NOT NULL,
            main_gas				INTEGER NOT NULL,
            gas						INTEGER NOT NULL,
            project_link			INTEGER NOT NULL,
            cycle_link				INTEGER NOT NULL,

            close_offset			INTEGER NOT NULL,
            open_offset				INTEGER NOT NULL,
            end_offset				INTEGER NOT NULL,
            open_lag_s				INTEGER NOT NULL,
            close_lag_s				INTEGER NOT NULL,
            end_lag_s				INTEGER NOT NULL,
            start_lag_s				INTEGER NOT NULL,
            min_calc_len			INTEGER NOT NULL,
            air_pressure			FLOAT,
            air_temperature			FLOAT,
            chamber_height			FLOAT,
            snow_depth_m			FLOAT,

            error_code				INTEGER,
            measurement_is_valid	BOOL,
            gas_is_valid			BOOL,
            manual_adjusted			BOOL NOT NULL,
            manual_valid			BOOL NOT NULL,
            deadband				FLOAT,
            t0_concentration		FLOAT,
            measurement_r2			FLOAT,

            lin_flux				FLOAT,
            lin_r2					FLOAT,
            lin_adj_r2				FLOAT,
            lin_intercept			FLOAT,
            lin_slope				FLOAT,
            lin_sigma				FLOAT,
            lin_p_value				FLOAT,
            lin_aic					FLOAT,
            lin_rmse				FLOAT,
            lin_range_start			FLOAT,
            lin_range_end			FLOAT,

            poly_flux				FLOAT,
            poly_r2					FLOAT,
            poly_adj_r2				FLOAT,
            poly_sigma				FLOAT,
            poly_aic				FLOAT,
            poly_rmse				FLOAT,
            poly_a0					FLOAT,
            poly_a1					FLOAT,
            poly_a2					FLOAT,
            poly_range_start		FLOAT,
            poly_range_end			FLOAT,

            roblin_flux				FLOAT,
            roblin_r2				FLOAT,
            roblin_adj_r2			FLOAT,
            roblin_intercept		FLOAT,
            roblin_slope			FLOAT,
            roblin_sigma			FLOAT,
            roblin_aic				FLOAT,
            roblin_rmse				FLOAT,
            roblin_range_start		FLOAT,
            roblin_range_end		FLOAT,

            FOREIGN KEY (cycle_link) REFERENCES cycles(id) ON DELETE CASCADE.
            FOREIGN KEY (project_link) REFERENCES projects(id) ON DELETE CASCADE,
            FOREIGN KEY (instrument_link) REFERENCES instruments(id) ON DELETE CASCAD#,
            FOREIGN KEY (main_instrument_link) REFERENCES instruments(id),

            UNIQUE (instrument_link, start_time, project_link, gas)
        )"
    .to_owned()
}

pub fn create_flux_history_table() -> String {
    "CREATE TABLE IF NOT EXISTS flux_history (
            archived_at				TEXT NOT NULL,

            start_time				INTEGER NOT NULL,
            chamber_id				TEXT NOT NULL,
            main_instrument_link	INTEGER NOT NULL,
            instrument_link			INTEGER NOT NULL,
            main_gas				INTEGER NOT NULL,
            gas				        INTEGER NOT NULL,
            project_link			INTEGER NOT NULL,
            cycle_link				INTEGER NOT NULL,

            close_offset			INTEGER NOT NULL,
            open_offset				INTEGER NOT NULL,
            end_offset				INTEGER NOT NULL,
            open_lag_s				INTEGER NOT NULL,
            close_lag_s				INTEGER NOT NULL,
            end_lag_s				INTEGER NOT NULL,
            start_lag_s				INTEGER NOT NULL,
            min_calc_len			INTEGER NOT NULL,
            air_pressure			FLOAT,
            air_temperature			FLOAT,
            chamber_height			FLOAT,
            snow_depth_m			FLOAT,

            error_code				INTEGER,
            measurement_is_valid	BOOL,
            gas_is_valid			BOOL,
            manual_adjusted			BOOL NOT NULL,
            manual_valid			BOOL NOT NULL,
            deadband				FLOAT,
            t0_concentration		FLOAT,
            measurement_r2			FLOAT,

            lin_flux				FLOAT,
            lin_r2				    FLOAT,
            lin_adj_r2				FLOAT,
            lin_intercept			FLOAT,
            lin_slope				FLOAT,
            lin_sigma				FLOAT,
            lin_p_value				FLOAT,
            lin_aic				    FLOAT,
            lin_rmse				FLOAT,
            lin_range_start			FLOAT,
            lin_range_end			FLOAT,

            poly_flux				FLOAT,
            poly_r2				    FLOAT,
            poly_adj_r2				FLOAT,
            poly_sigma				FLOAT,
            poly_aic				FLOAT,
            poly_rmse				FLOAT,
            poly_a0				    FLOAT,
            poly_a1				    FLOAT,
            poly_a2				    FLOAT,
            poly_range_start		FLOAT,
            poly_range_end			FLOAT,

            roblin_flux				FLOAT,
            roblin_r2				FLOAT,
            roblin_adj_r2			FLOAT,
            roblin_intercept		FLOAT,
            roblin_slope			FLOAT,
            roblin_sigma			FLOAT,
            roblin_aic				FLOAT,
            roblin_rmse				FLOAT,
            roblin_range_start		FLOAT,
            roblin_range_end		FLOAT,

            FOREIGN KEY (cycle_link) REFERENCES cycles(id) ON DELETE CASCADE
            FOREIGN KEY (project_link) REFERENCES projects(id) ON DELETE CASCADE
            FOREIGN KEY (main_instrument_link) REFERENCES instruments(id)
            FOREIGN KEY (instrument_link) REFERENCES instruments(id)
        )"
    .to_owned()
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

pub fn migrate_db() -> Result<i32> {
    let conn = Connection::open("fluxrs.db")?;
    // user_version is 0 by default
    let current_version: i32 = conn.query_row("PRAGMA user_version;", [], |row| row.get(0))?;
    let mut migrated = 0;

    let has_col = column_exists(&conn, "projects", "tz")?;
    // this is a migration needed for couple of the first users... remove later
    if current_version == 7 && !has_col {
        println!("Applying migration 1: Setting user_version to 1");
        conn.execute(&format!("PRAGMA user_version = {};", DB_VERSION), [])?;
        conn.execute("ALTER TABLE projects ADD COLUMN tz TEXT NOT NULL DEFAULT 'UTC';", [])?;
        migrated = 1
    }

    Ok(migrated)
}
pub fn initiate_tables() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open("fluxrs.db")?;
    // conn.execute("PRAGMA journal_mode=WAL;", [])?;
    // let wal_mode: String = conn.query_row("PRAGMA journal_mode=WAL;", [], |row| row.get(0))?;

    conn.execute(&format!("PRAGMA user_version = {};", DB_VERSION), [])?;
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    // conn.execute("PRAGMA journal_mode = WAL;", [])?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS projects (
            id                      INTEGER PRIMARY KEY AUTOINCREMENT,
            project_name            TEXT NOT NULL,
            main_gas                INTEGER NOT NULL,
            mode                    INTEGER NOT NULL,
            deadband                FLOAT NOT NULL,
            min_calc_len            FLOAT NOT NULL,
            tz                      TEXT NOT NULL,
            current                 INTEGER NOT NULL,
            main_instrument_link    INTEGER,
            FOREIGN KEY (main_instrument_link) REFERENCES instruments(id)
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS instruments (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            instrument_model    TEXT NOT NULL,
            instrument_serial   TEXT NOT NULL,
            project_link        INTEGER NOT NULL,

            FOREIGN KEY (project_link) REFERENCES projects(id) ON DELETE CASCADE
            UNIQUE (project_link, instrument_model, instrument_serial)
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS measurements (
            datetime            INTEGER,
            ch4                 FLOAT,
            co2                 FLOAT,
            h2o                 FLOAT,
            n2o                 FLOAT,
            diag                INTEGER,
            file_link           INTEGER NOT NULL,
            project_link        INTEGER NOT NULL,
            instrument_link     INTEGER NOT NULL,

            FOREIGN KEY (instrument_link) REFERENCES instruments(id)
            FOREIGN KEY (file_link) REFERENCES data_files(id) ON DELETE CASCADE
            FOREIGN KEY (project_link) REFERENCES projects(id) ON DELETE CASCADE

            PRIMARY KEY (datetime, instrument_link, project_link)
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS cycles (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            chamber_id      TEXT NOT NULL,
            start_time      INTEGER NOT NULL,
            close_offset    INTEGER NOT NULL,
            open_offset     INTEGER NOT NULL,
            end_offset      INTEGER NOT NULL,
            snow_depth      FLOAT NOT NULL,
            file_link       INTEGER NOT NULL,
            project_link    INTEGER NOT NULL,
            instrument_link INTEGER NOT NULL,


            FOREIGN KEY (instrument_link) REFERENCES instruments(id)
            FOREIGN KEY (project_link) REFERENCES projects(id) ON DELETE CASCADE
            FOREIGN KEY (file_link) REFERENCES data_files(id) ON DELETE CASCADE

            UNIQUE (start_time, chamber_id, project_link)
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS meteo (
            datetime        INTEGER,
            temperature     FLOAT,
            pressure        FLOAT,
            file_link       INTEGER NOT NULL,
            project_link    INTEGER NOT NULL,

            FOREIGN KEY (project_link) REFERENCES projects(id) ON DELETE CASCADE
            FOREIGN KEY (file_link) REFERENCES data_files(id) ON DELETE CASCADE
            PRIMARY KEY (datetime, project_link)
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS height (
            chamber_id      TEXT,
            datetime        INTEGER,
            height          FLOAT,
            project_link    INTEGER NOT NULL,
            file_link       INTEGER NOT NULL,

            FOREIGN KEY (file_link) REFERENCES data_files(id) ON DELETE CASCADE
            FOREIGN KEY (project_link) REFERENCES projects(id) ON DELETE CASCADE

            PRIMARY KEY (chamber_id, project_link, datetime)
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE chamber_metadata (
            id              INTEGER PRIMARY KEY,
            chamber_id      TEXT NOT NULL,
            shape           TEXT NOT NULL,
            diameter        REAL,
            height          REAL NOT NULL,
            width           REAL,
            length          REAL,
            file_link       INTEGER NOT NULL,
            project_link    INTEGER NOT NULL,

            FOREIGN KEY (project_link) REFERENCES projects(id) ON DELETE CASCADE
            FOREIGN KEY (file_link) REFERENCES data_files(id) ON DELETE CASCADE

            UNIQUE(chamber_id, project_link)
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS data_files (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            file_name       TEXT NOT NULL,
            data_type       TEXT NOT NULL,
            uploaded_at     TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            project_link    INTEGER NOT NULL,

            FOREIGN KEY (project_link) REFERENCES projects(id) ON DELETE CASCADE
        )",
        [],
    )?;
    conn.execute(&create_flux_table(), [])?;
    conn.execute(&create_flux_history_table(), [])?;

    Ok(())
}
