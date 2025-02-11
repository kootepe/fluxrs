pub struct LinReg {
        pub intercept: f64,
        pub slope: f64,
    }

impl LinReg {
    pub fn calculate(&self, x: f64) -> f64 {
        self.intercept + self.slope * x
    }

    pub fn train(input: &[(f64, f64)]) -> Self {
        let x: Vec<f64> = input.iter().map(|pairs| pairs.0).collect();
        let y: Vec<f64> = input.iter().map(|pairs| pairs.1).collect();

        let avg_x: f64 = x.iter().sum::<f64>() / x.len() as f64;
        let x_differences_to_average: Vec<f64> = x.iter().map(|value| avg_x - value).collect();
        let x_differences_to_average_squared: Vec<f64> = x_differences_to_average
            .iter()
            .map(|value| value.powi(2))
            .collect();
        let ss_xx: f64 = x_differences_to_average_squared.iter().sum();

        let avg_y = y.iter().sum::<f64>() / y.len() as f64;
        let y_differences_to_average: Vec<f64> = y.iter().map(|value| avg_y - value).collect();
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
