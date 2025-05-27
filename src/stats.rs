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

#[derive(Clone, Copy, Debug)]
pub struct PolyReg {
    pub a0: f64,
    pub a1: f64,
    pub a2: f64,
}

impl PolyReg {
    pub fn calculate(&self, x: f64) -> f64 {
        self.a0 + self.a1 * x + self.a2 * x * x
    }

    pub fn from_coeffs(a0: f64, a1: f64, a2: f64) -> Self {
        Self { a0, a1, a2 }
    }

    pub fn train(x: &[f64], y: &[f64]) -> Option<Self> {
        if x.len() < 3 {
            return None;
        }

        let n = x.len();
        let (mut sum_x, mut sum_x2, mut sum_x3, mut sum_x4) = (0.0, 0.0, 0.0, 0.0);
        let (mut sum_y, mut sum_xy, mut sum_x2y) = (0.0, 0.0, 0.0);

        for i in 0..n {
            let xi = x[i];
            let yi = y[i];
            let xi2 = xi * xi;
            let xi3 = xi2 * xi;
            let xi4 = xi3 * xi;

            sum_x += xi;
            sum_x2 += xi2;
            sum_x3 += xi3;
            sum_x4 += xi4;

            sum_y += yi;
            sum_xy += xi * yi;
            sum_x2y += xi2 * yi;
        }

        let a = nalgebra::DMatrix::from_row_slice(
            3,
            3,
            &[n as f64, sum_x, sum_x2, sum_x, sum_x2, sum_x3, sum_x2, sum_x3, sum_x4],
        );

        let b = nalgebra::DVector::from_row_slice(&[sum_y, sum_xy, sum_x2y]);

        a.lu().solve(&b).map(|result| Self { a0: result[0], a1: result[1], a2: result[2] })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RobReg {
    pub intercept: f64,
    pub slope: f64,
}

impl Default for RobReg {
    fn default() -> Self {
        Self::new()
    }
}
impl RobReg {
    pub fn new() -> Self {
        Self { intercept: 0.0, slope: 0.0 }
    }

    pub fn from_val(intercept: f64, slope: f64) -> Self {
        Self { intercept, slope }
    }

    pub fn calculate(&self, x: f64) -> f64 {
        self.intercept + self.slope * x
    }

    /// Train using Huber loss (simple robust linear regression)
    pub fn train(x: &[f64], y: &[f64], k: f64, max_iter: usize) -> Option<Self> {
        if x.len() != y.len() || x.len() < 2 {
            return None;
        }

        let n = x.len();

        // Normalize x to improve numerical stability
        let x0 = x[0];
        let x_norm: Vec<f64> = x.iter().map(|xi| xi - x0).collect();

        // Initialize with OLS
        let (mut slope, mut intercept) = Self::ols(&x_norm, y)?;

        for _ in 0..max_iter {
            let y_hat: Vec<f64> = x_norm.iter().map(|&xi| intercept + slope * xi).collect();
            let residuals: Vec<f64> =
                y.iter().zip(y_hat.iter()).map(|(&yi, &yhi)| yi - yhi).collect();
            let scale = mad(&residuals);

            let weights: Vec<f64> = residuals.iter().map(|&r| psi_huber(r / scale, k)).collect();

            let w_sum: f64 = weights.iter().sum();
            let xw_mean = x_norm.iter().zip(&weights).map(|(&xi, &w)| xi * w).sum::<f64>() / w_sum;
            let yw_mean = y.iter().zip(&weights).map(|(&yi, &w)| yi * w).sum::<f64>() / w_sum;

            let sxx_w = x_norm
                .iter()
                .zip(&weights)
                .map(|(&xi, &w)| w * (xi - xw_mean).powi(2))
                .sum::<f64>();

            let sxy_w = x_norm
                .iter()
                .zip(y.iter())
                .zip(&weights)
                .map(|((&xi, &yi), &w)| w * (xi - xw_mean) * (yi - yw_mean))
                .sum::<f64>();

            if sxx_w.abs() < 1e-12 {
                return None;
            }

            slope = sxy_w / sxx_w;
            intercept = yw_mean - slope * xw_mean;
        }

        Some(Self { intercept, slope })
    }

    fn ols(x: &[f64], y: &[f64]) -> Option<(f64, f64)> {
        let n = x.len();
        let x_mean = x.iter().sum::<f64>() / n as f64;
        let y_mean = y.iter().sum::<f64>() / n as f64;

        let sxy =
            x.iter().zip(y.iter()).map(|(&xi, &yi)| (xi - x_mean) * (yi - y_mean)).sum::<f64>();
        let sxx = x.iter().map(|&xi| (xi - x_mean).powi(2)).sum::<f64>();

        if sxx.abs() < 1e-12 {
            return None;
        }

        let slope = sxy / sxx;
        let intercept = y_mean - slope * x_mean;
        Some((slope, intercept))
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

// Huber psi function: full weight for small residuals, downweight large ones
fn psi_huber(u: f64, k: f64) -> f64 {
    if u.abs() <= k {
        1.0
    } else {
        k / u.abs()
    }
}

// Median Absolute Deviation (MAD)
fn mad(residuals: &[f64]) -> f64 {
    let mut res = residuals.to_vec();
    let med = median(&res);
    for r in res.iter_mut() {
        *r = (*r - med).abs();
    }
    median(&res) / 0.6745
}

fn median(data: &[f64]) -> f64 {
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}
#[cfg(test)]
mod tests {
    use super::{pearson_correlation, RobReg};

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
    // needs to fail since absolute value is returned
    fn test_pearsons_neg() {
        let x = vec![1., 2., 3., 4., 5.];
        let y = vec![5., 4., 3., 2., 1.];

        assert_ne!(pearson_correlation(&x, &y), Some(-1.));
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

    #[test]
    fn test_robust_reg_basic_fit() {
        let x_raw = vec![0.0, 1.0, 2.0, 3.0];
        let y = vec![1.0, 2.0, 3.0, 4.0]; // y = x + 1

        let x0 = x_raw[0];
        let x: Vec<f64> = x_raw.iter().map(|xi| xi - x0).collect();

        let model = RobReg::train(&x, &y, 1.0, 10).unwrap();
        dbg!(model.slope, model.intercept);
        assert!((model.slope - 1.0).abs() < 1e-6);
        assert!((model.intercept - 1.0).abs() < 1e-6);
    }
    #[test]
    fn test_robust_reg_with_outlier() {
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let y = vec![1.0, 2.0, 3.0, 4.0, 100.0]; // strong outlier at end

        let model = RobReg::train(&x, &y, 1.0, 10).unwrap();

        // Robust regression should still produce a reasonable slope
        dbg!(model.slope, model.intercept);
        assert!((model.slope - 1.0).abs() < 0.5);
        assert!(model.intercept < 10.0);
    }
}
