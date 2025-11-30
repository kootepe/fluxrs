use crate::data_formats::chamberdata::Chamber;
use crate::gaschannel::GasChannel;
use crate::gastype::GasType;
use crate::stats::{ExpReg, LinReg, PolyReg, RobReg};
use dyn_clone::DynClone;
use statrs::distribution::{ContinuousCDF, StudentsT};
use std::any::Any;
use std::fmt;
use std::str::FromStr;

#[derive(Debug)]
pub enum FluxFitError {
    LengthMismatch { len_x: usize, len_y: usize },
    NotEnoughPoints { len: usize, needed: usize },
    NonPositiveY,
    DegenerateX, // no variance in x
    NonFiniteSigma,
    NonFiniteSE,
    NonFiniteTStat,
    StatError(&'static str),
}

impl fmt::Display for FluxFitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FluxFitError::LengthMismatch { len_x, len_y } => {
                write!(f, "x and y have different lengths: {len_x} vs {len_y}")
            },
            FluxFitError::NotEnoughPoints { len, needed } => {
                write!(f, "not enough points: got {len}, need at least {needed}")
            },
            FluxFitError::NonPositiveY => {
                write!(f, "exponential model requires all y > 0")
            },
            FluxFitError::DegenerateX => {
                write!(f, "degenerate x: no variance in x")
            },
            FluxFitError::NonFiniteSigma => {
                write!(f, "non-finite sigma during fit")
            },
            FluxFitError::NonFiniteSE => {
                write!(f, "non-finite or non-positive standard error of slope")
            },
            FluxFitError::NonFiniteTStat => {
                write!(f, "non-finite t statistic")
            },
            FluxFitError::StatError(msg) => write!(f, "statistical error: {msg}"),
        }
    }
}

impl std::error::Error for FluxFitError {}

pub type FluxResult<T> = Result<T, FluxFitError>;

#[derive(Debug)]
pub struct ParseFluxUnitError(String);

impl fmt::Display for ParseFluxUnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseFluxUnitError {}

#[derive(Default, Clone, Copy, Hash, PartialEq, Eq)]
pub enum FluxUnit {
    #[default]
    UmolM2S,
    UmolM2H,
    MmolM2S,
    MmolM2H,
    MgM2S,
    MgM2H,

    NmolM2S,
    NmolM2H,
}

impl std::fmt::Display for FluxUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FluxUnit::UmolM2S => write!(f, "µmol/m2/s"),
            FluxUnit::UmolM2H => write!(f, "µmol/m2/h"),
            FluxUnit::MmolM2S => write!(f, "mmol/m2/s"),
            FluxUnit::MmolM2H => write!(f, "mmol/m2/h"),
            FluxUnit::MgM2S => write!(f, "mg/m2/s"),
            FluxUnit::MgM2H => write!(f, "mg/m2/h"),
            FluxUnit::NmolM2S => write!(f, "nmol/m2/s"),
            FluxUnit::NmolM2H => write!(f, "nmol/m2/h"),
        }
    }
}

impl FromStr for FluxUnit {
    type Err = ParseFluxUnitError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "µmol/m2/s" => Ok(FluxUnit::UmolM2S),
            "µmol/m2/h" => Ok(FluxUnit::UmolM2H),
            "mmol/m2/s" => Ok(FluxUnit::MmolM2S),
            "mmol/m2/h" => Ok(FluxUnit::MmolM2H),
            "mg/m2/s" => Ok(FluxUnit::MgM2S),
            "mg/m2/h" => Ok(FluxUnit::MgM2H),
            "nmol/m2/s" => Ok(FluxUnit::NmolM2S),
            "nmol/m2/h" => Ok(FluxUnit::NmolM2H),

            other => Err(ParseFluxUnitError(format!("Invalid unit: {other}"))),
        }
    }
}

