use crate::constants::MIN_CALC_AREA_RANGE;
use crate::errorcode::{ErrorCode, ErrorMask};
use crate::flux::{FluxKind, FluxModel, FluxRecord, LinearFlux, PolyFlux, RobustFlux};
use crate::fluxes_schema::{
    fluxes_col, make_insert_flux_history, make_insert_flux_results, make_insert_or_ignore_fluxes,
    make_select_fluxes, make_update_fluxes,
};
use crate::gasdata::{query_gas, query_gas2, query_gas_all};
use crate::instruments::GasType;
use crate::instruments::{get_instrument_by_model, InstrumentType};
use crate::processevent::{InsertEvent, ProcessEvent, ProgressEvent, QueryEvent, ReadEvent};
use crate::stats::{self, LinReg, PolyReg, RobReg};
use chrono::{DateTime, TimeDelta, Utc};
use eframe::glow::MIN;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rusqlite::{params, Connection, Error, Result};
use std::collections::{hash_map, HashMap};
use std::fmt;
use std::hash::Hash;
use tokio::sync::mpsc;

// the window of max r must be at least 240 seconds
pub const MIN_WINDOW_SIZE: usize = 180;
// how many seconds to increment the moving window searching for max r
pub const WINDOW_INCREMENT: usize = 5;

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
struct CycleKey {
    start_time: i64,
    instrument_serial: String,
    project_id: String,
    chamber_id: String,
}
#[derive(Clone)]
pub struct Cycle {
    pub id: i64,
    pub chamber_id: String,
    pub instrument_model: InstrumentType,
    pub instrument_serial: String,
    pub project_name: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub air_temperature: f64,
    pub air_pressure: f64,
    pub chamber_volume: f64,
    pub min_calc_range: f64,
    pub error_code: ErrorMask,
    pub is_valid: bool,
    pub gas_is_valid: HashMap<GasType, bool>,
    pub override_valid: Option<bool>,
    pub manual_valid: bool,
    pub main_gas: GasType,
    pub close_offset: i64,
    pub open_offset: i64,
    pub end_offset: i64,
    pub open_lag_s: f64,
    pub close_lag_s: f64,
    pub end_lag_s: f64,
    pub start_lag_s: f64,
    pub max_idx: f64,
    pub gases: Vec<GasType>,
    pub calc_range_start: HashMap<GasType, f64>,
    pub calc_range_end: HashMap<GasType, f64>,
    pub manual_adjusted: bool,
    pub min_y: HashMap<GasType, f64>,
    pub max_y: HashMap<GasType, f64>,
    pub flux: HashMap<GasType, f64>,
    pub linfit: HashMap<GasType, LinReg>,
    pub measurement_range_start: f64,
    pub measurement_range_end: f64,
    pub deadbands: HashMap<GasType, f64>,

    pub fluxes: HashMap<(GasType, FluxKind), FluxRecord>,
    pub measurement_r2: HashMap<GasType, f64>,
    pub calc_r2: HashMap<GasType, f64>,

    // datetime vectors
    // pub dt_v: Vec<chrono::DateTime<chrono::Utc>>,
    pub dt_v: Vec<f64>,
    // pub dt_v_f: Vec<f64>,
    pub calc_dt_v: HashMap<GasType, Vec<f64>>,
    pub measurement_dt_v: Vec<f64>,

    // gas vectors
    pub gas_v: HashMap<GasType, Vec<Option<f64>>>,
    pub gas_v_mole: HashMap<GasType, Vec<Option<f64>>>,
    pub calc_gas_v: HashMap<GasType, Vec<Option<f64>>>,
    pub measurement_gas_v: HashMap<GasType, Vec<Option<f64>>>,
    pub measurement_diag_v: Vec<i64>,
    pub t0_concentration: HashMap<GasType, f64>,

    pub diag_v: Vec<i64>,
}

impl fmt::Debug for Cycle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // let len: usize = self.measurement_dt_v.len();
        write!(
            f,
            // "Cycle id: {}, \nlag: {}, \nstart: {}, \nmeas_s: {}, \nmeas_e: {}",
            "start: {}",
            self.start_time,
        )
    }
}
impl Cycle {
    // pub fn _to_html_row(&self) -> Result<String, Box<dyn Error>> {
    //     let _plot_path = gas_plot::draw_gas_plot(self)?; // Call your plot function and get the path
    //     Ok(format!(
    //         "<tr>\
    //             <td>{}</td>\
    //             <td>{}</td>\
    //             <td>{}</td>\
    //             <td>{:.4}</td>\
    //             <td>{:.4}</td>\
    //         </tr>",
    //         self.chamber_id,
    //         self.start_time.to_rfc3339(),
    //         self.lag_s,
    //         self.r,
    //         self.flux
    //     ))
    // }

    pub fn get_is_valid(&self) -> bool {
        self.is_valid
    }
    pub fn get_start(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.start_lag_s
    }
    pub fn get_end(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.end_lag_s + self.end_offset as f64
    }

    pub fn get_start_no_lag(&self) -> f64 {
        self.start_time.timestamp() as f64
    }
    pub fn get_end_no_lag(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.end_offset as f64
    }

