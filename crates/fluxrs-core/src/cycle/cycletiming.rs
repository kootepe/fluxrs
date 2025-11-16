use crate::cycle::gaskey::GasKey;
use crate::types::FastMap;
use chrono::{DateTime, Duration, Utc};
use chrono_tz::{Tz, UTC};

impl Default for CycleTiming {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone, Debug)]
pub struct CycleTiming {
    /// Absolute start timestamp of the cycle
    start_time: DateTime<Tz>,

    /// Offsets relative to start_time (seconds or samples)
    close_offset: i64,
    open_offset: i64,
    end_offset: i64,

    /// Time lags (seconds)
    start_lag_s: f64,
    close_lag_s: f64,
    open_lag_s: f64,
    end_lag_s: f64,

    /// Measurement window (in dt-index units, usually seconds or indices)
    measurement_range_start: f64,
    measurement_range_end: f64,

    /// Deadbands per gas key (still scalar values)
    deadbands: FastMap<GasKey, f64>,

    min_calc_len: f64,

    calc_range_start: FastMap<GasKey, f64>,
    calc_range_end: FastMap<GasKey, f64>,
    /// Time axes
    dt_v: FastMap<i64, Vec<f64>>,
    calc_dt_v: FastMap<GasKey, Vec<f64>>,
    measurement_dt_v: Vec<f64>,
}

impl CycleTiming {
    pub fn new_from_fields(
        start: DateTime<Tz>,
        close: i64,
        open: i64,
        end: i64,
        start_lag: f64,
        close_lag: f64,
        open_lag: f64,
        end_lag: f64,
        meas_start: f64,
        meas_end: f64,
        deadbands: FastMap<GasKey, f64>,
        min_calc_len: f64,
    ) -> Self {
        Self {
            // core timing
            start_time: start,
            close_offset: close,
            open_offset: open,
            end_offset: end,

            // lags
            start_lag_s: start_lag,
            close_lag_s: close_lag,
            open_lag_s: open_lag,
            end_lag_s: end_lag,

            // measurement window
            measurement_range_start: meas_start,
            measurement_range_end: meas_end,

            // per-gas settings
            deadbands,
            min_calc_len,

            // derived / “to be filled later” ranges & axes
            calc_range_start: FastMap::default(),
            calc_range_end: FastMap::default(),
            dt_v: FastMap::default(),
            calc_dt_v: FastMap::default(),
            measurement_dt_v: Vec::new(),
        }
    }
    pub fn new_from_offsets(
        start: DateTime<Tz>,
        close: i64,
        open: i64,
        end: i64,
        min_len: f64,
    ) -> Self {
        Self {
            start_time: start,
            close_offset: close,
            open_offset: open,
            end_offset: end,
            min_calc_len: min_len,
            ..Default::default()
        }
    }

    pub fn new() -> Self {
        let now_utc: DateTime<Utc> = Utc::now();
        let now_tz: DateTime<Tz> = now_utc.with_timezone(&UTC);
        let start_time = now_tz - Duration::days(7);
        Self {
            start_time,
            close_offset: 0,
            open_offset: 0,
            end_offset: 0,
            start_lag_s: 0.,
            close_lag_s: 0.,
            open_lag_s: 0.,
            end_lag_s: 0.,
            measurement_range_start: 0.,
            measurement_range_end: 0.,
            min_calc_len: 0.,
            deadbands: FastMap::default(),
            calc_range_start: FastMap::default(),
            calc_range_end: FastMap::default(),
            dt_v: FastMap::default(),
            calc_dt_v: FastMap::default(),
            measurement_dt_v: Vec::new(),
        }
    }

    pub fn get_start(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.start_lag_s
    }
    pub fn get_start_time(&self) -> DateTime<Tz> {
        self.start_time
    }
    pub fn get_timezone(&self) -> Tz {
        self.start_time.timezone()
    }

    pub fn get_end(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.end_lag_s + self.end_offset as f64
    }

