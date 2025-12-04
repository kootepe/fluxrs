use fluxrs_core::cycle::cycle::Cycle;
use fluxrs_core::cycle::gaskey::GasKey;
use fluxrs_core::errorcode::ErrorCode;
use fluxrs_core::flux::FluxKind;
use fluxrs_core::types::FastMap;
use std::collections::HashSet;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn make_filter_with_traces(traces: &[(&str, bool)]) -> CycleFilter {
        let mut filter = CycleFilter::new();
        let mut visible = FastMap::default();
        let mut all = HashSet::new();

        for (name, vis) in traces {
            visible.insert((*name).to_string(), *vis);
            all.insert((*name).to_string());
        }

        filter.set_visible_traces(visible);
        filter.set_all_traces(all);
        filter
    }

    #[test]
    fn new_has_expected_defaults() {
        let filter = CycleFilter::new();

        assert!(filter.visible_traces().is_empty());
        assert!(filter.all_traces().is_empty());
        assert!(filter.show_valids());
        assert!(filter.show_invalids());
        assert!(!filter.show_bad());
    }

    #[test]
    fn hide_all_traces_sets_all_to_false() {
        let mut filter = make_filter_with_traces(&[("A", true), ("B", false), ("C", true)]);

        filter.hide_all_traces();

        for (&visible) in filter.visible_traces().values() {
            assert!(!visible, "expected all traces to be hidden");
        }
    }

    #[test]
    fn get_visible_trace_defaults_to_true() {
        let mut filter = CycleFilter::new();

        // not present in map -> should default to true
        assert!(filter.get_visible_trace(&"ch1".to_string()));
    }

    #[test]
    fn set_trace_visible_and_get_works() {
        let mut filter = CycleFilter::new();

        let name = "trace1".to_string();
        filter.set_trace_visible(name.clone(), false);
        assert!(!filter.get_visible_trace(&name));

        filter.set_trace_visible(name.clone(), true);
        assert!(filter.get_visible_trace(&name));
    }

    #[test]
    fn sorted_traces_numeric_and_non_numeric() {
        let mut all = HashSet::new();
        // numeric-like
        all.insert("10".to_string());
        all.insert("2".to_string());
        all.insert("1".to_string());
        // non-numeric
        all.insert("foo".to_string());
        all.insert("bar".to_string());

        let mut filter = CycleFilter::new();
        filter.set_all_traces(all);

        let sorted = filter.get_sorted_traces();

        // numeric ones in numeric order, then lexicographic for non-numeric
        let numeric_part: Vec<&str> =
            sorted.iter().filter(|s| s.parse::<f64>().is_ok()).map(|s| s.as_str()).collect();
        let non_numeric_part: Vec<&str> =
            sorted.iter().filter(|s| s.parse::<f64>().is_err()).map(|s| s.as_str()).collect();

        assert_eq!(numeric_part, vec!["1", "2", "10"]);
        // bar < foo lexicographically
        assert_eq!(non_numeric_part, vec!["bar", "foo"]);
    }

    #[test]
    fn toggle_visibility_toggles_when_multiple_visible() {
        let mut filter = make_filter_with_traces(&[("A", true), ("B", true)]);

        // Toggle one of them off
        filter.toggle_visibility("A");

        assert!(!filter.visible_traces().get("A").unwrap());
        assert!(*filter.visible_traces().get("B").unwrap());
    }

    #[test]
    fn toggle_visibility_does_not_hide_last_visible() {
        let mut filter = make_filter_with_traces(&[("only", true)]);

        // Try to hide the only visible trace
        filter.toggle_visibility("only");

        // Should remain visible
        assert!(
            *filter.visible_traces().get("only").unwrap(),
            "last visible trace must not be hidden"
        );
    }

    #[test]
    fn toggle_visibility_on_non_existent_trace_creates_and_hides_it() {
        let mut filter = CycleFilter::new();

        // Initially no traces
        assert!(filter.visible_traces().is_empty());

        // For a non-existent trace, is_visible() defaults to true,
        // and visible_count() is 0, so it should end up inserting false.
        filter.toggle_visibility("new_trace");

        assert_eq!(filter.visible_traces().len(), 1);
        assert_eq!(*filter.visible_traces().get("new_trace").unwrap(), false);
    }

    #[test]
    fn visible_traces_accessor_roundtrip() {
        let mut filter = CycleFilter::new();
        {
            let vt = filter.visible_traces_mut();
            vt.insert("A".to_string(), true);
        }

        assert_eq!(
            filter.visible_traces().get("A"),
            Some(&true),
            "visible_traces_mut should allow mutation"
        );
    }

    // --- Optional skeleton for is_cycle_visible tests ---
    //
    // Fill these in with concrete constructors for `Cycle` from your crate.
    //
    // use fluxrs_core::cycle::cycle::Cycle;
    // use fluxrs_core::cycle::gaskey::GasKey;
    // use fluxrs_core::errorcode::ErrorCode;
    // use fluxrs_core::flux::FluxKind;
    //
    // fn make_dummy_cycle(...) -> Cycle {
    // }
    //
    // #[test]
    // fn is_cycle_visible_respects_trace_visibility_and_flags() {
    //     let mut filter = CycleFilter::new();
    //     filter.set_trace_visible("ch1".to_string(), true);
    //
    //     let cycle = make_dummy_cycle(/* chamber_id: "ch1", ... */);
    //
    //     assert!(
    //         filter.is_cycle_visible(&cycle, p_val, rmse, r2, t0),
    //         "cycle should be visible with default flags"
    //     );
    // }
}
