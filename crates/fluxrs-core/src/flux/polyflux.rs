use crate::data_formats::chamberdata::Chamber;
use crate::flux::flux::{adjusted_r2, aic_from_rss, flux_umol_m2_s, r2_from_predictions, rmse};
use crate::flux::fluxfiterror::{FluxFitError, FluxResult};
use crate::flux::fluxkind::FluxKind;
use crate::flux::fluxmodel::FluxModel;
use crate::gaschannel::GasChannel;
use crate::gastype::GasType;
use crate::stats::PolyReg;

use statrs::distribution::{ContinuousCDF, StudentsT};

use std::any::Any;
use std::fmt;
use std::str::FromStr;

impl fmt::Display for PolyFlux {
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
pub struct PolyFlux {
    pub fit_id: String,
    pub gas_channel: GasChannel,
    pub flux: f64,
    pub r2: f64,
    pub adjusted_r2: f64,
    pub model: PolyReg,
    pub x_offset: f64,
    pub sigma: f64,
    pub aic: f64,
    pub rmse: f64,
    pub cv: f64,
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
    fn gas_channel(&self) -> GasChannel {
        self.gas_channel.clone()
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

    fn cv(&self) -> Option<f64> {
        Some(self.cv)
    }
}

impl PolyFlux {
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
            return Err(FluxFitError::LengthMismatch { len_x: x.len(), len_y: y.len() });
        }
        if x.len() < 3 {
            return Err(FluxFitError::NotEnoughPoints { len: x.len(), needed: 3 });
        }

        let x0 = x[0]; // normalize to start
        let x_norm: Vec<f64> = x.iter().map(|t| t - x0).collect();
        let n = y.len() as f64;

        let model = PolyReg::train(&x_norm, y)
            .ok_or(FluxFitError::StatError("PolyReg::train returned None"))?;

        let y_hat: Vec<f64> = x_norm.iter().map(|&xi| model.calculate(xi)).collect();
        let r2 = r2_from_predictions(y, &y_hat).unwrap_or(0.0);
        let y_mean = y.iter().copied().sum::<f64>() / n;
        let rmse = rmse(&y, &y_hat).unwrap_or(0.0);
        let cv = rmse / y_mean;

        let n = y.len();
        let k = 2; // predictors: x and x² (intercept is implicit)

        let adjusted_r2 = adjusted_r2(r2, n, k);
        let rss: f64 = y.iter().zip(&y_hat).map(|(&yi, &yhi)| (yi - yhi).powi(2)).sum();
        let aic = aic_from_rss(rss, n, k + 1); // k + 1 = slope + quad + intercept
        let sigma = (rss / (n as f64 - k as f64 - 1.0)).sqrt();
        if !sigma.is_finite() {
            return Err(FluxFitError::NonFiniteSigma);
        }

        // Evaluate slope at midpoint of the fit range (normalized)
        // let x_mid = ((start - x0) + (end - x0)) / 2.0;
        // let slope_at_mid = model.a1 + 2.0 * model.a2 * x_mid;
        let x_start = start - x0; // with your normalization, often 0.0
        let slope = model.a1 + 2.0 * model.a2 * x_start;

        let flux = flux_umol_m2_s(&channel, slope, air_temperature, air_pressure, &chamber);

        Ok(Self {
            fit_id: fit_id.to_string(),
            gas_channel: channel,
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
            cv,
        })
    }
}
