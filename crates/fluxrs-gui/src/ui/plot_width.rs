pub struct PlotAdjust {
    // TODO: Either make the fields tuples or make a struct for each plot
    pub lag_w: f32,
    pub lag_h: f32,

    pub gas_w: f32,
    pub gas_h: f32,

    pub flux_w: f32,
    pub flux_h: f32,

    pub measurement_r_w: f32,
    pub measurement_r_h: f32,

    pub calc_r_w: f32,
    pub calc_r_h: f32,

    pub conc_t0_w: f32,
    pub conc_t0_h: f32,
}

impl PlotAdjust {
    pub fn new() -> Self {
        Self {
            lag_w: 600.,
            lag_h: 350.,
            gas_w: 600.,
            gas_h: 350.,
            flux_w: 600.,
            flux_h: 350.,
            calc_r_w: 600.,
            calc_r_h: 350.,
            conc_t0_w: 600.,
            conc_t0_h: 350.,
            measurement_r_w: 600.,
            measurement_r_h: 350.,
        }
    }
}

impl Default for PlotAdjust {
    fn default() -> Self {
        Self::new()
    }
}