    pub fn get_close(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.close_offset as f64
    }
    pub fn get_open(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.open_offset as f64
    }
    pub fn get_lin_r2(&self, gas: GasType) -> Option<f64> {
        if let Some(flux) = self.fluxes.get(&(gas, FluxKind::Linear)) {
            return Some(flux.model.r2().unwrap_or(0.0));
        }
        None
    }
    pub fn get_lin_flux(&self, gas: GasType) -> Option<f64> {
        if let Some(flux) = self.fluxes.get(&(gas, FluxKind::Linear)) {
            return Some(flux.model.flux().unwrap_or(0.0));
        }
        None
    }
    pub fn get_lin_sigma(&self, gas: GasType) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(gas, FluxKind::Linear)) {
            return Some(model.model.sigma().unwrap_or(0.0));
        }
        None
    }
    pub fn get_lin_rmse(&self, gas: GasType) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(gas, FluxKind::Linear)) {
            return Some(model.model.rmse().unwrap_or(0.0));
        }
        None
    }
    pub fn get_lin_p_value(&self, gas: GasType) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(gas, FluxKind::Linear)) {
            return Some(model.model.p_value().unwrap_or(0.0));
        }
        None
    }
    pub fn get_roblin_flux(&self, gas: GasType) -> Option<f64> {
        if let Some(flux) = self.fluxes.get(&(gas, FluxKind::RobLin)) {
            return Some(flux.model.flux().unwrap_or(0.0));
        }
        None
    }
    pub fn get_roblin_sigma(&self, gas: GasType) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(gas, FluxKind::RobLin)) {
            return Some(model.model.sigma().unwrap_or(0.0));
        }
        None
    }
    pub fn get_roblin_rmse(&self, gas: GasType) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(gas, FluxKind::RobLin)) {
            return Some(model.model.rmse().unwrap_or(0.0));
        }
        None
    }
    pub fn get_poly_flux(&self, gas: GasType) -> Option<f64> {
        if let Some(flux) = self.fluxes.get(&(gas, FluxKind::Poly)) {
            return Some(flux.model.flux().unwrap_or(0.0));
        }
        None
    }
    pub fn get_poly_sigma(&self, gas: GasType) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(gas, FluxKind::Poly)) {
            return Some(model.model.sigma().unwrap_or(0.0));
        }
        None
    }
    pub fn get_poly_rmse(&self, gas: GasType) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(gas, FluxKind::Poly)) {
            return Some(model.model.rmse().unwrap_or(0.0));
        }
        None
    }
    pub fn get_flux(&self, gas: GasType, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(gas, kind)).and_then(|m| m.model.flux())
    }

    pub fn get_r2(&self, gas: GasType, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(gas, kind)).and_then(|m| m.model.r2())
    }

    pub fn get_adjusted_r2(&self, gas: GasType, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(gas, kind)).and_then(|m| m.model.adj_r2())
    }

    pub fn get_aic(&self, gas: GasType, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(gas, kind)).and_then(|m| m.model.aic())
    }

    pub fn get_p_value(&self, gas: GasType, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(gas, kind)).and_then(|m| m.model.p_value())
    }

    pub fn get_sigma(&self, gas: GasType, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(gas, kind)).and_then(|m| m.model.sigma())
    }

    pub fn get_rmse(&self, gas: GasType, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(gas, kind)).and_then(|m| m.model.rmse())
    }

    pub fn get_model(&self, gas_type: GasType, kind: FluxKind) -> Option<&dyn FluxModel> {
        self.fluxes.get(&(gas_type, kind)).map(|b| b.model.as_ref())
    }

    pub fn get_adjusted_close(&self) -> f64 {
        self.get_start() + self.close_offset as f64 + self.open_lag_s + self.close_lag_s
    }
    pub fn get_adjusted_open(&self) -> f64 {
        self.get_start() + self.open_offset as f64 + self.open_lag_s
    }
    // pub fn get_slope(&self, gas_type: &GasType) -> f64 {
    //     self.linfit.get(gas_type).unwrap().slope
    // }
    pub fn get_intercept(&self, gas: GasType, kind: FluxKind) -> Option<f64> {
        if let Some(flux) = self.fluxes.get(&(gas, kind)) {
            return Some(flux.model.intercept().unwrap());
        }

        None
    }
    pub fn get_slope(&self, gas: GasType, kind: FluxKind) -> Option<f64> {
        if let Some(flux) = self.fluxes.get(&(gas, kind)) {
            return Some(flux.model.slope().unwrap());
        }

        None
    }

    pub fn toggle_valid(&mut self) {
        self.is_valid = !self.is_valid; // Toggle `is_valid`
    }
    // pub fn dt_v_as_float(&self) -> Vec<f64> {
    //     self.dt_v.iter().map(|x| x.timestamp() as f64).collect()
    // }
    pub fn set_calc_start(&mut self, gas_type: GasType, value: f64) {
        let range_min = self.get_adjusted_close() + self.deadbands.get(&gas_type).unwrap_or(&0.0);
        // the calc area cant go beyond the measurement area
        if range_min > value {
            self.calc_range_start.insert(gas_type, range_min);
        } else {
            self.calc_range_start.insert(gas_type, value);
        }
        // self.adjust_calc_ranges_for_all_gases();
    }
    pub fn set_calc_end(&mut self, gas_type: GasType, value: f64) {
        let range_max = self.get_adjusted_open();
        // the calc area cant go beyond the measurement area
        if value > range_max {
            self.calc_range_end.insert(gas_type, range_max);
        } else {
            self.calc_range_end.insert(gas_type, value);
        }
        self.adjust_calc_range_all();
    }

    pub fn set_start_lag_s(&mut self, new_lag: f64) {
        let old_lag = self.start_lag_s;
        self.start_lag_s = new_lag;
        if self.get_start() > self.get_adjusted_close() {
            self.start_lag_s = old_lag;
            println!("Can't remove data from measurement.");
            return;
        }
        self.reload_gas_data();
    }
    pub fn calculate_concentration_at_t0(&mut self) {
        for gas_type in self.gases.clone() {
            let gas_v = self.get_measurement_gas_v2(gas_type);
            if gas_v.is_empty() {
                self.t0_concentration.insert(gas_type, 0.0);
            } else {
                let t0 = *gas_v.first().unwrap_or(&0.0);
                self.t0_concentration.insert(gas_type, t0);
            }
        }
    }

    pub fn set_end_lag_s(&mut self, new_lag: f64) {
        let old_lag = self.end_lag_s;
        self.end_lag_s = new_lag;
        if self.get_adjusted_open() > self.get_end() {
            self.end_lag_s = old_lag;
            println!("Can't remove data from the measurement.");
            return;
        }
        self.reload_gas_data();
    }

    pub fn get_deadband(&self, gas_type: GasType) -> f64 {
        *self.deadbands.get(&gas_type).unwrap_or(&0.0)
    }
    pub fn calc_area_can_move(&self, gas_type: GasType) -> bool {
        let s = self.get_calc_start(gas_type);
        let e = self.get_calc_end(gas_type);
        let ms = self.get_adjusted_close() + self.get_deadband(gas_type);
        let me = self.get_adjusted_open();
        let cs_at_ms = s <= ms;
        let ce_at_me = e >= me;

        let calc_at_bounds = cs_at_ms && ce_at_me;
        let at_min_range = self.min_calc_range >= self.get_calc_range(gas_type);
        let check = calc_at_bounds && at_min_range;

        !check
    }

    fn _adjust_calc_range_all<F>(&mut self, mut adjust_shortfall: F)
    where
        F: FnMut(&mut Self, GasType, f64),
    {
        let mut shortfall_adjustments = Vec::new();
        for gas_type in self.gases.iter().copied() {
            let deadband = self.get_deadband(gas_type);
            let range_min = self.get_adjusted_close() + deadband;
            let range_max = self.get_adjusted_open();
            let min_range = self.min_calc_range;

            let mut start = *self.calc_range_start.get(&gas_type).unwrap_or(&range_min);
            let mut end = *self.calc_range_end.get(&gas_type).unwrap_or(&range_max);

            let available_range = range_max - range_min;

            // If range is too short, adjust based on logic passed in
            if available_range < min_range {
                shortfall_adjustments.push((gas_type, available_range - min_range));
                // adjust_shortfall(self, gas_type, available_range - min_range);
            }

            // Clamp to bounds
            if start < range_min {
                start = range_min;
            }
            if end > range_max {
                end = range_max;
            }

            // Enforce minimum range
            let current_range = end - start;
            if current_range < min_range {
                let needed = min_range - current_range;
                let half = needed / 2.0;

                let new_start = (start - half).max(range_min);
                let new_end = (end + half).min(range_max);

                if new_end - new_start >= min_range {
                    start = new_start;
                    end = new_end;
                } else {
                    end = start + min_range;
                    if end > range_max {
                        start = range_max - min_range;
                        end = range_max;
                    }
                }
            }

            self.calc_range_start.insert(gas_type, start);
            self.calc_range_end.insert(gas_type, end);
        }
        for (gas_type, shortfall) in shortfall_adjustments {
            adjust_shortfall(self, gas_type, shortfall);
        }
    }
    pub fn set_deadband(&mut self, gas_type: GasType, deadband: f64) {
        self.deadbands.insert(gas_type, deadband);
        self.adjust_calc_range_all_deadband();

        self.check_errors();
        self.calculate_measurement_rs();
        self.compute_all_fluxes();
    }
    pub fn set_close_lag(&mut self, new_lag: f64) {
        self.close_lag_s = new_lag;

        self.adjust_calc_range_all();

        self.check_errors();
        self.calculate_measurement_rs();
        self.compute_all_fluxes();
    }
    pub fn set_open_lag(&mut self, new_lag: f64) {
        self.open_lag_s = new_lag;

        self.adjust_calc_range_all();
        self.check_errors();
        self.calculate_measurement_rs();
        self.compute_all_fluxes();
    }
    fn get_available_range(&self, gas_type: GasType) -> f64 {
        let range_min = self.get_adjusted_close() + self.deadbands.get(&gas_type).unwrap();
        let range_max = self.get_adjusted_open();
        range_max - range_min
    }
    fn adjust_calc_range_all_deadband(&mut self) {
        for gas_type in self.gases.iter().copied() {
            let mut deadband = self.get_deadband(gas_type);
            let range_min = self.get_adjusted_close() + deadband;
            let range_max = self.get_adjusted_open();
            let min_range = self.min_calc_range;
            let mut start = *self.calc_range_start.get(&gas_type).unwrap_or(&range_min);
            let mut end = *self.calc_range_end.get(&gas_type).unwrap_or(&range_max);

            let available_range = range_max - range_min;
            // Clamp to bounds
            if start < range_min {
                start = range_min;
            }
            if end > range_max {
                end = range_max;
            }

            // this seems it should also work
            // if available_range < min_range && range_max == end {
            //     self.close_lag_s += available_range - min_range
            // }

            // setting close_lag_s before this loop causes it go over bounds at times, the
            // available range should never be smaller than the minimum range of the measurement
            if available_range < min_range {
                deadband += available_range - min_range;
                self.deadbands.insert(gas_type, deadband);
            }
            // Ensure min range
            let current_range = end - start;
            // if available_range > current_range
            if current_range < min_range {
                let needed = min_range - current_range;
                let half = needed / 2.0;

                let new_start = (start - half).max(range_min);
                let new_end = (end + half).min(range_max);

                if new_end - new_start >= min_range {
                    start = new_start;
                    end = new_end;
                } else {
                    end = start + min_range;
                    if end > range_max {
                        start = range_max - min_range;
                        end = range_max;
                    }
                }
            }

            self.calc_range_start.insert(gas_type, start);
            self.calc_range_end.insert(gas_type, end);
        }
    }
    fn adjust_calc_range_all(&mut self) {
        for gas_type in self.gases.iter().copied() {
            let range_min = self.get_adjusted_close() + self.deadbands.get(&gas_type).unwrap();
            let range_max = self.get_adjusted_open();
            let min_range = self.min_calc_range;
            let mut start = *self.calc_range_start.get(&gas_type).unwrap_or(&range_min);
            let mut end = *self.calc_range_end.get(&gas_type).unwrap_or(&range_max);

            let available_range = range_max - range_min;
            // Clamp to bounds
            if start < range_min {
                start = range_min;
            }
            if end > range_max {
                end = range_max;
            }

            // this seems it should also work
            // if available_range < min_range && range_max == end {
            //     self.close_lag_s += available_range - min_range
            // }

            // setting close_lag_s before this loop causes it go over bounds at times, the
            // available range should never be smaller than the minimum range of the measurement
            if available_range < min_range {
                self.close_lag_s += available_range - min_range
            }
            // Ensure min range
            let current_range = end - start;
            // if available_range > current_range
            if current_range < min_range {
                let needed = min_range - current_range;
                let half = needed / 2.0;

                let new_start = (start - half).max(range_min);
                let new_end = (end + half).min(range_max);

                if new_end - new_start >= min_range {
                    start = new_start;
                    end = new_end;
                } else {
                    end = start + min_range;
                    if end > range_max {
                        start = range_max - min_range;
                        end = range_max;
                    }
                }
            }

            self.calc_range_start.insert(gas_type, start);
            self.calc_range_end.insert(gas_type, end);
        }
    }
    pub fn set_measurement_start(&mut self, value: f64) {
        self.measurement_range_start = value;
    }
    pub fn set_measurement_end(&mut self, value: f64) {
        self.measurement_range_end = value;
    }
    pub fn get_calc_start(&self, gas_type: GasType) -> f64 {
        *self.calc_range_start.get(&gas_type).unwrap_or(&0.0)
    }
    pub fn get_calc_end(&self, gas_type: GasType) -> f64 {
        *self.calc_range_end.get(&gas_type).unwrap_or(&0.0)
    }

    pub fn get_calc_range(&self, gas_type: GasType) -> f64 {
        let start = self.get_calc_start(gas_type);
        let end = self.get_calc_end(gas_type);
        end - start
    }
    pub fn get_measurement_start(&self) -> f64 {
        self.start_time.timestamp() as f64
            + self.close_offset as f64
            + self.open_lag_s
            + self.close_lag_s
    }
    pub fn get_measurement_end(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.open_offset as f64 + self.open_lag_s
        // + self.close_lag_s
    }
    pub fn set_automatic_valid(&mut self, valid: bool) {
        if self.override_valid.is_none() {
            self.is_valid = valid && self.error_code.0 == 0; // Ensure error codes affect validity
        }
    }
    pub fn toggle_manual_valid(&mut self) {
        let before_valid = self.is_valid;
        let before_override = self.override_valid;
        let before_errors = self.error_code.0;

        if self.override_valid.is_some() {
            // if we hit override after already toggling it, it will reset to None
            self.override_valid = None;
        } else {
            // if we have overriden, it should be opposite of valid
            self.override_valid = Some(!self.is_valid);
        }

        // always toggle is_valid
        self.is_valid = !self.is_valid;
        // manual valid to true if override is not None
        self.manual_valid = self.override_valid.is_some(); // Track manual changes
        if self.manual_valid && self.override_valid == Some(false) {
            self.add_error(ErrorCode::ManualInvalid)
        } else {
            self.remove_error(ErrorCode::ManualInvalid)
        }
        if self.manual_valid && self.override_valid == Some(true) {
            self.error_code = ErrorMask(0);
        }

        let after_valid = self.is_valid;
        let after_override = self.override_valid;
        let after_errors = self.error_code.0;

        if before_valid != after_valid
            || before_override != after_override
            || before_errors != after_errors
        {
            self.manual_adjusted = true;
        }
    }

    pub fn get_peak_near_timestamp(
        &mut self,
        gas_type: GasType,
        target_time: i64, // Now an i64 timestamp
    ) -> Option<f64> {
        if let Some(gas_v) = self.gas_v.get(&gas_type) {
            let len = gas_v.len();
            if len < 120 {
                println!("Less than 2minutes of data.");
                return None;
            }

            // Find index closest to `target_time` in `dt_v`
            let target_idx = self
                .dt_v
                .iter()
                .enumerate()
                .min_by_key(|(_, &dt)| (dt as i64 - target_time).abs())
                .map(|(idx, _)| idx)?;

            // Define search range (±5 seconds)
            let start_index = target_idx.saturating_sub(5);
            let end_index = (target_idx + 5).min(len - 1);

            // Find max in the range
            let max_idx = (start_index..=end_index).max_by(|&a, &b| {
                gas_v[a].partial_cmp(&gas_v[b]).unwrap_or(std::cmp::Ordering::Equal)
            });
            if let Some(idx) = max_idx {
                if let Some(peak_time) = self.dt_v.get(idx).cloned() {
                    let lags = peak_time
                        - (self.start_time + chrono::TimeDelta::seconds(self.open_offset))
                            .timestamp() as f64;
                    self.set_open_lag(lags);

                    return Some(peak_time);
                }
            }
        }
        None
    }
    pub fn get_peak_datetime(&mut self, gas_type: GasType) -> Option<f64> {
        if let Some(gas_v) = self.gas_v.get(&gas_type) {
            let len = gas_v.len();
            if len < 120 {
                return None;
            }
            // println!("{}", gas_v.len());

            let start_index = len.saturating_sub(240);
            let max_idx = gas_v[start_index..]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(idx, _)| start_index + idx);

            if let Some(idx) = max_idx {
                if let Some(peak_time) = self.dt_v.get(idx).cloned() {
                    self.open_lag_s = peak_time
                        - (self.start_time + chrono::TimeDelta::seconds(self.open_offset))
                            .timestamp() as f64;

                    return Some(peak_time);
                }
            }
        }
        None
    }
    pub fn check_diag(&mut self) {
        let total_count = self.diag_v.len();
        let nonzero_count = self.diag_v.iter().filter(|&&x| x != 0).count();

        // Check if more than 50% of the values are nonzero
        let check = nonzero_count as f64 / total_count as f64 > 0.5;
        if check {
            self.add_error(ErrorCode::TooManyDiagErrors)
        } else {
            self.remove_error(ErrorCode::TooManyDiagErrors)
        }
    }
    pub fn check_measurement_diag(&mut self) -> bool {
        let check = self.measurement_diag_v.iter().sum::<i64>() != 0;
        if check {
            self.add_error(ErrorCode::ErrorsInMeasurement)
        } else {
            self.remove_error(ErrorCode::ErrorsInMeasurement)
        }
        check
    }

    pub fn calculate_max_y(&mut self) {
        for (gas_type, gas_v) in &self.gas_v {
            let max_value = gas_v
            .iter()
            .filter_map(|&v| v) // discard None
            .filter(|v| !v.is_nan())
            .fold(f64::NEG_INFINITY, f64::max);

            self.max_y.insert(*gas_type, max_value);
        }
    }

    pub fn calculate_min_y(&mut self) {
        for (gas_type, gas_v) in &self.gas_v {
            let min_value = gas_v
            .iter()
            .filter_map(|&v| v) // discard None
            .filter(|v| !v.is_nan())
            .fold(f64::INFINITY, f64::min);

            self.min_y.insert(*gas_type, min_value);
        }
    }

    pub fn calculate_measurement_r(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
            // let dt_vv: Vec<f64> =
            //     self.measurement_dt_v.iter().map(|x| x.timestamp() as f64).collect();

            // Zip and filter together so only valid (Some) pairs remain
            let filtered: Vec<(f64, f64)> = self
                .measurement_dt_v
                .iter()
                .zip(gas_v.iter())
                .filter_map(|(&t, &g)| g.map(|g_val| (t, g_val)))
                .collect();

            // Unzip to separate vectors again
            let (filtered_x, filtered_y): (Vec<f64>, Vec<f64>) = filtered.into_iter().unzip();

            // Calculate and store r^2
            let r2 = stats::pearson_correlation(&filtered_x, &filtered_y).unwrap_or(0.0).powi(2);
            self.measurement_r2.insert(gas_type, r2);
        }
    }

    pub fn calculate_calc_r(&mut self, gas_type: GasType) {
        let dt = self.get_calc_dt2(gas_type);
        let gas = self.get_calc_gas_v(gas_type);

        let filtered: Vec<(&f64, &f64)> = dt.iter().zip(gas.iter()).collect();

        let (x, y): (Vec<f64>, Vec<f64>) = filtered.into_iter().unzip();

        self.calc_r2.insert(gas_type, stats::pearson_correlation(&x, &y).unwrap_or(0.0).powi(2));
    }

    pub fn calculate_calc_rs(&mut self) {
        for gas_type in self.gases.clone() {
            self.calculate_calc_r(gas_type);
        }
    }

    pub fn find_highest_r_windows(&mut self) {
        // Precompute timestamps as float
        let dt_v: Vec<f64> = self.measurement_dt_v.clone();

        // Precompute timestamp gaps (difference > 1.0 sec)
        let gaps: Vec<bool> = dt_v.windows(2).map(|w| (w[1] - w[0]).abs() > 1.0).collect();

        // Run analysis in parallel for all gases
        let results: Vec<_> = self
            .gases
            .par_iter()
            .filter_map(|&gas| {
                let gas_v = self.measurement_gas_v.get(&gas)?;

                if gas_v.len() < MIN_WINDOW_SIZE || dt_v.len() < MIN_WINDOW_SIZE {
                    return None;
                }

                find_best_window_for_gas(&dt_v, gas_v, &gaps, MIN_WINDOW_SIZE, WINDOW_INCREMENT)
                    .map(|(start, end, r2, best_y)| (gas, start, end, r2, best_y))
            })
            .collect();

        // Store results
        for (gas, start, end, r2, best_y) in results {
            self.calc_r2.insert(gas, r2);
            self.calc_range_start.insert(gas, self.measurement_dt_v[start]);
            self.calc_range_end.insert(gas, self.measurement_dt_v[end.saturating_sub(1)]);
            self.calc_dt_v.insert(gas, self.measurement_dt_v[start..end].to_vec());
            self.calc_gas_v.insert(gas, best_y);
        }
    }
    pub fn find_highest_r_window(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.measurement_gas_v.get(&gas_type) {
            if self.measurement_dt_v.len() < MIN_WINDOW_SIZE || MIN_WINDOW_SIZE == 0 {
                println!("Short data");
                return;
            }

            // Keep everything aligned with Option<f64>
            let dt_v: Vec<f64> = self.measurement_dt_v.clone();

            let max_window = gas_v.len();
            let mut max_r = f64::MIN;
            let mut start_idx = 0;
            let mut end_idx = 0;

            for win_size in (MIN_WINDOW_SIZE..max_window).step_by(WINDOW_INCREMENT) {
                for start in (0..=(max_window - win_size)).step_by(WINDOW_INCREMENT) {
                    let end = start + win_size;

                    let x_win = &dt_v[start..end];
                    let y_win = &gas_v[start..end];

                    // Extract only the (Some) valid pairs
                    let valid: Vec<(f64, f64)> = x_win
                        .iter()
                        .zip(y_win.iter())
                        .filter_map(|(&x, &y)| y.map(|val| (x, val)))
                        .collect();

                    let (x_vals, y_vals): (Vec<f64>, Vec<f64>) = valid.into_iter().unzip();

                    let r = stats::pearson_correlation(&x_vals, &y_vals).unwrap_or(0.0);
                    if r > max_r {
                        max_r = r;
                        start_idx = start;
                        end_idx = end;
                    }
                }
            }

            // Store full window (including None values) from original slice
            if max_r != f64::MIN {
                self.calc_r2.insert(gas_type, max_r);
                self.calc_range_start.insert(gas_type, self.measurement_dt_v[start_idx]);
                self.calc_range_end.insert(gas_type, self.measurement_dt_v[end_idx - 1]);
                self.calc_dt_v.insert(gas_type, self.measurement_dt_v[start_idx..end_idx].to_vec());
                self.calc_gas_v.insert(gas_type, gas_v[start_idx..end_idx].to_vec());
                // <- keeps Option<f64>
            }
        }
    }
    pub fn get_calc_datas(&mut self) {
        for &gas_type in &self.gases.clone() {
            self.get_calc_data(gas_type);
        }
    }

    // pub fn get_measurement_datas(&mut self) {
    //     for &gas_type in &self.gases.clone() {
    //         self.get_measurement_data(gas_type);
    //     }
    // }
    pub fn calculate_measurement_rs2(&mut self) {
        for &gas_type in &self.gases {
            let gas_v: Vec<f64> = self.get_measurement_gas_v2(gas_type);
            let dt_vv: Vec<f64> = self.get_measurement_dt_v2();

            // let filtered: Vec<(f64, f64)> = dt_vv.iter().zip(gas_v.iter()).collect();
            let filtered: Vec<(f64, f64)> =
                dt_vv.iter().zip(gas_v.iter()).map(|(&dt, &g)| (dt, g)).collect();
            let (x, y): (Vec<f64>, Vec<f64>) = filtered.into_iter().unzip();

            self.measurement_r2
                .insert(gas_type, stats::pearson_correlation(&x, &y).unwrap_or(0.0).powi(2));
        }
    }
    pub fn calculate_measurement_rs(&mut self) {
        for &gas_type in &self.gases {
            let gas_v = self.get_measurement_gas_v2(gas_type);
            let dt_vv = self.get_measurement_dt_v2();

            // let filtered: Vec<(f64, f64)> = dt_vv.iter().zip(gas_v.iter()).collect();
            let filtered: Vec<(f64, f64)> =
                dt_vv.iter().zip(gas_v.iter()).map(|(&dt, &g)| (dt, g)).collect();

            let (x, y): (Vec<f64>, Vec<f64>) = filtered.into_iter().unzip();

            self.measurement_r2
                .insert(gas_type, stats::pearson_correlation(&x, &y).unwrap_or(0.0).powi(2));
            // }
        }
    }

    pub fn has_error(&self, error: ErrorCode) -> bool {
        self.error_code.0 & error.to_mask() != 0
    }
    pub fn add_error(&mut self, error: ErrorCode) {
        self.error_code |= error;
        if self.error_code != ErrorMask(0) {
            self.is_valid = false; // Automatically invalidate on error
        }
    }
    pub fn remove_error(&mut self, error: ErrorCode) {
        self.error_code.0 &= !error.to_mask();
        if self.error_code.0 == 0 {
            self.is_valid = true; // If no errors remain, revalidate
        }
    }

    pub fn check_main_r(&mut self) {
        if let Some(r2) = self.measurement_r2.get(&self.main_gas) {
            if *r2 < 0.98 {
                self.add_error(ErrorCode::LowR);
            } else {
                self.remove_error(ErrorCode::LowR);
            }
        } else {
            self.add_error(ErrorCode::LowR); // Optionally handle missing model as error
        }
    }

    pub fn check_missing(&mut self) {
        if let Some(values) = self.gas_v.get(&self.main_gas) {
            let valid_count = values.iter().filter(|v| v.is_some()).count();
            let threshold = self.end_offset as f64 * 0.7;
            let check = (valid_count as f64) < threshold;

            if check {
                self.add_error(ErrorCode::TooFewMeasurements);
            } else {
                self.remove_error(ErrorCode::TooFewMeasurements);
            }
        } else {
            self.add_error(ErrorCode::TooFewMeasurements);
        }
    }
    pub fn check_errors(&mut self) {
        self.check_main_r();
        self.check_measurement_diag();
        self.check_missing();
        if self.error_code.0 == 0 || self.override_valid == Some(true) {
            self.is_valid = true
        }
    }
    pub fn reset_deadbands(&mut self) {
        for gas in &self.gases {
            self.deadbands.insert(*gas, 30.);
        }
    }
    pub fn reset(&mut self) {
        self.manual_adjusted = false;
        self.close_lag_s = 0.;
        self.open_lag_s = 0.;
        self.reset_deadbands();
        if self.end_lag_s != 0. || self.start_lag_s != 0. {
            self.end_lag_s = 0.;
            self.start_lag_s = 0.;
            self.reload_gas_data();
        }
        self.check_diag();
        self.check_missing();

        if !self.has_error(ErrorCode::TooManyDiagErrors)
            && !self.has_error(ErrorCode::TooFewMeasurements)
        {
            self.get_peak_datetime(self.main_gas);
            self.set_calc_ranges();
            // self.get_calc_datas();
            // self.get_measurement_datas();
            self.calculate_concentration_at_t0();
            self.calculate_measurement_rs();
            self.check_main_r();
            // self.find_highest_r_windows();
            self.compute_all_fluxes();
            self.calculate_max_y();
            self.calculate_min_y();
            self.check_errors();
        }
    }

    pub fn set_calc_ranges(&mut self) {
        for gas_type in self.gases.clone() {
            println!("{}", self.deadbands.get(&gas_type).unwrap_or(&0.0));
            let start =
                self.get_measurement_start() + self.deadbands.get(&gas_type).unwrap_or(&0.0);
            let end = start + self.min_calc_range;
            self.set_calc_start(gas_type, start);
            self.set_calc_end(gas_type, end);
        }
    }

    // pub fn change_measurement_range(&mut self) {
    //     self.get_measurement_datas();
    //     self.calculate_measurement_rs();
    //     // self.find_highest_r_windows();
    //     self.compute_all_fluxes();
    // }
    pub fn recalc_r(&mut self) {
        // self.find_highest_r_windows();
        self.compute_all_fluxes();
    }

    pub fn update_calc_attributes(&mut self, gas_type: GasType) {
        // self.get_calc_data(gas_type);
        self.calculate_concentration_at_t0();
        // self.calculate_calc_r(gas_type);
        self.compute_single_flux(gas_type);
    }
    pub fn update_measurement_attributes(&mut self, gas_type: GasType) {
        // self.get_measurement_datas();
        self.calculate_measurement_rs();
        // self.get_calc_data(gas_type);
        self.calculate_concentration_at_t0();
        // self.calculate_calc_r(gas_type);
        self.compute_single_flux(gas_type);
    }

    pub fn get_calc_data(&mut self, gas_type: GasType) {
        if let Some(gas_v) = self.gas_v.get(&gas_type) {
            let mut s = (self.calc_range_start.get(&gas_type).unwrap()
                - self.start_time.timestamp() as f64) as usize;
            let mut e = (self.calc_range_end.get(&gas_type).unwrap()
                - self.start_time.timestamp() as f64) as usize;

            // Clear previous results
            self.calc_gas_v.insert(gas_type, gas_v[s..e].to_vec());
            self.calc_dt_v.insert(gas_type, self.dt_v[s..e].to_vec());
        }
    }
    pub fn get_measurement_gas_v2(&self, gas_type: GasType) -> Vec<f64> {
        let s = ((self.get_adjusted_close() + self.get_deadband(gas_type))
            - self.start_time.timestamp() as f64) as usize;
        let e = (self.get_adjusted_open() - self.start_time.timestamp() as f64) as usize;
        let ret: Vec<f64> = self
            .gas_v
            .get(&gas_type)
            .map(|vec| vec.iter().map(|s| s.unwrap_or(0.0)).collect())
            .unwrap_or_default();
        if s > ret.len() {
            return ret;
        }
        ret[s..e].to_vec()
    }
    pub fn get_measurement_dt_v2(&self) -> Vec<f64> {
        let close_time = self.get_adjusted_close() - self.start_time.timestamp() as f64;
        let open_time = self.get_adjusted_open() - self.start_time.timestamp() as f64;

        let s = close_time as usize;
        let e = open_time as usize;
        if s > self.dt_v.len() {
            return self.dt_v.clone();
        }
        self.dt_v[s..e].to_vec()
    }
    // pub fn get_measurement_data(&mut self, gas_type: GasType) {
    //     if let Some(gas_v) = self.gas_v.get(&gas_type) {
    //         let close_time = self.get_adjusted_close();
    //         let open_time = self.get_adjusted_open();
    //
    //         let s = close_time;
    //         let e = open_time;
    //         // let e = s + 150.;
    //         // Clear previous results
    //         self.measurement_gas_v.insert(gas_type, Vec::new());
    //         self.measurement_dt_v.clear();
    //
    //         // Filter and store results in separate vectors
    //         self.dt_v
    //             .iter()
    //             .zip(gas_v.iter()) // Pair timestamps with gas values
    //             .filter(|(t, _)| (t.timestamp() as f64) >= s && (t.timestamp() as f64) <= e) // Filter by time range
    //             .for_each(|(t, d)| {
    //                 self.measurement_dt_v.push(*t);
    //                 self.measurement_gas_v.get_mut(&gas_type).unwrap().push(*d);
    //             });
    //     } else {
    //         println!("No gas data found for {}", gas_type);
    //     }
    // }

    // pub fn calculate_slope(&mut self, gas_type: GasType) {
    //     if let Some(gas_v) = self.calc_gas_v.get(&gas_type) {
    //         let time_vec: Vec<f64> = self
    //             .calc_dt_v
    //             .get(&gas_type)
    //             .unwrap()
    //             .iter()
    //             .map(|dt| dt.timestamp() as f64)
    //             .collect();
    //
    //         // Filter (x, y) pairs where y is Some
    //         let filtered: Vec<(f64, f64)> = time_vec
    //             .iter()
    //             .zip(gas_v.iter())
    //             .filter_map(|(&t, &v)| v.map(|val| (t, val)))
    //             .collect();
    //
    //         let (x_vals, y_vals): (Vec<f64>, Vec<f64>) = filtered.into_iter().unzip();
    //
    //         let linreg = stats::LinReg::train(&x_vals, &y_vals);
    //         self.linfit.insert(gas_type, linreg);
    //     } else {
    //         self.linfit.insert(gas_type, LinReg::default());
    //     }
    // }

    pub fn compute_all_fluxes(&mut self) {
        for &gas_type in &self.instrument_model.available_gases() {
            self.calculate_lin_flux(gas_type);
            self.calculate_poly_flux(gas_type);
            self.calculate_roblin_flux(gas_type);
        }
    }
    pub fn compute_single_flux(&mut self, gas: GasType) {
        self.calculate_lin_flux(gas);
        self.calculate_poly_flux(gas);
        self.calculate_roblin_flux(gas);
    }

    // pub fn get_calc_dt(&self, gas_type: GasType) -> Vec<f64> {
    //     let ret: Vec<f64> = *self.calc_dt_v.get(&gas_type).unwrap_or(&Vec::new());
    //     ret
    // }
    pub fn get_calc_dt2(&self, gas_type: GasType) -> Vec<f64> {
        let s = (self.get_calc_start(gas_type) - self.start_time.timestamp() as f64) as usize;
        let e = (self.get_calc_end(gas_type) - self.start_time.timestamp() as f64) as usize;
        let ret: Vec<f64> = self.dt_v.clone();
        if s > ret.len() {
            return ret;
        }
        ret[s..e].to_vec()
    }
    pub fn get_calc_gas_v2(&self, gas_type: GasType) -> Vec<f64> {
        let s = (self.get_calc_start(gas_type) - self.start_time.timestamp() as f64) as usize;
        let e = (self.get_calc_end(gas_type) - self.start_time.timestamp() as f64) as usize;
        let ret: Vec<f64> = self
            .gas_v
            .get(&gas_type)
            .map(|vec| vec.iter().map(|s| s.unwrap_or(0.0)).collect())
            .unwrap_or_default();
        if s > ret.len() {
            return ret;
        }
        ret[s..e].to_vec()
    }
    // pub fn get_measurement_dt_v(&self) -> Vec<f64> {
    //     self.measurement_dt_v.iter().map(|s| s.timestamp() as f64).collect()
    // }
    pub fn get_measurement_gas_v(&self, gas_type: GasType) -> Vec<f64> {
        self.measurement_gas_v
            .get(&gas_type)
            .map(|vec| vec.par_iter().map(|s| s.unwrap_or(0.0)).collect())
            .unwrap_or_default()
    }

    pub fn get_calc_gas_v(&self, gas_type: GasType) -> Vec<f64> {
        self.calc_gas_v
            .get(&gas_type)
            .map(|vec| vec.iter().map(|s| s.unwrap_or(0.0)).collect())
            .unwrap_or_default()
    }

    pub fn calculate_lin_flux(&mut self, gas_type: GasType) {
        let x = self.get_calc_dt2(gas_type).to_vec();
        let y = self.get_calc_gas_v2(gas_type).to_vec();
        let s = x.first().unwrap_or(&0.);
        let e = x.last().unwrap_or(&0.);
        // let pt_count = 300;
        //
        // let x = self.get_measurement_dt_v()[..pt_count].to_vec();
        // let y = self.get_measurement_gas_v(gas_type)[..pt_count].to_vec();
        // let s = x.first().unwrap_or(&0.);
        // let e = x.last().unwrap_or(&0.);

        if x.len() < 2 || y.len() < 2 || x.len() != y.len() {
            // Optionally: log or emit warning here
            return; // Not enough data to fit
        }

        if let Some(data) = LinearFlux::from_data(
            "lin",
            gas_type,
            &x,
            &y,
            *s,
            *e,
            self.air_temperature,
            self.air_pressure,
            self.chamber_volume,
        ) {
            self.fluxes.insert(
                (gas_type, FluxKind::Linear),
                FluxRecord {
                    model: Box::new(data),
                    is_valid: true, // default to valid unless user invalidates later
                },
            );
        } else {
            // Optionally log: fitting failed
        }
    }
    pub fn calculate_poly_flux(&mut self, gas_type: GasType) {
        // let x = self.get_measurement_dt_v().to_vec();
        // let y = self.get_measurement_gas_v(gas_type).to_vec();
        let x = self.get_calc_dt2(gas_type).to_vec();
        let y = self.get_calc_gas_v2(gas_type).to_vec();
        let s = x.first().unwrap_or(&0.);
        let e = x.last().unwrap_or(&0.);
        // let pt_count = 300;
        //
        // let x = self.get_measurement_dt_v()[..pt_count].to_vec();
        // let y = self.get_measurement_gas_v(gas_type)[..pt_count].to_vec();
        // let s = x.first().unwrap_or(&0.);
        // let e = x.last().unwrap_or(&0.);

        // Ensure valid input
        if x.len() < 3 || y.len() < 3 || x.len() != y.len() {
            // Optional: log or notify
            eprintln!(
                "Insufficient data for polynomial flux on gas {:?}: x = {}, y = {}",
                gas_type,
                x.len(),
                y.len()
            );
            return;
        }

        // Fit and insert if successful
        if let Some(data) = PolyFlux::from_data(
            "poly",
            gas_type,
            &x,
            &y,
            *s,
            *e,
            self.air_temperature,
            self.air_pressure,
            self.chamber_volume,
        ) {
            self.fluxes.insert(
                (gas_type, FluxKind::Poly),
                FluxRecord { model: Box::new(data), is_valid: true },
            );
        } else {
            eprintln!("Polynomial regression failed for gas {:?}", gas_type);
        }
    }
    pub fn calculate_roblin_flux(&mut self, gas_type: GasType) {
        let x = self.get_calc_dt2(gas_type).to_vec();
        let y = self.get_calc_gas_v2(gas_type).to_vec();
        let s = x.first().unwrap_or(&0.);
        let e = x.last().unwrap_or(&0.);
        // let pt_count = 300;
        //
        // let x = self.get_measurement_dt_v()[..pt_count].to_vec();
        // let y = self.get_measurement_gas_v(gas_type)[..pt_count].to_vec();
        // let s = x.first().unwrap_or(&0.);
        // let e = x.last().unwrap_or(&0.);

        if x.len() < 2 || y.len() < 2 || x.len() != y.len() {
            // Optionally: log or emit warning here
            return; // Not enough data to fit
        }

        if let Some(data) = RobustFlux::from_data(
            "roblin",
            gas_type,
            &x,
            &y,
            *s,
            *e,
            self.air_temperature,
            self.air_pressure,
            self.chamber_volume,
        ) {
            self.fluxes.insert(
                (gas_type, FluxKind::RobLin),
                FluxRecord { model: Box::new(data), is_valid: true },
            );
        } else {
            // Optionally log: fitting failed (maybe x.len != y.len or regression degenerate)
        }
    }
    pub fn ppb_to_nmol(&mut self) {
        // Constants
        const R: f64 = 8.314462618; // J/mol·K

        let pressure_pa = self.air_pressure * 100.0; // Convert hPa to Pa
        let temperature_k = self.air_temperature + 273.15; // Convert °C to K
        let volume_m3 = self.chamber_volume / 1000.0; // Convert L to m³

        let conversion_factor = (pressure_pa * volume_m3) / (R * temperature_k); // mol / mol-fraction
        let ppb_to_nmol = conversion_factor * 1e-9 * 1e9; // mol → nmol, and ppb = 1e-9
        let mut converted: HashMap<GasType, Vec<Option<f64>>> = HashMap::new();
        for gas_type in self.gases.clone() {
            if let Some(values) = self.gas_v.get(&gas_type) {
                let new_vals = values.par_iter().map(|v| v.map(|val| val * ppb_to_nmol)).collect();
                converted.insert(gas_type, new_vals);
            }
            // if let Some(values) = self.gas_v.get_mut(&gas_type) {
            //     for value in values.iter_mut().flatten() {
            //         let val = *value *= ppb_to_nmol;
            //     }
            // }
        }
        self.gas_v_mole = converted;
    }
    pub fn update_cycle(&mut self, _project: String) {
        // self.get_calc_datas();
        // self.get_measurement_datas();
        self.calculate_measurement_rs();
        // self.find_highest_r_windows();
        self.check_errors();
        self.compute_all_fluxes();
    }
    pub fn reload_gas_data(&mut self) {
        println!("###### Reload gas data #######");
        let conn = match Connection::open("fluxrs.db") {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Failed to open database: {}", e);
                return;
            },
        };

        let start = match DateTime::from_timestamp(self.get_start() as i64, 0) {
            Some(dt) => dt,
            None => {
                eprintln!("Invalid start timestamp for cycle");
                return;
            },
        };
        let end = match DateTime::from_timestamp(self.get_end() as i64, 0) {
            Some(dt) => dt,
            None => {
                eprintln!("Invalid end timestamp for cycle");
                return;
            },
        };

        match query_gas_all(
            &conn,
            start,
            end,
            self.project_name.clone(),
            self.instrument_serial.clone(),
        ) {
            Ok(gasdata) => {
                self.gas_v = gasdata.gas;
                self.dt_v = gasdata.datetime.par_iter().map(|t| t.timestamp() as f64).collect();
                self.diag_v = gasdata.diag;
            },
            Err(e) => {
                eprintln!("Error while loading gas data: {}", e);
            },
        }
        // self.get_calc_datas();
        // self.get_measurement_datas();
        self.calculate_max_y();
        self.calculate_min_y();
    }
    pub fn best_flux_by_aic(&self, gas_type: &GasType) -> Option<f64> {
        let candidates = FluxKind::all();

        candidates
            .iter()
            .filter_map(|kind| self.fluxes.get(&(*gas_type, *kind)))
            .filter_map(|m| m.model.aic().map(|aic| (aic, m.model.flux())))
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(_, flux)| flux.unwrap())
    }
    pub fn best_model_by_aic(&self, gas_type: &GasType) -> Option<FluxKind> {
        let candidates = FluxKind::all();

        candidates
            .iter()
            .filter_map(|kind| self.fluxes.get(&(*gas_type, *kind)))
            .filter_map(|m| m.model.aic().map(|aic| (aic, m.model.fit_id())))
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(_, fit_id)| fit_id)
    }

    pub fn is_valid_by_threshold(
        &self,
        gas_type: GasType,
        kind: FluxKind,
        p_val_thresh: f64,
        r2_thresh: f64,
        rmse_thresh: f64,
        t0_thresh: f64,
    ) -> bool {
        let p_val = self.fluxes.get(&(gas_type, kind)).unwrap().model.p_value().unwrap_or(0.0);
        let r2 = self.measurement_r2.get(&gas_type).unwrap_or(&0.0);
        let rmse = self.fluxes.get(&(gas_type, kind)).unwrap().model.rmse().unwrap_or(0.0);
        let t0 = self.t0_concentration.get(&gas_type).unwrap_or(&0.0);
        p_val < p_val_thresh && *r2 > r2_thresh && rmse < rmse_thresh && *t0 < t0_thresh
    }

    pub fn mark_flux_invalid(&mut self, gas: GasType, kind: FluxKind) {
        if let Some(record) = self.fluxes.get_mut(&(gas, kind)) {
            record.is_valid = false;
        }
    }

    pub fn mark_flux_valid(&mut self, gas: GasType, kind: FluxKind) {
        if let Some(record) = self.fluxes.get_mut(&(gas, kind)) {
            record.is_valid = true;
        }
    }
}
#[derive(Debug, Default, Clone)]
pub struct CycleBuilder {
    chamber_id: Option<String>,
    start_time: Option<DateTime<Utc>>,
    close_offset: Option<i64>,
    open_offset: Option<i64>,
    end_offset: Option<i64>,
    project: Option<String>,
}
impl CycleBuilder {
    /// Create a new CycleBuilder
    pub fn new() -> Self {
        Self {
            chamber_id: None,
            start_time: None,
            close_offset: None,
            open_offset: None,
            end_offset: None,
            project: None,
        }
    }

