#[derive(Clone, Copy)]
pub struct LinReg {
    pub intercept: f64,
    pub slope: f64,
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
        let x_differences_to_average: Vec<f64> = x.iter().map(|value| avg_x - value).collect();
        let x_differences_to_average_squared: Vec<f64> =
            x_differences_to_average.iter().map(|value| value.powi(2)).collect();
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

pub fn pearson_correlation(x: &[f64], y: &[f64]) -> Option<f64> {
    if x.len() < 5 {
        return None;
    }
    if y.len() < 5 {
        return None;
    }
    if x.len() != y.len() {
        println!("x_len: {}", x.len());
        println!("y_len: {}", y.len());
        println!("Vectors not the same length");
        return None;
    }
    if x.is_empty() {
        println!("Empty data.");
        return None;
    }

    let n = x.len() as f64;

    let mean_x = x.iter().sum::<f64>() / n;
    let mean_y = y.iter().sum::<f64>() / n;

    let numerator: f64 =
        x.iter().zip(y.iter()).map(|(&xi, &yi)| (xi - mean_x) * (yi - mean_y)).sum();

    let denominator_x: f64 = x.iter().map(|&xi| (xi - mean_x).powi(2)).sum();
    let denominator_y: f64 = y.iter().map(|&yi| (yi - mean_y).powi(2)).sum();

    let denominator = (denominator_x * denominator_y).sqrt();

    if denominator == 0.0 {
        None
    } else {
        Some((numerator / denominator).abs())
    }
}

#[cfg(test)]
mod tests {
    use super::pearson_correlation;

    #[test]
    fn test_pearson_length() {
        let x = vec![1., 2., 3., 4., 5., 6.];
        let y = vec![1., 2., 3., 4., 5.];

        assert_eq!(pearson_correlation(&x, &y), None);
    }
    #[test]
    fn test_pearsons_empty() {
        let x = vec![1., 2.];
        let y = vec![];

        assert_eq!(pearson_correlation(&x, &y), None);
    }
    #[test]
    fn test_pearsons_pos() {
        let x = vec![1., 2., 3., 4., 5.];
        let y = vec![1., 2., 3., 4., 5.];

        assert_eq!(pearson_correlation(&x, &y), Some(1.));
    }
    #[test]
    fn test_pearsons_neg() {
        let x = vec![1., 2., 3., 4., 5.];
        let y = vec![5., 4., 3., 2., 1.];

        assert_eq!(pearson_correlation(&x, &y), Some(-1.));
    }
    #[test]
    fn test_pearsons_short_x() {
        let x = vec![1., 2., 3.];
        let y = vec![5., 4., 3., 2., 1.];

        assert_eq!(pearson_correlation(&x, &y), None);
    }
    #[test]
    fn test_pearsons_short_y() {
        let x = vec![5., 4., 3., 2., 1.];
        let y = vec![1., 2., 3.];

        assert_eq!(pearson_correlation(&x, &y), None);
    }
}
