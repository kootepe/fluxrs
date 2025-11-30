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
