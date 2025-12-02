use crate::data_formats::chamberdata::Chamber;
use crate::flux::flux::{flux_umol_m2_s, GasChannelData, MeteoConditions, TimeRange};
use crate::flux::fluxfiterror::{FluxFitError, FluxResult};
use crate::flux::fluxkind::FluxKind;
use crate::flux::fluxmodel::FluxModel;
use crate::gaschannel::GasChannel;
use crate::stats::{adjusted_r2, aic_from_rss, r2_from_predictions, rmse, RobReg};

use std::any::Any;
use std::fmt;

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

        let flux = flux_umol_m2_s(
            &data.channel,
            slope_at_mid,
            &meteo.temperature,
            &meteo.pressure,
            &chamber,
        );

        Ok(Self {
            gas_channel: data.channel.clone(),
            flux,
            r2,
            adjusted_r2,
            model,
            sigma,
            aic,
            rmse: rmse_val,
            cv,
            range_start: range.start,
            range_end: range.end,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::concentrationunit::ConcentrationUnit;
    use crate::data_formats::meteodata::{MeteoPoint, MeteoSource};
    use crate::gastype::GasType;

    // ---- Test helpers -----------------------------------------------------

    fn test_channel() -> GasChannel {
        GasChannel::new(GasType::CH4, ConcentrationUnit::Ppb, "test_channel".to_owned())
    }

    fn test_chamber() -> Chamber {
        Chamber::default()
    }

    fn test_meteo() -> MeteoConditions {
        MeteoConditions {
            temperature: MeteoPoint {
                value: Some(10.),
                source: MeteoSource::Default,
                distance_from_target: None,
            },
            pressure: MeteoPoint {
                value: Some(980.),
                source: MeteoSource::Default,
                distance_from_target: None,
            },
        }
    }

    /// Simple helper returning (x, y) with one clear outlier in y.
    fn time_series_with_outlier() -> (Vec<f64>, Vec<f64>) {
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let y = vec![1.0, 2.0, 3.0, 4.0, 100.0]; // outlier at the last point
        (x, y)
    }

    /// Convenience constructor for GasChannelData.
    /// Adjust this to match your actual struct layout / constructor.
    fn gas_channel_data<'a>(channel: GasChannel, x: &'a [f64], y: &'a [f64]) -> GasChannelData<'a> {
        // If you have `GasChannelData::new`, use that instead:
        // GasChannelData::new(channel, x, y)

        GasChannelData::new(channel, x, y)
    }

    fn assert_flux_stats_valid(flux: &RobustFlux) {
        assert!((0.0..=1.0).contains(&flux.r2), "r2 must be between 0 and 1, got {}", flux.r2);
        assert!(
            flux.adjusted_r2 <= flux.r2,
            "adjusted_r2 ({}) must be <= r2 ({})",
            flux.adjusted_r2,
            flux.r2
        );
        assert!(flux.rmse >= 0.0, "rmse must be non-negative, got {}", flux.rmse);
        assert!(flux.sigma >= 0.0, "sigma must be non-negative, got {}", flux.sigma);
        assert!(flux.aic.is_finite(), "aic must be finite, got {}", flux.aic);
    }

    // ---- Tests ------------------------------------------------------------

    #[test]
    fn robust_flux_produces_valid_statistics_with_outlier() {
        // ---------- Arrange ----------
        let (x, y) = time_series_with_outlier();

        let range = TimeRange {
            start: *x.first().expect("non-empty x"),
            end: *x.last().expect("non-empty x"),
        };

        let meteo = test_meteo();
        let chamber = test_chamber();
        let channel = test_channel();
        let data = gas_channel_data(channel, &x, &y);

        // ---------- Act ----------
        let flux = RobustFlux::from_data(&data, &range, &meteo, &chamber)
            .expect("RobustFlux creation failed");

        // ---------- Assert ----------
        assert_flux_stats_valid(&flux);
    }

    #[test]
    fn robust_flux_errors_on_length_mismatch() {
        let x = vec![0.0, 1.0, 2.0];
        let y = vec![1.0, 2.0]; // shorter

        let range = TimeRange { start: 0.0, end: 2.0 };
        let meteo = test_meteo();
        let chamber = test_chamber();
        let channel = test_channel();
        let data = gas_channel_data(channel, &x, &y);

        let result = RobustFlux::from_data(&data, &range, &meteo, &chamber);

        match result {
            Err(FluxFitError::LengthMismatch { .. }) => {},
            Err(e) => panic!("Expected LengthMismatch error, got {}", e),
            Ok(_) => panic!("Expected LengthMismatch error, got Ok(_)"),
        }
    }

    #[test]
    fn robust_flux_errors_on_not_enough_points() {
        let x = vec![0.0, 1.0]; // only 2 points
        let y = vec![1.0, 2.0];

        let range = TimeRange { start: 0.0, end: 1.0 };
        let meteo = test_meteo();
        let chamber = test_chamber();
        let channel = test_channel();
        let data = gas_channel_data(channel, &x, &y);

        let result = RobustFlux::from_data(&data, &range, &meteo, &chamber);

        match result {
            Err(FluxFitError::NotEnoughPoints { .. }) => {},
            Err(e) => panic!("Expected NotEnoughPoints error, got {}", e),
            Ok(_) => panic!("Expected NotEnoughPoints error, got Ok(_)"),
        }
    }
}
