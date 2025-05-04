use chrono::Utc;

pub mod fluxes_col {
    pub const START_TIME: usize = 0;
    pub const CHAMBER_ID: usize = 1;
    pub const INSTRUMENT_MODEL: usize = 2;
    pub const INSTRUMENT_SERIAL: usize = 3;
    pub const MAIN_GAS: usize = 4;
    pub const PROJECT_ID: usize = 5;

    pub const CLOSE_OFFSET: usize = 6;
    pub const OPEN_OFFSET: usize = 7;
    pub const END_OFFSET: usize = 8;
    pub const OPEN_LAG_S: usize = 9;
    pub const CLOSE_LAG_S: usize = 10;

    pub const AIR_PRESSURE: usize = 11;
    pub const AIR_TEMPERATURE: usize = 12;
    pub const CHAMBER_VOLUME: usize = 13;

    pub const ERROR_CODE: usize = 14;
    pub const IS_VALID: usize = 15;
    pub const MAIN_GAS_R2: usize = 16;

    pub const CH4_FLUX: usize = 17;
    pub const CH4_R2: usize = 18;
    pub const CH4_MEASUREMENT_R2: usize = 19;
    pub const CH4_SLOPE: usize = 20;
    pub const CH4_CALC_START: usize = 21;
    pub const CH4_CALC_END: usize = 22;

    pub const CO2_FLUX: usize = 23;
    pub const CO2_R2: usize = 24;
    pub const CO2_MEASUREMENT_R2: usize = 25;
    pub const CO2_SLOPE: usize = 26;
    pub const CO2_CALC_START: usize = 27;
    pub const CO2_CALC_END: usize = 28;

    pub const H2O_FLUX: usize = 29;
    pub const H2O_R2: usize = 30;
    pub const H2O_MEASUREMENT_R2: usize = 31;
    pub const H2O_SLOPE: usize = 32;
    pub const H2O_CALC_START: usize = 33;
    pub const H2O_CALC_END: usize = 34;

    pub const N2O_FLUX: usize = 35;
    pub const N2O_R2: usize = 36;
    pub const N2O_MEASUREMENT_R2: usize = 37;
    pub const N2O_SLOPE: usize = 38;
    pub const N2O_CALC_START: usize = 39;
    pub const N2O_CALC_END: usize = 40;

    pub const MANUAL_ADJUSTED: usize = 41;
    pub const MANUAL_VALID: usize = 42;
}

pub const OTHER_COLS: &[&str] = &[
    "instrument_model",
    "instrument_serial",
    "chamber_id",
    "main_gas",
    "project_id",
    "start_time",
    "close_offset",
    "open_offset",
    "end_offset",
    "open_lag_s",
    "close_lag_s",
    "air_pressure",
    "air_temperature",
    "chamber_volume",
    "error_code",
    "is_valid",
    "main_gas_r2",
    "manual_adjusted",
    "manual_valid",
];

pub const FLUXES_COLUMNS: &[&str] = &[
    "start_time",
    "chamber_id",
    "instrument_model",
    "instrument_serial",
    "main_gas",
    "project_id",
    "close_offset",
    "open_offset",
    "end_offset",
    "open_lag_s",
    "close_lag_s",
    "air_pressure",
    "air_temperature",
    "chamber_volume",
    "error_code",
    "is_valid",
    "main_gas_r2",
    "ch4_flux",
    "ch4_r2",
    "ch4_measurement_r2",
    "ch4_slope",
    "ch4_calc_range_start",
    "ch4_calc_range_end",
    "co2_flux",
    "co2_r2",
    "co2_measurement_r2",
    "co2_slope",
    "co2_calc_range_start",
    "co2_calc_range_end",
    "h2o_flux",
    "h2o_r2",
    "h2o_measurement_r2",
    "h2o_slope",
    "h2o_calc_range_start",
    "h2o_calc_range_end",
    "n2o_flux",
    "n2o_r2",
    "n2o_measurement_r2",
    "n2o_slope",
    "n2o_calc_range_start",
    "n2o_calc_range_end",
    "manual_adjusted",
    "manual_valid",
];
pub fn make_select_all_fluxes() -> String {
    format!(
        "SELECT {} FROM fluxes WHERE project_id = ?1 ORDER BY start_time",
        FLUXES_COLUMNS.join(", ")
    )
}

pub fn make_select_fluxes() -> String {
    format!(
        "SELECT {} FROM fluxes WHERE start_time BETWEEN ?1 AND ?2 AND project_id = ?3 ORDER BY start_time",
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
        "instrument_serial = ?{} AND start_time = ?{}",
        fluxes_col::INSTRUMENT_SERIAL + 1,
        fluxes_col::START_TIME + 1
    );

    format!("UPDATE fluxes SET {} WHERE {}", set_clause.join(", "), where_clause)
}

pub fn create_flux_table() -> String {
    "CREATE TABLE IF NOT EXISTS fluxes (
            instrument_model TEXT NOT NULL,
            instrument_serial TEXT NOT NULL,
            chamber_id TEXT NOT NULL,
            main_gas TEXT NOT NULL,
            project_id TEXT NOT NULL,
            start_time INTEGER NOT NULL,

            close_offset INTEGER NOT NULL,
            open_offset INTEGER NOT NULL,
            end_offset INTEGER NOT NULL,
            open_lag_s INTEGER NOT NULL,
            close_lag_s INTEGER NOT NULL,
            air_pressure FLOAT,
            air_temperature FLOAT,

            error_code INTEGER,
            is_valid BOOL,
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

            manual_adjusted BOOL NOT NULL,
            manual_valid bool NOT NULL,
            chamber_volume FLOAT,
            PRIMARY KEY (instrument_serial, start_time, project_id)
        )"
    .to_owned()
}

pub fn create_flux_history_table() -> String {
    "CREATE TABLE IF NOT EXISTS flux_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,

            archived_at TEXT NOT NULL,
            instrument_model TEXT NOT NULL,
            instrument_serial TEXT NOT NULL,
            chamber_id TEXT NOT NULL,
            main_gas TEXT NOT NULL,
            project_id TEXT NOT NULL,
            start_time INTEGER NOT NULL,

            close_offset INTEGER NOT NULL,
            open_offset INTEGER NOT NULL,
            end_offset INTEGER NOT NULL,
            open_lag_s INTEGER NOT NULL,
            close_lag_s INTEGER NOT NULL,
            air_pressure FLOAT,
            air_temperature FLOAT,

            error_code INTEGER,
            is_valid BOOL,
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

            manual_adjusted BOOL NOT NULL,
            manual_valid bool NOT NULL,
            chamber_volume FLOAT
        )"
    .to_owned()
}
