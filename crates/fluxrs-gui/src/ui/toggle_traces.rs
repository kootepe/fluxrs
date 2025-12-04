use fluxrs_core::cycle::cycle::Cycle;
use fluxrs_core::cycle::gaskey::GasKey;
use fluxrs_core::errorcode::ErrorCode;
use fluxrs_core::flux::FluxKind;
use fluxrs_core::types::FastMap;
use std::collections::HashSet;
use std::hash::Hash;

pub struct TraceToggler {
    visible_traces: FastMap<String, bool>,
    all_traces: HashSet<String>,
    show_valids: bool,
    show_invalids: bool,
    show_bad: bool,
}

impl TraceToggler {
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
    pub fn show_valids(&self) -> bool {
        self.show_valids
    }
    pub fn show_invalids(&self) -> bool {
        self.show_invalids
    }
    pub fn show_bad(&self) -> bool {
        self.show_bad
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