    pub fn get_start_ts(&self) -> i64 {
        self.start_time.timestamp()
    }
    pub fn get_end_ts(&self) -> i64 {
        self.get_start_ts() + self.end_offset
    }
    pub fn get_start_utc_ts(&self) -> i64 {
        self.start_time.to_utc().timestamp()
    }
    pub fn get_end_utc_ts(&self) -> i64 {
        self.get_start_utc_ts() + self.end_offset
    }
    pub fn get_close(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.close_offset as f64
    }
    pub fn get_open(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.open_offset as f64
    }
    pub fn get_close_offset(&self) -> i64 {
        self.close_offset
    }
    pub fn get_open_offset(&self) -> i64 {
        self.open_offset
    }
    pub fn get_end_offset(&self) -> i64 {
        self.end_offset
    }
    pub fn get_start_lag(&self) -> f64 {
        self.start_lag_s
    }
    pub fn get_close_lag(&self) -> f64 {
        self.close_lag_s
    }
    pub fn get_open_lag(&self) -> f64 {
        self.open_lag_s
    }
    pub fn get_end_lag(&self) -> f64 {
        self.end_lag_s
    }
    pub fn get_min_calc_len(&self) -> f64 {
        self.min_calc_len
    }
    pub fn get_adjusted_close(&self) -> f64 {
        self.start_time.timestamp() as f64
            + self.close_offset as f64
            + self.open_lag_s
            + self.close_lag_s
    }
    pub fn get_adjusted_open(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.open_offset as f64 + self.open_lag_s
    }
    pub fn get_calc_start(&self, key: &GasKey) -> f64 {
        *self.calc_range_start.get(key).unwrap_or(&0.0)
    }

    pub fn get_calc_end(&self, key: &GasKey) -> f64 {
        *self.calc_range_end.get(key).unwrap_or(&0.0)
    }

    pub fn get_measurement_start(&self) -> f64 {
        self.start_time.timestamp() as f64
            + self.close_offset as f64
            + self.open_lag_s
            + self.close_lag_s
    }

    pub fn set_dt_v(&mut self, key: i64, dt_v: &[f64]) {
        self.dt_v.insert(key, dt_v.to_vec());
    }
    pub fn set_dt_v_all(&mut self, dt_v: FastMap<i64, Vec<f64>>) {
        self.dt_v = dt_v;
    }
    pub fn set_calc_dt_v(&mut self, key: &GasKey, dt_v: &[f64]) {
        self.calc_dt_v.insert(*key, dt_v.to_vec());
    }
    pub fn get_measurement_end(&self) -> f64 {
        self.start_time.timestamp() as f64 + self.open_offset as f64 + self.open_lag_s
    }

    pub fn get_deadband(&self, key: &GasKey) -> f64 {
        *self.deadbands.get(key).unwrap_or(&0.0)
    }

    pub fn set_calc_start(&mut self, key: &GasKey, value: f64) {
        let range_min = self.get_adjusted_close() + self.deadbands.get(key).unwrap_or(&0.0);
        // the calc area cant go beyond the measurement area
        if range_min > value {
            self.calc_range_start.insert(*key, range_min);
        } else {
            self.calc_range_start.insert(*key, value);
        }
    }
    pub fn set_calc_start_all(&mut self, gases: &[GasKey], value: f64) {
        for key in gases.iter() {
            let range_min = self.get_adjusted_close() + self.deadbands.get(key).unwrap_or(&0.0);
            // the calc area cant go beyond the measurement area
            if range_min > value {
                self.calc_range_start.insert(*key, range_min);
            } else {
                self.calc_range_start.insert(*key, value);
            }
        }
    }
    pub fn set_calc_end(&mut self, key: &GasKey, value: f64) {
        let range_max = self.get_adjusted_open();
        // the calc area cant go beyond the measurement area
        if value > range_max {
            self.calc_range_end.insert(*key, range_max);
        } else {
            self.calc_range_end.insert(*key, value);
        }
        // self.adjust_calc_range_all();
    }
    pub fn set_calc_end_all(&mut self, gases: &[GasKey], value: f64) {
        for key in gases.iter() {
            let range_min = self.get_adjusted_close() + self.deadbands.get(key).unwrap_or(&0.0);
            // the calc area cant go beyond the measurement area
            if range_min > value {
                self.calc_range_end.insert(*key, range_min);
            } else {
                self.calc_range_end.insert(*key, value);
            }
        }
    }

    pub fn set_start_lag_s(&mut self, new_lag: f64) {
        let old_lag = self.start_lag_s;
        self.start_lag_s = new_lag;
        if self.get_start() > self.get_adjusted_close() {
            self.start_lag_s = old_lag;
            println!("Can't remove data from measurement.");
        }
    }

    pub fn set_close_lag(&mut self, new_lag: f64) {
        self.close_lag_s = new_lag;
    }

    pub fn set_open_lag(&mut self, new_lag: f64) {
        self.open_lag_s = new_lag;
    }
    pub fn set_end_lag(&mut self, new_lag: f64) {
        self.end_lag_s = new_lag;
    }

