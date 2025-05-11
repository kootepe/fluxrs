// use crate::cycle::Cycle;
use crate::stats::{pearson_correlation, LinReg};
use crate::GasType;
use dyn_clone::DynClone;
use std::any::Any;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FluxKind {
    Linear,
    Robust,
    Poly,
}

impl FluxKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FluxKind::Linear => "linear",
            FluxKind::Robust => "robust",
            FluxKind::Poly => "poly",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            FluxKind::Linear => "linear",
            FluxKind::Robust => "robust",
            FluxKind::Poly => "polynomial",
        }
    }
}

pub trait FluxModel: Sync + Send + DynClone {
    fn flux(&self) -> f64;
    fn r2(&self) -> f64;
    // fn fit_id(&self) -> &str;
    fn fit_id(&self) -> FluxKind;
    fn gas_type(&self) -> GasType;
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
    pub model: LinReg,
    // pub intercept: f64,
    // pub slope: f64,
    pub range_start: f64,
    pub range_end: f64,
}

impl FluxModel for LinearFlux {
    fn flux(&self) -> f64 {
        self.flux
    }
    fn r2(&self) -> f64 {
        self.r2
    }
    // fn fit_id(&self) -> &str {
    //     &self.fit_id
    // }
    fn fit_id(&self) -> FluxKind {
        FluxKind::Linear
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
        if x.len() != y.len() || x.len() < 2 {
            return None;
        }

        let model = LinReg::train(x, y);

        let r2 = pearson_correlation(x, y).unwrap_or(0.0).powi(2);
        let flux = calculate_flux(gas_type, model.slope, air_temperature, air_pressure, volume);

        let range_start = start;
        let range_end = end;
        println!("fluxs: {}", range_start);
        println!("fluxe: {}", range_end);

        Some(Self { fit_id: fit_id.to_string(), gas_type, flux, r2, model, range_start, range_end })
    }
    pub fn from_values(
        fit_id: &str,
        gas_type: GasType,
        flux: f64,
        r2: f64,
        model: LinReg,
        range_start: f64,
        range_end: f64,
    ) -> Option<Self> {
        Some(Self { fit_id: fit_id.to_string(), gas_type, flux, r2, model, range_start, range_end })
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

pub struct RobustFlux {
    pub fit_id: String,
    pub gas_type: GasType,
    pub flux: f64,
    pub r2: f64,
    pub intercept: f64,
    pub slope: f64,
    pub diagnostics: Option<String>,
}

pub struct PolyFlux {
    pub fit_id: String,
    pub gas_type: GasType,
    pub flux: f64,
    pub r2: f64,
    pub coefficients: Vec<f64>, // degree 2, 3, etc.
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
