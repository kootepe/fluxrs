use fluxrs_core::cycle::gaskey::GasKey;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct EnabledPlots {
    pub gases: BTreeSet<GasKey>,
    pub calc_r: BTreeSet<GasKey>,

    pub lin_fluxes: BTreeSet<GasKey>,
    pub poly_fluxes: BTreeSet<GasKey>,
    pub roblin_fluxes: BTreeSet<GasKey>,
    pub exp_fluxes: BTreeSet<GasKey>,

    pub lin_adj_r2: BTreeSet<GasKey>,
    pub lin_p_val: BTreeSet<GasKey>,
    pub lin_sigma: BTreeSet<GasKey>,
    pub lin_rmse: BTreeSet<GasKey>,
    pub lin_cv: BTreeSet<GasKey>,
    pub lin_aic: BTreeSet<GasKey>,

    pub roblin_adj_r2: BTreeSet<GasKey>,
    pub roblin_sigma: BTreeSet<GasKey>,
    pub roblin_rmse: BTreeSet<GasKey>,
    pub roblin_cv: BTreeSet<GasKey>,
    pub roblin_aic: BTreeSet<GasKey>,

    pub poly_sigma: BTreeSet<GasKey>,
    pub poly_adj_r2: BTreeSet<GasKey>,
    pub poly_rmse: BTreeSet<GasKey>,
    pub poly_cv: BTreeSet<GasKey>,
    pub poly_aic: BTreeSet<GasKey>,

    pub exp_adj_r2: BTreeSet<GasKey>,
    pub exp_p_val: BTreeSet<GasKey>,
    pub exp_sigma: BTreeSet<GasKey>,
    pub exp_rmse: BTreeSet<GasKey>,
    pub exp_cv: BTreeSet<GasKey>,
    pub exp_aic: BTreeSet<GasKey>,

    pub measurement_rs: BTreeSet<GasKey>,
    pub conc_t0: BTreeSet<GasKey>,
}

impl EnabledPlots {
    pub fn is_gas_enabled(&self, key: &GasKey) -> bool {
        self.gases.contains(key)
    }

    pub fn is_lin_flux_enabled(&self, key: &GasKey) -> bool {
        self.lin_fluxes.contains(key)
    }

    pub fn is_lin_p_val_enabled(&self, key: &GasKey) -> bool {
        self.lin_p_val.contains(key)
    }

    pub fn is_lin_rmse_enabled(&self, key: &GasKey) -> bool {
        self.lin_rmse.contains(key)
    }

    pub fn is_lin_cv_enabled(&self, key: &GasKey) -> bool {
        self.lin_cv.contains(key)
    }

    pub fn is_lin_sigma_enabled(&self, key: &GasKey) -> bool {
        self.lin_sigma.contains(key)
    }

    pub fn is_lin_adj_r2_enabled(&self, key: &GasKey) -> bool {
        self.lin_adj_r2.contains(key)
    }

    pub fn is_lin_aic_enabled(&self, key: &GasKey) -> bool {
        self.lin_aic.contains(key)
    }

    pub fn is_poly_flux_enabled(&self, key: &GasKey) -> bool {
        self.poly_fluxes.contains(key)
    }

    pub fn is_poly_rmse_enabled(&self, key: &GasKey) -> bool {
        self.poly_rmse.contains(key)
    }

    pub fn is_poly_cv_enabled(&self, key: &GasKey) -> bool {
        self.poly_cv.contains(key)
    }

    pub fn is_poly_sigma_enabled(&self, key: &GasKey) -> bool {
        self.poly_sigma.contains(key)
    }

    pub fn is_poly_adj_r2_enabled(&self, key: &GasKey) -> bool {
        self.poly_adj_r2.contains(key)
    }

    pub fn is_poly_aic_enabled(&self, key: &GasKey) -> bool {
        self.poly_aic.contains(key)
    }

    pub fn is_roblin_rmse_enabled(&self, key: &GasKey) -> bool {
        self.roblin_rmse.contains(key)
    }

    pub fn is_roblin_cv_enabled(&self, key: &GasKey) -> bool {
        self.roblin_cv.contains(key)
    }

    pub fn is_roblin_sigma_enabled(&self, key: &GasKey) -> bool {
        self.roblin_sigma.contains(key)
    }

    pub fn is_roblin_adj_r2_enabled(&self, key: &GasKey) -> bool {
        self.roblin_adj_r2.contains(key)
    }