    /// Set the chamber ID
    pub fn chamber_id(mut self, id: String) -> Self {
        self.chamber_id = Some(id);
        self
    }

    /// Set the start time
    pub fn start_time(mut self, time: DateTime<Utc>) -> Self {
        self.start_time = Some(time);
        self
    }

    /// Set the close offset (seconds from start)
    pub fn close_offset(mut self, offset: i64) -> Self {
        self.close_offset = Some(offset);
        self
    }

    /// Set the open offset (seconds from start)
    pub fn open_offset(mut self, offset: i64) -> Self {
        self.open_offset = Some(offset);
        self
    }

    /// Set the end offset (seconds from start)
    pub fn end_offset(mut self, offset: i64) -> Self {
        self.end_offset = Some(offset);
        self
    }
    pub fn project_name(mut self, project: String) -> Self {
        self.project = Some(project);
        self
    }

    /// Build the Cycle struct
    pub fn build_db(self) -> Result<Cycle, Error> {
        let start =
            self.start_time.ok_or(Error::InvalidColumnName("Start time is required".to_owned()))?;
        let chamber =
            self.chamber_id.ok_or(Error::InvalidColumnName("Chamber ID is required".to_owned()))?;
        let close = self
            .close_offset
            .ok_or(Error::InvalidColumnName("Close offset is required".to_owned()))?;
        let open = self
            .open_offset
            .ok_or(Error::InvalidColumnName("Open offset is required".to_owned()))?;
        let end =
            self.end_offset.ok_or(Error::InvalidColumnName("End offset is required".to_owned()))?;

        Ok(Cycle {
            id: 0,
            chamber_id: chamber,
            start_time: start,
            instrument_model: InstrumentType::Li7810,
            instrument_serial: String::new(),
            project_name: String::new(),
            min_calc_range: MIN_CALC_AREA_RANGE,
            // close_time: start + chrono::Duration::seconds(close),
            // open_time: start + chrono::Duration::seconds(open),
            // end_time: start + chrono::Duration::seconds(end),
            close_offset: close,
            open_offset: open,
            end_offset: end,
            main_gas: GasType::CH4,
            error_code: ErrorMask(0),
            manual_adjusted: false,
            calc_range_end: HashMap::new(),
            calc_range_start: HashMap::new(),
            min_y: HashMap::new(),
            max_y: HashMap::new(),
            t0_concentration: HashMap::new(),
            open_lag_s: 0.,
            close_lag_s: 0.,
            end_lag_s: 0.,
            start_lag_s: 0.,
            deadbands: HashMap::new(),
            max_idx: 0.,
            flux: HashMap::new(),
            fluxes: HashMap::new(),
            linfit: HashMap::new(),
            calc_r2: HashMap::new(),
            measurement_r2: HashMap::new(),
            measurement_range_start: 0.,
            measurement_range_end: 0.,
            diag_v: vec![],
            dt_v: vec![],
            gas_v: HashMap::new(),
            gas_v_mole: HashMap::new(),
            calc_gas_v: HashMap::new(),
            calc_dt_v: HashMap::new(),
            measurement_gas_v: HashMap::new(),
            measurement_dt_v: vec![],
            measurement_diag_v: Vec::new(),
            gases: vec![],
            air_pressure: 1000.,
            air_temperature: 10.,
            chamber_volume: 1.,
            is_valid: true,
            gas_is_valid: HashMap::new(),
            override_valid: None,
            manual_valid: false,
        })
    }
    pub fn build(self) -> Result<Cycle, Box<dyn std::error::Error + Send + Sync>> {
        let start = self.start_time.ok_or("Start time is required")?;
        let chamber = self.chamber_id.ok_or("Chamber ID is required")?;
        let close = self.close_offset.ok_or("Close offset is required")?;
        let open = self.open_offset.ok_or("Open offset is required")?;
        let end = self.end_offset.ok_or("End offset is required")?;
        let project = self.project.ok_or("Project is required")?;

        Ok(Cycle {
            id: 0,
            chamber_id: chamber,
            instrument_model: InstrumentType::Li7810,
            instrument_serial: String::new(),
            min_calc_range: MIN_CALC_AREA_RANGE,
            project_name: project,
            start_time: start,
            // close_time: start + chrono::Duration::seconds(close),
            // open_time: start + chrono::Duration::seconds(open),
            // end_time: start + chrono::Duration::seconds(end),
            close_offset: close,
            open_offset: open,
            end_offset: end,
            error_code: ErrorMask(0),
            main_gas: GasType::CH4,
            manual_adjusted: false,
            calc_range_end: HashMap::new(),
            calc_range_start: HashMap::new(),
            min_y: HashMap::new(),
            max_y: HashMap::new(),
            t0_concentration: HashMap::new(),
            open_lag_s: 0.,
            close_lag_s: 0.,
            end_lag_s: 0.,
            start_lag_s: 0.,
            deadbands: HashMap::new(),
            max_idx: 0.,
            flux: HashMap::new(),
            fluxes: HashMap::new(),
            linfit: HashMap::new(),
            calc_r2: HashMap::new(),
            measurement_r2: HashMap::new(),
            measurement_range_start: 0.,
            measurement_range_end: 0.,
            diag_v: vec![],
            dt_v: vec![],
            gas_v: HashMap::new(),
            gas_v_mole: HashMap::new(),
            calc_gas_v: HashMap::new(),
            calc_dt_v: HashMap::new(),
            measurement_gas_v: HashMap::new(),
            measurement_dt_v: vec![],
            measurement_diag_v: Vec::new(),
            gases: vec![],
            air_pressure: 1000.,
            air_temperature: 10.,
            chamber_volume: 1.,
            is_valid: true,
            gas_is_valid: HashMap::new(),
            override_valid: None,
            manual_valid: false,
        })
    }
}

