use std::fmt;
#[derive(Debug)]
pub enum FluxFitError {
    LengthMismatch { len_x: usize, len_y: usize },
    NotEnoughPoints { len: usize, needed: usize },
    NonPositiveY,
    DegenerateX, // no variance in x
    NonFiniteSigma,
    NonFiniteSE,
    NonFiniteTStat,
    StatError(&'static str),
}

impl fmt::Display for FluxFitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FluxFitError::LengthMismatch { len_x, len_y } => {
                write!(f, "x and y have different lengths: {len_x} vs {len_y}")
            },
            FluxFitError::NotEnoughPoints { len, needed } => {
                write!(f, "not enough points: got {len}, need at least {needed}")
            },
            FluxFitError::NonPositiveY => {
                write!(f, "exponential model requires all y > 0")
            },
            FluxFitError::DegenerateX => {
                write!(f, "degenerate x: no variance in x")
            },
            FluxFitError::NonFiniteSigma => {
                write!(f, "non-finite sigma during fit")
            },
            FluxFitError::NonFiniteSE => {
                write!(f, "non-finite or non-positive standard error of slope")
            },
            FluxFitError::NonFiniteTStat => {
                write!(f, "non-finite t statistic")
            },
            FluxFitError::StatError(msg) => write!(f, "statistical error: {msg}"),
        }
    }
}

impl std::error::Error for FluxFitError {}

pub type FluxResult<T> = Result<T, FluxFitError>;
