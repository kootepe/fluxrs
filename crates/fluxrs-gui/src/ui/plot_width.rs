pub struct PlotAdjust {
    // TODO: Either make the fields tuples or make a struct for each plot width
    pub lag_plot_w: f32,
    pub lag_plot_h: f32,
    pub gas_plot_w: f32,
    pub gas_plot_h: f32,
    pub flux_plot_w: f32,
    pub flux_plot_h: f32,
    pub measurement_r_plot_w: f32,
    pub measurement_r_plot_h: f32,
    pub calc_r_plot_w: f32,
    pub calc_r_plot_h: f32,
    pub conc_t0_plot_w: f32,
    pub conc_t0_plot_h: f32,
}

impl PlotAdjust {
    pub fn new() -> Self {
        Self {
            lag_plot_w: 600.,
            lag_plot_h: 350.,
            gas_plot_w: 600.,
            gas_plot_h: 350.,
            flux_plot_w: 600.,
            flux_plot_h: 350.,
            calc_r_plot_w: 600.,
            calc_r_plot_h: 350.,
            conc_t0_plot_w: 600.,
            conc_t0_plot_h: 350.,
            measurement_r_plot_w: 600.,
            measurement_r_plot_h: 350.,
        }
    }
}

impl Default for PlotAdjust {
    fn default() -> Self {
        Self::new()
    }
}
