use crate::gastype::GasType;
use std::fmt;

type InstrumentId = i64;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GasKey {
    pub gas_type: GasType,
    pub id: InstrumentId,
}
impl fmt::Display for GasKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}, {}", self.gas_type, self.id)
    }
}
impl GasKey {
    /// Creates a new `GasKey`.
    pub fn new(gas_type: GasType, id: impl Into<i64>) -> Self {
        Self { gas_type, id: id.into() }
    }

    /// Returns a reference to the gas type.
    pub fn gas_type(&self) -> &GasType {
        &self.gas_type
    }

    /// Returns a reference to the label.
    pub fn id(&self) -> &i64 {
        &self.id
    }
}
impl From<(&GasType, &i64)> for GasKey {
    fn from(tuple: (&GasType, &i64)) -> Self {
        Self { gas_type: *tuple.0, id: *tuple.1 }
    }
}
