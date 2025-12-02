use crate::data_formats::chamberdata::Chamber;
use crate::flux::flux::{flux_umol_m2_s, GasChannelData, MeteoConditions, TimeRange};
use crate::flux::fluxfiterror::{FluxFitError, FluxResult};
use crate::flux::fluxkind::FluxKind;
use crate::flux::fluxmodel::FluxModel;
use crate::gaschannel::GasChannel;
use crate::stats::{adjusted_r2, aic_from_rss, r2_from_predictions, rmse, ExpReg, LinReg};

use statrs::distribution::{ContinuousCDF, StudentsT};

use std::any::Any;

#[derive(Clone)]
pub struct ExponentialFlux {
    pub gas_channel: GasChannel,
    pub flux: f64,
    pub r2: f64,
    pub adjusted_r2: f64,
    pub model: ExpReg,
    pub p_value: f64,
    pub sigma: f64,
    pub aic: f64,
    pub rmse: f64,
    pub cv: f64,
    pub range_start: f64,
    pub range_end: f64,
}

impl FluxModel for ExponentialFlux {
    fn flux(&self) -> Option<f64> {
        Some(self.flux)
    }

    fn r2(&self) -> Option<f64> {
        Some(self.r2)
    }

    fn adj_r2(&self) -> Option<f64> {
        Some(self.adjusted_r2)
    }

    fn kind(&self) -> FluxKind {
        FluxKind::Exponential
    }
    fn gas_channel(&self) -> GasChannel {
        self.gas_channel.clone()
    }