    pub fn increment_start_lag(&mut self, delta: f64) {
        self.start_lag_s += delta;
    }
    pub fn increment_close_lag(&mut self, delta: f64) {
        self.close_lag_s += delta;
    }
    pub fn increment_open_lag(&mut self, delta: f64) {
        self.open_lag_s += delta;
    }
    pub fn increment_end_lag(&mut self, delta: f64) {
        self.end_lag_s += delta;

        let old_lag = self.end_lag_s;
        self.end_lag_s += delta;
        if self.get_adjusted_open() > self.get_end() {
            self.end_lag_s = old_lag;
        }
    }

    pub fn set_end_lag_only(&mut self, new_lag: f64) {
        self.set_end_lag_s(new_lag)
    }

    pub fn set_end_lag_s(&mut self, new_lag: f64) {
        let old_lag = self.end_lag_s;
        self.end_lag_s = new_lag;
        if self.get_adjusted_open() > self.get_end() {
            self.end_lag_s = old_lag;
        }
    }
    pub fn set_deadband(&mut self, key: &GasKey, deadband: f64) {
        self.deadbands.insert(*key, deadband.max(0.));
        // self.adjust_calc_range_all_deadband();

        // self.check_errors();
        // self.calculate_measurement_rs();
        // self.compute_all_fluxes();
    }

    pub fn set_deadband_constant_calc(&mut self, gases: &[GasKey], x: f64) {
        for &key in gases {
            let deadband = self.deadbands.get(&key).unwrap_or(&0.0);
            let new_db = deadband + x;
            self.deadbands.insert(key, new_db.max(0.));

            let s = self.get_calc_start(&key);
            let new_s = s + x;
            self.calc_range_start.insert(key, new_s);

            let e = self.get_calc_end(&key);
            let new_e = e + x;
            self.calc_range_end.insert(key, new_e);
        }
    }
    pub fn get_dt_v(&self, key: &i64) -> Vec<f64> {
        match self.dt_v.get(key) {
            Some(vec) => vec.clone(),
            None => vec![],
        }
    }

    pub fn set_measurement_start(&mut self, value: f64) {
        self.measurement_range_start = value;
    }
    pub fn set_measurement_end(&mut self, value: f64) {
        self.measurement_range_end = value;
    }
    fn clamp_range(
        (min_b, max_b): (f64, f64),
        mut start: f64,
        mut end: f64,
        min_len: f64,
    ) -> (f64, f64) {
        if end < start {
            std::mem::swap(&mut start, &mut end);
        }
        if end - start < min_len {
            end = (start + min_len).min(max_b);
            start = (end - min_len).max(min_b);
        }
        if start < min_b {
            let d = min_b - start;
            start += d;
            end += d;
        }
        if end > max_b {
            let d = end - max_b;
            start -= d;
            end -= d;
        }
        (start, end)
    }

    fn set_calc_length_sticky_start(&mut self, key: &GasKey, new_len: f64) {
        let (min_b, max_b) = self.bounds_for(key);
        let (s, e) = Self::clamp_range((min_b, max_b), min_b, min_b + new_len, self.min_calc_len);
        self.calc_range_start.insert(*key, s);
        self.calc_range_end.insert(*key, e);
    }

    pub fn stick_calc_to_range_start(&mut self, key: &GasKey) {
        let (min_b, max_b) = self.bounds_for(key);
        let cur_len = (self.get_calc_end(key) - self.get_calc_start(key)).max(self.min_calc_len);
        let (s, e) = Self::clamp_range((min_b, max_b), min_b, min_b + cur_len, self.min_calc_len);
        self.calc_range_start.insert(*key, s);
        self.calc_range_end.insert(*key, e);
    }
    pub fn stick_calc_to_range_start_for_all(&mut self, gases: &[GasKey]) {
        let keys: Vec<_> = gases.to_vec();
        for key in &keys {
            self.stick_calc_to_range_start(key);
        }
    }

    pub fn drag_main(&mut self, key: &GasKey, dx_steps: f64) {
        let (min_b, max_b) = self.bounds_for(key);
        let s0 = self.get_calc_start(key);
        let e0 = self.get_calc_end(key);
        let (s, e) =
            Self::clamp_range((min_b, max_b), s0 + dx_steps, e0 + dx_steps, self.min_calc_len);
        self.calc_range_start.insert(*key, s);
        self.calc_range_end.insert(*key, e);
    }

