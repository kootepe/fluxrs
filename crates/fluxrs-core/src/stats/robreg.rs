use crate::stats::stats::{mad, weight_huber};
use std::fmt;

#[derive(Clone, Copy, Debug)]
pub struct RobReg {
    pub intercept: f64,
    pub slope: f64,
}

impl fmt::Display for RobReg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RobReg")
    }
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

#[cfg(test)]
mod tests {
    use super::RobReg;

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
