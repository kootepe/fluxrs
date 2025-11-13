use egui::Color32;
use fluxrs_core::gastype::GasType;

pub trait GasColor {
    fn color(&self) -> Color32;
}
impl GasColor for GasType {
    fn color(&self) -> Color32 {
        match self {
            GasType::CH4 => Color32::GREEN,
            GasType::CO2 => Color32::ORANGE,
            GasType::H2O => Color32::CYAN,
            GasType::N2O => Color32::LIGHT_RED,
        }
    }
}
