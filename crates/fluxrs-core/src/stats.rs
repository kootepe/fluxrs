use std::fmt;

#[derive(Clone, Copy)]
pub struct LinReg {
    pub intercept: f64,
    pub slope: f64,
}
impl fmt::Display for ExpReg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ExpReg")
    }
}
impl fmt::Display for LinReg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "LinReg")
    }
}
impl fmt::Display for PolyReg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PolyReg")
    }
}
impl fmt::Display for RobReg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RobReg")
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

#[derive(Clone, Copy)]
pub struct ExpReg {
    /// a in y = a * exp(b * x)
    pub a: f64,
    /// b in y = a * exp(b * x)
    pub b: f64,
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
        let (mut slope, mut intercept) = Self::trimmed_ols(&x_norm, y, 0.1)?;

        for _ in 0..max_iter {
            let y_hat: Vec<f64> = x_norm.iter().map(|&xi| intercept + slope * xi).collect();
            let residuals: Vec<f64> =
                y.iter().zip(y_hat.iter()).map(|(&yi, &yhi)| yi - yhi).collect();
            let scale = mad(&residuals);

            let weights: Vec<f64> = residuals.iter().map(|&r| weight_huber(r / scale, k)).collect();

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

    fn trimmed_ols(x: &[f64], y: &[f64], trim_frac: f64) -> Option<(f64, f64)> {
        use std::cmp::Ordering;

        // Basic sanity checks
        if x.len() != y.len() || x.len() < 3 {
            return None;
        }

        // Guard trim fraction: must leave at least 1 point on each side
        if !(0.0..0.5).contains(&trim_frac) {
            return None;
        }

        // Ensure inputs are finite
        if x.iter().any(|v| !v.is_finite()) || y.iter().any(|v| !v.is_finite()) {
            return None;
        }

        // Initial OLS fit
        let n = x.len();
        let x_mean = x.iter().sum::<f64>() / n as f64;
        let y_mean = y.iter().sum::<f64>() / n as f64;

        let sxx = x.iter().map(|xi| (xi - x_mean).powi(2)).sum::<f64>();
        let sxy = x.iter().zip(y).map(|(xi, yi)| (xi - x_mean) * (yi - y_mean)).sum::<f64>();

        if sxx.abs() < 1e-12 {
            return None;
        }

        let slope = sxy / sxx;
        let intercept = y_mean - slope * x_mean;

        // Compute residuals: (x, y, |residual|)
        let mut residuals: Vec<(f64, f64, f64)> = x
            .iter()
            .zip(y)
            .map(|(&xi, &yi)| {
                let y_hat = intercept + slope * xi;
                let resid = (yi - y_hat).abs();
                (xi, yi, resid)
            })
            .collect();

        // Ensure residuals are finite
        if residuals.iter().any(|&(_, _, r)| !r.is_finite()) {
            return None;
        }

        // Sort by residual size, NaN-safe using total_cmp
        residuals.sort_by(|a, b| a.2.total_cmp(&b.2));

        let trim_n = ((n as f64) * trim_frac).floor() as usize;

        // Ensure trimming leaves something
        if trim_n * 2 >= n {
            return None;
        }

        let trimmed = &residuals[trim_n..n - trim_n];

        // Recompute OLS on trimmed data
        let x_vals: Vec<f64> = trimmed.iter().map(|t| t.0).collect();
        let y_vals: Vec<f64> = trimmed.iter().map(|t| t.1).collect();

        let m = x_vals.len();
        if m < 2 {
            return None;
        }

        let x_mean = x_vals.iter().sum::<f64>() / m as f64;
        let y_mean = y_vals.iter().sum::<f64>() / m as f64;

        let sxx = x_vals.iter().map(|xi| (xi - x_mean).powi(2)).sum::<f64>();
        let sxy =
            x_vals.iter().zip(&y_vals).map(|(xi, yi)| (xi - x_mean) * (yi - y_mean)).sum::<f64>();

        if sxx.abs() < 1e-12 {
            return None;
        }

        let slope = sxy / sxx;
        let intercept = y_mean - slope * x_mean;

        Some((slope, intercept))
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

pub fn fast_pearson(x: &[f64], y: &[f64]) -> Option<f64> {
    if x.len() < 5 || x.len() != y.len() {
        return None;
    }

    let n = x.len() as f64;
    let sum_x = x.iter().sum::<f64>();
    let sum_y = y.iter().sum::<f64>();
    let mean_x = sum_x / n;
    let mean_y = sum_y / n;

    let mut num = 0.0;
    let mut denom_x = 0.0;
    let mut denom_y = 0.0;

    for (&xi, &yi) in x.iter().zip(y.iter()) {
        let dx = xi - mean_x;
        let dy = yi - mean_y;
        num += dx * dy;
        denom_x += dx * dx;
        denom_y += dy * dy;
    }

    let denom = (denom_x * denom_y).sqrt();
    if denom == 0.0 {
        None
    } else {
        Some((num / denom).abs())
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
    if x.iter().any(|v| !v.is_finite()) || y.iter().any(|v| !v.is_finite()) {
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

fn weight_huber(r: f64, k: f64) -> f64 {
    let abs_r = r.abs();
    if abs_r <= k {
        1.0
    } else {
        k / abs_r
    }
}
// Median Absolute Deviation (MAD)
fn mad(residuals: &[f64]) -> f64 {
    let mut res = residuals.to_vec();
    let med = median(&res);
    for r in res.iter_mut() {
        *r = (*r - med).abs();
    }
    let mad = median(&res) / 0.6745;
    if mad < 1e-12 {
        1e-12
    } else {
        mad
    }
}

pub fn median(data: &[f64]) -> f64 {
    let mut sorted: Vec<f64> = data.iter().cloned().filter(|v| !v.is_nan()).collect();

    let len = sorted.len();
    if len == 0 {
        return f64::NAN;
    }

    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mid = len / 2;
    if len % 2 == 0 {
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
        let x = [1., 2., 3., 4., 5., 6.];
        let y = [1., 2., 3., 4., 5.];

        assert_eq!(pearson_correlation(&x, &y), None);
    }
    #[test]
    fn test_pearsons_empty() {
        let x = [1., 2.];
        let y = [];

        assert_eq!(pearson_correlation(&x, &y), None);
    }
    #[test]
    fn test_pearsons_pos() {
        let x = [1., 2., 3., 4., 5.];
        let y = [1., 2., 3., 4., 5.];

        assert_eq!(pearson_correlation(&x, &y), Some(1.));
    }
    #[test]
    // needs to fail since absolute value is returned
    fn test_pearsons_neg() {
        let x = [1., 2., 3., 4., 5.];
        let y = [5., 4., 3., 2., 1.];

        assert_ne!(pearson_correlation(&x, &y), Some(-1.));
    }
    #[test]
    fn test_pearsons_short_x() {
        let x = [1., 2., 3.];
        let y = [5., 4., 3., 2., 1.];

        assert_eq!(pearson_correlation(&x, &y), None);
    }
    #[test]
    fn test_pearsons_short_y() {
        let x = [5., 4., 3., 2., 1.];
        let y = [1., 2., 3.];

        assert_eq!(pearson_correlation(&x, &y), None);
    }

    #[test]
    fn test_robust_reg_basic_fit() {
        let x_raw = [0.0, 1.0, 2.0, 3.0];
        let y = [1.0, 2.0, 3.0, 4.0]; // y = x + 1

        let x0 = x_raw[0];
        let x: Vec<f64> = x_raw.iter().map(|xi| xi - x0).collect();

        let model = RobReg::train(&x, &y, 1.0, 10).unwrap();
        dbg!(model.slope, model.intercept);
        assert!((model.slope - 1.0).abs() < 1e-6);
        assert!((model.intercept - 1.0).abs() < 1e-6);
    }
    #[test]
    fn test_robust_reg_with_outlier() {
        let mut x = vec![];
        let mut y = vec![];

        // Linear trend with noise
        for i in 0..50 {
            x.push(i as f64);
            y.push(i as f64 + rand::random::<f64>() * 0.5 - 0.25);
        }

        // Add an outlier
        x.push(100.0);
        y.push(1000.0); // big deviation

        let model = RobReg::train(&x, &y, 0.1, 10).unwrap();

        dbg!(model.slope, model.intercept);
        assert!((model.slope - 1.0).abs() < 0.1);
        assert!(model.intercept.abs() < 1.0);
    }
}