impl FluxUnit {
    pub fn all() -> &'static [FluxUnit] {
        use FluxUnit::*;
        &[UmolM2S, UmolM2H, MmolM2S, MmolM2H, MgM2S, MgM2H, NmolM2S, NmolM2H]
    }

    pub fn from_umol_m2_s(&self, value_umol_m2_s: f64, gas: GasType) -> f64 {
        match self {
            // nmol/m²/s = µmol * 1000
            FluxUnit::NmolM2S => value_umol_m2_s * 1000.0,

            // nmol/m²/h = µmol * 1000 * 3600
            FluxUnit::NmolM2H => value_umol_m2_s * 1000.0 * 3600.0,

            // base
            FluxUnit::UmolM2S => value_umol_m2_s,

            // µmol → hours
            FluxUnit::UmolM2H => value_umol_m2_s * 3600.0,

            // mmol
            FluxUnit::MmolM2S => value_umol_m2_s / 1000.0,
            FluxUnit::MmolM2H => value_umol_m2_s / 1000.0 * 3600.0,

            // mg
            FluxUnit::MgM2S => value_umol_m2_s * gas.mol_mass() / 1000.0,
            FluxUnit::MgM2H => value_umol_m2_s * gas.mol_mass() / 1000.0 * 3600.0,
        }
    }

    pub fn suffix(&self) -> &'static str {
        match self {
            FluxUnit::UmolM2S => "umol_m2_s",
            FluxUnit::UmolM2H => "umol_m2_h",
            FluxUnit::MmolM2S => "mmol_m2_s",
            FluxUnit::MmolM2H => "mmol_m2_h",
            FluxUnit::MgM2S => "mg_m2_s",
            FluxUnit::MgM2H => "mg_m2_h",
            FluxUnit::NmolM2S => "nmol_m2_s",
            FluxUnit::NmolM2H => "nmol_m2_h",
        }
    }
}

#[derive(Clone)]
pub struct FluxRecord {
    pub model: Box<dyn FluxModel>,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FluxKind {
    Linear,
    Exponential,
    RobLin,
    Poly,
}

impl std::fmt::Display for FluxKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FluxKind::Linear => write!(f, "Linear"),
            FluxKind::Exponential => write!(f, "Exponential"),
            FluxKind::RobLin => write!(f, "Robust linear"),
            FluxKind::Poly => write!(f, "Polynomial"),
        }
    }
}

impl FluxKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FluxKind::Linear => "linear",
            FluxKind::Exponential => "exponential",
            FluxKind::RobLin => "roblin",
            FluxKind::Poly => "poly",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            FluxKind::Linear => "linear",
            FluxKind::Exponential => "exponential",
            FluxKind::RobLin => "roblin",
            FluxKind::Poly => "poly",
        }
    }
    pub fn all() -> &'static [FluxKind] {
        use FluxKind::*;
        &[Linear, Exponential, RobLin, Poly]
    }
}

pub trait FluxModel: Sync + Send + DynClone {
    fn fit_id(&self) -> FluxKind;
    fn gas_channel(&self) -> GasChannel;
    fn flux(&self) -> Option<f64>;
    fn r2(&self) -> Option<f64>;
    fn adj_r2(&self) -> Option<f64>;
    fn intercept(&self) -> Option<f64>;
    fn slope(&self) -> Option<f64>;
    fn p_value(&self) -> Option<f64>;
    fn sigma(&self) -> Option<f64>;
    fn rmse(&self) -> Option<f64>;
    fn cv(&self) -> Option<f64>;
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

impl fmt::Display for dyn FluxModel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:?}, {:?}, flux: {:?}, r2: {:?}, len: {:?}",
            self.fit_id(),
            self.gas_channel().gas,
            self.flux(),
            self.r2(),
            match (self.range_start(), self.range_end()) {
                (Some(start), Some(end)) => Some(end - start),
                _ => None,
            }
        )
    }
}
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

#[derive(Clone)]
pub struct ExponentialFlux {
    pub fit_id: String,
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

