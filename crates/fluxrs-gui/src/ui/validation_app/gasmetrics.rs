// unused struct for replacing the massive line of btreeset in validationapp
#[derive(Debug, Clone, Default)]
pub struct GasMetrics {
    pub enabled_gas: bool,
    pub enabled_calc_r: bool,

    pub lin: LinMetrics,
    pub poly: PolyMetrics,
    pub roblin: RoblinMetrics,

    pub measurement_r: bool,
    pub conc_t0: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LinMetrics {
    pub flux: bool,
    pub adj_r2: bool,
    pub p_val: bool,
    pub sigma: bool,
    pub rmse: bool,
    pub cv: bool,
    pub aic: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PolyMetrics {
    pub flux: bool,
    pub adj_r2: bool,
    pub sigma: bool,
    pub rmse: bool,
    pub cv: bool,
    pub aic: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RoblinMetrics {
    pub flux: bool,
    pub adj_r2: bool,
    pub sigma: bool,
    pub rmse: bool,
    pub cv: bool,
    pub aic: bool,
}
