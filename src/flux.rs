use crate::stats::{pearson_correlation, LinReg, PolyReg, RobReg};
use crate::GasType;
use dyn_clone::DynClone;
use egui::{Color32, Stroke};
use egui_plot::{Line, LineStyle};
use statrs::distribution::{ContinuousCDF, StudentsT};
use std::any::Any;

#[derive(Clone)]
pub struct FluxRecord {
    pub model: Box<dyn FluxModel>,
    pub is_valid: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FluxKind {
    Linear,
    RobLin,
    Poly,
}

impl FluxKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FluxKind::Linear => "linear",
            FluxKind::RobLin => "roblin",
            FluxKind::Poly => "poly",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            FluxKind::Linear => "linear",
            FluxKind::RobLin => "roblin",
            FluxKind::Poly => "poly",
        }
    }
    pub fn color(&self) -> Color32 {
        match self {
            FluxKind::Linear => Color32::RED,
            FluxKind::RobLin => Color32::RED,
            FluxKind::Poly => Color32::RED,
        }
    }
    pub fn stroke(&self) -> Stroke {
        match self {
            FluxKind::Linear => Stroke::new(1.5, self.color()),
            FluxKind::RobLin => Stroke::new(1.5, self.color()),
            FluxKind::Poly => Stroke::new(1.5, self.color()),
        }
    }
    pub fn style(&self) -> LineStyle {
        match self {
            FluxKind::Linear => LineStyle::Solid,
            FluxKind::RobLin => LineStyle::dashed_dense(),
            FluxKind::Poly => LineStyle::dashed_loose(),
        }
    }
    pub fn all() -> &'static [FluxKind] {
        use FluxKind::*;
        &[Linear, Poly, RobLin]
    }
}

pub trait FluxModel: Sync + Send + DynClone {
    fn fit_id(&self) -> FluxKind;
    fn gas_type(&self) -> GasType;
    fn flux(&self) -> Option<f64>;
    fn r2(&self) -> Option<f64>;
    fn adj_r2(&self) -> Option<f64>;
    fn intercept(&self) -> Option<f64>;
    fn slope(&self) -> Option<f64>;
    fn p_value(&self) -> Option<f64>;
    fn sigma(&self) -> Option<f64>;
    fn rmse(&self) -> Option<f64>;
    fn aic(&self) -> Option<f64>;
    fn predict(&self, x: f64) -> Option<f64>;
    fn set_range_start(&mut self, value: f64);
    fn set_range_end(&mut self, value: f64);
    fn range_start(&self) -> Option<f64>;
    fn range_end(&self) -> Option<f64>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
dyn_clone::clone_trait_object!(FluxModel);

#[derive(Clone)]
pub struct LinearFlux {
    pub fit_id: String,
    pub gas_type: GasType,
    pub flux: f64,
    pub r2: f64,
    pub adjusted_r2: f64,
    pub model: LinReg,
    pub p_value: f64,
    pub sigma: f64,
    pub aic: f64,
    pub rmse: f64,
    // pub intercept: f64,
    // pub slope: f64,
    pub range_start: f64,
    pub range_end: f64,
}

impl FluxModel for LinearFlux {
    fn flux(&self) -> Option<f64> {
        Some(self.flux)
    }
    fn r2(&self) -> Option<f64> {
        Some(self.r2)
    }

    fn adj_r2(&self) -> Option<f64> {
        Some(self.adjusted_r2)
    }
    fn fit_id(&self) -> FluxKind {
        FluxKind::Linear
    }
    fn gas_type(&self) -> GasType {
        self.gas_type
    }
    fn predict(&self, x: f64) -> Option<f64> {
        Some(self.model.calculate(x - self.range_start)) // normalized input
    }
    fn set_range_start(&mut self, value: f64) {
        self.range_start = value;
    }

