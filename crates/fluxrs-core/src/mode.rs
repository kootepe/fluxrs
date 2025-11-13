use std::fmt;
use std::str::FromStr;

#[derive(Debug)]
pub struct ParseModeError(String);

impl fmt::Display for ParseModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ParseModeError {}

// how to find the flux calculation area
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Mode {
    AfterDeadband,
    BestPearsonsR,
}

impl FromStr for Mode {
    type Err = ParseModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "deadband" => Ok(Mode::AfterDeadband),
            "pearsons" => Ok(Mode::BestPearsonsR),
            "bestr" => Ok(Mode::BestPearsonsR),
            other => Err(ParseModeError(format!("invalid mode: {other}"))),
        }
    }
}

impl Default for Mode {
    fn default() -> Self {
        Self::BestPearsonsR
    }
}

impl Mode {
    pub fn as_int(&self) -> u8 {
        match self {
            Mode::AfterDeadband => 1,
            Mode::BestPearsonsR => 2,
        }
    }
    pub fn from_int(i: u8) -> Option<Mode> {
        match i {
            1 => Some(Mode::AfterDeadband),
            2 => Some(Mode::BestPearsonsR),
            _ => None,
        }
    }
}
// Display trait for nicer labels in the ComboBox
impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::AfterDeadband => write!(f, "After Deadband"),
            Mode::BestPearsonsR => write!(f, "Best Pearson's R"),
        }
    }
}
impl std::fmt::Debug for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::AfterDeadband => write!(f, "After Deadband"),
            Mode::BestPearsonsR => write!(f, "Best Pearson's R"),
        }
    }
}
