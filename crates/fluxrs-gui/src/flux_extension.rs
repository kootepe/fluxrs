use egui::{Color32, Stroke};
use egui_plot::LineStyle;
use fluxrs_core::flux::FluxKind;

pub trait UiColor {
    fn color(&self) -> Color32;
    fn stroke(&self) -> Stroke;
    fn style(&self) -> LineStyle;
}

impl UiColor for FluxKind {
    fn color(&self) -> Color32 {
        match self {
            FluxKind::Linear => Color32::RED,
            FluxKind::Exponential => Color32::RED,
            FluxKind::RobLin => Color32::RED,
            FluxKind::Poly => Color32::RED,
        }
    }
    fn stroke(&self) -> Stroke {
        match self {
            FluxKind::Linear => Stroke::new(1.5, self.color()),
            FluxKind::Exponential => Stroke::new(1.5, self.color()),
            FluxKind::RobLin => Stroke::new(1.5, self.color()),
            FluxKind::Poly => Stroke::new(1.5, self.color()),
        }
    }
    fn style(&self) -> LineStyle {
        match self {
            FluxKind::Linear => LineStyle::Solid,
            FluxKind::Exponential => LineStyle::dotted_dense(),
            FluxKind::RobLin => LineStyle::dashed_dense(),
            FluxKind::Poly => LineStyle::dashed_loose(),
        }
    }
}