    fn set_range_end(&mut self, value: f64) {
        self.range_end = value;
    }
    fn range_start(&self) -> Option<f64> {
        Some(self.range_start)
    }
    fn range_end(&self) -> Option<f64> {
        Some(self.range_end)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn intercept(&self) -> Option<f64> {
        Some(self.model.intercept)
    }
    fn slope(&self) -> Option<f64> {
        Some(self.model.slope)
    }
    fn sigma(&self) -> Option<f64> {
        Some(self.sigma)
    }
    fn p_value(&self) -> Option<f64> {
        Some(self.p_value)
    }
    fn aic(&self) -> Option<f64> {
        Some(self.aic)
    }
    fn rmse(&self) -> Option<f64> {
        Some(self.rmse)
    }
}

impl LinearFlux {
    pub fn from_data(
        fit_id: &str,
        gas_type: GasType,
        x: &[f64],
        y: &[f64],
        start: f64,
        end: f64,
        air_temperature: f64,
        air_pressure: f64,
        volume: f64,
    ) -> Option<Self> {
        if x.len() != y.len() || x.len() < 3 {
            return None;
        }

        let x0 = x[0]; // normalize x
        let x_norm: Vec<f64> = x.iter().map(|&t| t - x0).collect();
        let n = x.len() as f64;

        let model = LinReg::train(&x_norm, y);

        let y_hat: Vec<f64> = x_norm.iter().map(|&xi| model.calculate(xi)).collect();
        let residuals: Vec<f64> = y.iter().zip(&y_hat).map(|(&yi, &yhi)| yi - yhi).collect();
        let rss: f64 = residuals.iter().map(|r| r.powi(2)).sum();

        let sigma = (rss / (n - 2.0)).sqrt();
        let rmse_val = rmse(&y, &y_hat).unwrap_or(0.0);

        let x_mean = x_norm.iter().copied().sum::<f64>() / n;
        let ss_xx: f64 = x_norm.iter().map(|xi| (xi - x_mean).powi(2)).sum();
        let se_slope = sigma / ss_xx.sqrt();

        let t_stat = model.slope / se_slope;
        let dist = StudentsT::new(0.0, 1.0, n - 2.0).ok()?;
        let p_value = 2.0 * (1.0 - dist.cdf(t_stat.abs()));

        let aic = aic_from_rss(rss, n as usize, 2);

        let r2 = r2_from_predictions(y, &y_hat).unwrap_or(0.0);
        let adjusted_r2 = adjusted_r2(r2, n as usize, 1);

        let flux = calculate_flux(gas_type, model.slope, air_temperature, air_pressure, volume);

        Some(Self {
            fit_id: fit_id.to_string(),
            gas_type,
            flux,
            adjusted_r2,
            r2,
            model,
            p_value,
            sigma,
            aic,
            rmse: rmse_val,
            range_start: start,
            range_end: end,
        })
    }
    // pub fn from_data(
    //     fit_id: &str,
    //     gas_type: GasType,
    //     x: &[f64],
    //     y: &[f64],
    //     start: f64,
    //     end: f64,
    //     air_temperature: f64,
    //     air_pressure: f64,
    //     volume: f64,
    // ) -> Option<Self> {
    //     if x.len() != y.len() || x.len() < 3 {
    //         return None;
    //     }
    //
    //     let n = x.len() as f64;
    //     let model = LinReg::train(x, y);
    //
    //     // Compute predictions and residuals
    //     let y_hat: Vec<f64> = x.iter().map(|&xi| model.calculate(xi)).collect();
    //     let residuals: Vec<f64> = y.iter().zip(&y_hat).map(|(&yi, &yhi)| yi - yhi).collect();
    //     let rss: f64 = residuals.iter().map(|r| r.powi(2)).sum();
    //     // Standard error of regression (sigma)
    //     let sigma = (rss / (n - 2.0)).sqrt();
    //     let rmse_val = rmse(&y, &y_hat).unwrap_or(0.0);
    //
    //     // Standard error of the slope
    //     let x_mean = x.iter().copied().sum::<f64>() / n;
    //     let ss_xx: f64 = x.iter().map(|xi| (xi - x_mean).powi(2)).sum();
    //     let se_slope = sigma / ss_xx.sqrt();
    //
    //     // t-statistic and p-value
    //     let t_stat = model.slope / se_slope;
    //     let dist = StudentsT::new(0.0, 1.0, n - 2.0).ok()?;
    //     let p_value = 2.0 * (1.0 - dist.cdf(t_stat.abs()));
    //
    //     let aic = aic_from_rss(rss, n as usize, 2);
    //
    //     let r2 = pearson_correlation(x, y).unwrap_or(0.0).powi(2);
    //     let adjusted_r2 = adjusted_r2(r2, n as usize, 1);
    //     let flux = calculate_flux(gas_type, model.slope, air_temperature, air_pressure, volume);
    //
    //     Some(Self {
    //         fit_id: fit_id.to_string(),
    //         gas_type,
    //         flux,
    //         adjusted_r2,
    //         r2,
    //         model,
    //         p_value,
    //         sigma,
    //         aic,
    //         rmse: rmse_val,
    //         range_start: start,
    //         range_end: end,
    //     })
    // }
    pub fn from_values(
        fit_id: &str,
        gas_type: GasType,
        flux: f64,
        r2: f64,
        adjusted_r2: f64,
        model: LinReg,
        range_start: f64,
        range_end: f64,
        sigma: f64,
        p_value: f64,
        aic: f64,
        rmse: f64,
    ) -> Option<Self> {
        Some(Self {
            fit_id: fit_id.to_string(),
            gas_type,
            flux,
            r2,
            adjusted_r2,
            model,
            range_start,
            range_end,
            sigma,
            p_value,
            aic,
            rmse,
        })
    }
    pub fn flux_from_vec(
        &mut self,
        x: Vec<f64>,
        y: Vec<f64>,
        temperature: f64,
        pressure: f64,
        volume: f64,
    ) {
        self.model = LinReg::train(&x, &y);
        self.calculate_flux(temperature, pressure, volume)
    }
    fn calculate_flux(&mut self, temperature: f64, pressure: f64, volume: f64) {
        let mol_mass = self.gas_type.mol_mass();
        let slope_ppm = self.model.slope / self.gas_type.conv_factor();
        let slope_ppm_hour = slope_ppm * 60. * 60.;
        let p = pressure * 100.0;
        let t = temperature + 273.15;
        let r = 8.314;

        self.flux = slope_ppm_hour / 1_000_000.0 * volume * ((mol_mass * p) / (r * t)) * 1000.0
    }
}

#[derive(Clone)]
pub struct PolyFlux {
    pub fit_id: String,
    pub gas_type: GasType,
    pub flux: f64,
    pub r2: f64,
    pub adjusted_r2: f64,
    pub model: PolyReg,
    pub x_offset: f64,
    pub sigma: f64,
    pub aic: f64,
    pub rmse: f64,
    pub range_start: f64,
    pub range_end: f64,
}

impl FluxModel for PolyFlux {
    fn flux(&self) -> Option<f64> {
        Some(self.flux)
    }