    pub fn drag_left_to(&mut self, key: &GasKey, new_start: f64) {
        let (min_b, max_b) = self.bounds_for(key);
        let e0 = self.get_calc_end(key);
        let (s, e) = Self::clamp_range((min_b, max_b), new_start, e0, self.min_calc_len);
        self.calc_range_start.insert(*key, s);
        self.calc_range_end.insert(*key, e);
    }

    pub fn drag_right_to(&mut self, key: &GasKey, new_end: f64) {
        let (min_b, max_b) = self.bounds_for(key);
        let s0 = self.get_calc_start(key);
        let (s, e) = Self::clamp_range((min_b, max_b), s0, new_end, self.min_calc_len);
        self.calc_range_start.insert(*key, s);
        self.calc_range_end.insert(*key, e);
    }

    pub fn bounds_for(&self, key: &GasKey) -> (f64, f64) {
        let min_b = self.get_measurement_start() + self.get_deadband(key);
        let max_b = self.get_measurement_end();
        (min_b, max_b)
    }

    pub fn adjust_calc_range_all(&mut self, gases: &[GasKey]) {
        for key in gases.iter().copied() {
            let range_min =
                self.get_adjusted_close() + self.deadbands.get(&key).copied().unwrap_or(0.0);
            let range_max = self.get_adjusted_open();

            // Get current calc interval (fall back to full available)
            let start0 = *self.calc_range_start.get(&key).unwrap_or(&range_min);
            let end0 = *self.calc_range_end.get(&key).unwrap_or(&range_max);

            let (start, end) = self.adjust_interval_to_bounds(
                start0,
                end0,
                range_min,
                range_max,
                self.min_calc_len,
            );

            self.calc_range_start.insert(key, start);
            self.calc_range_end.insert(key, end);
        }
    }

    fn adjust_interval_to_bounds(
        &self,
        mut start: f64,
        mut end: f64,
        range_min: f64,
        range_max: f64,
        min_range: f64,
    ) -> (f64, f64) {
        // Normalize & basics
        if end < start {
            std::mem::swap(&mut start, &mut end);
        }
        let mut L = (end - start).max(0.0);
        let available = (range_max - range_min).max(0.0);

        // Case A: need at least min_range
        if L < min_range {
            let target = min_range.min(available);
            if target <= f64::EPSILON {
                return (range_min, range_min);
            }

            // Try to keep center if possible
            let mut center =
                if L > f64::EPSILON { 0.5 * (start + end) } else { 0.5 * (range_min + range_max) };
            let half = 0.5 * target;

            // Clamp center to keep [center - half, center + half] inside bounds
            center = center.clamp(range_min + half, range_max - half);

            let new_start = center - half;
            let new_end = center + half;
            return (new_start, new_end);
        }

        // Case B: keep length if possible
        let mut keep_len = L.min(available);
        if keep_len <= f64::EPSILON {
            return (range_min, range_min);
        }

        // First, try to keep the same start/end but clamp into bounds while preserving length.
        // Shift right if we overlap the new min.
        if start < range_min {
            start = range_min;
            end = start + keep_len;
        }
        // Shift left if we overlap the new max.
        if end > range_max {
            end = range_max;
            start = end - keep_len;
        }

        // If length no longer fits (because available < original L), shrink from the side we just clamped.
        // Prefer keeping the interval adjacent to its last position:
        if (end - start) < keep_len - f64::EPSILON {
            keep_len = available; // must shrink
                                  // Anchor to whichever side was touching bounds:
            if start <= range_min + f64::EPSILON {
                // anchored to left
                start = range_min;
                end = start + keep_len;
            } else if end >= range_max - f64::EPSILON {
                // anchored to right
                end = range_max;
                start = end - keep_len;
            } else {
                // otherwise re-center
                let half = 0.5 * keep_len;
                let mut center = 0.5 * (start + end);
                center = center.clamp(range_min + half, range_max - half);
                start = center - half;
                end = center + half;
            }
        }

        // Final clamps (numerical safety)
        start = start.clamp(range_min, range_max - keep_len);
        end = start + keep_len;

        (start, end)
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

    pub fn get_calc_range(&self, key: &GasKey) -> f64 {
        let start = self.get_calc_start(key);
        let end = self.get_calc_end(key);
        end - start
    }
    pub fn adjust_calc_range_all_deadband(&mut self, gases: &[GasKey]) {
        let keys: Vec<_> = gases.to_vec();
        for key in &keys {
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
                self.deadbands.insert(*key, deadband);
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

            self.calc_range_start.insert(*key, start);
            self.calc_range_end.insert(*key, end);
        }
    }
}