    fn predict(&self, x: f64) -> Option<f64> {
        // normalize like LinearFlux: prediction on (x - range_start)
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

    /// Intercept here is y(0) = a (with time normalized to 0 at start).
    fn intercept(&self) -> Option<f64> {
        Some(self.model.a)
    }

    /// "Slope" is the initial derivative f0 = dy/dx at t0:
    /// For y = a * exp(b x), dy/dx|_{x=0} = a * b
    fn slope(&self) -> Option<f64> {
        Some(self.model.a * self.model.b)
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

impl ExponentialFlux {
    pub fn from_data(
        data: &GasChannelData,
        range: &TimeRange,
        meteo: &MeteoConditions,
        chamber: &Chamber,
    ) -> FluxResult<Self> {
        if !data.equal_len() {
            return Err(FluxFitError::LengthMismatch { len_x: data.xlen(), len_y: data.ylen() });
        }
        if data.xlen() < 3 {
            return Err(FluxFitError::NotEnoughPoints { len: data.xlen(), needed: 3 });
        }

        let x = data.x();
        let y = data.y();
        if !y.iter().all(|&v| v > 0.0) {
            return Err(FluxFitError::NonPositiveY);
        }

        let n = x.len() as f64;

        // Normalize time so that t0 = 0 (like in LinearFlux)
        let x0 = x[0];
        let x_norm: Vec<f64> = x.iter().map(|&t| t - x0).collect();

        // --- Fit exponential model y = a * exp(b * x) ---

        let model = ExpReg::train(&x_norm, y);

        // Predictions in original space
        let y_hat: Vec<f64> = x_norm.iter().map(|&xi| model.calculate(xi)).collect();
        let residuals: Vec<f64> = y.iter().zip(&y_hat).map(|(&yi, &yhi)| yi - yhi).collect();

        let rss: f64 = residuals.iter().map(|r| r.powi(2)).sum();
        let sigma = (rss / (n - 2.0)).sqrt();

        let rmse_val = rmse(y, &y_hat).unwrap_or(0.0);
        let y_mean = y.iter().copied().sum::<f64>() / n;
        let cv = rmse_val / y_mean;

        // R² and adjusted R² based on original y
        let r2 = r2_from_predictions(y, &y_hat).unwrap_or(0.0);
        let adjusted_r2 = adjusted_r2(r2, n as usize, 2); // a and b

        // AIC based on RSS in original space
        let aic = aic_from_rss(rss, n as usize, 2);

        // --- p-value for b from log-linear fit ln(y) = ln(a) + b x ---

        let ln_y: Vec<f64> = y.iter().map(|v| v.ln()).collect();
        let ln_model = LinReg::train(&x_norm, &ln_y);

        let ln_y_hat: Vec<f64> = x_norm.iter().map(|&xi| ln_model.calculate(xi)).collect();

        let ln_residuals: Vec<f64> =
            ln_y.iter().zip(&ln_y_hat).map(|(&yi, &yhi)| yi - yhi).collect();

        let rss_ln: f64 = ln_residuals.iter().map(|r| r.powi(2)).sum();
        let sigma_ln = (rss_ln / (n - 2.0)).sqrt();
        if !sigma_ln.is_finite() {
            return Err(FluxFitError::NonFiniteSigma);
        }
        let x_mean = x_norm.iter().copied().sum::<f64>() / n;
        let ss_xx: f64 = x_norm.iter().map(|xi| (xi - x_mean).powi(2)).sum();
        if !ss_xx.is_finite() || ss_xx <= f64::EPSILON {
            return Err(FluxFitError::DegenerateX);
        }
        let se_b = sigma_ln / ss_xx.sqrt();
        if !se_b.is_finite() || se_b <= 0.0 {
            // e.g. perfect fit (sigma = 0) or degenerate
            // you can decide how to handle this; example: p_value = 0 or 1
            return Err(FluxFitError::NonFiniteSE);
        }

        let t_stat = ln_model.slope / se_b;
        if !t_stat.is_finite() {
            return Err(FluxFitError::NonFiniteTStat);
        }

        let dist = StudentsT::new(0.0, 1.0, n - 2.0)
            .map_err(|_| FluxFitError::StatError("failed to construct StudentsT"))?;
        let p_value = 2.0 * (1.0 - dist.cdf(t_stat.abs()));

        // --- Flux calculation ---
        //
        // For y = a * exp(b t), with t normalized so t0 = 0:
        // f0 = dC/dt |_{t=0} = a * b
        let f0 = model.a * model.b;

        // Reuse your existing flux helper
        let flux = flux_umol_m2_s(&data.channel, f0, &meteo.temperature, &meteo.pressure, &chamber);

        Ok(Self {
            gas_channel: data.channel.clone(),
            flux,
            adjusted_r2,
            r2,
            model,
            p_value,
            sigma,
            aic,
            rmse: rmse_val,
            cv,
            range_start: range.start,
            range_end: range.end,
        })
    }

    pub fn from_values(
        gas_channel: GasChannel,
        flux: f64,
        r2: f64,
        adjusted_r2: f64,
        model: ExpReg,
        range_start: f64,
        range_end: f64,
        sigma: f64,
        p_value: f64,
        aic: f64,
        rmse: f64,
        cv: f64,
    ) -> Option<Self> {
        Some(Self {
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
        })
    }

    /// Simple alternative "update" like your LinearFlux::flux_from_vec;
    /// here using chamber-based flux helper and initial slope f0 = a * b.
    pub fn flux_from_vec(
        &mut self,
        data: GasChannelData,
        meteo: &MeteoConditions,
        chamber: Chamber,
    ) -> FluxResult<()> {
        let x = data.x();
        let y = data.y();

        if x.len() != y.len() || x.len() < 2 || !y.iter().all(|v| *v > 0.0) {
            return Err(FluxFitError::LengthMismatch { len_x: x.len(), len_y: y.len() });
        }
        if x.len() < 2 {
            return Err(FluxFitError::NotEnoughPoints { len: x.len(), needed: 2 });
        }
        if !y.iter().all(|v| *v > 0.0) {
            return Err(FluxFitError::NonPositiveY);
        }
        let x0 = x[0];
        let x_norm: Vec<f64> = x.iter().map(|t| t - x0).collect();

        self.model = ExpReg::train(&x_norm, &y);
        let f0 = self.model.a * self.model.b;
        self.flux =
            flux_umol_m2_s(&self.gas_channel, f0, &meteo.temperature, &meteo.pressure, &chamber);
        Ok(())
    }
}
