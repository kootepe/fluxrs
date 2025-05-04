use crate::cycle::Cycle;
use crate::fluxes_schema::make_insert_flux_history;
use crate::GasType;
use chrono::Utc;
use rusqlite::types::Value;

pub struct ArchiveRecord {
    pub sql: String,
    pub params: Vec<rusqlite::types::Value>,
}

impl Default for ArchiveRecord {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveRecord {
    pub fn new() -> Self {
        Self { sql: String::new(), params: Vec::new() }
    }
}
#[allow(clippy::vec_init_then_push)]
pub fn build_archive_record(cycle: &Cycle, project: &str) -> ArchiveRecord {
    let sql = make_insert_flux_history();
    let archived_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false);

    let mut params: Vec<rusqlite::types::Value> = vec![];
    params.push(archived_at.into());
    params.push(cycle.start_time.timestamp().into());
    params.push(cycle.chamber_id.clone().into());
    params.push(cycle.instrument_model.to_string().into());
    params.push(cycle.instrument_serial.clone().into());
    params.push(cycle.main_gas.to_string().into());
    params.push(project.to_string().into());
    params.push(cycle.close_offset.into());
    params.push(cycle.open_offset.into());
    params.push(cycle.end_offset.into());
    params.push((cycle.open_lag_s as i64).into());
    params.push(cycle.air_pressure.into());
    params.push(cycle.air_temperature.into());
    params.push(cycle.chamber_volume.into());
    params.push(cycle.error_code.0.into());
    params.push(cycle.is_valid.into());
    params.push(Value::from(*cycle.calc_r2.get(&cycle.main_gas).unwrap_or(&0.0)));

    for gas in &[GasType::CH4, GasType::CO2, GasType::H2O, GasType::N2O] {
        params.push(Value::from(*cycle.flux.get(gas).unwrap_or(&0.0)));
        params.push(Value::from(*cycle.calc_r2.get(gas).unwrap_or(&0.0)));
        params.push(Value::from(*cycle.measurement_r2.get(gas).unwrap_or(&0.0)));
        params.push(Value::from(*cycle.slope.get(gas).unwrap_or(&0.0)));
        params.push(Value::from(*cycle.calc_range_start.get(gas).unwrap_or(&0.0)));
        params.push(Value::from(*cycle.calc_range_end.get(gas).unwrap_or(&0.0)));
    }

    params.push(cycle.manual_adjusted.into());
    params.push(cycle.manual_valid.into());

    ArchiveRecord { sql, params }
}
