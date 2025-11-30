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

pub fn weight_huber(r: f64, k: f64) -> f64 {
    let abs_r = r.abs();
    if abs_r <= k {
        1.0
    } else {
        k / abs_r
    }
}
// Median Absolute Deviation (MAD)
pub fn mad(residuals: &[f64]) -> f64 {
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

pub fn rmse(y: &[f64], y_hat: &[f64]) -> Option<f64> {
    if y.len() != y_hat.len() || y.is_empty() {
        return None;
    }

    let sum_sq: f64 = y.iter().zip(y_hat.iter()).map(|(&yi, &yhi)| (yi - yhi).powi(2)).sum();

    Some((sum_sq / y.len() as f64).sqrt())
}

pub fn aic_from_rss(rss: f64, n: usize, k: usize) -> f64 {
    if rss <= 0.0 || n == 0 {
        return f64::INFINITY; // Avoid log(0) or divide-by-zero
    }
    n as f64 * (rss / n as f64).ln() + 2.0 * k as f64
}

pub fn r2_from_predictions(y: &[f64], y_hat: &[f64]) -> Option<f64> {
    if y.len() != y_hat.len() || y.len() < 2 {
        return None;
    }

    let y_mean = y.iter().sum::<f64>() / y.len() as f64;

    let ss_res: f64 = y.iter().zip(y_hat).map(|(&yi, &yhi)| (yi - yhi).powi(2)).sum();
    let ss_tot: f64 = y.iter().map(|&yi| (yi - y_mean).powi(2)).sum();

    if ss_tot == 0.0 {
        return None;
    }

    Some(1.0 - ss_res / ss_tot)
}
pub fn adjusted_r2(r2: f64, n: usize, k: usize) -> f64 {
    if n <= k + 1 {
        return r2; // Not enough data to adjust
    }
    1.0 - (1.0 - r2) * (n as f64 - 1.0) / (n as f64 - k as f64 - 1.0)
}

#[cfg(test)]
mod tests {
    use super::pearson_correlation;

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
}
