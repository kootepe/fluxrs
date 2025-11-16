use crate::cycle::gaskey::GasKey;
use crate::flux::FluxKind;

use crate::cycle::cycle::Cycle;
use crate::errorcode::ErrorCode;

use std::cell::Cell;
use std::collections::HashMap;

#[derive(Default, Clone, Debug)]
pub struct Index(Cell<usize>);

impl Index {
    pub fn get(&self) -> usize {
        self.0.get()
    }

    pub fn set(&self, val: usize) {
        self.0.set(val);
    }

    pub fn increment(&self) {
        self.0.set(self.get() + 1);
    }

    pub fn decrement(&self) {
        self.0.set(self.get().saturating_sub(1));
    }

    pub fn reset(&self) {
        self.set(0);
    }
}

pub struct CycleNavigator {
    pub visible_cycles: Vec<usize>, // Holds indexes of visible cycles
    cycle_pos: Index,               // Position *within* visible_cycles
}

impl Default for CycleNavigator {
    fn default() -> Self {
        Self::new()
    }
}

impl CycleNavigator {
    pub fn new() -> Self {
        Self { visible_cycles: Vec::new(), cycle_pos: Index::default() }
    }

    /// Recomputes which cycle indexes are visible based on filters
    pub fn recompute_visible_indexes(
        &mut self,
        cycles: &[Cycle],
        visible_traces: &HashMap<String, bool>,
        show_valids: bool,
        show_invalids: bool,
        show_bad: bool,
        p_val_thresh: f64,
        rmse_thresh: f64,
        r2_thresh: f64,
        t0_thresh: f64,
    ) {
        let previous_start_time =
            self.current_index().and_then(|idx| cycles.get(idx)).map(|cycle| cycle.get_start_ts());

        self.visible_cycles.clear();

        for (index, cycle) in cycles.iter().enumerate() {
            if is_cycle_visible(
                cycle,
                visible_traces,
                show_valids,
                show_invalids,
                show_bad,
                p_val_thresh,
                rmse_thresh,
                r2_thresh,
                t0_thresh,
            ) {
                self.visible_cycles.push(index);
            }
        }

        if let Some(target_time) = previous_start_time {
            if let Some(new_idx) = self.find_closest_visible_cycle(cycles, target_time) {
                self.cycle_pos.set(new_idx);
            } else {
                self.cycle_pos.reset();
            }
        } else {
            self.cycle_pos.reset();
        }
    }

    /// Returns the actual index into `cycles`, based on current position
    pub fn current_index(&self) -> Option<usize> {
        self.visible_cycles.get(self.cycle_pos.get()).copied()
    }

    /// Steps forward (cyclically) through visible cycles
    pub fn step_forward(&self) {
        let len = self.visible_cycles.len();
        if len == 0 {
            return;
        }
        let next = (self.cycle_pos.get() + 1) % len;
        self.cycle_pos.set(next);
    }

    /// Steps backward (cyclically) through visible cycles
    pub fn step_back(&self) {
        let len = self.visible_cycles.len();
        if len == 0 {
            return;
        }
        let prev = (self.cycle_pos.get() + len - 1) % len;
        self.cycle_pos.set(prev);
    }

    /// Jumps directly to a visible cycle if it matches an actual cycle index
    pub fn jump_to_visible_index(&self, index: usize) {
        if let Some(pos) = self.visible_cycles.iter().position(|&i| i == index) {
            self.cycle_pos.set(pos);
        }
    }

    /// How many cycles are currently visible?
    pub fn visible_count(&self) -> usize {
        self.visible_cycles.len()
    }

