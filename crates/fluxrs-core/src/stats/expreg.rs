use crate::stats::linreg::LinReg;
use std::fmt;

#[derive(Clone, Copy)]
pub struct ExpReg {
    /// a in y = a * exp(b * x)
    pub a: f64,
    /// b in y = a * exp(b * x)
    pub b: f64,
}

impl fmt::Display for ExpReg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ExpReg")
    }
}

impl ExpReg {
    pub fn new() -> Self {
        Self { a: 1.0, b: 0.0 }
    }

    pub fn from_val(a: f64, b: f64) -> Self {
        Self { a, b }
    }

    /// Evaluate y = a * exp(b * x)
    pub fn calculate(&self, x: f64) -> f64 {
        self.a * (self.b * x).exp()
    }
    /// dy/dx = a * b * exp(b * x)
    pub fn derivative(&self, x: f64) -> f64 {
        self.a * self.b * (self.b * x).exp()
    }

    /// Train from (x, y) data assuming y > 0 for all points.
    /// Uses log-transform: ln(y) = ln(a) + b * x
    pub fn train(x: &[f64], y: &[f64]) -> Self {
        assert!(x.len() == y.len(), "Input vectors x and y must have the same length");
        assert!(y.iter().all(|&v| v > 0.0), "All y values must be > 0 for exponential regression");

        // log-transform y
        let ln_y: Vec<f64> = y.iter().map(|v| v.ln()).collect();

        // reuse your linear regression on (x, ln_y)
        let lin = LinReg::train(x, &ln_y);

        let a = lin.intercept.exp();
        let b = lin.slope;

        Self { a, b }
    }
}

impl Default for ExpReg {
    fn default() -> Self {
        Self::new()
    }
}
