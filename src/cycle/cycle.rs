use crate::concentrationunit::ConcentrationUnit;
use crate::constants::MIN_CALC_AREA_RANGE;
use crate::data_formats::gasdata::{query_gas2, query_gas_all};
use crate::errorcode::{ErrorCode, ErrorMask};
use crate::flux::{FluxKind, FluxModel, FluxRecord, LinearFlux, PolyFlux, RobustFlux};
use crate::fluxes_schema::{
    make_insert_flux_history, make_insert_flux_results, make_insert_or_ignore_fluxes,
    make_update_fluxes,
};
use crate::gaschannel::GasChannel;
use crate::gastype::GasType;
use crate::instruments::instruments::{Instrument, InstrumentType};
use crate::processevent::{ProcessEvent, ProgressEvent, QueryEvent};
use crate::stats::{self, LinReg, PolyReg, RobReg};
use crate::ui::validation_ui::GasKey;
use crate::ui::validation_ui::{InstrumentSerial, Mode};
use crate::Project;
use chrono_tz::Tz;

use crate::data_formats::chamberdata::{query_chambers, ChamberShape};
use crate::data_formats::gasdata::GasData;
use crate::data_formats::heightdata::HeightData;
use crate::data_formats::meteodata::MeteoData;
use crate::data_formats::timedata::{get_instrument_by_project_and_id, TimeData};
use crate::types::FastMap;

use chrono::{DateTime, TimeDelta, Utc};
use rayon::prelude::*;
use rusqlite::{params, Connection, Error, Result};
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::Hash;
use std::process;
use tokio::sync::mpsc;

// the window of max r must be at least 240 seconds
pub const MIN_WINDOW_SIZE: f64 = 180.;
// how many seconds to increment the moving window searching for max r
pub const WINDOW_INCREMENT: usize = 1;

// type InstrumentSerial = String;

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
struct CycleKey {
    start_time: i64,
    instrument_serial: InstrumentSerial,
    project_id: i64,
    chamber_id: String,
}

#[derive(Clone)]
pub struct Cycle {
    pub id: i64,
    pub chamber_id: String,
    pub main_instrument: Instrument,
    pub instrument: Instrument,
    pub instruments: FastMap<i64, Instrument>,
    pub chamber: ChamberShape,
    pub project_id: Option<i64>,
    pub start_time: chrono::DateTime<Tz>,
    pub air_temperature: f64,
    pub air_pressure: f64,
    pub chamber_height: f64,
    pub snow_depth_m: f64,
    pub error_code: ErrorMask,
    pub is_valid: bool,
    pub gas_is_valid: FastMap<GasKey, bool>,
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
    pub gases: Vec<GasKey>,
    pub calc_range_start: FastMap<GasKey, f64>,
    pub calc_range_end: FastMap<GasKey, f64>,
    pub manual_adjusted: bool,
    pub min_y: FastMap<GasKey, f64>,
    pub max_y: FastMap<GasKey, f64>,
    pub flux: FastMap<GasKey, f64>,
    pub linfit: FastMap<GasKey, LinReg>,
    pub measurement_range_start: f64,
    pub measurement_range_end: f64,
    pub deadbands: FastMap<GasKey, f64>,

    pub fluxes: FastMap<(GasKey, FluxKind), FluxRecord>,
    pub measurement_r2: FastMap<GasKey, f64>,
    pub calc_r2: FastMap<GasKey, f64>,

    // datetime vectors
    // pub dt_v: Vec<chrono::DateTime<chrono::Utc>>,
    // pub dt_v: Vec<f64>,
    pub dt_v: FastMap<i64, Vec<f64>>,
    // pub dt_v_f: Vec<f64>,
    pub calc_dt_v: FastMap<GasKey, Vec<f64>>,
    pub measurement_dt_v: Vec<f64>,

    // gas vectors
    pub gas_v: FastMap<GasKey, Vec<Option<f64>>>,
    pub gas_v_mole: FastMap<GasKey, Vec<Option<f64>>>,
    pub calc_gas_v: FastMap<GasKey, Vec<Option<f64>>>,
    pub measurement_gas_v: FastMap<GasKey, Vec<Option<f64>>>,
    pub t0_concentration: FastMap<GasKey, f64>,

    pub diag_v: FastMap<i64, Vec<i64>>,
    pub measurement_diag_v: FastMap<i64, Vec<i64>>,
    pub min_calc_len: f64,
    pub gas_channels: FastMap<GasKey, GasChannel>,
}