    /// Returns a reference to the currently selected visible Cycle, if any
    pub fn current_cycle<'a>(&self, cycles: &'a [Cycle]) -> Option<&'a Cycle> {
        self.current_index().and_then(move |i| cycles.get(i))
    }
    /// Returns a mutable reference to the currently selected visible Cycle, if any
    pub fn current_cycle_mut<'a>(&self, cycles: &'a mut [Cycle]) -> Option<&'a mut Cycle> {
        self.current_index().and_then(move |i| cycles.get_mut(i))
    }
    pub fn update_current_cycle<F>(&self, cycles: &mut [Cycle], mut f: F)
    where
        F: FnMut(&mut Cycle),
    {
        if let Some(cycle) = self.current_cycle_mut(cycles) {
            f(cycle);
        }
    }
    /// Finds the closest visible cycle based on start_time
    pub fn find_closest_visible_cycle(&self, cycles: &[Cycle], target_time: i64) -> Option<usize> {
        if self.visible_cycles.is_empty() {
            return None;
        }

        let result = self
            .visible_cycles
            .binary_search_by_key(&target_time, |&idx| cycles[idx].get_start_ts());

        match result {
            Ok(pos) => Some(pos), // Exact match, perfect
            Err(insert_pos) => {
                // Not exact match, choose closer neighbor
                let before = insert_pos.checked_sub(1);
                let after =
                    if insert_pos < self.visible_cycles.len() { Some(insert_pos) } else { None };

                let best_pos = match (before, after) {
                    (Some(b), Some(a)) => {
                        let b_idx = self.visible_cycles[b];
                        let a_idx = self.visible_cycles[a];
                        let b_diff = (cycles[b_idx].get_start_ts() - target_time).abs();
                        let a_diff = (cycles[a_idx].get_start_ts() - target_time).abs();
                        if b_diff <= a_diff {
                            b
                        } else {
                            a
                        }
                    },
                    (Some(b), None) => b,
                    (None, Some(a)) => a,
                    (None, None) => return None,
                };

                Some(best_pos)
            },
        }
    }
}

fn is_cycle_visible(
    cycle: &Cycle,
    visible_traces: &HashMap<String, bool>,
    show_valids: bool,
    show_invalids: bool,
    show_bad: bool,
    p_val_thresh: f64,
    rmse_thresh: f64,
    r2_thresh: f64,
    t0_thresh: f64,
) -> bool {
    let main_gas = cycle.main_gas;
    let main_id = cycle.main_instrument.id.unwrap();
    let kind =
        cycle.best_model_by_aic(&(GasKey::from((&main_gas, &main_id)))).unwrap_or(FluxKind::Linear);
    let key = GasKey::from((&main_gas, &main_id));
    // let p_val = cycle
    //     .fluxes
    //     .get(&(GasKey::from((&main_gas, main_instr)), best_model))
    //     .map_or(0.0, |f| f.model.p_value().unwrap_or(0.0));
    // let r2 = cycle.measurement_r2.get(&(GasKey::from((&main_gas, main_instr)))).unwrap_or(&0.0);
    // let rmse = cycle
    //     .fluxes
    //     .get(&(GasKey::from((&main_gas, main_instr)), best_model))
    //     .map_or(0.0, |f| f.model.p_value().unwrap_or(0.0));
    // let t0 = cycle.t0_concentration.get(&(GasKey::from((&main_gas, main_instr)))).unwrap_or(&0.0);
    // let stats_valid =
    //     p_val < p_val_thresh && *r2 > r2_thresh && rmse < rmse_thresh && *t0 < t0_thresh;
    let is_valid =
        cycle.is_valid_by_threshold(&key, kind, p_val_thresh, r2_thresh, rmse_thresh, t0_thresh)
            && cycle.error_code.0 == 0;

    let trace_visible = visible_traces.get(&cycle.chamber_id).copied().unwrap_or(true);
    let bad_ok = show_bad || !cycle.error_code.contains(ErrorCode::FailedMeasurement);
    let valid_ok = show_valids || !is_valid;
    let invalid_ok = show_invalids || is_valid;
    trace_visible && valid_ok && invalid_ok && bad_ok
}

pub fn compute_visible_indexes(
    cycles: &[Cycle],
    visible_traces: &HashMap<String, bool>,
    show_valids: bool,
    show_invalids: bool,
    show_bad: bool,
    p_val_thresh: f64,
    rmse_thresh: f64,
    r2_thresh: f64,
    t0_thresh: f64,
) -> Vec<usize> {
    cycles
        .iter()
        .enumerate()
        .filter(|(_, cycle)| {
            is_cycle_visible(
                cycle,
                visible_traces,
                show_valids,
                show_invalids,
                show_bad,
                p_val_thresh,
                rmse_thresh,
                r2_thresh,
                t0_thresh,
            )
        })
        .map(|(i, _)| i)
        .collect()
}