    fn r2(&self) -> Option<f64> {
        Some(self.r2)
    }

    fn adj_r2(&self) -> Option<f64> {
        Some(self.adjusted_r2)
    }
    fn fit_id(&self) -> FluxKind {
        FluxKind::Poly
    }
    fn predict(&self, x: f64) -> Option<f64> {
        Some(self.model.calculate(x - self.x_offset))
    }
    fn gas_type(&self) -> GasType {
        self.gas_type
    }

    fn set_range_start(&mut self, value: f64) {
        self.range_start = value;
    }

    fn set_range_end(&mut self, value: f64) {
        self.range_end = value;
    }

    fn range_start(&self) -> Option<f64> {
        Some(self.range_start)
    }

    fn range_end(&self) -> Option<f64> {
        Some(self.range_end)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn intercept(&self) -> Option<f64> {
        Some(self.model.a0)
    }

    fn slope(&self) -> Option<f64> {
        Some(self.model.a1) // derivative at x=0
    }

    fn sigma(&self) -> Option<f64> {
        Some(self.sigma)
    }

    fn p_value(&self) -> Option<f64> {
        None
    }
    fn aic(&self) -> Option<f64> {
        Some(self.aic)
    }
    fn rmse(&self) -> Option<f64> {
        Some(self.rmse)
    }
}

impl PolyFlux {
    pub fn from_data(
        fit_id: &str,
        gas_type: GasType,
        x: &[f64],
        y: &[f64],
        start: f64,
        end: f64,
        air_temperature: f64,
        air_pressure: f64,
        volume: f64,
    ) -> Option<Self> {
        if x.len() != y.len() || x.len() < 3 {
            return None;
        }

        let x0 = x[0]; // normalize to start
        let x_norm: Vec<f64> = x.iter().map(|t| t - x0).collect();

        let model = PolyReg::train(&x_norm, y)?;

        let y_hat: Vec<f64> = x_norm.iter().map(|&xi| model.calculate(xi)).collect();
        let r2 = r2_from_predictions(y, &y_hat).unwrap_or(0.0);
        let rmse = rmse(&y, &y_hat).unwrap_or(0.0);

        let n = y.len();
        let k = 2; // predictors: x and x² (intercept is implicit)

        let adjusted_r2 = adjusted_r2(r2, n, k);
        let rss: f64 = y.iter().zip(&y_hat).map(|(&yi, &yhi)| (yi - yhi).powi(2)).sum();
        let aic = aic_from_rss(rss, n, k + 1); // k + 1 = slope + quad + intercept
        let sigma = (rss / (n as f64 - k as f64 - 1.0)).sqrt();

        // Evaluate slope at midpoint of the fit range (normalized)
        let x_mid = ((start - x0) + (end - x0)) / 2.0;
        let slope_at_mid = model.a1 + 2.0 * model.a2 * x_mid;

        let flux = calculate_flux(gas_type, slope_at_mid, air_temperature, air_pressure, volume);

        Some(Self {
            fit_id: fit_id.to_string(),
            gas_type,
            flux,
            r2,
            adjusted_r2,
            model,
            range_start: start,
            range_end: end,
            x_offset: x0,
            aic,
            sigma,
            rmse,
        })
    }
}

#[derive(Clone)]
pub struct RobustFlux {
    pub fit_id: String,
    pub gas_type: GasType,
    pub flux: f64,
    pub r2: f64,
    pub adjusted_r2: f64,
    pub model: RobReg,
    pub sigma: f64,
    pub aic: f64,
    pub rmse: f64,
    pub range_start: f64,
    pub range_end: f64,
}

impl FluxModel for RobustFlux {
    fn flux(&self) -> Option<f64> {
        Some(self.flux)
    }