pub fn insert_fluxes_ignore_duplicates(
    conn: &mut Connection,
    cycles: &[Option<Cycle>],
    project: String,
) -> Result<(usize, usize)> {
    let mut inserted = 0;
    let mut skipped = 0;
    let tx = conn.transaction()?; // Start transaction for bulk insertion

    {
        let mut insert_stmt = tx.prepare(&make_insert_or_ignore_fluxes())?;
        for cycle in cycles {
            match cycle {
                Some(c) => {
                    execute_insert(&mut insert_stmt, c, &project)?;
                    inserted += 1;
                },
                None => {
                    skipped += 1;
                },
            }
            // execute_insert(&mut insert_stmt, cycle, &project)?;
            // inserted += 1;
        }
    }

    tx.commit()?;
    Ok((inserted, skipped))
}

pub fn insert_flux_results(
    conn: &mut Connection,
    cycle_id: i64,
    fluxes: HashMap<(GasType, FluxKind), FluxRecord>,
) -> rusqlite::Result<(usize, usize)> {
    let mut inserted = 0;
    let mut skipped = 0;

    let tx = conn.transaction()?;

    {
        let mut stmt = tx.prepare(&make_insert_flux_results())?;

        for (_key, model) in fluxes {
            // Only handling LinearFlux for now — add others as needed
            if let Some(lin) = model.model.as_any().downcast_ref::<LinearFlux>() {
                // Skip if flux is NaN or invalid
                if lin.flux.is_nan() || lin.r2.is_nan() {
                    skipped += 1;
                    continue;
                }

                stmt.execute(params![
                    cycle_id,
                    lin.fit_id,
                    lin.gas_type.to_string(),
                    lin.flux,
                    lin.r2,
                    lin.model.intercept,
                    lin.model.slope,
                    lin.range_start,
                    lin.range_end,
                ])?;
                inserted += 1;
            } else {
                skipped += 1;
            }
        }
    }

    tx.commit()?;
    Ok((inserted, skipped))
}
// pub fn insert_flux_results(
//     conn: &mut Connection,
//     cycle_id: i64,
//     fluxes: HashMap<(GasType, FluxKind), Box<dyn FluxModel>>,
// ) -> rusqlite::Result<(usize, usize)> {
//     let mut inserted = 0;
//     let mut skipped = 0;
//
//     let tx = conn.transaction()?;
//
//     {
//         let mut stmt = tx.prepare(&make_insert_flux_results())?;
//
//         for model in fluxes {
//             // Only handling LinearFlux for now — add others as needed
//             if let Some(lin) = model.as_any().downcast_ref::<LinearFlux>() {
//                 // Skip if flux is NaN or invalid
//                 if lin.flux.is_nan() || lin.r2.is_nan() {
//                     skipped += 1;
//                     continue;
//                 }
//
//                 stmt.execute(params![
//                     cycle_id,
//                     lin.fit_id,
//                     lin.gas_type.to_string(),
//                     lin.flux,
//                     lin.r2,
//                     lin.model.intercept,
//                     lin.model.slope,
//                     lin.range_start,
//                     lin.range_end,
//                 ])?;
//                 inserted += 1;
//             } else {
//                 skipped += 1;
//             }
//         }
//     }
//
//     tx.commit()?;
//     Ok((inserted, skipped))
// }
// pub fn insert_flux_results(
//     conn: &mut Connection,
//     cycle_id: i64,
//     fluxes: &[Box<dyn FluxModel>],
// ) -> Result<usize, usize> {
//     let mut inserted = 0;
//     let mut skipped = 0;
//     let tx = conn.transaction()?;
//
//     {
//         let mut stmt = tx.prepare(
//             "INSERT INTO flux_results (
//             cycle_id, fit_id, gas_type,
//             flux, r2, intercept, slope,
//             range_start, range_end
//         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
//         )?;
//
//         for model in fluxes {
//             if let Some(lin) = model.as_any().downcast_ref::<LinearFlux>() {
//                 match model {
//                     Some(m) => {
//                         stmt.execute(params![
//                             cycle_id,
//                             lin.fit_id,
//                             lin.gas_type.to_string(),
//                             lin.flux,
//                             lin.r2,
//                             lin.model.intercept,
//                             lin.model.slope,
//                             lin.range_start,
//                             lin.range_end,
//                         ])?;
//                     },
//                     None => println!("fail"),
//                 };
//             }
//             // Add other model types here if needed
//         }
//     }
//
//     tx.commit()?;
//     Ok((inserted, skipped))
// }
pub fn update_fluxes(
    conn: &mut Connection,
    cycles: &[Cycle],
    project: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tx = conn.transaction()?; // Start transaction for consistency
    {
        let mut update_stmt = tx.prepare(&make_update_fluxes())?;

        for cycle in cycles {
            match execute_update(&mut update_stmt, cycle, &project) {
                Ok(_) => println!("Fluxes updated successfully!"),
                Err(e) => eprintln!("Error updating fluxes: {}", e),
            }
        }
    }
    tx.commit()?;
    Ok(())
}
pub fn insert_flux_history(
    conn: &mut Connection,
    cycles: &[Cycle],
    project: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let archived_at = Utc::now().to_rfc3339();
    let tx = conn.transaction()?; // Start transaction for consistency
    {
        let mut insert_stmt = tx.prepare(&make_insert_flux_history())?;

        for cycle in cycles {
            match execute_history_insert(&mut insert_stmt, &archived_at, cycle, &project) {
                Ok(_) => println!("Archived cycle successfully."),
                Err(e) => eprintln!("Error archiving fluxes: {}", e),
            }
        }
    }
    tx.commit()?;
    Ok(())
}
fn execute_history_insert(
    stmt: &mut rusqlite::Statement,
    archived_at: &String,
    cycle: &Cycle,
    project: &String,
) -> Result<()> {
    for gas_type in cycle.instrument_model.available_gases() {
        let linear = cycle.fluxes.get(&(gas_type, FluxKind::Linear));
        let polynomial = cycle.fluxes.get(&(gas_type, FluxKind::Poly));
        let robustlinear = cycle.fluxes.get(&(gas_type, FluxKind::RobLin));
        let lin = linear.map(|m| m.model.as_ref());
        let poly = polynomial.map(|m| m.model.as_ref());
        let roblin = robustlinear.map(|m| m.model.as_ref());
        // NOTE: for a specific
        let lin_valid = linear.map(|m| m.is_valid).unwrap_or(false);
        let deadband = cycle.deadbands.get(&gas_type);

        // Skip row if neither model exists
        if linear.is_none() && poly.is_none() && roblin.is_none() {
            eprintln!("Skipping gas {:?}: no models available", gas_type);
            continue;
        }

        stmt.execute(params![
            archived_at,
            cycle.start_time.timestamp(),
            cycle.chamber_id,
            cycle.instrument_model.to_string(),
            cycle.instrument_serial,
            cycle.main_gas.integer_repr(),
            gas_type.integer_repr(),
            project,
            cycle.close_offset,
            cycle.open_offset,
            cycle.end_offset,
            cycle.open_lag_s as i64,
            cycle.close_lag_s as i64,
            cycle.end_lag_s as i64,
            cycle.start_lag_s as i64,
            cycle.air_pressure,
            cycle.air_temperature,
            cycle.chamber_volume,
            cycle.error_code.0,
            cycle.is_valid,
            lin_valid,
            cycle.manual_adjusted,
            cycle.manual_valid,
            deadband,
            cycle.t0_concentration.get(&gas_type).copied().unwrap_or(0.0),
            cycle.measurement_r2.get(&cycle.main_gas).copied().unwrap_or(1.0),
            // Linear fields
            lin.and_then(|m| m.flux()).unwrap_or(0.0),
            lin.and_then(|m| m.r2()).unwrap_or(0.0),
            lin.and_then(|m| m.adj_r2()).unwrap_or(0.0),
            lin.and_then(|m| m.intercept()).unwrap_or(0.0),
            lin.and_then(|m| m.slope()).unwrap_or(0.0),
            lin.and_then(|m| m.sigma()).unwrap_or(0.0),
            lin.and_then(|m| m.p_value()).unwrap_or(1.0),
            lin.and_then(|m| m.aic()).unwrap_or(0.0),
            lin.and_then(|m| m.rmse()).unwrap_or(0.0),
            lin.and_then(|m| m.range_start()).unwrap_or(0.0),
            lin.and_then(|m| m.range_end()).unwrap_or(0.0),
            // Polynomial fields
            poly.and_then(|m| m.flux()).unwrap_or(0.0),
            poly.and_then(|m| m.r2()).unwrap_or(0.0),
            poly.and_then(|m| m.adj_r2()).unwrap_or(0.0),
            poly.and_then(|m| m.sigma()).unwrap_or(0.0),
            poly.and_then(|m| m.aic()).unwrap_or(0.0),
            poly.and_then(|m| m.rmse()).unwrap_or(0.0),
            // Store coefficients if needed (assumes casting to PolyFlux)
            poly.and_then(|m| m.as_any().downcast_ref::<PolyFlux>())
                .map(|m| m.model.a0)
                .unwrap_or(0.0),
            poly.and_then(|m| m.as_any().downcast_ref::<PolyFlux>())
                .map(|m| m.model.a1)
                .unwrap_or(0.0),
            poly.and_then(|m| m.as_any().downcast_ref::<PolyFlux>())
                .map(|m| m.model.a2)
                .unwrap_or(0.0),
            poly.and_then(|m| m.range_start()).unwrap_or(0.0),
            poly.and_then(|m| m.range_end()).unwrap_or(0.0),
            // Roblinear fields
            roblin.and_then(|m| m.flux()).unwrap_or(0.0),
            roblin.and_then(|m| m.r2()).unwrap_or(0.0),
            roblin.and_then(|m| m.adj_r2()).unwrap_or(0.0),
            roblin.and_then(|m| m.intercept()).unwrap_or(0.0),
            roblin.and_then(|m| m.slope()).unwrap_or(0.0),
            roblin.and_then(|m| m.sigma()).unwrap_or(0.0),
            roblin.and_then(|m| m.aic()).unwrap_or(0.0),
            roblin.and_then(|m| m.rmse()).unwrap_or(0.0),
            roblin.and_then(|m| m.range_start()).unwrap_or(0.0),
            roblin.and_then(|m| m.range_end()).unwrap_or(0.0),
        ])?;
    }
    Ok(())
}

