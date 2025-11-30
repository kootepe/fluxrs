use crate::data_formats::chamberdata::Chamber;
use crate::flux::flux::flux_umol_m2_s;
use crate::flux::fluxfiterror::{FluxFitError, FluxResult};
use crate::flux::fluxkind::FluxKind;
use crate::flux::fluxmodel::FluxModel;
use crate::gaschannel::GasChannel;
use crate::gastype::GasType;
use crate::stats::{adjusted_r2, aic_from_rss, r2_from_predictions, rmse, RobReg};

use statrs::distribution::{ContinuousCDF, StudentsT};

use std::any::Any;
use std::fmt;
use std::str::FromStr;

impl fmt::Display for RobustFlux {
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
pub struct RobustFlux {
    pub gas_channel: GasChannel,
    pub flux: f64,
    pub r2: f64,
    pub adjusted_r2: f64,
    pub model: RobReg,
    pub sigma: f64,
    pub aic: f64,
    pub rmse: f64,
    pub cv: f64,
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
    fn kind(&self) -> FluxKind {
        FluxKind::RobLin
    }

    fn gas_channel(&self) -> GasChannel {
        self.gas_channel.clone()
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
    fn cv(&self) -> Option<f64> {
        Some(self.cv)
    }
}

impl RobustFlux {
    pub fn from_data(
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

        let x0 = x[0];
        let x_norm: Vec<f64> = x.iter().map(|t| t - x0).collect();

        let model = RobReg::train(&x_norm, y, 1.0, 10)
            .ok_or(FluxFitError::StatError("RobReg::train returned None"))?;

        let y_hat: Vec<f64> = x_norm.iter().map(|&xi| model.calculate(xi)).collect();
        let r2 = r2_from_predictions(y, &y_hat).unwrap_or(0.0);
        let rmse_val = rmse(y, &y_hat).unwrap_or(0.0);

        let n = y.len();
        let y_mean = y.iter().copied().sum::<f64>() / n as f64;
        let cv = rmse_val / y_mean;

        let adjusted_r2 = adjusted_r2(r2, n, 2);
        let rss: f64 = y.iter().zip(&y_hat).map(|(&yi, &yhi)| (yi - yhi).powi(2)).sum();
        let sigma = (rss / (n as f64 - 2.0)).sqrt();
        let aic = aic_from_rss(rss, n, 2);

        // slope at midpoint of range (normalized x)
        let slope_at_mid = model.slope; // constant for linear model

        let flux = flux_umol_m2_s(&channel, slope_at_mid, air_temperature, air_pressure, &chamber);

        Ok(Self {
            gas_channel: channel,
            flux,
            r2,
            adjusted_r2,
            model,
            sigma,
            aic,
            rmse: rmse_val,
            cv,
            range_start: start,
            range_end: end,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::concentrationunit;

    use super::*;

    #[test]
    fn test_robust_flux_fit() {
        let gas = GasType::CH4;
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let y = vec![1.0, 2.0, 3.0, 4.0, 100.0]; // outlier included

        let start = x[0];
        let end = x[x.len() - 1];
        let temperature = 20.0; // °C
        let pressure = 1013.25; // hPa
        let chamber = Chamber::default();
        let channel =
            GasChannel::new(gas, concentrationunit::ConcentrationUnit::Ppb, "asd".to_owned());

        let flux =
            RobustFlux::from_data(channel, &x, &y, start, end, temperature, pressure, chamber)
                .expect("RobustFlux creation failed");

        // Check computed values
        dbg!(flux.r2, flux.adjusted_r2, flux.rmse, flux.sigma, flux.aic);
        assert!(flux.r2 >= 0.0 && flux.r2 <= 1.0);
        assert!(flux.adjusted_r2 <= flux.r2);
        assert!(flux.rmse >= 0.0);
        assert!(flux.sigma >= 0.0);
        assert!(flux.aic.is_finite());
    }
}