    fn r2(&self) -> Option<f64> {
        Some(self.r2)
    }

    fn adj_r2(&self) -> Option<f64> {
        Some(self.adjusted_r2)
    }

    fn fit_id(&self) -> FluxKind {
        FluxKind::RobLin
    }

    fn gas_type(&self) -> GasType {
        self.gas_type
    }
    fn predict(&self, x: f64) -> Option<f64> {
        Some(self.model.calculate(x - self.range_start))
    }
    fn set_range_start(&mut self, value: f64) {
        self.range_start = value;
    }

    fn set_range_end(&mut self, value: f64) {
        self.range_end = value;
    }

    fn range_start(&self) -> Option<f64> {
        Some(self.range_start)
    }

    fn range_end(&self) -> Option<f64> {
        Some(self.range_end)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn intercept(&self) -> Option<f64> {
        Some(self.model.intercept)
    }

    fn slope(&self) -> Option<f64> {
        Some(self.model.slope)
    }

    fn sigma(&self) -> Option<f64> {
        Some(self.sigma)
    }

    fn p_value(&self) -> Option<f64> {
        None
    }

    fn aic(&self) -> Option<f64> {
        Some(self.aic)
    }

    fn rmse(&self) -> Option<f64> {
        Some(self.rmse)
    }
}

impl RobustFlux {
    pub fn from_data(
        fit_id: &str,
        gas_type: GasType,
        x: &[f64],
        y: &[f64],
        start: f64,
        end: f64,
        air_temperature: f64,
        air_pressure: f64,
        volume: f64,
    ) -> Option<Self> {
        if x.len() != y.len() || x.len() < 3 {
            return None;
        }

        let x0 = x[0];
        let x_norm: Vec<f64> = x.iter().map(|t| t - x0).collect();

        let model = RobReg::train(&x_norm, y, 1.0, 10)?;

        let y_hat: Vec<f64> = x_norm.iter().map(|&xi| model.calculate(xi)).collect();
        let r2 = r2_from_predictions(y, &y_hat).unwrap_or(0.0);
        let rmse_val = rmse(y, &y_hat).unwrap_or(0.0);

        let n = y.len();
        let k = 1;

        let adjusted_r2 = adjusted_r2(r2, n, k);
        let rss: f64 = y.iter().zip(&y_hat).map(|(&yi, &yhi)| (yi - yhi).powi(2)).sum();
        let sigma = (rss / (n as f64 - 2.0)).sqrt();
        let aic = aic_from_rss(rss, n, 2);

        // slope at midpoint of range (normalized x)
        let slope_at_mid = model.slope; // constant for linear model

        let flux = calculate_flux(gas_type, slope_at_mid, air_temperature, air_pressure, volume);

        Some(Self {
            fit_id: fit_id.to_string(),
            gas_type,
            flux,
            r2,
            adjusted_r2,
            model,
            sigma,
            aic,
            rmse: rmse_val,
            range_start: start,
            range_end: end,
        })
    }
}

pub fn calculate_flux(
    gas_type: GasType,
    slope: f64,
    air_temperature: f64,
    air_pressure: f64,
    volume: f64,
) -> f64 {
    let mol_mass = gas_type.mol_mass();
    let slope_ppm = slope / gas_type.conv_factor();
    let slope_ppm_hour = slope_ppm * 60. * 60.;
    let p = air_pressure * 100.0;
    let t = air_temperature + 273.15;
    let r = 8.314;

    slope_ppm_hour / 1_000_000.0 * volume * ((mol_mass * p) / (r * t)) * 1000.0
}

pub fn r2_from_predictions(y: &[f64], y_hat: &[f64]) -> Option<f64> {
    if y.len() != y_hat.len() || y.len() < 2 {
        return None;
    }

    let y_mean = y.iter().sum::<f64>() / y.len() as f64;

    let ss_res: f64 = y.iter().zip(y_hat).map(|(&yi, &yhi)| (yi - yhi).powi(2)).sum();
    let ss_tot: f64 = y.iter().map(|&yi| (yi - y_mean).powi(2)).sum();

    if ss_tot == 0.0 {
        return None;
    }

    Some(1.0 - ss_res / ss_tot)
}
pub fn adjusted_r2(r2: f64, n: usize, k: usize) -> f64 {
    if n <= k + 1 {
        return r2; // Not enough data to adjust
    }
    1.0 - (1.0 - r2) * (n as f64 - 1.0) / (n as f64 - k as f64 - 1.0)
}

pub fn rmse(y: &[f64], y_hat: &[f64]) -> Option<f64> {
    if y.len() != y_hat.len() || y.is_empty() {
        return None;
    }

    let sum_sq: f64 = y.iter().zip(y_hat.iter()).map(|(&yi, &yhi)| (yi - yhi).powi(2)).sum();

    Some((sum_sq / y.len() as f64).sqrt())
}
pub fn aic_from_rss(rss: f64, n: usize, k: usize) -> f64 {
    if rss <= 0.0 || n == 0 {
        return f64::INFINITY; // Avoid log(0) or divide-by-zero
    }
    n as f64 * (rss / n as f64).ln() + 2.0 * k as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_robust_flux_fit() {
        let gas = GasType::CH4;
        let fit_id = "robust";
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let y = vec![1.0, 2.0, 3.0, 4.0, 100.0]; // outlier included

        let start = x[0];
        let end = x[x.len() - 1];
        let temperature = 20.0; // °C
        let pressure = 1013.25; // hPa
        let volume = 10.0; // L

        let flux =
            RobustFlux::from_data(fit_id, gas, &x, &y, start, end, temperature, pressure, volume)
                .expect("RobustFlux creation failed");

        // Check computed values
        dbg!(flux.r2, flux.adjusted_r2, flux.rmse, flux.sigma, flux.aic);
        assert!(flux.r2 >= 0.0 && flux.r2 <= 1.0);
        assert!(flux.adjusted_r2 <= flux.r2);
        assert!(flux.rmse >= 0.0);
        assert!(flux.sigma >= 0.0);
        assert!(flux.aic.is_finite());
        assert_eq!(flux.fit_id(), FluxKind::RobLin);
    }
}
