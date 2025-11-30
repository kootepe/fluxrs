use std::fmt;

#[derive(Clone, Copy, Debug)]
pub struct PolyReg {
    pub a0: f64,
    pub a1: f64,
    pub a2: f64,
}

impl fmt::Display for PolyReg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PolyReg")
    }
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
