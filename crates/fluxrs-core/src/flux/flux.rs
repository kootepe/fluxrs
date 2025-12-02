use crate::data_formats::chamberdata::Chamber;
use crate::data_formats::meteodata::{MeteoPoint, MeteoSource};
use crate::flux::fluxmodel::FluxModel;
use crate::gaschannel::GasChannel;

#[derive(Clone, Copy)]
pub struct MeteoConditions {
    pub temperature: MeteoPoint,
    pub pressure: MeteoPoint,
}

impl MeteoConditions {
    pub fn new(temperature: MeteoPoint, pressure: MeteoPoint) -> Self {
        Self { temperature, pressure }
    }
    pub fn pressure(&self) -> MeteoPoint {
        self.pressure
    }
    pub fn temperature(&self) -> MeteoPoint {
        self.temperature
    }
    pub fn pressure_val(&self) -> Option<f64> {
        self.pressure.value
    }
    pub fn temperature_val(&self) -> Option<f64> {
        self.temperature.value
    }
    pub fn temperature_source(&self) -> MeteoSource {
        self.temperature.source
    }
    pub fn pressure_source(&self) -> MeteoSource {
        self.pressure.source
    }
    pub fn temperature_distance(&self) -> Option<i64> {
        self.temperature.distance_from_target
    }
    pub fn pressure_distance(&self) -> Option<i64> {
        self.pressure.distance_from_target
    }
}

impl Default for MeteoConditions {
    fn default() -> Self {
        MeteoConditions { temperature: MeteoPoint::default(), pressure: MeteoPoint::default() }
    }
}

pub struct TimeRange {
    pub start: f64,
    pub end: f64,
}

impl TimeRange {
    pub fn new(start: f64, end: f64) -> Self {
        Self { start, end }
    }
}

pub struct XYSeries {
    y: Vec<f64>,
    x: Vec<f64>,
}

impl XYSeries {
    fn equal_len(&self) -> bool {
        self.x.len() == self.y.len()
    }
    fn xlen(&self) -> usize {
        self.x.len()
    }
    fn ylen(&self) -> usize {
        self.y.len()
    }
}

pub struct GasChannelData {
    pub channel: GasChannel,
    pub data: XYSeries,
}

impl GasChannelData {
    pub fn new(channel: GasChannel, x: Vec<f64>, y: Vec<f64>) -> Self {
        Self { channel, data: XYSeries { x, y } }
    }
    pub fn equal_len(&self) -> bool {
        self.data.equal_len()
    }
    pub fn xlen(&self) -> usize {
        self.data.xlen()
    }
    pub fn ylen(&self) -> usize {
        self.data.ylen()
    }
    pub fn x(&self) -> &[f64] {
        &self.data.x
    }
    pub fn y(&self) -> &[f64] {
        &self.data.y
    }
}

#[derive(Clone)]
pub struct FluxRecord {
    pub model: Box<dyn FluxModel>,
    pub is_valid: bool,
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
    air_temperature_c: MeteoPoint,
    air_pressure_hpa: MeteoPoint,
    chamber: &Chamber,
) -> f64 {
    flux_umol_m2_s_core(
        channel,
        slope_x_per_s,
        air_temperature_c.value.unwrap(),
        air_pressure_hpa.value.unwrap(),
        chamber,
    )
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