    fn fit_id(&self) -> FluxKind {
        FluxKind::Exponential // you’ll need to add this variant
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
        fit_id: &str,
        channel: GasChannel,
        x: &[f64],
        y: &[f64],
        start: f64,
        end: f64,
        air_temperature: f64,
        air_pressure: f64,
        chamber: Chamber,
    ) -> Option<Self> {
        if x.len() != y.len() || x.len() < 3 {
            return None;
        }

        if !y.iter().all(|&v| v > 0.0) {
            // exponential model requires positive y
            return None;
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
            return None;
        }
        let x_mean = x_norm.iter().copied().sum::<f64>() / n;
        let ss_xx: f64 = x_norm.iter().map(|xi| (xi - x_mean).powi(2)).sum();
        if !ss_xx.is_finite() || ss_xx <= f64::EPSILON {
            return None;
        }

        let se_b = sigma_ln / ss_xx.sqrt();
        if !se_b.is_finite() || se_b <= 0.0 {
            // e.g. perfect fit (sigma = 0) or degenerate
            // you can decide how to handle this; example: p_value = 0 or 1
            return None;
        }

        let t_stat = ln_model.slope / se_b;
        if !t_stat.is_finite() {
            return None;
        }
        let dist = StudentsT::new(0.0, 1.0, n - 2.0).ok()?;
        let p_value = 2.0 * (1.0 - dist.cdf(t_stat.abs()));

        // --- Flux calculation ---
        //
        // For y = a * exp(b t), with t normalized so t0 = 0:
        // f0 = dC/dt |_{t=0} = a * b
        let f0 = model.a * model.b;

        // Reuse your existing flux helper
        let flux = flux_umol_m2_s(&channel, f0, air_temperature, air_pressure, &chamber);

        Some(Self {
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
        })
    }

