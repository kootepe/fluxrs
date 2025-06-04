pub mod fluxes_col {
    pub const START_TIME: usize = 0;
    pub const CHAMBER_ID: usize = 1;
    pub const INSTRUMENT_MODEL: usize = 2;
    pub const INSTRUMENT_SERIAL: usize = 3;
    pub const MAIN_GAS: usize = 4;
    pub const GAS: usize = 5;
    pub const PROJECT_ID: usize = 6;
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
    pub const CHAMBER_VOLUME: usize = 17;
    pub const ERROR_CODE: usize = 18;
    pub const IS_VALID: usize = 19;
    pub const MANUAL_ADJUSTED: usize = 20;
    pub const MANUAL_VALID: usize = 21;
    pub const T0_CONC: usize = 22;
    pub const MEASUREMENT_R2: usize = 22;
    pub const FLUX: usize = 24;
    pub const R2: usize = 25;
    pub const INTERCEPT: usize = 26;
    pub const SLOPE: usize = 27;
    pub const CALC_START: usize = 28;
    pub const CALC_END: usize = 29;
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
    "end_lag_s",
    "start_lag_s",
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
    "gas",
    "project_id",
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
    "chamber_volume",
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
        "instrument_serial = ?{} AND start_time = ?{} AND gas = ?{} AND project_id = ?{}",
        fluxes_col::INSTRUMENT_SERIAL + 1,
        fluxes_col::START_TIME + 1,
        fluxes_col::GAS + 1,
        fluxes_col::PROJECT_ID + 1,
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
            start_time INTEGER NOT NULL,
            chamber_id TEXT NOT NULL,
            instrument_model TEXT NOT NULL,
            instrument_serial TEXT NOT NULL,
            main_gas INTEGER NOT NULL,
            gas INTEGER NOT NULL,
            project_id TEXT NOT NULL,

            close_offset INTEGER NOT NULL,
            open_offset INTEGER NOT NULL,
            end_offset INTEGER NOT NULL,
            open_lag_s INTEGER NOT NULL,
            close_lag_s INTEGER NOT NULL,
            end_lag_s INTEGER NOT NULL,
            start_lag_s INTEGER NOT NULL,
            min_calc_len INTEGER NOT NULL,
            air_pressure FLOAT,
            air_temperature FLOAT,
            chamber_volume FLOAT,

            error_code INTEGER,
            measurement_is_valid BOOL,
            gas_is_valid BOOL,
            manual_adjusted BOOL NOT NULL,
            manual_valid bool NOT NULL,
            deadband FLOAT,
            t0_concentration FLOAT,
            measurement_r2 FLOAT,

            lin_flux FLOAT,
            lin_r2 FLOAT,
            lin_adj_r2 FLOAT,
            lin_intercept FLOAT,
            lin_slope FLOAT,
            lin_sigma FLOAT,
            lin_p_value FLOAT,
            lin_aic FLOAT,
            lin_rmse FLOAT,
            lin_range_start FLOAT,
            lin_range_end FLOAT,

            poly_flux FLOAT,
            poly_r2 FLOAT,
            poly_adj_r2 FLOAT,
            poly_sigma FLOAT,
            poly_aic FLOAT,
            poly_rmse FLOAT,
            poly_a0 FLOAT,
            poly_a1 FLOAT,
            poly_a2 FLOAT,
            poly_range_start FLOAT,
            poly_range_end FLOAT,

            roblin_flux FLOAT,
            roblin_r2 FLOAT,
            roblin_adj_r2 FLOAT,
            roblin_intercept FLOAT,
            roblin_slope FLOAT,
            roblin_sigma FLOAT,
            roblin_aic FLOAT,
            roblin_rmse FLOAT,
            roblin_range_start FLOAT,
            roblin_range_end FLOAT,
            PRIMARY KEY (instrument_serial, start_time, project_id, gas)
        )"
    .to_owned()
}

pub fn create_flux_history_table() -> String {
    "CREATE TABLE IF NOT EXISTS flux_history (
            archived_at TEXT NOT NULL,

            instrument_model TEXT NOT NULL,
            instrument_serial TEXT NOT NULL,
            chamber_id TEXT NOT NULL,
            main_gas INTEGER NOT NULL,
            gas INTEGER NOT NULL,
            project_id TEXT NOT NULL,
            start_time INTEGER NOT NULL,

            close_offset INTEGER NOT NULL,
            open_offset INTEGER NOT NULL,
            end_offset INTEGER NOT NULL,
            open_lag_s INTEGER NOT NULL,
            close_lag_s INTEGER NOT NULL,
            end_lag_s INTEGER NOT NULL,
            start_lag_s INTEGER NOT NULL,
            min_calc_len INTEGER NOT NULL,
            air_pressure FLOAT,
            air_temperature FLOAT,
            chamber_volume FLOAT,

            error_code INTEGER,
            measurement_is_valid BOOL,
            gas_is_valid BOOL,
            manual_adjusted BOOL NOT NULL,
            manual_valid bool NOT NULL,
            deadband FLOAT,
            t0_concentration FLOAT,
            measurement_r2 FLOAT,

            lin_flux FLOAT,
            lin_r2 FLOAT,
            lin_adj_r2 FLOAT,
            lin_intercept FLOAT,
            lin_slope FLOAT,
            lin_sigma FLOAT,
            lin_p_value FLOAT,
            lin_aic FLOAT,
            lin_rmse FLOAT,
            lin_range_start FLOAT,
            lin_range_end FLOAT,

            poly_flux FLOAT,
            poly_r2 FLOAT,
            poly_adj_r2 FLOAT,
            poly_sigma FLOAT,
            poly_aic FLOAT,
            poly_rmse FLOAT,
            poly_a0 FLOAT,
            poly_a1 FLOAT,
            poly_a2 FLOAT,
            poly_range_start FLOAT,
            poly_range_end FLOAT,

            roblin_flux FLOAT,
            roblin_r2 FLOAT,
            roblin_adj_r2 FLOAT,
            roblin_intercept FLOAT,
            roblin_slope FLOAT,
            roblin_sigma FLOAT,
            roblin_aic FLOAT,
            roblin_rmse FLOAT,
            roblin_range_start FLOAT,
            roblin_range_end FLOAT
        )"
    .to_owned()
}