fn execute_insert(stmt: &mut rusqlite::Statement, cycle: &Cycle, project: &String) -> Result<()> {
    for gas_type in cycle.instrument_model.available_gases() {
        let linear = cycle.fluxes.get(&(gas_type, FluxKind::Linear));
        let polynomial = cycle.fluxes.get(&(gas_type, FluxKind::Poly));
        let robustlinear = cycle.fluxes.get(&(gas_type, FluxKind::RobLin));
        let lin = linear.map(|m| m.model.as_ref());
        let poly = polynomial.map(|m| m.model.as_ref());
        let roblin = robustlinear.map(|m| m.model.as_ref());
        // NOTE: for a specific
        let lin_valid = linear.map(|m| m.is_valid).unwrap_or(false);
        let deadband = cycle.deadbands.get(&gas_type);

        // Skip row if neither model exists
        if linear.is_none() && poly.is_none() && roblin.is_none() {
            eprintln!("Skipping gas {:?}: no models available", gas_type);
            continue;
        }

        stmt.execute(params![
            cycle.start_time.timestamp(),
            cycle.chamber_id,
            cycle.instrument_model.to_string(),
            cycle.instrument_serial,
            cycle.main_gas.integer_repr(),
            gas_type.integer_repr(),
            project,
            cycle.close_offset,
            cycle.open_offset,
            cycle.end_offset,
            cycle.open_lag_s as i64,
            cycle.close_lag_s as i64,
            cycle.end_lag_s as i64,
            cycle.start_lag_s as i64,
            cycle.air_pressure,
            cycle.air_temperature,
            cycle.chamber_volume,
            cycle.error_code.0,
            cycle.is_valid,
            lin_valid,
            cycle.manual_adjusted,
            cycle.manual_valid,
            deadband,
            cycle.t0_concentration.get(&gas_type).copied().unwrap_or(0.0),
            cycle.measurement_r2.get(&cycle.main_gas).copied().unwrap_or(1.0),
            // Linear fields
            lin.and_then(|m| m.flux()).unwrap_or(0.0),
            lin.and_then(|m| m.r2()).unwrap_or(0.0),
            lin.and_then(|m| m.adj_r2()).unwrap_or(0.0),
            lin.and_then(|m| m.intercept()).unwrap_or(0.0),
            lin.and_then(|m| m.slope()).unwrap_or(0.0),
            lin.and_then(|m| m.sigma()).unwrap_or(0.0),
            lin.and_then(|m| m.p_value()).unwrap_or(1.0),
            lin.and_then(|m| m.aic()).unwrap_or(0.0),
            lin.and_then(|m| m.rmse()).unwrap_or(0.0),
            lin.and_then(|m| m.range_start()).unwrap_or(0.0),
            lin.and_then(|m| m.range_end()).unwrap_or(0.0),
            // Polynomial fields
            poly.and_then(|m| m.flux()).unwrap_or(0.0),
            poly.and_then(|m| m.r2()).unwrap_or(0.0),
            poly.and_then(|m| m.adj_r2()).unwrap_or(0.0),
            poly.and_then(|m| m.sigma()).unwrap_or(0.0),
            poly.and_then(|m| m.aic()).unwrap_or(0.0),
            poly.and_then(|m| m.rmse()).unwrap_or(0.0),
            // Store coefficients if needed (assumes casting to PolyFlux)
            poly.and_then(|m| m.as_any().downcast_ref::<PolyFlux>())
                .map(|m| m.model.a0)
                .unwrap_or(0.0),
            poly.and_then(|m| m.as_any().downcast_ref::<PolyFlux>())
                .map(|m| m.model.a1)
                .unwrap_or(0.0),
            poly.and_then(|m| m.as_any().downcast_ref::<PolyFlux>())
                .map(|m| m.model.a2)
                .unwrap_or(0.0),
            poly.and_then(|m| m.range_start()).unwrap_or(0.0),
            poly.and_then(|m| m.range_end()).unwrap_or(0.0),
            // Roblinear fields
            roblin.and_then(|m| m.flux()).unwrap_or(0.0),
            roblin.and_then(|m| m.r2()).unwrap_or(0.0),
            roblin.and_then(|m| m.adj_r2()).unwrap_or(0.0),
            roblin.and_then(|m| m.intercept()).unwrap_or(0.0),
            roblin.and_then(|m| m.slope()).unwrap_or(0.0),
            roblin.and_then(|m| m.sigma()).unwrap_or(0.0),
            roblin.and_then(|m| m.aic()).unwrap_or(0.0),
            roblin.and_then(|m| m.rmse()).unwrap_or(0.0),
            roblin.and_then(|m| m.range_start()).unwrap_or(0.0),
            roblin.and_then(|m| m.range_end()).unwrap_or(0.0),
        ])?;
    }
    Ok(())
}
fn execute_update(stmt: &mut rusqlite::Statement, cycle: &Cycle, project: &String) -> Result<()> {
    for gas_type in cycle.instrument_model.available_gases() {
        let linear = cycle.fluxes.get(&(gas_type, FluxKind::Linear));
        let polynomial = cycle.fluxes.get(&(gas_type, FluxKind::Poly));
        let robustlinear = cycle.fluxes.get(&(gas_type, FluxKind::RobLin));
        let lin = linear.map(|m| m.model.as_ref());
        let poly = polynomial.map(|m| m.model.as_ref());
        let roblin = robustlinear.map(|m| m.model.as_ref());
        let lin_valid = linear.map(|m| m.is_valid).unwrap_or(false);
        let deadband = cycle.deadbands.get(&gas_type);
        // Skip row if neither model exists
        if linear.is_none() && poly.is_none() && roblin.is_none() {
            eprintln!("Skipping gas {:?}: no models available", gas_type);
            continue;
        }

        stmt.execute(params![
            cycle.start_time.timestamp(),
            cycle.chamber_id,
            cycle.instrument_model.to_string(),
            cycle.instrument_serial,
            cycle.main_gas.integer_repr(),
            gas_type.integer_repr(),
            project,
            cycle.close_offset,
            cycle.open_offset,
            cycle.end_offset,
            cycle.open_lag_s as i64,
            cycle.close_lag_s as i64,
            cycle.end_lag_s as i64,
            cycle.start_lag_s as i64,
            cycle.air_pressure,
            cycle.air_temperature,
            cycle.chamber_volume,
            cycle.error_code.0,
            cycle.is_valid,
            lin_valid,
            cycle.manual_adjusted,
            cycle.manual_valid,
            deadband,
            cycle.t0_concentration.get(&gas_type).copied().unwrap_or(0.0),
            cycle.measurement_r2.get(&cycle.main_gas).copied().unwrap_or(1.0),
            // Linear fields
            lin.and_then(|m| m.flux()).unwrap_or(0.0),
            lin.and_then(|m| m.r2()).unwrap_or(0.0),
            lin.and_then(|m| m.adj_r2()).unwrap_or(0.0),
            lin.and_then(|m| m.intercept()).unwrap_or(0.0),
            lin.and_then(|m| m.slope()).unwrap_or(0.0),
            lin.and_then(|m| m.sigma()).unwrap_or(0.0),
            lin.and_then(|m| m.p_value()).unwrap_or(1.0),
            lin.and_then(|m| m.aic()).unwrap_or(0.0),
            lin.and_then(|m| m.rmse()).unwrap_or(0.0),
            lin.and_then(|m| m.range_start()).unwrap_or(0.0),
            lin.and_then(|m| m.range_end()).unwrap_or(0.0),
            // Polynomial fields
            poly.and_then(|m| m.flux()).unwrap_or(0.0),
            poly.and_then(|m| m.r2()).unwrap_or(0.0),
            poly.and_then(|m| m.adj_r2()).unwrap_or(0.0),
            poly.and_then(|m| m.sigma()).unwrap_or(0.0),
            poly.and_then(|m| m.aic()).unwrap_or(0.0),
            poly.and_then(|m| m.rmse()).unwrap_or(0.0),
            // Store coefficients if needed (assumes casting to PolyFlux)
            poly.and_then(|m| m.as_any().downcast_ref::<PolyFlux>())
                .map(|m| m.model.a0)
                .unwrap_or(0.0),
            poly.and_then(|m| m.as_any().downcast_ref::<PolyFlux>())
                .map(|m| m.model.a1)
                .unwrap_or(0.0),
            poly.and_then(|m| m.as_any().downcast_ref::<PolyFlux>())
                .map(|m| m.model.a2)
                .unwrap_or(0.0),
            poly.and_then(|m| m.range_start()).unwrap_or(0.0),
            poly.and_then(|m| m.range_end()).unwrap_or(0.0),
            // Roblinear fields
            roblin.and_then(|m| m.flux()).unwrap_or(0.0),
            roblin.and_then(|m| m.r2()).unwrap_or(0.0),
            roblin.and_then(|m| m.adj_r2()).unwrap_or(0.0),
            roblin.and_then(|m| m.intercept()).unwrap_or(0.0),
            roblin.and_then(|m| m.slope()).unwrap_or(0.0),
            roblin.and_then(|m| m.sigma()).unwrap_or(0.0),
            roblin.and_then(|m| m.aic()).unwrap_or(0.0),
            roblin.and_then(|m| m.rmse()).unwrap_or(0.0),
            roblin.and_then(|m| m.range_start()).unwrap_or(0.0),
            roblin.and_then(|m| m.range_end()).unwrap_or(0.0),
        ])?;
    }
    Ok(())
}
pub fn load_cycles(
    conn: &Connection,
    project: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) -> Result<Vec<Cycle>> {
    println!("loading cycles");
    let mut date: Option<String> = None;
    let start = start.timestamp();
    let end = end.timestamp();
    let gas_data = query_gas2(conn, start, end, project.to_owned())?;
    let mut stmt = conn.prepare(
        "SELECT * FROM fluxes
         WHERE project_id = ?1 AND start_time BETWEEN ?2 AND ?3
         ORDER BY start_time",
    )?;
    let mut cycle_map: HashMap<CycleKey, Cycle> = HashMap::new();

    let column_names: Vec<String> = stmt.column_names().par_iter().map(|s| s.to_string()).collect();
    let column_index: HashMap<String, usize> =
        column_names.iter().enumerate().map(|(i, name)| (name.clone(), i)).collect();

    let mut rows = stmt.query(params![project, start, end])?;

    while let Some(row) = rows.next()? {
        let deadband = row.get(*column_index.get("deadband").unwrap())?;
        let start_time: i64 = row.get(*column_index.get("start_time").unwrap())?;
        let instrument_serial: String = row.get(*column_index.get("instrument_serial").unwrap())?;
        let project_id: String = row.get(*column_index.get("project_id").unwrap())?;
        let chamber_id: String = row.get(*column_index.get("chamber_id").unwrap())?;
        let key = CycleKey {
            start_time,
            instrument_serial: instrument_serial.clone(),
            project_id: project_id.clone(),
            chamber_id: chamber_id.clone(),
        };
        let model_string: String = row.get(*column_index.get("instrument_model").unwrap())?;
        let instrument_model = InstrumentType::from_str(&model_string);
        let instrument_serial: String = row.get(*column_index.get("instrument_serial").unwrap())?;
        let start_timestamp: i64 = row.get(*column_index.get("start_time").unwrap())?;
        let chamber_id: String = row.get(*column_index.get("chamber_id").unwrap())?;

        let main_gas_i = row.get(*column_index.get("main_gas").unwrap())?;
        let main_gas = GasType::from_int(main_gas_i).unwrap();
        let start_time = chrono::DateTime::from_timestamp(start_timestamp, 0).unwrap();
        let day = start_time.format("%Y-%m-%d").to_string(); // Format as YYYY-MM-DD
        if let Some(prev_date) = date.clone() {
            if prev_date != day {
                progress_sender.send(ProcessEvent::Progress(ProgressEvent::Day(day.clone()))).ok();
            }
        }

        date = Some(day.clone());
        let close_offset: i64 = row.get(*column_index.get("close_offset").unwrap())?;
        let open_offset: i64 = row.get(*column_index.get("open_offset").unwrap())?;
        let end_offset: i64 = row.get(*column_index.get("end_offset").unwrap())?;
        // needs to be on two rows
        let open_lag_s: f64 = row.get(*column_index.get("open_lag_s").unwrap())?;
        let close_lag_s: f64 = row.get(*column_index.get("close_lag_s").unwrap())?;
        let end_lag_s: f64 = row.get(*column_index.get("end_lag_s").unwrap())?;
        let start_lag_s: f64 = row.get(*column_index.get("start_lag_s").unwrap())?;

        let air_pressure: f64 = row.get(*column_index.get("air_pressure").unwrap())?;
        let air_temperature: f64 = row.get(*column_index.get("air_temperature").unwrap())?;
        let chamber_volume: f64 = row.get(*column_index.get("chamber_volume").unwrap())?;

        let end_time = start_time + TimeDelta::seconds(end_offset);

        let error_code_u16: u16 = row.get(*column_index.get("error_code").unwrap())?;
        let error_code = ErrorMask::from_u16(error_code_u16);
        let is_valid: bool = row.get(*column_index.get("measurement_is_valid").unwrap())?;
        let gas_is_valid: bool = row.get(*column_index.get("gas_is_valid").unwrap())?;
        let project_name = row.get(*column_index.get("project_id").unwrap())?;
        let manual_adjusted = row.get(*column_index.get("manual_adjusted").unwrap())?;
        let manual_valid: bool = row.get(*column_index.get("manual_valid").unwrap())?;
        let m_r2: f64 = row.get(*column_index.get("measurement_r2").unwrap())?;

        let mut override_valid = None;
        if manual_valid {
            override_valid = Some(is_valid);
        }

        let mut dt_v = Vec::new();
        let mut diag_v = Vec::new();
        let mut gas_v = HashMap::new();
        let mut measurement_dt_v = Vec::new();
        let mut measurement_diag_v = Vec::new();
        let mut measurement_gas_v = HashMap::new();
        let mut min_y = HashMap::new();
        let mut max_y = HashMap::new();
        let mut deadbands = HashMap::new();
        let mut t0_concentration = HashMap::new();
        let mut measurement_r2 = HashMap::new();
        let measurement_range_start =
            start_time.timestamp() as f64 + close_offset as f64 + close_lag_s + open_lag_s;
        let measurement_range_end =
            start_time.timestamp() as f64 + open_offset as f64 + close_lag_s + open_lag_s;

        if let Some(gas_data_day) = gas_data.get(&day) {
            for (i, gas) in instrument_model.available_gases().iter().enumerate() {
                if let Some(g_values) = gas_data_day.gas.get(&gas) {
                    let (meas_dt, meas_vals) = filter_data_in_range(
                        &gas_data_day.datetime,
                        g_values,
                        start_time.timestamp() as f64,
                        end_time.timestamp() as f64,
                    );
                    let (_, diag_vals) = filter_diag_data(
                        &gas_data_day.datetime,
                        &gas_data_day.diag,
                        start_time.timestamp() as f64,
                        end_time.timestamp() as f64,
                    );
                    if i == 0 {
                        dt_v = meas_dt;
                        diag_v = diag_vals.to_vec();
                    }
                    gas_v.insert(*gas, meas_vals.clone());
                    max_y.insert(*gas, calculate_max_y_from_vec(&meas_vals));
                    min_y.insert(*gas, calculate_min_y_from_vec(&meas_vals));
                    let target =
                        close_offset + close_lag_s as i64 + open_lag_s as i64 + deadband as i64;
                    let end_target = open_offset + close_lag_s as i64 + open_lag_s as i64;

                    let s = target as usize;
                    let mut e = end_target as usize;
                    if e > dt_v.len() {
                        e = dt_v.len()
                    }
                    deadbands.insert(*gas, deadband);

                    let y: Vec<f64> = dt_v[s..e].to_vec();
                    let x: Vec<f64> = meas_vals[s..e].par_iter().map(|g| g.unwrap()).collect();

                    let r2 = stats::pearson_correlation(&x, &y).unwrap_or(0.0).powi(2);
                    let t0 = meas_vals[target as usize].unwrap();
                    t0_concentration.insert(*gas, t0);
                    measurement_r2.insert(*gas, m_r2);
                }
                if i == 0 {
                    let (_, diag_vals) = filter_diag_data(
                        &gas_data_day.datetime,
                        &gas_data_day.diag,
                        start_time.timestamp() as f64,
                        end_time.timestamp() as f64,
                    );
                    diag_v = diag_vals;
                }
            }
        }
        // Get or insert a new Cycle
        let cycle = cycle_map.entry(key.clone()).or_insert_with(|| Cycle {
            id: 0, // you might get this from elsewhere
            chamber_id: chamber_id.clone(),
            instrument_model: InstrumentType::default(), // Set properly if stored
            instrument_serial: instrument_serial.clone(),
            project_name,
            start_time,
            air_temperature,
            air_pressure,
            chamber_volume,
            min_calc_range: MIN_CALC_AREA_RANGE,
            error_code,
            is_valid,
            gas_is_valid: HashMap::new(),
            override_valid,
            manual_valid,
            main_gas,
            close_offset,
            open_offset,
            end_offset,
            open_lag_s,
            close_lag_s,
            deadbands,
            end_lag_s,
            start_lag_s,
            max_idx: 0.0,
            gases: vec![],
            calc_range_start: HashMap::new(),
            calc_range_end: HashMap::new(),
            manual_adjusted,
            min_y,
            max_y,
            flux: HashMap::new(),
            linfit: HashMap::new(),
            measurement_range_start,
            measurement_range_end,
            fluxes: HashMap::new(),
            measurement_r2,
            calc_r2: HashMap::new(),
            dt_v,
            calc_dt_v: HashMap::new(),
            measurement_dt_v,
            gas_v,
            gas_v_mole: HashMap::new(),
            calc_gas_v: HashMap::new(),
            measurement_gas_v,
            measurement_diag_v,
            t0_concentration,
            diag_v,
        });

        // Now add gas-specific data
        let gas_type = GasType::from_int(row.get(*column_index.get("gas").unwrap())?).unwrap();

        if let (
            Ok(flux),
            Ok(r2),
            Ok(adjusted_r2),
            Ok(intercept),
            Ok(slope),
            Ok(sigma),
            Ok(p_value),
            Ok(aic),
            Ok(rmse),
            Ok(calc_start),
            Ok(calc_end),
        ) = (
            row.get(*column_index.get("lin_flux").unwrap()),
            row.get(*column_index.get("lin_r2").unwrap()),
            row.get(*column_index.get("lin_adj_r2").unwrap()),
            row.get(*column_index.get("lin_intercept").unwrap()),
            row.get(*column_index.get("lin_slope").unwrap()),
            row.get(*column_index.get("lin_sigma").unwrap()),
            row.get(*column_index.get("lin_p_value").unwrap()),
            row.get(*column_index.get("lin_aic").unwrap()),
            row.get(*column_index.get("lin_rmse").unwrap()),
            row.get(*column_index.get("lin_range_start").unwrap()),
            row.get(*column_index.get("lin_range_end").unwrap()),
        ) {
            cycle.calc_range_start.insert(gas_type, calc_start);
            cycle.calc_range_end.insert(gas_type, calc_end + 1.);
            let s = (calc_start - cycle.start_time.timestamp() as f64) as usize;
            let e = (calc_end - cycle.start_time.timestamp() as f64) as usize;
            cycle.calc_dt_v.insert(gas_type, cycle.dt_v[s..e].to_vec());
            cycle.calc_gas_v.insert(gas_type, cycle.gas_v.get(&gas_type).unwrap()[s..e].to_vec());
            let lin = LinearFlux {
                fit_id: "linear".to_string(),
                gas_type,
                flux,
                r2,
                adjusted_r2,
                model: LinReg::from_val(intercept, slope),
                sigma,
                p_value,
                aic,
                rmse,
                range_start: calc_start,
                range_end: calc_end,
            };
            cycle.fluxes.insert(
                (gas_type, FluxKind::Linear),
                FluxRecord { model: Box::new(lin), is_valid: gas_is_valid },
            );
        }
        if let (
            Ok(flux),
            Ok(r2),
            Ok(adjusted_r2),
            Ok(intercept),
            Ok(slope),
            Ok(sigma),
            Ok(aic),
            Ok(rmse),
            Ok(calc_start),
            Ok(calc_end),
        ) = (
            row.get(*column_index.get("roblin_flux").unwrap()),
            row.get(*column_index.get("roblin_r2").unwrap()),
            row.get(*column_index.get("roblin_adj_r2").unwrap()),
            row.get(*column_index.get("roblin_intercept").unwrap()),
            row.get(*column_index.get("roblin_slope").unwrap()),
            row.get(*column_index.get("roblin_sigma").unwrap()),
            row.get(*column_index.get("roblin_aic").unwrap()),
            row.get(*column_index.get("roblin_rmse").unwrap()),
            row.get(*column_index.get("roblin_range_start").unwrap()),
            row.get(*column_index.get("roblin_range_end").unwrap()),
        ) {
            cycle.calc_range_start.insert(gas_type, calc_start);
            cycle.calc_range_end.insert(gas_type, calc_end + 1.);
            let s = (calc_start - cycle.start_time.timestamp() as f64) as usize;
            let e = (calc_end - cycle.start_time.timestamp() as f64) as usize;
            cycle.calc_dt_v.insert(gas_type, cycle.dt_v[s..e].to_vec());
            cycle.calc_gas_v.insert(gas_type, cycle.gas_v.get(&gas_type).unwrap()[s..e].to_vec());
            let lin = RobustFlux {
                fit_id: "roblin".to_string(),
                gas_type,
                flux,
                r2,
                adjusted_r2,
                model: RobReg::from_val(intercept, slope),
                sigma,
                aic,
                rmse,
                range_start: calc_start,
                range_end: calc_end,
            };
            cycle.fluxes.insert(
                (gas_type, FluxKind::RobLin),
                FluxRecord { model: Box::new(lin), is_valid: gas_is_valid },
            );
        }
        if let (
            Ok(flux),
            Ok(r2),
            Ok(adjusted_r2),
            Ok(sigma),
            Ok(aic),
            Ok(rmse),
            Ok(a0),
            Ok(a1),
            Ok(a2),
            Ok(calc_start),
            Ok(calc_end),
        ) = (
            row.get(*column_index.get("poly_flux").unwrap()),
            row.get(*column_index.get("poly_r2").unwrap()),
            row.get(*column_index.get("poly_adj_r2").unwrap()),
            row.get(*column_index.get("poly_sigma").unwrap()),
            row.get(*column_index.get("poly_aic").unwrap()),
            row.get(*column_index.get("poly_rmse").unwrap()),
            row.get(*column_index.get("poly_a0").unwrap()),
            row.get(*column_index.get("poly_a1").unwrap()),
            row.get(*column_index.get("poly_a2").unwrap()),
            row.get(*column_index.get("poly_range_start").unwrap()),
            row.get(*column_index.get("poly_range_end").unwrap()),
        ) {
            cycle.calc_range_start.insert(gas_type, calc_start);
            cycle.calc_range_end.insert(gas_type, calc_end + 1.);
            let s = (calc_start - cycle.start_time.timestamp() as f64) as usize;
            let e = (calc_end - cycle.start_time.timestamp() as f64) as usize;
            cycle.calc_dt_v.insert(gas_type, cycle.dt_v[s..e].to_vec());
            cycle.calc_gas_v.insert(gas_type, cycle.gas_v.get(&gas_type).unwrap()[s..e].to_vec());
            let lin = PolyFlux {
                fit_id: "linear".to_string(),
                gas_type,
                flux,
                r2,
                adjusted_r2,
                model: PolyReg::from_coeffs(a0, a1, a2),
                sigma,
                aic,
                rmse,
                range_start: calc_start,
                range_end: calc_end,
                x_offset: calc_start,
            };
            cycle.fluxes.insert(
                (gas_type, FluxKind::Poly),
                FluxRecord { model: Box::new(lin), is_valid: gas_is_valid },
            );
        }

        if !cycle.gases.contains(&gas_type) {
            cycle.gases.push(gas_type);
        }
    }
    let mut cycles: Vec<Cycle> = cycle_map.into_values().collect();
    cycles.sort_by_key(|c| c.start_time);
    if cycles.is_empty() {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    Ok(cycles)
}
fn filter_data_in_range(
    datetimes: &[DateTime<Utc>],
    values: &[Option<f64>],
    range_start: f64,
    range_end: f64,
) -> (Vec<f64>, Vec<Option<f64>>) {
    // Zip the datetimes and values, filter by comparing each datetime's timestamp
    // to the given range, and then unzip the filtered pairs.
    datetimes
        .iter()
        .zip(values.iter())
        .filter(|(dt, _)| {
            let t = dt.timestamp() as f64;
            t >= range_start && t <= range_end
        })
        .map(|(dt, &v)| (dt.timestamp() as f64, v))
        .unzip()
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
        .map(|(dt, &d)| (dt, d))
        .unzip()
}

fn find_best_window_for_gas(
    dt_v: &[f64],
    gas_v: &[Option<f64>],
    gaps: &[bool],
    min_window: usize,
    step: usize,
) -> Option<(usize, usize, f64, Vec<Option<f64>>)> {
    let max_len = gas_v.len();
    let mut best_r2 = f64::MIN;
    let mut best_range = (0, 0);
    let mut best_y = Vec::new();

    for win_size in (min_window..=max_len).step_by(step) {
        for start in (0..=max_len.saturating_sub(win_size)).step_by(step) {
            let end = start + win_size;

            if end - start < min_window {
                continue;
            }

            // Skip window if it has timestamp gaps
            if gaps[start..end.saturating_sub(1)].iter().any(|&gap| gap) {
                continue;
            }

            let window_dt = &dt_v[start..end];
            let window_gas = &gas_v[start..end];

            // Pair and filter out None
            let valid_pairs: Vec<(f64, f64)> = window_dt
                .iter()
                .zip(window_gas.iter())
                .filter_map(|(&x, &y)| y.map(|val| (x, val)))
                .collect();

            if valid_pairs.len() < min_window {
                continue;
            }

            let (x_vals, y_vals): (Vec<f64>, Vec<f64>) = valid_pairs.into_iter().unzip();
            let r2 = stats::pearson_correlation(&x_vals, &y_vals).unwrap_or(0.0).powi(2);

            if r2 > best_r2 {
                best_r2 = r2;
                best_range = (start, end);
                best_y = window_gas.to_vec(); // preserves None values
            }
        }
    }

    if best_range.1 == 0 {
        None
    } else {
        Some((best_range.0, best_range.1, best_r2, best_y))
    }
}

pub fn calculate_max_y_from_vec(values: &[Option<f64>]) -> f64 {
    values.iter().filter_map(|&v| v).filter(|v| !v.is_nan()).fold(f64::NEG_INFINITY, f64::max)
}

pub fn calculate_min_y_from_vec(values: &[Option<f64>]) -> f64 {
    values.iter().filter_map(|&v| v).filter(|v| !v.is_nan()).fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Debug, Clone, Copy)]
    struct DummyFlux {
        aic: f64,
        flux: f64,
    }

    impl FluxModel for DummyFlux {
        fn flux(&self) -> Option<f64> {
            Some(self.flux)
        }

        fn predict(&self, x: f64) -> Option<f64> {
            None
        }
        fn aic(&self) -> Option<f64> {
            Some(self.aic)
        }

        // Minimal dummy implementations
        fn fit_id(&self) -> FluxKind {
            FluxKind::Linear
        }

        fn gas_type(&self) -> GasType {
            GasType::CH4
        }

        fn r2(&self) -> Option<f64> {
            Some(0.0)
        }

        fn adj_r2(&self) -> Option<f64> {
            None
        }

        fn intercept(&self) -> Option<f64> {
            None
        }

        fn slope(&self) -> Option<f64> {
            None
        }

        fn sigma(&self) -> Option<f64> {
            None
        }

        fn p_value(&self) -> Option<f64> {
            None
        }

        fn rmse(&self) -> Option<f64> {
            None
        }

        fn set_range_start(&mut self, _value: f64) {}
        fn set_range_end(&mut self, _value: f64) {}
        fn range_start(&self) -> Option<f64> {
            None
        }
        fn range_end(&self) -> Option<f64> {
            None
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    #[derive(Default)]
    struct DummyCycle {
        fluxes: HashMap<(GasType, FluxKind), Box<dyn FluxModel>>,
    }

    impl DummyCycle {
        pub fn best_flux_by_aic(&self, gas_type: &GasType) -> Option<f64> {
            let candidates = vec![FluxKind::Linear, FluxKind::Poly, FluxKind::RobLin];

            candidates
                .iter()
                .filter_map(|kind| self.fluxes.get(&(*gas_type, *kind)))
                .filter_map(|m| m.aic().map(|aic| (aic, m.flux())))
                .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(_, flux)| flux.unwrap())
        }
    }

    #[test]
    fn test_best_flux_by_aic() {
        let mut cycle = DummyCycle::default();
        cycle.fluxes.insert(
            (GasType::CH4, FluxKind::Linear),
            Box::new(DummyFlux { aic: 100.0, flux: 10.0 }),
        );
        cycle
            .fluxes
            .insert((GasType::CH4, FluxKind::Poly), Box::new(DummyFlux { aic: 80.0, flux: 20.0 }));
        cycle.fluxes.insert(
            (GasType::CH4, FluxKind::RobLin),
            Box::new(DummyFlux { aic: 90.0, flux: 15.0 }),
        );

        let result = cycle.best_flux_by_aic(&GasType::CH4);
        assert_eq!(result, Some(20.0)); // Lowest AIC is 80.0 -> flux = 20.0
    }
}
