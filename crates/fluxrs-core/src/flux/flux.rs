use crate::data_formats::chamberdata::Chamber;
use crate::flux::fluxmodel::FluxModel;
use crate::gaschannel::GasChannel;

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
