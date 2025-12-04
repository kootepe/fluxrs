use fluxrs_core::cycle::cycle::Cycle;
use fluxrs_core::cycle::gaskey::GasKey;
use fluxrs_core::errorcode::ErrorCode;
use fluxrs_core::flux::FluxKind;
use fluxrs_core::types::FastMap;
use std::collections::HashSet;
use std::hash::Hash;

pub struct CycleFilter {
    visible_traces: FastMap<String, bool>,
    all_traces: HashSet<String>,
    pub show_valids: bool,
    pub show_invalids: bool,
    pub show_bad: bool,
}

impl CycleFilter {
    pub fn new() -> Self {
        Self {
            visible_traces: FastMap::default(),
            all_traces: HashSet::new(),
            show_valids: true,
            show_invalids: true,
            show_bad: false,
        }
    }
    pub fn visible_traces(&self) -> &FastMap<String, bool> {
        &self.visible_traces
    }
    pub fn visible_traces_mut(&mut self) -> &mut FastMap<String, bool> {
        &mut self.visible_traces
    }
    pub fn hide_all_traces(&mut self) {
        for value in self.visible_traces.values_mut() {
            *value = false;
        }
    }
    pub fn set_trace_visible(&mut self, name: String, visible: bool) {
        self.visible_traces.insert(name, visible);
    }
    pub fn get_visible_trace(&mut self, name: &String) -> bool {
        *self.visible_traces.get(name).unwrap_or(&true)
    }
    pub fn all_traces(&self) -> &HashSet<String> {
        &self.all_traces
    }
    pub fn set_all_traces(&mut self, traces: HashSet<String>) {
        self.all_traces = traces
    }
    pub fn set_visible_traces(&mut self, traces: FastMap<String, bool>) {
        self.visible_traces = traces
    }
    pub fn show_valids(&self) -> bool {
        self.show_valids
    }
    pub fn show_invalids(&self) -> bool {
        self.show_invalids
    }
    pub fn show_bad(&self) -> bool {
        self.show_bad
    }
    pub fn get_sorted_traces(&self) -> Vec<String> {
        let mut traces: Vec<String> = self.all_traces().iter().cloned().collect();

        traces.sort_by(|a, b| {
            let num_a = a.parse::<f64>().ok();
            let num_b = b.parse::<f64>().ok();
            match (num_a, num_b) {
                (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.cmp(b),
            }
        });
        traces
    }
    fn visible_count(&self) -> usize {
        self.visible_traces.values().filter(|&&v| v).count()
    }

    // Private: get whether a specific trace is visible
    fn is_visible(&self, chamber_id: &str) -> bool {
        self.visible_traces.get(chamber_id).copied().unwrap_or(true) // your default
    }

    // Private: set the visibility for a specific trace
    fn set_visibility(&mut self, chamber_id: &str, visible: bool) {
        self.visible_traces.insert(chamber_id.to_owned(), visible);
    }

    // Public: the actual toggle operation
    pub fn toggle_visibility(&mut self, chamber_id: &str) {
        let visible_count = self.visible_count();
        let is_visible = self.is_visible(chamber_id);

        // Prevent hiding the last visible trace
        if is_visible && visible_count == 1 {
            return;
        }

        self.set_visibility(chamber_id, !is_visible);
    }
    pub fn is_cycle_visible(
        &self,
        cycle: &Cycle,
        p_val_thresh: f64,
        rmse_thresh: f64,
        r2_thresh: f64,
        t0_thresh: f64,
    ) -> bool {
        let main_gas = cycle.main_gas;
        let main_id = cycle.main_instrument.id.unwrap();
        let key = GasKey::from((&main_gas, &main_id));
        let kind = cycle.best_model_by_aic(&key).unwrap_or(FluxKind::Linear);

        let is_valid = cycle.is_valid_by_threshold(
            &key,
            kind,
            p_val_thresh,
            r2_thresh,
            rmse_thresh,
            t0_thresh,
        ) && cycle.error_code.0 == 0;

        let trace_visible = self.visible_traces.get(&cycle.chamber_id).copied().unwrap_or(true);
        let bad_ok = self.show_bad || !cycle.error_code.contains(ErrorCode::FailedMeasurement);
        let valid_ok = self.show_valids || !is_valid;
        let invalid_ok = self.show_invalids || is_valid;

        trace_visible && valid_ok && invalid_ok && bad_ok
    }
}
