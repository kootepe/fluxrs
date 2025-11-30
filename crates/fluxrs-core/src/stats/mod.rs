pub mod expreg;
pub mod linreg;
pub mod polyreg;
pub mod robreg;
pub mod stats;

pub use expreg::ExpReg;
pub use linreg::LinReg;
pub use polyreg::PolyReg;
pub use robreg::RobReg;
pub use stats::{adjusted_r2, aic_from_rss, r2_from_predictions, rmse};
