use std::fmt;
use std::ops::{BitOr, BitOrAssign};

pub trait EqualLen {
    fn validate_lengths(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ErrorsInMeasurement,
    LowR,
    TooManyMeasurements,
    TooFewMeasurements,
    FewUnique,
    ManualInvalid,
    TooManyDiagErrors,
    BadOpenClose,
}

impl ErrorCode {
    // Define bitmask constants
    pub const DIAG_ERROR_IN_MEASUREMENT: u16 = 1 << 0;
    pub const LOW_R: u16 = 1 << 1;
    pub const FEW_UNIQUE: u16 = 1 << 2;
    pub const TOO_MANY_MEASUREMENTS: u16 = 1 << 3;
    pub const TOO_FEW_MEASUREMENTS: u16 = 1 << 4;
    pub const MANUAL_INVALID: u16 = 1 << 5;
    pub const MOSTLY_DIAG_ERRORS: u16 = 1 << 6;
    pub const BAD_OPEN_CLOSE: u16 = 1 << 7;

    /// Convert an `ErrorCode` to its corresponding bitmask
    pub fn to_mask(&self) -> u16 {
        match self {
            ErrorCode::ErrorsInMeasurement => Self::DIAG_ERROR_IN_MEASUREMENT,
            ErrorCode::LowR => Self::LOW_R,
            ErrorCode::FewUnique => Self::FEW_UNIQUE,
            ErrorCode::TooManyMeasurements => Self::TOO_MANY_MEASUREMENTS,
            ErrorCode::TooFewMeasurements => Self::TOO_FEW_MEASUREMENTS,
            ErrorCode::ManualInvalid => Self::MANUAL_INVALID,
            ErrorCode::TooManyDiagErrors => Self::MOSTLY_DIAG_ERRORS,
            ErrorCode::BadOpenClose => Self::BAD_OPEN_CLOSE,
        }
    }

    /// Convert a bitmask into a list of `ErrorCode` values
    pub fn from_mask(mask: u16) -> Vec<ErrorCode> {
        let mut errors = Vec::new();
        for error in [
            ErrorCode::ErrorsInMeasurement,
            ErrorCode::LowR,
            ErrorCode::FewUnique,
            ErrorCode::TooManyMeasurements,
            ErrorCode::TooFewMeasurements,
            ErrorCode::ManualInvalid,
            ErrorCode::TooManyDiagErrors,
            ErrorCode::BadOpenClose,
        ] {
            if mask & error.to_mask() != 0 {
                errors.push(error);
            }
        }
        errors
    }
}

/// Wrapper struct for managing the error bitmask
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorMask(pub u16);

/// Implement `BitOr` for `ErrorCode`, allowing `ErrorCode | ErrorCode`
impl BitOr for ErrorCode {
    type Output = ErrorMask;
    fn bitor(self, rhs: Self) -> Self::Output {
        ErrorMask(self.to_mask() | rhs.to_mask())
    }
}

/// Implement `BitOrAssign<ErrorCode>` for `ErrorMask` (allow `ErrorMask |= ErrorCode`)
impl BitOrAssign<ErrorCode> for ErrorMask {
    fn bitor_assign(&mut self, rhs: ErrorCode) {
        self.0 |= rhs.to_mask();
    }
}

/// Implement `BitOrAssign<ErrorMask>` for `ErrorMask` (allow `ErrorMask |= ErrorMask`)
impl BitOrAssign<ErrorMask> for ErrorMask {
    fn bitor_assign(&mut self, rhs: ErrorMask) {
        self.0 |= rhs.0;
    }
}

impl ErrorMask {
    pub fn from_u16(value: u16) -> Self {
        ErrorMask(value)
    }
    pub fn contains(&self, code: ErrorCode) -> bool {
        self.0 & code.to_mask() != 0
    }
    pub fn toggle(&mut self, code: ErrorCode) {
        let mask = code.to_mask();
        if self.contains(code) {
            self.0 &= !mask; // Unset the bit
        } else {
            self.0 |= mask; // Set the bit
        }
    }
}

/// Implement `Display` for error messages
impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let message = match self {
            ErrorCode::ErrorsInMeasurement => "Instrument diagnostic errors in measurement",
            ErrorCode::LowR => "Low r value",
            ErrorCode::FewUnique => "Too few unique values",
            ErrorCode::TooManyMeasurements => "Too many values",
            ErrorCode::TooFewMeasurements => "Too few values",
            ErrorCode::ManualInvalid => "Manual invalid",
            ErrorCode::TooManyDiagErrors => "Too many instrument diagnostic errors",
            ErrorCode::BadOpenClose => "Bad opening and/or closing of chamber",
        };
        write!(f, "{}", message)
    }
}