impl fmt::Debug for Cycle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Cycle start: {}", self.start_time)?;
        writeln!(f, "Fluxes:")?;

        for ((gas_key, flux_kind), flux_record) in &self.fluxes {
            writeln!(f, "  ({}, {}): {}", gas_key, flux_kind, flux_record.model)?;
        }

        Ok(())
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
    pub fn get_lin_r2(&self, key: GasKey) -> Option<f64> {
        if let Some(flux) = self.fluxes.get(&(key, FluxKind::Linear)) {
            return Some(flux.model.r2().unwrap_or(0.0));
        }
        None
    }
    pub fn get_lin_flux(&self, key: &GasKey) -> Option<f64> {
        if let Some(flux) = self.fluxes.get(&(key.clone(), FluxKind::Linear)) {
            return Some(flux.model.flux().unwrap_or(0.0));
        }
        None
    }
    pub fn get_lin_sigma(&self, key: GasKey) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(key, FluxKind::Linear)) {
            return Some(model.model.sigma().unwrap_or(0.0));
        }
        None
    }
    pub fn get_lin_rmse(&self, key: GasKey) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(key, FluxKind::Linear)) {
            return Some(model.model.rmse().unwrap_or(0.0));
        }
        None
    }
    pub fn get_lin_p_value(&self, key: GasKey) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(key, FluxKind::Linear)) {
            return Some(model.model.p_value().unwrap_or(0.0));
        }
        None
    }
    pub fn get_roblin_flux(&self, key: GasKey) -> Option<f64> {
        if let Some(flux) = self.fluxes.get(&(key, FluxKind::RobLin)) {
            return Some(flux.model.flux().unwrap_or(0.0));
        }
        None
    }
    pub fn get_roblin_sigma(&self, key: GasKey) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(key, FluxKind::RobLin)) {
            return Some(model.model.sigma().unwrap_or(0.0));
        }
        None
    }
    pub fn get_roblin_rmse(&self, key: GasKey) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(key, FluxKind::RobLin)) {
            return Some(model.model.rmse().unwrap_or(0.0));
        }
        None
    }
    pub fn get_poly_flux(&self, key: GasKey) -> Option<f64> {
        if let Some(flux) = self.fluxes.get(&(key, FluxKind::Poly)) {
            return Some(flux.model.flux().unwrap_or(0.0));
        }
        None
    }
    pub fn get_poly_sigma(&self, key: GasKey) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(key, FluxKind::Poly)) {
            return Some(model.model.sigma().unwrap_or(0.0));
        }
        None
    }
    pub fn get_poly_rmse(&self, key: GasKey) -> Option<f64> {
        if let Some(model) = self.fluxes.get(&(key, FluxKind::Poly)) {
            return Some(model.model.rmse().unwrap_or(0.0));
        }
        None
    }
    pub fn get_flux(&self, key: GasKey, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(key, kind)).and_then(|m| m.model.flux())
    }

    pub fn get_r2(&self, key: GasKey, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(key, kind)).and_then(|m| m.model.r2())
    }

    pub fn get_adjusted_r2(&self, key: GasKey, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(key, kind)).and_then(|m| m.model.adj_r2())
    }

    pub fn get_aic(&self, key: GasKey, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(key, kind)).and_then(|m| m.model.aic())
    }

    pub fn get_p_value(&self, key: GasKey, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(key, kind)).and_then(|m| m.model.p_value())
    }

    pub fn get_sigma(&self, key: GasKey, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(key, kind)).and_then(|m| m.model.sigma())
    }

    pub fn get_rmse(&self, key: GasKey, kind: FluxKind) -> Option<f64> {
        self.fluxes.get(&(key, kind)).and_then(|m| m.model.rmse())
    }

    pub fn get_model(&self, key: GasKey, kind: FluxKind) -> Option<&dyn FluxModel> {
        self.fluxes.get(&(key, kind)).map(|b| b.model.as_ref())
    }
    pub fn get_adjusted_close_i(&self) -> usize {
        (self.close_offset as f64 + self.open_lag_s + self.close_lag_s) as usize
    }

    pub fn get_adjusted_open_i(&self) -> usize {
        (self.open_offset as f64 + self.open_lag_s) as usize
    }
    pub fn get_adjusted_close(&self) -> f64 {
        self.start_time.timestamp() as f64
            + self.close_offset as f64
            + self.open_lag_s
            + self.close_lag_s
        // self.get_start() + self.close_offset as f64 + self.open_lag_s + self.close_lag_s
        // let idx = (self.close_offset as f64 + self.open_lag_s + self.close_lag_s) as usize;
        // self.dt_v[idx]
    }
    pub fn get_adjusted_open(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.open_offset as f64 + self.open_lag_s
        // let idx = (self.open_offset as f64 + self.open_lag_s) as usize;
        // self.dt_v[idx]
    }
    pub fn get_intercept(&self, key: &GasKey, kind: FluxKind) -> Option<f64> {
        if let Some(flux) = self.fluxes.get(&(key.clone(), kind)) {
            return Some(flux.model.intercept().unwrap());
        }

        None
    }
    pub fn get_slope(&self, key: &GasKey, kind: FluxKind) -> Option<f64> {
        if let Some(flux) = self.fluxes.get(&(key.clone(), kind)) {
            return Some(flux.model.slope().unwrap());
        }

        None
    }

    pub fn toggle_valid(&mut self) {
        self.is_valid = !self.is_valid; // Toggle `is_valid`
    }
    pub fn set_calc_start(&mut self, key: &GasKey, value: f64) {
        let range_min = self.get_adjusted_close() + self.deadbands.get(key).unwrap_or(&0.0);
        // the calc area cant go beyond the measurement area
        if range_min > value {
            self.calc_range_start.insert(key.clone(), range_min);
        } else {
            self.calc_range_start.insert(key.clone(), value);
        }
    }
    pub fn set_calc_end(&mut self, key: &GasKey, value: f64) {
        let range_max = self.get_adjusted_open();
        // the calc area cant go beyond the measurement area
        if value > range_max {
            self.calc_range_end.insert(key.clone(), range_max);
        } else {
            self.calc_range_end.insert(key.clone(), value);
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
    pub fn calculate_concentration_at_t0(&mut self) {
        for key in self.gases.clone() {
            // let gas_v = self.get_measurement_gas_v2(&key);
            let (_, gas_v) = self.get_measurement_data(&key);
            if gas_v.is_empty() {
                self.t0_concentration.insert(key, 0.0);
            } else {
                let t0 = *gas_v.first().unwrap_or(&0.0);
                self.t0_concentration.insert(key, t0);
            }
        }
    }

    pub fn get_deadband(&self, key: &GasKey) -> f64 {
        *self.deadbands.get(key).unwrap_or(&0.0)
    }
    pub fn calc_area_can_move(&self, key: &GasKey) -> bool {
        let s = self.get_calc_start(key);
        let e = self.get_calc_end(key);
        let ms = self.get_adjusted_close() + self.get_deadband(key);
        let me = self.get_adjusted_open();
        let cs_at_ms = s <= ms;
        let ce_at_me = e >= me;

        let calc_at_bounds = cs_at_ms && ce_at_me;
        let at_min_range = self.min_calc_len >= self.get_calc_range(key);
        let check = calc_at_bounds && at_min_range;

        !check
    }

    // fn _adjust_calc_range_all<F>(&mut self, mut adjust_shortfall: F)
    // where
    //     F: FnMut(&mut Self, GasKey, f64),
    // {
    //     let mut shortfall_adjustments = Vec::new();
    //     for key in self.gases.iter().clone() {
    //         let deadband = self.get_deadband(key.clone());
    //         let range_min = self.get_adjusted_close() + deadband;
    //         let range_max = self.get_adjusted_open();
    //         let min_range = self.min_calc_len;
    //
    //         let mut start = *self.calc_range_start.get(&key).unwrap_or(&range_min);
    //         let mut end = *self.calc_range_end.get(&key).unwrap_or(&range_max);
    //
    //         let available_range = range_max - range_min;
    //
    //         // If range is too short, adjust based on logic passed in
    //         if available_range < min_range {
    //             shortfall_adjustments.push((key, available_range - min_range));
    //             // adjust_shortfall(self, key, available_range - min_range);
    //         }
    //
    //         // Clamp to bounds
    //         if start < range_min {
    //             start = range_min;
    //         }
    //         if end > range_max {
    //             end = range_max;
    //         }
    //
    //         // Enforce minimum range
    //         let current_range = end - start;
    //         if current_range < min_range {
    //             let needed = min_range - current_range;
    //             let half = needed / 2.0;
    //
    //             let new_start = (start - half).max(range_min);
    //             let new_end = (end + half).min(range_max);
    //
    //             if new_end - new_start >= min_range {
    //                 start = new_start;
    //                 end = new_end;
    //             } else {
    //                 end = start + min_range;
    //                 if end > range_max {
    //                     start = range_max - min_range;
    //                     end = range_max;
    //                 }
    //             }
    //         }
    //
    //         self.calc_range_start.insert(key.clone(), start);
    //         self.calc_range_end.insert(key.clone(), end);
    //     }
    //     for (key, shortfall) in shortfall_adjustments {
    //         adjust_shortfall(self, key.clone(), shortfall);
    //     }
    // }
    pub fn set_deadband(&mut self, key: &GasKey, deadband: f64) {
        self.deadbands.insert(key.clone(), deadband.max(0.));
        self.adjust_calc_range_all_deadband();

        self.check_errors();
        self.calculate_measurement_rs();
        self.compute_all_fluxes();
    }
    pub fn set_deadband_constant_calc(&mut self, x: f64) {
        for key in self.gases.clone() {
            let deadband = self.deadbands.get(&key).unwrap_or(&0.0);
            let new_db = deadband + x;
            self.deadbands.insert(key.clone(), new_db.max(0.));

            let s = self.get_calc_start(&key);
            let new_s = s + x;
            self.calc_range_start.insert(key.clone(), new_s);

            let e = self.get_calc_end(&key);
            let new_e = e + x;
            self.calc_range_end.insert(key.clone(), new_e);
        }
        self.adjust_calc_range_all();

        self.check_errors();
        self.calculate_measurement_rs();
        self.compute_all_fluxes();
    }
    pub fn set_deadband_and_start(&mut self, x: f64) {
        for key in self.gases.clone() {
            let deadband = self.deadbands.get(&key).unwrap_or(&0.0);
            let new_db = deadband + x;
            self.deadbands.insert(key.clone(), new_db.max(0.));
            let s = self.get_calc_start(&key);
            let new_s = s + x;

            self.calc_range_start.insert(key.clone(), new_s);
        }
        self.adjust_calc_range_all();

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

    pub fn increment_open_lag(&mut self, delta: f64) {
        self.open_lag_s += delta;
        self.adjust_calc_range_all();
        self.check_errors();
        self.calculate_measurement_rs();
        self.calculate_concentration_at_t0();
        self.compute_all_fluxes();
    }

    pub fn increment_close_lag(&mut self, delta: f64) {
        self.close_lag_s += delta;
        self.adjust_calc_range_all();
        self.check_errors();
        self.calculate_measurement_rs();
        self.calculate_concentration_at_t0();
        self.compute_all_fluxes();
    }

    pub fn search_new_open_lag(&mut self, key: GasKey) {
        self.search_open_lag(key);

        self.adjust_calc_range_all();
        self.check_errors();
        self.calculate_measurement_rs();
        self.compute_all_fluxes();
    }

    fn _get_available_range(&self, key: &GasKey) -> f64 {
        let range_min = self.get_adjusted_close() + self.deadbands.get(key).unwrap();
        let range_max = self.get_adjusted_open();
        range_max - range_min
    }
    fn adjust_calc_range_all_deadband(&mut self) {
        for key in self.gases.iter().clone() {
            let mut deadband = self.get_deadband(key);
            let range_min = self.get_adjusted_close() + deadband;
            let range_max = self.get_adjusted_open();
            let min_range = self.min_calc_len;
            let mut start = *self.calc_range_start.get(key).unwrap_or(&range_min);
            let mut end = *self.calc_range_end.get(key).unwrap_or(&range_max);

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
                self.deadbands.insert(key.clone(), deadband);
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

            self.calc_range_start.insert(key.clone(), start);
            self.calc_range_end.insert(key.clone(), end);
        }
    }
    fn adjust_calc_range_all(&mut self) {
        for key in self.gases.iter().clone() {
            let range_min = self.get_adjusted_close() + self.deadbands.get(key).unwrap();
            let range_max = self.get_adjusted_open();
            let min_range = self.min_calc_len;
            let mut start = *self.calc_range_start.get(key).unwrap_or(&range_min);
            let mut end = *self.calc_range_end.get(key).unwrap_or(&range_max);

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

            self.calc_range_start.insert(key.clone(), start);
            self.calc_range_end.insert(key.clone(), end);
        }
    }
    pub fn set_measurement_start(&mut self, value: f64) {
        self.measurement_range_start = value;
    }
    pub fn set_measurement_end(&mut self, value: f64) {
        self.measurement_range_end = value;
    }
    pub fn get_calc_start(&self, key: &GasKey) -> f64 {
        *self.calc_range_start.get(key).unwrap_or(&0.0)
    }
    pub fn get_calc_end(&self, key: &GasKey) -> f64 {
        *self.calc_range_end.get(key).unwrap_or(&0.0)
    }
    pub fn get_calc_start_i(&self, key: &GasKey) -> usize {
        (self.get_calc_start(key) - self.get_start()) as usize
    }
    pub fn get_calc_end_i(&self, key: &GasKey) -> usize {
        (self.get_calc_end(key) - self.get_start()) as usize
    }

    pub fn get_calc_range(&self, key: &GasKey) -> f64 {
        let start = self.get_calc_start(key);
        let end = self.get_calc_end(key);
        end - start
    }
    pub fn get_measurement_start(&self) -> f64 {
        self.start_time.timestamp() as f64
            + self.close_offset as f64
            + self.open_lag_s
            + self.close_lag_s
        // let idx = self.close_offset as f64 + self.open_lag_s + self.close_lag_s;
        // self.dt_v.get(&self.main_instrument_serial).unwrap()[idx as usize]
    }
    pub fn get_measurement_end(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.open_offset as f64 + self.open_lag_s
        // let idx = self.open_offset as f64 + self.open_lag_s;
        // println!("end idx: {}", idx);
        // self.dt_v.get(&self.main_instrument_serial).unwrap()[idx as usize]
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

    /// gets the peak gas concentration +-5 from the given timestamp
    pub fn get_peak_near_timestamp(
        &mut self,
        key: &GasKey,
        target_time: i64, // Now an i64 timestamp
    ) -> Option<f64> {
        if let Some(gas_v) = self.gas_v.get(key) {
            let len = gas_v.len();
            if len < 120 {
                println!("Less than 2minutes of data.");
                return None;
            }

            // Find index closest to `target_time` in `dt_v`
            let target_idx = self
                .dt_v
                .get(&key.id)
                .unwrap()
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
                if let Some(peak_time) = self.dt_v.get(&key.id).unwrap().get(idx).cloned() {
                    let lags = peak_time - (self.start_time.timestamp() + self.open_offset) as f64;
                    self.set_open_lag(lags);

                    return Some(peak_time);
                }
            }
        }
        None
    }
    /// gets the timestamp of the highest gas concentration from the last 240 entries
    pub fn search_open_lag(&mut self, key: GasKey) -> Option<f64> {
        if let Some(gas_v) = self.gas_v.get(&key) {
            let len = gas_v.len();
            let search_len = len / 4;
            if len < 120 {
                return None;
            }

            let start_index = len.saturating_sub(search_len);
            let max_idx = gas_v[start_index..]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(idx, _)| start_index + idx);

            if let Some(idx) = max_idx {
                if let Some(peak_time) = self.dt_v.get(&key.id).unwrap().get(idx) {
                    self.open_lag_s =
                        peak_time - (self.start_time.timestamp() + self.open_offset) as f64;

                    return Some(*peak_time);
                }
            }
        }
        None
    }
    pub fn check_diag(&mut self) {
        // let diag_v = self.get_measurement_diag(&self.main_key());
        // let total_count = diag_v.len();
        // let nonzero_count = diag_v.iter().filter(|&&x| x != 0).count();
        // println!("{}/{}", nonzero_count, total_count);
        let nonzero_count = self
            .diag_v
            .get(&self.main_instrument.id.unwrap())
            .unwrap()
            .iter()
            .filter(|&&x| x != 0)
            .count();
        let total_count = self.diag_v.get(&self.main_instrument.id.unwrap()).unwrap().len();

        // Check if more than 50% of the values are nonzero
        let check = (nonzero_count as f64 / total_count as f64) > 0.5;
        if check {
            self.add_error(ErrorCode::ErrorsInMeasurement)
        } else {
            self.remove_error(ErrorCode::ErrorsInMeasurement)
        }
    }
    pub fn check_measurement_diag(&mut self) -> bool {
        let nonzero_count =
            self.get_measurement_diag(&self.main_key()).iter().filter(|&&x| x != 0).count();
        let check = nonzero_count > 0;
        if check {
            self.add_error(ErrorCode::ErrorsInMeasurement)
        } else {
            self.remove_error(ErrorCode::ErrorsInMeasurement)
        }
        check
    }

    pub fn calculate_max_y(&mut self) {
        for (key, gas_v) in &self.gas_v {
            let max_value = gas_v
            .iter()
            .filter_map(|&v| v) // discard None
            .filter(|v| !v.is_nan())
            .fold(f64::NEG_INFINITY, f64::max);

            self.max_y.insert(key.clone(), max_value);
        }
    }

    pub fn calculate_min_y(&mut self) {
        for (key, gas_v) in &self.gas_v {
            let min_value = gas_v
            .iter()
            .filter_map(|&v| v) // discard None
            .filter(|v| !v.is_nan())
            .fold(f64::INFINITY, f64::min);

            self.min_y.insert(key.clone(), min_value);
        }
    }

    pub fn _calculate_measurement_r(&mut self, key: GasKey) {
        if let Some(gas_v) = self.measurement_gas_v.get(&key) {
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
            self.measurement_r2.insert(key, r2);
        }
    }

    pub fn calculate_calc_r(&mut self, key: GasKey) {
        // let dt = self.get_calc_dt2(&key.clone());
        // let gas = self.get_calc_gas_v(key.clone());
        let (dt, gas) = self.get_calc_data2(&key);

        let filtered: Vec<(&f64, &f64)> = dt.iter().zip(gas.iter()).collect();

        let (x, y): (Vec<f64>, Vec<f64>) = filtered.into_iter().unzip();

        self.calc_r2.insert(key, stats::pearson_correlation(&x, &y).unwrap_or(0.0).powi(2));
    }

    pub fn calculate_calc_rs(&mut self) {
        for key in self.gases.clone() {
            self.calculate_calc_r(key);
        }
    }
    pub fn find_best_r_indices_for_gas(&mut self, key: &GasKey) {
        // Precompute timestamps as float
        // let dt_v: Vec<f64> = self.get_measurement_dt_v2(&key);

        let (dt_v, gas_v) = self.get_measurement_data(key);
        // Precompute timestamp gaps (difference > 1.0 sec)
        let gaps: Vec<bool> = dt_v.windows(2).map(|w| (w[1] - w[0]).abs() > 1.0).collect();

        // let gas_v = self.get_measurement_gas_v2(&key);

        if gas_v.len() < self.min_calc_len as usize || dt_v.len() < self.min_calc_len as usize {
            return;
        }

        if let Some((start, end, _r)) = find_best_window_for_gas_par(
            &dt_v,
            &gas_v,
            &gaps,
            self.min_calc_len as usize,
            WINDOW_INCREMENT,
        ) {
            let start_time = dt_v[start];
            let end_time = dt_v[end - 1];
            self.set_calc_start(key, start_time);
            self.set_calc_end(key, end_time);
        }
    }

    pub fn find_best_r_indices(&mut self) {
        // precompute timestamps as float

        // precompute timestamp gaps (difference > 1.0 sec)
        // let gaps: Vec<bool> = dt_v.windows(2).map(|w| (w[1] - w[0]).abs() > 1.0).collect();

        // prepare gas value vectors
        let mut gas_vecs = FastMap::default();
        let mut dt_vecs = FastMap::default();
        for key in self.gases.clone() {
            let (dv, gv) = self.get_measurement_data(&key);
            // let gv = self.get_measurement_gas_v2(&key);
            // let dv = self.get_measurement_dt_v2(&key);
            gas_vecs.insert(key.clone(), gv);
            dt_vecs.insert(key.clone(), dv);
        }

        // run analysis in parallel for all gases
        let results: Vec<_> = self
            .gases
            .iter()
            .filter_map(|key| {
                let gas_v = gas_vecs.get(key)?;
                let dt_v = dt_vecs.get(key)?;
                let gaps: Vec<bool> = dt_v.windows(2).map(|w| (w[1] - w[0]).abs() > 1.0).collect();

                if gas_v.len() < self.min_calc_len as usize
                    || dt_v.len() < self.min_calc_len as usize
                {
                    return None;
                }

                find_best_window_for_gas_par(
                    dt_v,
                    gas_v,
                    &gaps,
                    self.min_calc_len as usize,
                    WINDOW_INCREMENT, // Assuming this is available
                )
                .map(|(start, end, r)| (key.clone(), start, end, r))
            })
            .collect();

        // Apply results
        for (key, start, end, _) in results {
            let start_time = dt_vecs.get(&key).unwrap()[start];
            let end_time = dt_vecs.get(&key).unwrap()[end - 1];
            // println!("{} {:?}", start_time - self.get_start(), key);
            // println!("{} {:?}", end_time - self.get_start(), key);
            self.set_calc_start(&key, start_time);
            self.set_calc_end(&key, end_time);
        }
    }
    pub fn get_calc_datas(&mut self) {
        for key in &self.gases.clone() {
            self.get_calc_data(key);
        }
    }

    pub fn calculate_measurement_rs2(&mut self) {
        let results: Vec<(GasKey, f64)> = self
            .gases
            .par_iter()
            .filter_map(|key| {
                let (dt_vv, gas_v) = self.get_measurement_data(key);
                // let gas_v = self.get_measurement_gas_v2(key);
                // let dt_vv = self.get_measurement_dt_v2(key); // shared, safe
                if gas_v.len() != dt_vv.len() || gas_v.len() < 5 {
                    return None;
                }

                let r2 = stats::pearson_correlation(&dt_vv, &gas_v).unwrap_or(0.0).powi(2);
                Some((key.clone(), r2))
            })
            .collect();

        for (gas, r2) in results {
            self.measurement_r2.insert(gas, r2);
        }
    }
    pub fn calculate_measurement_rs(&mut self) {
        for key in &self.gases {
            let (dt_vv, gas_v) = self.get_measurement_data(key);
            // let gas_v = self.get_measurement_gas_v2(key);
            // let dt_vv = self.get_measurement_dt_v2(key);

            // let filtered: Vec<(f64, f64)> = dt_vv.iter().zip(gas_v.iter()).collect();
            let filtered: Vec<(f64, f64)> =
                dt_vv.iter().zip(gas_v.iter()).map(|(&dt, &g)| (dt, g)).collect();

            let (x, y): (Vec<f64>, Vec<f64>) = filtered.into_iter().unzip();

            self.measurement_r2
                .insert(key.clone(), stats::pearson_correlation(&x, &y).unwrap_or(0.0).powi(2));
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

    pub fn main_key(&self) -> GasKey {
        GasKey::from((&self.main_gas, &self.main_instrument.id.unwrap()))
    }

    pub fn check_main_r(&mut self) {
        if let Some(r2) = self
            .measurement_r2
            .get(&(GasKey::from((&self.main_gas, &self.main_instrument.id.unwrap()))))
        {
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
        if let Some(values) =
            self.gas_v.get(&(GasKey::from((&self.main_gas, &self.main_instrument.id.unwrap()))))
        {
            let valid_count = values.iter().filter(|v| v.is_some()).count();
            let threshold = self.end_offset as f64 * 0.7;
            let check = (valid_count as f64) < threshold;
            let check2 = values.len() < (self.end_offset as f64 * 0.99) as usize;
            if check || check2 {
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
    pub fn reset_deadbands(&mut self, deadband: f64) {
        for key in &self.gases {
            self.deadbands.insert(key.clone(), deadband);
        }
    }
    pub fn init(
        &mut self,
        use_best_r: bool,
        deadband: f64,
        progress_sender: mpsc::UnboundedSender<ProcessEvent>,
    ) {
        self.manual_adjusted = false;
        self.close_lag_s = 0.;
        self.open_lag_s = 0.;
        self.reset_deadbands(deadband);

        // self.check_diag();
        self.check_missing();

        if !self.has_error(ErrorCode::ErrorsInMeasurement)
            || !self.has_error(ErrorCode::TooFewMeasurements)
        {
            self.search_open_lag(GasKey::from((&self.main_gas, &self.main_instrument.id.unwrap())));
            if use_best_r {
                self.find_best_r_indices();
            } else {
                self.set_calc_ranges();
            }
            self.check_measurement_diag();
            self.calculate_concentration_at_t0();
            self.calculate_measurement_rs();
            self.check_main_r();
            self.compute_all_fluxes();
            self.calculate_max_y();
            self.calculate_min_y();
            self.check_errors();
        } else {
        }
    }

    pub fn set_calc_ranges(&mut self) {
        for key in self.gases.clone() {
            let start = self.get_measurement_start() + self.deadbands.get(&key).unwrap_or(&0.0);
            let end = start + self.min_calc_len;
            // println!("cstart idx{}", self.get_start() - start);
            // println!("cend idx{}", self.get_start() - end);

            self.set_calc_start(&key, start);
            self.set_calc_end(&key, end);
        }
    }
    pub fn set_calc_ranges_to_best_r(&mut self) {
        for key in self.gases.clone() {
            let start = self.get_measurement_start() + self.deadbands.get(&key).unwrap_or(&0.0);
            let end = start + self.min_calc_len;
            self.set_calc_start(&key, start);
            self.set_calc_end(&key, end);
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
        self.find_best_r_indices();
        self.compute_all_fluxes();
    }

    pub fn update_calc_attributes(&mut self, key: &GasKey) {
        // self.get_calc_data(key);
        self.calculate_concentration_at_t0();
        // self.calculate_calc_r(key);
        self.compute_single_flux(key);
    }
    pub fn update_measurement_attributes(&mut self, key: &GasKey) {
        // self.get_measurement_datas();
        self.calculate_measurement_rs();
        // self.get_calc_data(key);
        self.calculate_concentration_at_t0();
        // self.calculate_calc_r(key);
        self.compute_single_flux(key);
    }

    pub fn get_calc_data(&mut self, key: &GasKey) {
        if let Some(gas_v) = self.gas_v.get(key) {
            let s = (self.calc_range_start.get(key).unwrap() - self.start_time.timestamp() as f64)
                as usize;
            let e = (self.calc_range_end.get(key).unwrap() - self.start_time.timestamp() as f64)
                as usize;

            // Clear previous results
            self.calc_gas_v.insert(key.clone(), gas_v[s..e].to_vec());
            self.calc_dt_v.insert(key.clone(), self.dt_v.get(&key.id).unwrap()[s..e].to_vec());
        }
    }
    pub fn get_gas_v(&self, key: &GasKey) -> Vec<f64> {
        self.gas_v
            .get(key)
            .map(|vec| vec.iter().map(|s| s.unwrap_or(f64::NAN)).collect())
            .unwrap_or_default()
    }
    pub fn get_dt_v(&self, key: &GasKey) -> Vec<f64> {
        self.dt_v.clone().get(&key.clone().id).unwrap().to_vec()
    }
    pub fn get_measurement_gas_v2(&self, key: &GasKey) -> Vec<f64> {
        let s = self.get_adjusted_close_i();
        let e = self.get_adjusted_open_i();
        let ret: Vec<f64> = self
            .gas_v
            .get(key)
            .map(|vec| vec.iter().map(|s| s.unwrap_or(f64::NAN)).collect())
            .unwrap_or_default();
        if s > ret.len() {
            return ret;
        }
        ret[s..e].to_vec()
    }
    pub fn get_measurement_dt_v2(&self, key: &GasKey) -> Vec<f64> {
        // let close_time = self.get_adjusted_close() - self.start_time.timestamp() as f64;
        // let open_time = self.get_adjusted_open() - self.start_time.timestamp() as f64;

        // let s = close_time as usize;
        // let e = open_time as usize;
        let s = self.get_adjusted_close_i();
        let e = self.get_adjusted_open_i();
        let ret = self.dt_v.get(&key.id).unwrap();
        if s > ret.len() {
            return ret.to_vec();
        }
        ret[s..e].to_vec()
    }
    pub fn get_measurement_diag(&mut self, key: &GasKey) -> Vec<i64> {
        let start_time = self.get_adjusted_close();
        let end_time = self.get_adjusted_open();

        let dt_vec = match self.dt_v.get(&key.id) {
            Some(vec) => vec,
            None => &vec![],
        };

        let diag_vec = match self.diag_v.get(key.id()) {
            Some(vec) => vec,
            None => &vec![],
        };

        let mut filtered_dt = Vec::new();
        let mut filtered_diag = Vec::new();

        for (i, &t) in dt_vec.iter().enumerate() {
            if t >= start_time && t < end_time {
                filtered_dt.push(t);
                let value = diag_vec.get(i).copied().unwrap_or(0);
                filtered_diag.push(value);
            }
        }

        filtered_diag
    }

    pub fn get_measurement_data(&self, key: &GasKey) -> (Vec<f64>, Vec<f64>) {
        let start_time = self.get_adjusted_close();
        let end_time = self.get_adjusted_open();

        let dt_vec = match self.dt_v.get(&key.id) {
            Some(vec) => vec,
            None => return (vec![], vec![]),
        };

        let gas_vec = match self.gas_v.get(key) {
            Some(vec) => vec,
            None => return (vec![], vec![]),
        };

        let mut filtered_dt = Vec::new();
        let mut filtered_gas = Vec::new();

        for (i, &t) in dt_vec.iter().enumerate() {
            if t >= start_time && t < end_time {
                filtered_dt.push(t);
                let value = gas_vec.get(i).copied().unwrap_or(None).unwrap_or(f64::NAN);
                filtered_gas.push(value);
            }
        }

        (filtered_dt, filtered_gas)
    }
    // pub fn get_measurement_data(&mut self, key: GasKey) {
    //     if let Some(gas_v) = self.gas_v.get(&key) {
    //         let close_time = self.get_adjusted_close();
    //         let open_time = self.get_adjusted_open();
    //
    //         let s = close_time;
    //         let e = open_time;
    //         // let e = s + 150.;
    //         // Clear previous results
    //         self.measurement_gas_v.insert(key, Vec::new());
    //         self.measurement_dt_v.clear();
    //
    //         // Filter and store results in separate vectors
    //         self.dt_v
    //             .iter()
    //             .zip(gas_v.iter()) // Pair timestamps with gas values
    //             .filter(|(t, _)| (t.timestamp() as f64) >= s && (t.timestamp() as f64) <= e) // Filter by time range
    //             .for_each(|(t, d)| {
    //                 self.measurement_dt_v.push(*t);
    //                 self.measurement_gas_v.get_mut(&key).unwrap().push(*d);
    //             });
    //     } else {
    //         println!("No gas data found for {}", key);
    //     }
    // }

    // pub fn calculate_slope(&mut self, key: GasType) {
    //     if let Some(gas_v) = self.calc_gas_v.get(&key) {
    //         let time_vec: Vec<f64> = self
    //             .calc_dt_v
    //             .get(&key)
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
    //         self.linfit.insert(key, linreg);
    //     } else {
    //         self.linfit.insert(key, LinReg::default());
    //     }
    // }

    pub fn compute_all_fluxes(&mut self) {
        for key in &self.gases.clone() {
            self.calculate_lin_flux(key.clone());
            self.calculate_poly_flux(key.clone());
            self.calculate_roblin_flux(key.clone());
        }
    }
    pub fn compute_single_flux(&mut self, key: &GasKey) {
        self.calculate_lin_flux(key.clone());
        self.calculate_poly_flux(key.clone());
        self.calculate_roblin_flux(key.clone());
    }

    // pub fn get_calc_dt(&self, key: GasType) -> Vec<f64> {
    //     let ret: Vec<f64> = *self.calc_dt_v.get(&key).unwrap_or(&Vec::new());
    //     ret
    // }
    pub fn _get_calc_dt2(&self, key: GasKey) -> Vec<f64> {
        let s = (self.get_calc_start(&key) - self.start_time.timestamp() as f64) as usize;
        let e = (self.get_calc_end(&key) - self.start_time.timestamp() as f64) as usize;
        let ret: Vec<f64> = self.dt_v.get(&key.id).unwrap().clone();
        if s > ret.len() {
            return ret;
        }
        ret[s..e].to_vec()
    }
    pub fn _get_calc_gas_v2(&self, key: GasKey) -> Vec<f64> {
        let s = (self.get_calc_start(&key) - self.start_time.timestamp() as f64) as usize;
        let e = (self.get_calc_end(&key) - self.start_time.timestamp() as f64) as usize;
        let ret: Vec<f64> = self
            .gas_v
            .get(&key)
            .map(|vec| vec.iter().map(|s| s.unwrap_or(0.0)).collect())
            .unwrap_or_default();
        if s > ret.len() {
            println!("END BEYOND DATA e: {}, l {}", e, ret.len());
            return ret;
        }
        ret[s..e].to_vec()
    }
    pub fn get_calc_dt2(&self, key: &GasKey) -> Vec<f64> {
        let s = self.get_calc_start_i(key);
        let e = self.get_calc_end_i(key);
        let ret = self.get_dt_v(key);
        if s > ret.len() || e > ret.len() {
            return ret;
        }
        ret[s..e].to_vec()
    }
    pub fn get_calc_gas_v2(&self, key: &GasKey) -> Vec<f64> {
        let s = self.get_calc_start_i(key);
        let e = self.get_calc_end_i(key);
        let ret = self.get_gas_v(key);

        if s > ret.len() {
            return ret;
        }
        ret[s..e].to_vec()
    }

    pub fn get_calc_data2(&self, key: &GasKey) -> (Vec<f64>, Vec<f64>) {
        let start_time = self.get_calc_start(key);
        let end_time = self.get_calc_end(key);

        let dt_vec = match self.dt_v.get(&key.id) {
            Some(vec) => vec,
            None => return (vec![], vec![]),
        };

        let gas_vec = match self.gas_v.get(key) {
            Some(vec) => vec,
            None => return (vec![], vec![]),
        };

        let mut filtered_dt = Vec::new();
        let mut filtered_gas = Vec::new();

        for (i, &t) in dt_vec.iter().enumerate() {
            if t >= start_time && t < end_time {
                filtered_dt.push(t);
                let gas_value = gas_vec.get(i).and_then(|v| *v).unwrap_or(f64::NAN);
                filtered_gas.push(gas_value);
            }
        }

        (filtered_dt, filtered_gas)
    }
    // pub fn get_measurement_dt_v(&self) -> Vec<f64> {
    //     self.measurement_dt_v.iter().map(|s| s.timestamp() as f64).collect()
    // }
    pub fn get_measurement_gas_v(&self, key: GasKey) -> Vec<f64> {
        self.measurement_gas_v
            .get(&key)
            .map(|vec| vec.iter().map(|s| s.unwrap_or(0.0)).collect())
            .unwrap_or_default()
    }

    pub fn get_calc_gas_v(&self, key: GasKey) -> Vec<f64> {
        self.calc_gas_v
            .get(&key)
            .map(|vec| vec.iter().map(|s| s.unwrap_or(0.0)).collect())
            .unwrap_or_default()
    }

    pub fn calculate_lin_flux(&mut self, key: GasKey) {
        // println!("{:?}", key);
        // let x = self.get_calc_dt2(&key);
        // let y = self.get_calc_gas_v2(&key);
        let (x, y) = self.get_calc_data2(&key);
        // println!("diff {}", x.first().unwrap() - self.get_start());
        // println!("y1 {}", self.get_calc_start_i(&key));
        // println!("y2 {}", self.get_calc_end_i(&key));
        // println!("diff {}", x.first().unwrap() - self.get_measurement_start());
        let s = x.first().unwrap_or(&0.);
        let e = x.last().unwrap_or(&0.);

        let ss = self.get_calc_start(&key);
        let ee = self.get_calc_end(&key);
        // if &ss != s {
        //     println!("bad! s: {} ss: {}", s, ss)
        // } else {
        //     println!("pass s: {} ss: {}", s, ss)
        // }
        // if &ee != e {
        //     println!("bad! e: {} ee: {}", e, ee)
        // } else {
        //     println!("pass s: {} ss: {}", s, ss)
        // }
        // println!("xl: {}", x.len());
        // println!("yl: {}", y.len());
        // let pt_count = 300;
        //
        // let x = self.get_measurement_dt_v()[..pt_count].to_vec();
        // let y = self.get_measurement_gas_v(key)[..pt_count].to_vec();
        // let s = x.first().unwrap_or(&0.);
        // let e = x.last().unwrap_or(&0.);

        if x.len() < 2 || y.len() < 2 || x.len() != y.len() {
            // Optionally: log or emit warning here
            return; // Not enough data to fit
        }

        // println!("channel {:?}", self.gas_channels);
        // println!("key {}", &key);
        let channel = self.gas_channels.get(&key).unwrap().clone();
        if let Some(data) = LinearFlux::from_data(
            "lin",
            channel,
            &x,
            &y,
            *s,
            *e,
            self.air_temperature,
            self.air_pressure,
            self.chamber,
        ) {
            self.fluxes.insert(
                (key, FluxKind::Linear),
                FluxRecord {
                    model: Box::new(data),
                    is_valid: true, // default to valid unless user invalidates later
                },
            );
        } else {
            // Optionally log: fitting failed
        }
    }
    pub fn calculate_poly_flux(&mut self, key: GasKey) {
        // let x = self.get_measurement_dt_v().to_vec();
        // let y = self.get_measurement_gas_v(key).to_vec();
        // let x = self.get_calc_dt2(&key).to_vec();
        // let y = self.get_calc_gas_v2(&key).to_vec();
        let (x, y) = self.get_calc_data2(&key);
        let s = x.first().unwrap_or(&0.);
        let e = x.last().unwrap_or(&0.);
        // let pt_count = 300;
        //
        // let x = self.get_measurement_dt_v()[..pt_count].to_vec();
        // let y = self.get_measurement_gas_v(key)[..pt_count].to_vec();
        // let s = x.first().unwrap_or(&0.);
        // let e = x.last().unwrap_or(&0.);

        // Ensure valid input
        if x.len() < 3 || y.len() < 3 || x.len() != y.len() {
            // Optional: log or notify
            eprintln!(
                "Insufficient data for polynomial flux on gas {:?}: x = {}, y = {}",
                key.gas_type,
                x.len(),
                y.len()
            );
            return;
        }

        let channel = self.gas_channels.get(&key).unwrap().clone();
        // Fit and insert if successful
        if let Some(data) = PolyFlux::from_data(
            "poly",
            channel,
            &x,
            &y,
            *s,
            *e,
            self.air_temperature,
            self.air_pressure,
            self.chamber.clone(),
        ) {
            // println!("{}", data);
            self.fluxes.insert(
                (key, FluxKind::Poly),
                FluxRecord { model: Box::new(data), is_valid: true },
            );
        } else {
            eprintln!("Polynomial regression failed for gas {:?}", key.gas_type);
        }
    }
    pub fn calculate_roblin_flux(&mut self, key: GasKey) {
        // let x = self.get_calc_dt2(&key).to_vec();
        // let y = self.get_calc_gas_v2(&key).to_vec();
        let (x, y) = self.get_calc_data2(&key);
        let s = x.first().unwrap_or(&0.);
        let e = x.last().unwrap_or(&0.);
        // let pt_count = 300;
        //
        // let x = self.get_measurement_dt_v()[..pt_count].to_vec();
        // let y = self.get_measurement_gas_v(key)[..pt_count].to_vec();
        // let s = x.first().unwrap_or(&0.);
        // let e = x.last().unwrap_or(&0.);

        if x.len() < 2 || y.len() < 2 || x.len() != y.len() {
            // Optionally: log or emit warning here
            return; // Not enough data to fit
        }

        let channel = self.gas_channels.get(&key).unwrap().clone();
        if let Some(data) = RobustFlux::from_data(
            "roblin",
            channel,
            &x,
            &y,
            *s,
            *e,
            self.air_temperature,
            self.air_pressure,
            self.chamber.clone(),
        ) {
            // println!("{}", data);
            self.fluxes.insert(
                (key, FluxKind::RobLin),
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
        let volume_m3 = self.chamber_height / 1000.0; // Convert L to m³

        let conversion_factor = (pressure_pa * volume_m3) / (R * temperature_k); // mol / mol-fraction
        let ppb_to_nmol = conversion_factor * 1e-9 * 1e9; // mol → nmol, and ppb = 1e-9
        let mut converted: FastMap<GasKey, Vec<Option<f64>>> = FastMap::default();
        for key in self.gases.clone() {
            if let Some(values) = self.gas_v.get(&key) {
                let new_vals = values.iter().map(|v| v.map(|val| val * ppb_to_nmol)).collect();
                converted.insert(key, new_vals);
            }
            // if let Some(values) = self.gas_v.get_mut(&key) {
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
        // println!("###### Reload gas data #######");
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
        let end = match DateTime::from_timestamp(self.get_end() as i64 - 1, 0) {
            Some(dt) => dt,
            None => {
                eprintln!("Invalid end timestamp for cycle");
                return;
            },
        };

        let tz = self.start_time.timezone();
        match query_gas_all(&conn, start, end, tz, self.project_id.unwrap()) {
            Ok(gasdata) => {
                self.gas_v = gasdata.gas;
                // self.dt_v = gasdata.datetime.iter().map(|t| t.timestamp() as f64).collect();
                self.dt_v = gasdata
                    .datetime
                    .iter()
                    .map(|(id, dt_list)| {
                        let timestamps =
                            dt_list.iter().map(|t| t.timestamp() as f64).collect::<Vec<f64>>();
                        (*id, timestamps)
                    })
                    .collect();
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
    pub fn best_flux_by_aic(&self, key: &GasKey) -> Option<f64> {
        let candidates = FluxKind::all();

        candidates
            .iter()
            .filter_map(|kind| self.fluxes.get(&(key.clone(), *kind)))
            .filter_map(|m| m.model.aic().map(|aic| (aic, m.model.flux())))
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(_, flux)| flux.unwrap())
    }
    pub fn best_model_by_aic(&self, key: &GasKey) -> Option<FluxKind> {
        let candidates = FluxKind::all();

        candidates
            .iter()
            .filter_map(|kind| self.fluxes.get(&(key.clone(), *kind)))
            .filter_map(|m| m.model.aic().map(|aic| (aic, m.model.fit_id())))
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(_, fit_id)| fit_id)
    }

    pub fn is_valid_by_threshold(
        &self,
        key: GasKey,
        kind: FluxKind,
        p_val_thresh: f64,
        r2_thresh: f64,
        rmse_thresh: f64,
        t0_thresh: f64,
    ) -> bool {
        let flux = match self.fluxes.get(&(key.clone(), kind)) {
            Some(f) => f,
            None => return false,
        };

        let p_val = flux.model.p_value().unwrap_or(0.);
        let rmse = flux.model.rmse().unwrap_or(f64::MAX);

        let r2 = match self.measurement_r2.get(&key) {
            Some(val) => *val,
            None => return false,
        };

        let t0 = match self.t0_concentration.get(&key) {
            Some(val) => *val,
            None => return false,
        };

        p_val < p_val_thresh && r2 > r2_thresh && rmse < rmse_thresh && t0 < t0_thresh
    }

    pub fn mark_flux_invalid(&mut self, key: GasKey, kind: FluxKind) {
        if let Some(record) = self.fluxes.get_mut(&(key, kind)) {
            record.is_valid = false;
        }
    }

    pub fn mark_flux_valid(&mut self, key: GasKey, kind: FluxKind) {
        if let Some(record) = self.fluxes.get_mut(&(key, kind)) {
            record.is_valid = true;
        }
    }
}
#[derive(Debug, Default, Clone)]
pub struct CycleBuilder {
    chamber_id: Option<String>,
    start_time: Option<DateTime<Tz>>,
    close_offset: Option<i64>,
    open_offset: Option<i64>,
    end_offset: Option<i64>,
    min_calc_len: Option<f64>,
    snow_depth: Option<f64>,
    project: Option<Project>,
    id: Option<i64>,
    instrument_model: Option<InstrumentType>,
    instrument_serial: Option<String>,
    instrument_id: Option<i64>,
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
            min_calc_len: None,
            snow_depth: None,
            project: None,
            id: None,
            instrument_model: None,
            instrument_serial: None,
            instrument_id: None,
        }
    }

    /// Set the chamber ID
    pub fn chamber_id(mut self, id: String) -> Self {
        self.chamber_id = Some(id);
        self
    }

    /// Set the start time
    pub fn start_time(mut self, time: DateTime<Tz>) -> Self {
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
    pub fn project_id(mut self, id: i64) -> Self {
        self.id = Some(id);
        self
    }
    pub fn project(mut self, project: Project) -> Self {
        self.project = Some(project);
        self
    }
    pub fn min_calc_len(mut self, min_calc_len: f64) -> Self {
        self.min_calc_len = Some(min_calc_len);
        self
    }
    pub fn instrument_id(mut self, instrument_id: i64) -> Self {
        self.instrument_id = Some(instrument_id);
        self
    }
    pub fn instrument_serial(mut self, instrument_serial: String) -> Self {
        self.instrument_serial = Some(instrument_serial);
        self
    }
    pub fn instrument_model(mut self, instrument_model: InstrumentType) -> Self {
        self.instrument_model = Some(instrument_model);
        self
    }
    pub fn snow_depth(mut self, snow_depth: f64) -> Self {
        self.snow_depth = Some(snow_depth);
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
            main_instrument: Instrument::default(),
            instrument: Instrument::default(),
            instruments: FastMap::default(),
            chamber: ChamberShape::Box {
                width_m: 1.,
                length_m: 1.,
                height_m: 1.,
                snow_height_m: 0.,
            },
            snow_depth_m: 0.,
            project_id: None,
            min_calc_len: MIN_CALC_AREA_RANGE,
            close_offset: close,
            open_offset: open,
            end_offset: end,
            main_gas: GasType::CH4,
            error_code: ErrorMask(0),
            manual_adjusted: false,
            calc_range_end: FastMap::default(),
            calc_range_start: FastMap::default(),
            min_y: FastMap::default(),
            max_y: FastMap::default(),
            t0_concentration: FastMap::default(),
            open_lag_s: 0.,
            close_lag_s: 0.,
            end_lag_s: 0.,
            start_lag_s: 0.,
            deadbands: FastMap::default(),
            max_idx: 0.,
            flux: FastMap::default(),
            fluxes: FastMap::default(),
            linfit: FastMap::default(),
            calc_r2: FastMap::default(),
            measurement_r2: FastMap::default(),
            measurement_range_start: 0.,
            measurement_range_end: 0.,
            diag_v: FastMap::default(),
            dt_v: FastMap::default(),
            gas_v: FastMap::default(),
            gas_v_mole: FastMap::default(),
            calc_gas_v: FastMap::default(),
            calc_dt_v: FastMap::default(),
            measurement_gas_v: FastMap::default(),
            measurement_dt_v: vec![],
            measurement_diag_v: FastMap::default(),
            gases: vec![],
            air_pressure: 1000.,
            air_temperature: 10.,
            chamber_height: 1.,
            is_valid: true,
            gas_is_valid: FastMap::default(),
            override_valid: None,
            manual_valid: false,
            gas_channels: FastMap::default(),
        })
    }
    pub fn build(self) -> Result<Cycle, Box<dyn std::error::Error + Send + Sync>> {
        let start = self.start_time.ok_or("Start time is required")?;
        let chamber = self.chamber_id.ok_or("Chamber ID is required")?;
        let close = self.close_offset.ok_or("Close offset is required")?;
        let open = self.open_offset.ok_or("Open offset is required")?;
        let end = self.end_offset.ok_or("End offset is required")?;
        // FIX: snow_depth is unused
        let snow_depth_m = self.snow_depth.ok_or("Snow depth is required")?;
        let instrument_id = self.instrument_id.ok_or("Instrument id is required")?;
        let project = self.project.ok_or("Project is required")?;
        let min_calc_len = self.min_calc_len.ok_or("Project is required")?;

        let conn = Connection::open("fluxrs.db").expect("Failed to open database");
        let instrument = get_instrument_by_project_and_id(
            &conn,
            project.id.unwrap(),
            self.instrument_id.unwrap(),
        )
        .expect("Failure");
        // let instrument = match get_instrument_by_project_and_id(
        //     &conn,
        //     project.id.unwrap(),
        //     self.instrument_id.unwrap(),
        // )? {
        //     Some(inst) => inst,
        //     None => {
        //         eprintln!(
        //             "Instrument not found for project={} id={}",
        //             project.id.unwrap(),
        //             self.instrument_id.unwrap()
        //         );
        //     },
        // };

        Ok(Cycle {
            id: 0,
            chamber_id: chamber,
            main_instrument: instrument.clone().unwrap(),
            instrument: instrument.unwrap(),
            instruments: FastMap::default(),
            chamber: ChamberShape::Box {
                width_m: 1.,
                length_m: 1.,
                height_m: 1.,
                snow_height_m: 0.,
            },
            snow_depth_m,
            min_calc_len,
            project_id: project.id,
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
            calc_range_end: FastMap::default(),
            calc_range_start: FastMap::default(),
            min_y: FastMap::default(),
            max_y: FastMap::default(),
            t0_concentration: FastMap::default(),
            open_lag_s: 0.,
            close_lag_s: 0.,
            end_lag_s: 0.,
            start_lag_s: 0.,
            deadbands: FastMap::default(),
            max_idx: 0.,
            flux: FastMap::default(),
            fluxes: FastMap::default(),
            linfit: FastMap::default(),
            calc_r2: FastMap::default(),
            measurement_r2: FastMap::default(),
            measurement_range_start: 0.,
            measurement_range_end: 0.,
            diag_v: FastMap::default(),
            dt_v: FastMap::default(),
            gas_v: FastMap::default(),
            gas_v_mole: FastMap::default(),
            calc_gas_v: FastMap::default(),
            calc_dt_v: FastMap::default(),
            measurement_gas_v: FastMap::default(),
            measurement_dt_v: vec![],
            measurement_diag_v: FastMap::default(),
            gases: vec![],
            air_pressure: 1000.,
            air_temperature: 10.,
            chamber_height: 1.,
            is_valid: true,
            gas_is_valid: FastMap::default(),
            override_valid: None,
            manual_valid: false,
            gas_channels: FastMap::default(),
        })
    }
}

pub fn insert_fluxes_ignore_duplicates(
    conn: &mut Connection,
    cycles: &[Option<Cycle>],
    project_id: &i64,
) -> Result<(usize, usize)> {
    let mut inserted = 0;
    // BUG: figure out this skipped logic... it starts in cycle processing when inserting nones
    let mut skipped = 0;
    let tx = conn.transaction()?; // Start transaction for bulk insertion

    {
        let mut insert_stmt = tx.prepare(&make_insert_or_ignore_fluxes())?;
        for cycle in cycles {
            match cycle {
                Some(c) => {
                    let affected = execute_insert(&mut insert_stmt, c, &project_id)?;
                    if affected > 0 {
                        inserted += 1
                    } else {
                        skipped += 1
                    }
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
    fluxes: FastMap<(GasKey, FluxKind), FluxRecord>,
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
                    lin.gas_channel.gas.to_string(),
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
pub fn update_fluxes(
    conn: &mut Connection,
    cycles: &[Cycle],
    project: Project,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tx = conn.transaction()?; // Start transaction for consistency
    {
        let mut update_stmt = tx.prepare(&make_update_fluxes())?;

        for cycle in cycles {
            match execute_update(&mut update_stmt, cycle, &project.id.unwrap()) {
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
    project: &Project,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let archived_at = Utc::now().to_rfc3339();
    let tx = conn.transaction()?; // Start transaction for consistency
    {
        let mut insert_stmt = tx.prepare(&make_insert_flux_history())?;

        for cycle in cycles {
            match execute_history_insert(&mut insert_stmt, &archived_at, cycle, &project.name) {
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
    for key in cycle.gases.clone() {
        let linear = cycle.fluxes.get(&(key.clone(), FluxKind::Linear));
        let polynomial = cycle.fluxes.get(&(key.clone(), FluxKind::Poly));
        let robustlinear = cycle.fluxes.get(&(key.clone(), FluxKind::RobLin));
        let lin = linear.map(|m| m.model.as_ref());
        let poly = polynomial.map(|m| m.model.as_ref());
        let roblin = robustlinear.map(|m| m.model.as_ref());
        // NOTE: for a specific
        let lin_valid = linear.map(|m| m.is_valid).unwrap_or(false);
        let deadband = cycle.deadbands.get(&key);

        // Skip row if neither model exists
        if linear.is_none() && poly.is_none() && roblin.is_none() {
            eprintln!("Skipping gas {:?}: no models available", key);
            continue;
        }

        stmt.execute(params![
            archived_at,
            cycle.start_time.timestamp(),
            cycle.chamber_id,
            cycle.main_instrument.id,
            cycle.instrument.id,
            cycle.main_gas.as_int(),
            key.gas_type.as_int(),
            project,
            cycle.close_offset,
            cycle.open_offset,
            cycle.end_offset,
            cycle.open_lag_s as i64,
            cycle.close_lag_s as i64,
            cycle.end_lag_s as i64,
            cycle.start_lag_s as i64,
            cycle.min_calc_len,
            cycle.air_pressure,
            cycle.air_temperature,
            cycle.chamber_height,
            cycle.snow_depth_m,
            cycle.error_code.0,
            cycle.is_valid,
            lin_valid,
            cycle.manual_adjusted,
            cycle.manual_valid,
            deadband,
            cycle.t0_concentration.get(&key).copied().unwrap_or(0.0),
            cycle
                .measurement_r2
                .get(&(GasKey::from((&cycle.main_gas, &cycle.instrument.id.unwrap()))))
                .copied()
                .unwrap_or(0.0),
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

fn execute_insert(
    stmt: &mut rusqlite::Statement,
    cycle: &Cycle,
    project_id: &i64,
) -> Result<usize> {
    let mut affected = 0;
    for key in cycle.gases.clone() {
        let linear = cycle.fluxes.get(&(key.clone(), FluxKind::Linear));
        let polynomial = cycle.fluxes.get(&(key.clone(), FluxKind::Poly));
        let robustlinear = cycle.fluxes.get(&(key.clone(), FluxKind::RobLin));
        let lin = linear.map(|m| m.model.as_ref());
        let poly = polynomial.map(|m| m.model.as_ref());
        let roblin = robustlinear.map(|m| m.model.as_ref());
        // NOTE: FluxRecord is gas specific
        let lin_valid = linear.map(|m| m.is_valid).unwrap_or(false);
        let deadband = cycle.deadbands.get(&key);
        let measurement_r2 = cycle.measurement_r2.get(&key).unwrap_or(&0.0);
        // Skip row if no model exists
        if linear.is_none() && poly.is_none() && roblin.is_none() {
            eprintln!("Skipping gas {} {}: no models available", key, cycle.start_time);
            continue;
        }

        let tx = Connection::open("fluxrs.db").expect("Failed to open database");
        let instrument_id = &key.id;

        let inserts = stmt.execute(params![
            cycle.start_time.timestamp(),
            cycle.chamber_id,
            cycle.main_instrument.id,
            instrument_id,
            cycle.main_gas.as_int(),
            key.gas_type.as_int(),
            project_id,
            cycle.id,
            cycle.close_offset,
            cycle.open_offset,
            cycle.end_offset,
            cycle.open_lag_s as i64,
            cycle.close_lag_s as i64,
            cycle.end_lag_s as i64,
            cycle.start_lag_s as i64,
            cycle.min_calc_len,
            cycle.air_pressure,
            cycle.air_temperature,
            cycle.chamber_height,
            cycle.snow_depth_m,
            cycle.error_code.0,
            cycle.is_valid,
            lin_valid,
            cycle.manual_adjusted,
            cycle.manual_valid,
            deadband,
            cycle.t0_concentration.get(&key).copied().unwrap_or(0.0),
            measurement_r2,
            // cycle
            //     .measurement_r2
            //     .get(&(GasKey::from((&cycle.main_gas, cycle.instrument_serial.as_str()))))
            //     .copied()
            //     .unwrap_or(0.0),
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
        affected += inserts;
    }
    Ok(affected)
}
fn execute_update(stmt: &mut rusqlite::Statement, cycle: &Cycle, project_id: &i64) -> Result<()> {
    for key in cycle.gases.clone() {
        let linear = cycle.fluxes.get(&(key.clone(), FluxKind::Linear));
        let polynomial = cycle.fluxes.get(&(key.clone(), FluxKind::Poly));
        let robustlinear = cycle.fluxes.get(&(key.clone(), FluxKind::RobLin));
        let lin = linear.map(|m| m.model.as_ref());
        let poly = polynomial.map(|m| m.model.as_ref());
        let roblin = robustlinear.map(|m| m.model.as_ref());
        let lin_valid = linear.map(|m| m.is_valid).unwrap_or(false);
        let deadband = cycle.deadbands.get(&key);
        // Skip row if neither model exists
        if linear.is_none() && poly.is_none() && roblin.is_none() {
            eprintln!("Skipping gas {:?}: no models available", key);
            continue;
        }

        stmt.execute(params![
            cycle.start_time.timestamp(),
            cycle.chamber_id,
            cycle.main_instrument.id,
            cycle.instrument.id,
            cycle.main_gas.as_int(),
            key.gas_type.as_int(),
            project_id,
            cycle.id,
            cycle.close_offset,
            cycle.open_offset,
            cycle.end_offset,
            cycle.open_lag_s as i64,
            cycle.close_lag_s as i64,
            cycle.end_lag_s as i64,
            cycle.start_lag_s as i64,
            cycle.min_calc_len,
            cycle.air_pressure,
            cycle.air_temperature,
            cycle.chamber_height,
            cycle.snow_depth_m,
            cycle.error_code.0,
            cycle.is_valid,
            lin_valid,
            cycle.manual_adjusted,
            cycle.manual_valid,
            deadband,
            cycle.t0_concentration.get(&key).copied().unwrap_or(0.0),
            cycle
                .measurement_r2
                .get(&(GasKey::from((&cycle.main_gas, &cycle.instrument.id.unwrap()))))
                .copied()
                .unwrap_or(0.0),
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
pub fn load_cycles_sync(
    conn: &Connection,
    project: &Project,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) -> Result<Vec<Cycle>> {
    println!("loading cycles");
    let mut date: Option<String> = None;
    let start = start.timestamp();
    let end = end.timestamp();
    let gas_data = query_gas2(conn, start, end, project.to_owned())?;
    let chamber_metadata = query_chambers(conn, project.id.unwrap())?;
    let instruments = get_instruments_by_project_map(conn, project.id.unwrap())?;
    let mut stmt = conn.prepare(
        "
            SELECT
                f.*,

                main_i.id              AS main_instrument_id,
                main_i.instrument_model AS main_instrument_model,
                main_i.instrument_serial AS main_instrument_serial,

                i.id                   AS instrument_id,
                i.instrument_model     AS instrument_model,
                i.instrument_serial    AS instrument_serial

            FROM fluxes f
            LEFT JOIN instruments main_i ON f.main_instrument_link = main_i.id
            LEFT JOIN instruments i       ON f.instrument_link      = i.id
            WHERE f.project_link = ?1
            AND f.start_time BETWEEN ?2 AND ?3
            ORDER BY f.start_time;
",
    )?;
    let mut cycle_map: FastMap<CycleKey, Cycle> = FastMap::default();

    let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let column_index: FastMap<String, usize> =
        column_names.iter().enumerate().map(|(i, name)| (name.clone(), i)).collect();
    let mut serials: HashSet<String> = HashSet::new();

    let mut rows = stmt.query(params![project.id.unwrap(), start, end])?;

    while let Some(row) = rows.next()? {
        let deadband = row.get(*column_index.get("deadband").unwrap())?;
        let start_time: i64 = row.get(*column_index.get("start_time").unwrap())?;
        let instrument_serial: String = row.get(*column_index.get("instrument_serial").unwrap())?;
        let main_instrument_id: i64 = row.get(*column_index.get("main_instrument_id").unwrap())?;
        let instrument_id: i64 = row.get(*column_index.get("instrument_id").unwrap())?;
        serials.insert(instrument_serial.clone());
        let chamber_id: String = row.get(*column_index.get("chamber_id").unwrap())?;

        let mut chamber = chamber_metadata.get(&chamber_id).cloned().unwrap_or_default();
        let main_model_string: String =
            row.get(*column_index.get("main_instrument_model").unwrap())?;
        let main_instrument_serial: String =
            row.get(*column_index.get("main_instrument_serial").unwrap())?;

        let key = CycleKey {
            start_time,
            instrument_serial: main_instrument_serial.clone(),
            project_id: project.id.unwrap(),
            chamber_id: chamber_id.clone(),
        };

        let main_instrument_model = match main_model_string.parse::<InstrumentType>() {
            Ok(val) => val,
            Err(_) => {
                eprintln!("Unexpected invalid instrument type from DB: '{}'", main_model_string);
                process::exit(1);
            },
        };
        // let main_instrument_model =
        //     main_model_string.parse::<InstrumentType>().expect("Invalid instrument type");

        let model_string: String = row.get(*column_index.get("instrument_model").unwrap())?;
        let instrument_model = match model_string.parse::<InstrumentType>() {
            Ok(val) => val,
            Err(_) => {
                eprintln!("Unexpected invalid instrument type from DB: '{}'", model_string);
                continue;
            },
        };

        let start_timestamp: i64 = row.get(*column_index.get("start_time").unwrap())?;
        let chamber_id: String = row.get(*column_index.get("chamber_id").unwrap())?;

        let main_gas_i = row.get(*column_index.get("main_gas").unwrap())?;
        let main_gas = GasType::from_int(main_gas_i).unwrap();
        let gas_i = row.get(*column_index.get("gas").unwrap())?;
        let gas = GasType::from_int(gas_i).unwrap();
        let utc_start = chrono::DateTime::from_timestamp(start_timestamp, 0).unwrap();
        let start_time = utc_start.with_timezone(&project.tz);
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
        let chamber_height: f64 = row.get(*column_index.get("chamber_height").unwrap())?;
        let snow_depth_m: f64 = row.get(*column_index.get("snow_depth_m").unwrap())?;
        chamber.set_snow_height(snow_depth_m);

        let end_time = start_time + TimeDelta::seconds(end_offset);

        let error_code_u16: u16 = row.get(*column_index.get("error_code").unwrap())?;
        let error_code = ErrorMask::from_u16(error_code_u16);
        let is_valid: bool = row.get(*column_index.get("measurement_is_valid").unwrap())?;
        let gas_is_valid: bool = row.get(*column_index.get("gas_is_valid").unwrap())?;
        let manual_adjusted = row.get(*column_index.get("manual_adjusted").unwrap())?;
        let manual_valid: bool = row.get(*column_index.get("manual_valid").unwrap())?;
        let m_r2: f64 = row.get(*column_index.get("measurement_r2").unwrap())?;
        let min_calc_len: f64 = row.get(*column_index.get("min_calc_len").unwrap())?;

        let mut override_valid = None;
        if manual_valid {
            override_valid = Some(is_valid);
        }

        let dt_v = FastMap::default();
        let diag_v = FastMap::default();
        let gases = Vec::new();
        let gas_v = FastMap::default();
        let measurement_dt_v = Vec::new();
        let measurement_diag_v = FastMap::default();
        let measurement_gas_v = FastMap::default();
        let min_y = FastMap::default();
        let max_y = FastMap::default();
        let deadbands = FastMap::default();
        let t0_concentration = FastMap::default();
        let measurement_r2 = FastMap::default();
        let measurement_range_start =
            start_time.timestamp() as f64 + close_offset as f64 + close_lag_s + open_lag_s;
        let measurement_range_end =
            start_time.timestamp() as f64 + open_offset as f64 + close_lag_s + open_lag_s;

        if let Some(gas_data_day) = gas_data.get(&day) {
            let serial = instrument_serial.clone();
            let gas_key = GasKey::from((&gas, &instrument_id));
            let mut gas_channels = FastMap::default();
            let instrument_cfg = instrument_model.get_config();
            for ch in &instrument_cfg.channels {
                let gas_key = GasKey::from((&ch.gas, &instrument_id));
                match ch.gas {
                    GasType::CH4 => {
                        gas_channels.insert(
                            gas_key.clone(),
                            GasChannel {
                                gas: ch.gas,
                                unit: ch.unit,
                                instrument_id: ch.instrument_id.clone(),
                            },
                        );
                    },
                    GasType::CO2 => {
                        gas_channels.insert(
                            gas_key.clone(),
                            GasChannel {
                                gas: ch.gas,
                                unit: ch.unit,
                                instrument_id: ch.instrument_id.clone(),
                            },
                        );
                    },
                    GasType::H2O => {
                        gas_channels.insert(
                            gas_key.clone(),
                            GasChannel {
                                gas: ch.gas,
                                unit: ch.unit,
                                instrument_id: ch.instrument_id.clone(),
                            },
                        );
                    },
                    GasType::N2O => {
                        gas_channels.insert(
                            gas_key.clone(),
                            GasChannel {
                                gas: ch.gas,
                                unit: ch.unit,
                                instrument_id: ch.instrument_id.clone(),
                            },
                        );
                    },
                    _ => {},
                }
            }
            // for (serial, dt_values) in &gas_data_day.datetime {
            //     for (i, gas) in InstrumentType::from_str(
            //         &gas_data_day.model_key.get(serial).unwrap().to_string(),
            //     )
            //     .available_gases()
            //     .iter()
            //     .enumerate()
            //     {
            let dt_values = gas_data_day.datetime.get(&instrument_id).unwrap();
            let diag_values = gas_data_day.diag.get(&instrument_id).unwrap();
            // let g_values = gas_data_day.gas.get(&gas_key.clone()).unwrap();
            let main_instrument = Instrument {
                model: main_instrument_model,
                serial: main_instrument_serial,
                id: Some(main_instrument_id),
            };
            let instrument = Instrument {
                model: instrument_model,
                serial: instrument_serial,
                id: Some(instrument_id),
            };

            let cycle = cycle_map.entry(key.clone()).or_insert_with(|| Cycle {
                id: 0, // you might get this from elsewhere
                chamber_id: chamber_id.clone(),
                main_instrument,
                instrument,
                instruments: instruments.clone(),
                chamber,
                project_id: Some(project.id.unwrap()),
                start_time,
                air_temperature,
                air_pressure,
                chamber_height,
                snow_depth_m,
                error_code,
                is_valid,
                gas_is_valid: FastMap::default(),
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
                gases,
                calc_range_start: FastMap::default(),
                calc_range_end: FastMap::default(),
                manual_adjusted,
                min_y,
                max_y,
                flux: FastMap::default(),
                linfit: FastMap::default(),
                measurement_range_start,
                measurement_range_end,
                fluxes: FastMap::default(),
                measurement_r2,
                calc_r2: FastMap::default(),
                dt_v,
                calc_dt_v: FastMap::default(),
                measurement_dt_v,
                gas_v,
                gas_v_mole: FastMap::default(),
                calc_gas_v: FastMap::default(),
                measurement_gas_v,
                measurement_diag_v,
                t0_concentration,
                diag_v,
                min_calc_len,
                gas_channels: gas_channels.clone(),
            });
            if let Some(g_values) = gas_data_day.gas.get(&gas_key) {
                let start_target = start_time.timestamp() + start_lag_s as i64;
                let end_target = end_time.timestamp() + end_lag_s as i64;

                let matching_indices: Vec<usize> = dt_values
                    .iter()
                    .enumerate()
                    .filter(|(_, &t)| t.timestamp() >= start_target && t.timestamp() < end_target)
                    .map(|(i, _)| i)
                    .collect();
                let (meas_dt, meas_vals) = filter_data_in_range(
                    dt_values,
                    g_values,
                    start_target as f64,
                    end_target as f64,
                );
                // let (_, diag_vals) = filter_diag_data(
                //     dt_values,
                //     diag_values,
                //     start_target as f64,
                //     end_target as f64,
                // );

                // println!("{}", day);
                // println!("dt: {}", dt_values.len());
                // println!("gv: {}", g_values.len());
                // println!("di: {}", diag_values.len());
                let diag_slice: Vec<i64> =
                    matching_indices.iter().filter_map(|&i| diag_values.get(i).copied()).collect();
                let gas_slice: Vec<Option<f64>> =
                    matching_indices.iter().filter_map(|&i| g_values.get(i).copied()).collect();
                let dt_slice: Vec<f64> =
                    matching_indices.iter().map(|&i| dt_values[i].timestamp() as f64).collect();

                if meas_vals.is_empty() {
                    continue;
                }

                let target = dt_values
                    .iter()
                    .enumerate()
                    .find(|(_, &t)| t.timestamp() as f64 >= measurement_range_start)
                    .map(|(i, _)| i);

                cycle.dt_v.insert(instrument_id, dt_slice);

                cycle.gas_v.insert(gas_key.clone(), gas_slice.clone());
                let t0 = g_values.get(target.unwrap()).unwrap_or(&Some(0.));
                cycle.t0_concentration.insert(gas_key.clone(), t0.unwrap());

                cycle.diag_v.insert(instrument_id, diag_slice);

                if !cycle.gases.contains(&gas_key) {
                    cycle.gases.push(gas_key.clone());
                    // println!("pushed : {:?}", gas_key);
                }

                cycle.max_y.insert(gas_key.clone(), calculate_max_y_from_vec(&meas_vals));
                cycle.min_y.insert(gas_key.clone(), calculate_min_y_from_vec(&meas_vals));
                cycle.measurement_r2.insert(gas_key.clone(), m_r2);
                cycle.deadbands.insert(gas_key.clone(), deadband);
            }

            let gk = gas_key.clone();
            // println!("getting models for: {:?}", gk);
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
                Ok(gas_i),
                Ok(instrument_id),
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
                row.get(*column_index.get("gas").unwrap()),
                row.get(*column_index.get("instrument_id").unwrap()),
            ) {
                let gas_type = GasType::from_int(gas_i).unwrap();
                let key: GasKey = GasKey::from((&gas_type, &instrument_id));
                let gas_channel = gas_channels.get(&key).unwrap().clone();
                cycle.calc_range_start.insert(gk.clone(), calc_start);
                cycle.calc_range_end.insert(gk.clone(), calc_end + 1.);
                let lin = LinearFlux {
                    fit_id: "linear".to_string(),
                    gas_channel,
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
                // println!("{}", lin);
                cycle.fluxes.insert(
                    (gk.clone(), FluxKind::Linear),
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
                Ok(gas_i),
                Ok(instrument_id),
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
                row.get(*column_index.get("gas").unwrap()),
                row.get(*column_index.get("instrument_id").unwrap()),
            ) {
                let gas_type = GasType::from_int(gas_i).unwrap();
                if gas_type != gk.clone().gas_type {
                    continue;
                }
                let key: GasKey = GasKey::from((&gas_type, &instrument_id));
                let gas_channel = gas_channels.get(&key).unwrap().clone();
                cycle.calc_range_start.insert(gk.clone(), calc_start);
                cycle.calc_range_end.insert(gk.clone(), calc_end + 1.);
                let roblin = RobustFlux {
                    fit_id: "roblin".to_string(),
                    gas_channel,
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
                // println!("{}", lin);
                cycle.fluxes.insert(
                    (gk.clone(), FluxKind::RobLin),
                    FluxRecord { model: Box::new(roblin), is_valid: gas_is_valid },
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
                Ok(gas_i),
                Ok(instrument_id),
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
                row.get(*column_index.get("gas").unwrap()),
                row.get(*column_index.get("instrument_id").unwrap()),
            ) {
                let gas_type = GasType::from_int(gas_i).unwrap();
                if gas_type != gk.gas_type {
                    continue;
                }
                let key: GasKey = GasKey::from((&gas_type, &instrument_id));
                let gas_channel = gas_channels.get(&key).unwrap().clone();
                cycle.calc_range_start.insert(gk.clone(), calc_start);
                cycle.calc_range_end.insert(gk.clone(), calc_end + 1.);
                let poly = PolyFlux {
                    fit_id: "poly".to_string(),
                    gas_channel,
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
                // println!("{}", lin);
                cycle.fluxes.insert(
                    (gk, FluxKind::Poly),
                    FluxRecord { model: Box::new(poly), is_valid: gas_is_valid },
                );
                // }
            }
        }
    }
    let mut cycles: Vec<Cycle> = cycle_map.into_values().collect();
    cycles.sort_by_key(|c| c.start_time);
    // cycles.iter().for_each(|c| println!("{:?}", c.gas_v.keys()));
    if cycles.is_empty() {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    Ok(cycles)
}
fn filter_data_in_range(
    datetimes: &[DateTime<Tz>],
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

use std::sync::atomic::{AtomicUsize, Ordering};

pub fn find_best_window_for_gas_par_print(
    dt_v: &[f64],
    gas_v: &[f64],
    gaps: &[bool],
    min_window: usize,
    step: usize,
) -> Option<(usize, usize, f64)> {
    let max_len = gas_v.len();
    let counter = AtomicUsize::new(0);

    let result = (min_window..=max_len)
        .step_by(step)
        .flat_map(move |win_size| {
            let last_start = max_len.saturating_sub(win_size);
            (0..=last_start).step_by(step).map(move |start| (start, win_size))
        })
        .par_bridge()
        .filter_map(|(start, win_size)| {
            let end = start + win_size;

            // Skip windows with gaps
            if gaps.get(start..end.saturating_sub(1))?.iter().any(|&gap| gap) {
                return None;
            }

            let window_dt = &dt_v[start..end];
            let window_gas = &gas_v[start..end];

            let r = stats::fast_pearson(window_dt, window_gas).unwrap_or(0.0);

            counter.fetch_add(1, Ordering::Relaxed); // increment the counter

            Some((start, end, r))
        })
        .reduce_with(|a, b| if a.2 > b.2 { a } else { b });

    println!("Total iterations (evaluated windows): {}", counter.load(Ordering::Relaxed));

    result
}
pub fn find_best_window_for_gas_par(
    dt_v: &[f64],
    gas_v: &[f64],
    gaps: &[bool],
    min_window: usize,
    step: usize,
) -> Option<(usize, usize, f64)> {
    let max_len = gas_v.len();

    (min_window..=max_len)
        .step_by(step)
        .flat_map(move |win_size| {
            let last_start = max_len.saturating_sub(win_size);
            (0..=last_start).step_by(step).map(move |start| (start, win_size))
        })
        .par_bridge()
        .filter_map(|(start, win_size)| {
            let end = start + win_size;

            // Skip windows with gaps
            if gaps.get(start..end.saturating_sub(1))?.iter().any(|&gap| gap) {
                return None;
            }

            let window_dt = &dt_v[start..end];
            let window_gas = &gas_v[start..end];

            let r = stats::fast_pearson(window_dt, window_gas).unwrap_or(0.0);

            Some((start, end, r))
        })
        .reduce_with(|a, b| if a.2 > b.2 { a } else { b })
}
fn _find_best_window_for_gas_(
    dt_v: &[f64],
    gas_v: &[f64],
    gaps: &[bool],
    min_window: usize,
    step: usize,
) -> Option<(usize, usize, f64)> {
    let max_len = gas_v.len();

    // Generate all candidate (start, win_size) pairs
    let candidates: Vec<(usize, usize)> = (min_window..=max_len)
        .step_by(step)
        .flat_map(|win_size| {
            let last_start = max_len.saturating_sub(win_size);
            (0..=last_start).step_by(step).map(move |start| (start, win_size))
        })
        .collect();

    candidates
        .into_iter()
        .filter_map(|(start, win_size)| {
            let end = start + win_size;
            if gaps[start..end.saturating_sub(1)].iter().any(|&gap| gap) {
                return None;
            }

            let window_dt = &dt_v[start..end];
            let window_gas = &gas_v[start..end];
            let r2 = stats::pearson_correlation(window_dt, window_gas).unwrap_or(0.0).powi(2);
            Some((start, end, r2))
        })
        .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap())
}

pub fn calculate_max_y_from_vec(values: &[Option<f64>]) -> f64 {
    values.iter().filter_map(|&v| v).filter(|v| !v.is_nan()).fold(f64::NEG_INFINITY, f64::max)
}

pub fn calculate_min_y_from_vec(values: &[Option<f64>]) -> f64 {
    values.iter().filter_map(|&v| v).filter(|v| !v.is_nan()).fold(f64::INFINITY, f64::min)
}

pub fn process_cycles<V>(
    timev: &TimeData,
    gas_by_day: &HashMap<String, V>,
    meteo_data: &MeteoData,
    height_data: &HeightData,
    chamber_data: &HashMap<String, ChamberShape>,
    project: Project,
    progress_sender: mpsc::UnboundedSender<ProcessEvent>,
) -> Result<Vec<Option<Cycle>>, Box<dyn std::error::Error + Send + Sync>>
where
    V: Borrow<GasData>, // <-- key trick
{
    let mut cycle_vec = Vec::new();

    for (chamber, start, close, open, end, snow_depth, id, project_id, instrument_id) in
        timev.iter()
    {
        let day = start.format("%Y-%m-%d").to_string();

        let mut cycle = CycleBuilder::new()
            .chamber_id(chamber.to_owned())
            .start_time(*start)
            .close_offset(*close)
            .open_offset(*open)
            .end_offset(*end)
            .instrument_id(*instrument_id)
            .snow_depth(*snow_depth)
            .project(project.clone())
            .min_calc_len(project.min_calc_len)
            .build()?;

        let Some(cur_data) = gas_by_day.get(&day).map(|v| v.borrow()) else {
            // let _ = progress_sender.send(ProcessEvent::Query(QueryEvent::NoGasDataDay(day)));
            cycle_vec.push(None);
            continue;
        };
        cycle.id = *id;

        let mut found_data = false;

        // Iterate only the serials that actually exist that day
        for (ser, datetimes) in &cur_data.datetime {
            // Skip serials that don’t cover the start time
            if datetimes.is_empty() || *start < datetimes[0] || *start > *datetimes.last().unwrap()
            {
                continue;
            }

            // Use binary search instead of linear find to align start index exactly
            // (avoids the equality “hack” later).
            let si_idx = match datetimes.binary_search(start) {
                Ok(i) => i,
                Err(i) => {
                    // first >= start; if start isn’t exactly present, we still align to the window
                    if i >= datetimes.len() {
                        continue;
                    }
                    i
                },
            };

            let si_time = datetimes[si_idx].timestamp();
            let ei_time = si_time + *end;

            // Find end index (first index with ts >= ei_time). This emulates
            // std::slice::partition_point (stable since 1.52 in Rust core nightly; manual here).
            let mut lo = si_idx;
            let mut hi = datetimes.len();
            while lo < hi {
                let mid = (lo + hi) / 2;
                if datetimes[mid].timestamp() < ei_time {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
            let end_idx = lo;

            if end_idx <= si_idx {
                continue;
            }

            // Now we can build slices by index range — no per-item filter scans
            let idx_range = si_idx..end_idx;

            // Timestamp vector (keep as i64 where possible; only cast when storing if required)
            let dt_slice: Vec<f64> =
                datetimes[idx_range.clone()].iter().map(|t| t.timestamp() as f64).collect();

            // If you still want to enforce exact start alignment, compare to seconds (no FP issues)
            if cycle.start_time.timestamp() != si_time {
                // If this is *too strict*, you can relax or log instead of skipping entirely.
                continue;
            }

            // Diag vector
            if let Some(diags) = cur_data.diag.get(ser) {
                let diag_slice: Vec<i64> = diags[idx_range.clone()].to_vec();
                cycle.diag_v.insert(ser.clone(), diag_slice);
            }

            // Model + serial for this cycle (uses the matched serial)

            // cycle.main_instrument = project.instrument.clone();
            // cycle.instrument = Instrument {
            //     model: *cur_data.model_key.get(ser).unwrap(),
            //     serial: ser.clone(),
            //     id: None,
            // };
            for item in &cur_data.instruments {
                if item.id.unwrap() == *ser {
                    cycle.instrument = item.clone();
                }
            }
            let tx = Connection::open("fluxrs.db").expect("Failed to open database");
            let instrument_id = get_instrument_id_by_project_and_serial(
                &tx,
                project.id.unwrap(),
                &cycle.instrument.serial,
            )
            .expect("Failure");

            cycle.instrument.id = instrument_id;

            // Gas values for this serial only
            for (key, gas_values) in &cur_data.gas {
                if &key.id != ser {
                    continue;
                }

                if cur_data
                    .model_key
                    .get(&key.id)
                    .unwrap()
                    .available_gases()
                    .contains(&key.gas_type)
                {
                    let instrument = cur_data.model_key.get(&key.id).unwrap();
                    let cfg = instrument.get_config();
                    let key_gas = key.gas_type;
                    for ch in &cfg.channels {
                        match ch.gas {
                            GasType::CH4 => {
                                cycle.gas_channels.insert(
                                    key.clone(),
                                    GasChannel {
                                        gas: ch.gas,
                                        unit: ch.unit,
                                        instrument_id: ch.instrument_id.clone(),
                                    },
                                );
                            },
                            GasType::CO2 => {
                                cycle.gas_channels.insert(
                                    key.clone(),
                                    GasChannel {
                                        gas: ch.gas,
                                        unit: ch.unit,
                                        instrument_id: ch.instrument_id.clone(),
                                    },
                                );
                            },
                            GasType::H2O => {
                                cycle.gas_channels.insert(
                                    key.clone(),
                                    GasChannel {
                                        gas: ch.gas,
                                        unit: ch.unit,
                                        instrument_id: ch.instrument_id.clone(),
                                    },
                                );
                            },
                            GasType::N2O => {
                                cycle.gas_channels.insert(
                                    key.clone(),
                                    GasChannel {
                                        gas: ch.gas,
                                        unit: ch.unit,
                                        instrument_id: ch.instrument_id.clone(),
                                    },
                                );
                            },
                            _ => {},
                        }
                    }

                    let gas_slice: Vec<Option<f64>> = gas_values[idx_range.clone()].to_vec();
                    cycle.gas_v.insert(key.clone(), gas_slice);
                }
            }

            // Timestamps for this serial
            cycle.dt_v.insert(ser.clone(), dt_slice);

            found_data = true;
            // If one serial is enough per cycle, you can break here
            // break;
        }

        if found_data {
            cycle.gases = cycle.gas_v.keys().cloned().collect();
            cycle.main_gas = project.main_gas.unwrap();
            cycle.main_instrument = project.instrument.clone();

            // println!("gase: {:?}", cycle.gases);
            // println!("chan: {:?}", cycle.gas_channels);
            // let mut gas_channels = FastMap::default();
            // let instrument_cfg = cycle.instrument.model.get_config();
            // for ch in &instrument_cfg.channels {
            //     let gas_key = GasKey::from((&ch.gas, cycle.instrument.serial.as_str()));
            //     match ch.gas {
            //         GasType::CH4 => {
            //             gas_channels.insert(
            //                 gas_key.clone(),
            //                 GasChannel {
            //                     gas: ch.gas,
            //                     unit: ch.unit,
            //                     instrument_id: ch.instrument_id.clone(),
            //                 },
            //             );
            //         },
            //         GasType::CO2 => {
            //             gas_channels.insert(
            //                 gas_key.clone(),
            //                 GasChannel {
            //                     gas: ch.gas,
            //                     unit: ch.unit,
            //                     instrument_id: ch.instrument_id.clone(),
            //                 },
            //             );
            //         },
            //         GasType::H2O => {
            //             gas_channels.insert(
            //                 gas_key.clone(),
            //                 GasChannel {
            //                     gas: ch.gas,
            //                     unit: ch.unit,
            //                     instrument_id: ch.instrument_id.clone(),
            //                 },
            //             );
            //         },
            //         GasType::N2O => {
            //             gas_channels.insert(
            //                 gas_key.clone(),
            //                 GasChannel {
            //                     gas: ch.gas,
            //                     unit: ch.unit,
            //                     instrument_id: ch.instrument_id.clone(),
            //                 },
            //             );
            //         },
            //         _ => {},
            //     }
            // }
            // cycle.gas_channels = gas_channels;

            let target = (*start + chrono::TimeDelta::seconds(*close)).timestamp();

            // Meteo
            let (temp, pressure) = meteo_data.get_nearest(target).unwrap_or((10.0, 1000.0));
            cycle.air_temperature = temp;
            cycle.air_pressure = pressure;

            // Height
            let maybe_height = height_data.get_nearest_previous_height(target, &cycle.chamber_id);
            cycle.chamber = chamber_data.get(chamber).cloned().unwrap_or_default();
            if let Some(h) = maybe_height {
                cycle.chamber_height = h;
                match &mut cycle.chamber {
                    ChamberShape::Cylinder { height_m, .. }
                    | ChamberShape::Box { height_m, .. } => *height_m = h,
                }
            } else {
                cycle.chamber_height = match &cycle.chamber {
                    ChamberShape::Cylinder { height_m, .. }
                    | ChamberShape::Box { height_m, .. } => *height_m,
                };
            }

            // Deadbands
            for gas_key in &cycle.gases {
                cycle.deadbands.insert(gas_key.clone(), project.deadband);
            }

            // Init
            cycle.init(
                project.mode == Mode::BestPearsonsR,
                project.deadband,
                progress_sender.clone(),
            );

            cycle_vec.push(Some(cycle));
        } else {
            let msg = format!("{}, ID: {} {}", start, chamber, cycle.instrument.serial);
            let _ = progress_sender.send(ProcessEvent::Query(QueryEvent::NoGasData(msg)));
            cycle_vec.push(None);
        }
    }

    Ok(cycle_vec)
}

pub fn get_instrument_id_by_project_and_serial(
    conn: &Connection,
    project_id: i64,
    serial: &str,
) -> Result<Option<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id
         FROM instruments
         WHERE project_link = ?1 AND instrument_serial = ?2",
    )?;

    let result = stmt.query_row(params![project_id, serial], |row| {
        let id: i64 = row.get(0)?;
        Ok(id)
    });

    match result {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}
pub fn get_instruments_by_project_map(
    conn: &Connection,
    project_id: i64,
) -> Result<FastMap<i64, Instrument>> {
    let mut stmt = conn.prepare(
        "SELECT id, instrument_model, instrument_serial
         FROM instruments
         WHERE project_link = ?1",
    )?;

    let instrument_iter = stmt.query_map(params![project_id], |row| {
        let id: i64 = row.get(0)?;
        let model_str: String = row.get(1)?;
        let serial: String = row.get(2)?;

        // Parse InstrumentType from string
        let model = model_str.parse::<InstrumentType>().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
        })?;

        Ok((id, Instrument { id: Some(id), model, serial }))
    })?;

    // Collect into a FastMap<i64, Instrument>
    let mut instruments: FastMap<i64, Instrument> = FastMap::default();
    for item in instrument_iter {
        let (id, inst) = item?;
        instruments.insert(id, inst);
    }

    Ok(instruments)
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

        fn gas_channel(&self) -> GasChannel {
            GasChannel {
                gas: GasType::CH4,
                unit: ConcentrationUnit::Ppb,
                instrument_id: "asd".to_owned(),
            }
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
        fluxes: FastMap<(GasType, FluxKind), Box<dyn FluxModel>>,
    }

    impl DummyCycle {
        pub fn best_flux_by_aic(&self, key: &GasType) -> Option<f64> {
            let candidates = [FluxKind::Linear, FluxKind::Poly, FluxKind::RobLin];

            candidates
                .iter()
                .filter_map(|kind| self.fluxes.get(&(*key, *kind)))
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
