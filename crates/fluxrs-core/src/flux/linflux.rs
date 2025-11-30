use crate::data_formats::chamberdata::Chamber;
use crate::flux::flux::flux_umol_m2_s;
use crate::flux::fluxfiterror::{FluxFitError, FluxResult};
use crate::flux::fluxkind::FluxKind;
use crate::flux::fluxmodel::FluxModel;
use crate::gaschannel::GasChannel;
use crate::stats::{adjusted_r2, aic_from_rss, r2_from_predictions, rmse, LinReg};

use statrs::distribution::{ContinuousCDF, StudentsT};

use std::any::Any;
use std::fmt;

impl fmt::Display for LinearFlux {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}, {}, flux: {}, r2: {}, len: {}",
            self.model,
            self.gas_channel.gas,
            self.flux,
            self.r2,
            (self.range_end - self.range_start)
        )
    }
}

#[derive(Clone)]
pub struct LinearFlux {
    pub fit_id: String,
    pub gas_channel: GasChannel,
    pub flux: f64,
    pub r2: f64,
    pub adjusted_r2: f64,
    pub model: LinReg,
    pub p_value: f64,
    pub sigma: f64,
    pub aic: f64,
    pub rmse: f64,
    pub cv: f64,
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
    fn gas_channel(&self) -> GasChannel {
        self.gas_channel.clone()
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
    fn cv(&self) -> Option<f64> {
        Some(self.cv)
    }
}

impl LinearFlux {
    pub fn from_data(
        fit_id: &str,
        channel: GasChannel,
        x: &[f64],
        y: &[f64],
        start: f64,
        end: f64,
        air_temperature: f64,
        air_pressure: f64,
        chamber: Chamber,
    ) -> FluxResult<Self> {
        if x.len() != y.len() {
            return return Err(FluxFitError::LengthMismatch { len_x: x.len(), len_y: y.len() });
        }
        if x.len() < 3 {
            return Err(FluxFitError::NotEnoughPoints { len: x.len(), needed: 3 });
        }

        let x0 = x[0]; // normalize x
        let x_norm: Vec<f64> = x.iter().map(|&t| t - x0).collect();
        let n = x.len() as f64;
        // println!("{} {} {}", n, x.len(), y.len());
        // println!("{:?} {:?}", y, x);

        let model = LinReg::train(&x_norm, y);

        let y_hat: Vec<f64> = x_norm.iter().map(|&xi| model.calculate(xi)).collect();
        let residuals: Vec<f64> = y.iter().zip(&y_hat).map(|(&yi, &yhi)| yi - yhi).collect();
        let rss: f64 = residuals.iter().map(|r| r.powi(2)).sum();

        let rmse_val = rmse(y, &y_hat).unwrap_or(0.0);
        let y_mean = y.iter().copied().sum::<f64>() / n;
        let cv = rmse_val / y_mean;

        let x_mean = x_norm.iter().copied().sum::<f64>() / n;
        let ss_xx: f64 = x_norm.iter().map(|xi| (xi - x_mean).powi(2)).sum();

        // no variance in x, no meaningful regression
        if !ss_xx.is_finite() || ss_xx <= f64::EPSILON {
            return Err(FluxFitError::DegenerateX);
        }
        let sigma = (rss / (n - 2.0)).sqrt();
        if !sigma.is_finite() {
            return Err(FluxFitError::NonFiniteSigma);
        }

        let se_slope = sigma / ss_xx.sqrt();
        if !se_slope.is_finite() || se_slope <= 0.0 {
            // e.g. perfect fit (sigma = 0) or degenerate
            // you can decide how to handle this; example: p_value = 0 or 1
            return Err(FluxFitError::NonFiniteSE);
        }

        let t_stat = model.slope / se_slope;
        if !t_stat.is_finite() {
            return Err(FluxFitError::NonFiniteTStat);
        }
        let dist = StudentsT::new(0.0, 1.0, n - 2.0)
            .map_err(|_| FluxFitError::StatError("failed to construct StudentsT"))?;
        let p_value = 2.0 * (1.0 - dist.cdf(t_stat.abs()));

        let aic = aic_from_rss(rss, n as usize, 2);

        let r2 = r2_from_predictions(y, &y_hat).unwrap_or(0.0);
        let adjusted_r2 = adjusted_r2(r2, n as usize, 1);

        let flux = flux_umol_m2_s(&channel, model.slope, air_temperature, air_pressure, &chamber);

        Ok(Self {
            fit_id: fit_id.to_string(),
            gas_channel: channel,
            flux,
            adjusted_r2,
            r2,
            model,
            p_value,
            sigma,
            aic,
            rmse: rmse_val,
            cv,
            range_start: start,
            range_end: end,
        })
    }
    pub fn from_values(
        fit_id: &str,
        gas_channel: GasChannel,
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
        cv: f64,
    ) -> Self {
        Self {
            fit_id: fit_id.to_string(),
            gas_channel,
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
            cv,
        }
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
        self.flux_umol_m2_s(temperature, pressure, volume)
    }
    fn flux_umol_m2_s(&mut self, temperature: f64, pressure: f64, volume: f64) {
        let mol_mass = self.gas_channel.gas.mol_mass();
        let slope_ppm = self.model.slope / self.gas_channel.gas.conv_factor();
        let slope_ppm_hour = slope_ppm * 60. * 60.;
        let p = pressure * 100.0;
        let t = temperature + 273.15;
        let r = 8.314;

        self.flux = slope_ppm_hour / 1_000_000.0 * volume * ((mol_mass * p) / (r * t)) * 1000.0
    }
}