    /// Simple alternative "update" like your LinearFlux::flux_from_vec;
    /// here using chamber-based flux helper and initial slope f0 = a * b.
    pub fn flux_from_vec(
        &mut self,
        x: Vec<f64>,
        y: Vec<f64>,
        air_temperature: f64,
        air_pressure: f64,
        chamber: Chamber,
    ) {
        if x.len() != y.len() || x.len() < 2 || !y.iter().all(|v| *v > 0.0) {
            return;
        }

        let x0 = x[0];
        let x_norm: Vec<f64> = x.iter().map(|t| t - x0).collect();

        self.model = ExpReg::train(&x_norm, &y);
        let f0 = self.model.a * self.model.b;
        self.flux = flux_umol_m2_s(&self.gas_channel, f0, air_temperature, air_pressure, &chamber);
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
    ) -> Option<Self> {
        if x.len() != y.len() || x.len() < 3 {
            return None;
        }

        let x0 = x[0]; // normalize to start
        let x_norm: Vec<f64> = x.iter().map(|t| t - x0).collect();
        let n = y.len() as f64;

        let model = PolyReg::train(&x_norm, y)?;

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

        // Evaluate slope at midpoint of the fit range (normalized)
        // let x_mid = ((start - x0) + (end - x0)) / 2.0;
        // let slope_at_mid = model.a1 + 2.0 * model.a2 * x_mid;
        let x_start = start - x0; // with your normalization, often 0.0
        let slope = model.a1 + 2.0 * model.a2 * x_start;

        let flux = flux_umol_m2_s(&channel, slope, air_temperature, air_pressure, &chamber);

        Some(Self {
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

#[derive(Clone)]
pub struct RobustFlux {
    pub fit_id: String,
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

    fn fit_id(&self) -> FluxKind {
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
        fit_id: &str,
        channel: GasChannel,
        x: &[f64],
        y: &[f64],
        start: f64,
        end: f64,
        air_temperature: f64,
        air_pressure: f64,
        chamber: Chamber,
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
        let y_mean = y.iter().copied().sum::<f64>() / n as f64;
        let cv = rmse_val / y_mean;

        let adjusted_r2 = adjusted_r2(r2, n, 2);
        let rss: f64 = y.iter().zip(&y_hat).map(|(&yi, &yhi)| (yi - yhi).powi(2)).sum();
        let sigma = (rss / (n as f64 - 2.0)).sqrt();
        let aic = aic_from_rss(rss, n, 2);

        // slope at midpoint of range (normalized x)
        let slope_at_mid = model.slope; // constant for linear model

        let flux = flux_umol_m2_s(&channel, slope_at_mid, air_temperature, air_pressure, &chamber);

        Some(Self {
            fit_id: fit_id.to_string(),
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

fn flux_umol_m2_s_core(
    channel: &GasChannel,
    slope_x_per_s: f64, // instrument slope (whatever that is)
    air_temperature_c: f64,
    air_pressure_hpa: f64,
    chamber: &Chamber,
) -> f64 {
    // phys constants + env
    let p_pa = air_pressure_hpa * 100.0; // hPa -> Pa
    let t_k = air_temperature_c + 273.15; // °C -> K
    let r = 8.314_f64; // Pa·m3/(mol·K)

    // convert slope to ppm/s in dry mole fraction terms
    let slope_ppm_per_s = channel.slope_ppm_per_s(slope_x_per_s);

    // ideal gas concentration (mol/m³)
    let mol_per_m3_air = p_pa / (r * t_k);

    // ppm/s (µmol/mol/s) -> mol/mol/s
    let slope_mol_per_mol_per_s = slope_ppm_per_s * 1e-6;

    // dC/dt in mol/m³/s
    let dmol_per_m3_per_s = slope_mol_per_mol_per_s * mol_per_m3_air;

    // chamber geometry
    let chamber_volume_m3 = chamber.adjusted_volume();
    let chamber_area_m2 = chamber.area_m2();

    // flux (mol/m²/s)
    let flux_mol_m2_s = dmol_per_m3_per_s * chamber_volume_m3 / chamber_area_m2;

    // mol/m²/s -> µmol/m²/s
    flux_mol_m2_s * 1e6
}

/// Flux in µmol m⁻² s⁻¹
pub fn flux_umol_m2_s(
    channel: &GasChannel,
    slope_x_per_s: f64,
    air_temperature_c: f64,
    air_pressure_hpa: f64,
    chamber: &Chamber,
) -> f64 {
    flux_umol_m2_s_core(&channel, slope_x_per_s, air_temperature_c, air_pressure_hpa, chamber)
}

/// Flux in mg m⁻² s⁻¹
pub fn flux_mg_m2_s(
    channel: &GasChannel,
    slope_x_per_s: f64,
    air_temperature_c: f64,
    air_pressure_hpa: f64,
    chamber: &Chamber,
) -> f64 {
    let flux_umol =
        flux_umol_m2_s_core(&channel, slope_x_per_s, air_temperature_c, air_pressure_hpa, chamber);

    // convert µmol m⁻² s⁻¹ → mg m⁻² s⁻¹
    //
    // 1 µmol = 1e-6 mol
    // mass (g) = mol * mol_mass_g_per_mol
    // mg       = g * 1000
    //
    // combine: 1 µmol = mol_mass_g_per_mol * 1e-3 mg
    let mol_mass = channel.gas.mol_mass(); // g/mol
    flux_umol * (mol_mass * 1e-3)
}

/// Flux in mg m⁻² h⁻¹
pub fn flux_mg_m2_h(
    channel: GasChannel,
    slope_x_per_s: f64,
    air_temperature_c: f64,
    air_pressure_hpa: f64,
    chamber: &Chamber,
) -> f64 {
    // start from mg m⁻² s⁻¹ then scale by 3600
    let flux_mg_s =
        flux_mg_m2_s(&channel, slope_x_per_s, air_temperature_c, air_pressure_hpa, chamber);
    flux_mg_s * 3600.0
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
    use crate::concentrationunit;

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
        let chamber = Chamber::default();
        let channel =
            GasChannel::new(gas, concentrationunit::ConcentrationUnit::Ppb, fit_id.to_owned());

        let flux = RobustFlux::from_data(
            fit_id,
            channel,
            &x,
            &y,
            start,
            end,
            temperature,
            pressure,
            chamber,
        )
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
