use std::fmt;

#[derive(Clone, Copy)]
pub struct LinReg {
    pub intercept: f64,
    pub slope: f64,
}

impl fmt::Display for LinReg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "LinReg")
    }
}

impl Default for LinReg {
    fn default() -> Self {
        Self::new()
    }
}

impl LinReg {
    pub fn new() -> Self {
        Self { intercept: 0., slope: 0. }
    }
    pub fn calculate(&self, x: f64) -> f64 {
        self.intercept + self.slope * x
    }
    pub fn from_val(intercept: f64, slope: f64) -> Self {
        Self { intercept, slope }
    }

    pub fn train(x: &[f64], y: &[f64]) -> Self {
        assert!(x.len() == y.len(), "Input vectors x and y must have the same length");

        let avg_x: f64 = x.iter().sum::<f64>() / x.len() as f64;
        let x_differences_to_average: Vec<f64> = x.iter().map(|value| value - avg_x).collect();

        let x_differences_to_average_squared: Vec<f64> =
            x_differences_to_average.iter().map(|value| value.powi(2)).collect();

        let ss_xx: f64 = x_differences_to_average_squared.iter().sum();

        let avg_y = y.iter().sum::<f64>() / y.len() as f64;
        let y_differences_to_average: Vec<f64> = y.iter().map(|value| value - avg_y).collect();
        let x_and_y_differences_multiplied: Vec<f64> = x_differences_to_average
            .iter()
            .zip(y_differences_to_average.iter())
            .map(|(a, b)| a * b)
            .collect();
        let ss_xy: f64 = x_and_y_differences_multiplied.iter().sum();
        let slope = ss_xy / ss_xx;
        let intercept = avg_y - slope * avg_x;

        Self { intercept, slope }
    }
}