    pub fn is_roblin_aic_enabled(&self, key: &GasKey) -> bool {
        self.roblin_aic.contains(key)
    }

    pub fn is_roblin_flux_enabled(&self, key: &GasKey) -> bool {
        self.roblin_fluxes.contains(key)
    }

    pub fn is_exp_flux_enabled(&self, key: &GasKey) -> bool {
        self.exp_fluxes.contains(key)
    }

    pub fn is_exp_p_val_enabled(&self, key: &GasKey) -> bool {
        self.exp_p_val.contains(key)
    }

    pub fn is_exp_rmse_enabled(&self, key: &GasKey) -> bool {
        self.exp_rmse.contains(key)
    }

    pub fn is_exp_cv_enabled(&self, key: &GasKey) -> bool {
        self.exp_cv.contains(key)
    }

    pub fn is_exp_sigma_enabled(&self, key: &GasKey) -> bool {
        self.exp_sigma.contains(key)
    }

    pub fn is_exp_adj_r2_enabled(&self, key: &GasKey) -> bool {
        self.exp_adj_r2.contains(key)
    }

    pub fn is_exp_aic_enabled(&self, key: &GasKey) -> bool {
        self.exp_aic.contains(key)
    }

    pub fn is_calc_r_enabled(&self, key: &GasKey) -> bool {
        self.calc_r.contains(key)
    }

    pub fn is_measurement_r_enabled(&self, key: &GasKey) -> bool {
        self.measurement_rs.contains(key)
    }

    pub fn is_conc_t0_enabled(&self, key: &GasKey) -> bool {
        self.conc_t0.contains(key)
    }

    pub fn get_lin_flux_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_lin_flux_enabled(&gas))).collect()
    }

    pub fn get_lin_p_val_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_lin_p_val_enabled(&gas))).collect()
    }

    pub fn get_lin_adj_r2_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_lin_adj_r2_enabled(&gas))).collect()
    }

    pub fn get_lin_sigma_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_lin_sigma_enabled(&gas))).collect()
    }

    pub fn get_lin_rmse_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_lin_rmse_enabled(&gas))).collect()
    }

    pub fn get_lin_cv_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_lin_cv_enabled(&gas))).collect()
    }

    pub fn get_lin_aic_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_lin_aic_enabled(&gas))).collect()
    }

    pub fn get_roblin_flux_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_roblin_flux_enabled(&gas))).collect()
    }

    pub fn get_roblin_adj_r2_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_roblin_adj_r2_enabled(&gas))).collect()
    }

    pub fn get_roblin_sigma_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_roblin_sigma_enabled(&gas))).collect()
    }

    pub fn get_roblin_rmse_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_roblin_rmse_enabled(&gas))).collect()
    }

    pub fn get_roblin_cv_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_roblin_cv_enabled(&gas))).collect()
    }

    pub fn get_roblin_aic_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_roblin_aic_enabled(&gas))).collect()
    }

    pub fn get_poly_flux_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_poly_flux_enabled(&gas))).collect()
    }

    pub fn get_poly_adj_r2_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_poly_adj_r2_enabled(&gas))).collect()
    }

    pub fn get_poly_sigma_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_poly_sigma_enabled(&gas))).collect()
    }

    pub fn get_poly_rmse_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_poly_rmse_enabled(&gas))).collect()
    }

    pub fn get_poly_cv_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_poly_cv_enabled(&gas))).collect()
    }

    pub fn get_poly_aic_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_poly_aic_enabled(&gas))).collect()
    }

    pub fn get_exp_flux_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_exp_flux_enabled(&gas))).collect()
    }

    pub fn get_exp_p_val_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_exp_p_val_enabled(&gas))).collect()
    }

    pub fn get_exp_adj_r2_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_exp_adj_r2_enabled(&gas))).collect()
    }

    pub fn get_exp_sigma_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_exp_sigma_enabled(&gas))).collect()
    }

    pub fn get_exp_rmse_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_exp_rmse_enabled(&gas))).collect()
    }

    pub fn get_exp_cv_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_exp_cv_enabled(&gas))).collect()
    }

    pub fn get_exp_aic_gases(&self, gases: &[GasKey]) -> Vec<(GasKey, bool)> {
        gases.iter().copied().map(|gas| (gas, self.is_exp_aic_enabled(&gas))).collect()
    }
}
