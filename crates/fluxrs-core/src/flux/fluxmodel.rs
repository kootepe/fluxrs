use crate::gaschannel::GasChannel;

use dyn_clone::DynClone;

use std::any::Any;
use std::fmt;

use crate::flux::fluxkind::FluxKind;

pub trait FluxModel: Sync + Send + DynClone {
    fn gas_channel(&self) -> GasChannel;
    fn flux(&self) -> Option<f64>;
    fn r2(&self) -> Option<f64>;
    fn adj_r2(&self) -> Option<f64>;
    fn intercept(&self) -> Option<f64>;
    fn slope(&self) -> Option<f64>;
    fn p_value(&self) -> Option<f64>;
    fn sigma(&self) -> Option<f64>;
    fn rmse(&self) -> Option<f64>;
    fn cv(&self) -> Option<f64>;
    fn aic(&self) -> Option<f64>;
    fn predict(&self, x: f64) -> Option<f64>;
    fn kind(&self) -> FluxKind;
    fn set_range_start(&mut self, value: f64);
    fn set_range_end(&mut self, value: f64);
    fn range_start(&self) -> Option<f64>;
    fn range_end(&self) -> Option<f64>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
dyn_clone::clone_trait_object!(FluxModel);

impl fmt::Display for dyn FluxModel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:?}, flux: {:?}, r2: {:?}, len: {:?}",
            self.gas_channel().gas,
            self.flux(),
            self.r2(),
            match (self.range_start(), self.range_end()) {
                (Some(start), Some(end)) => Some(end - start),
                _ => None,
            }
        )
    }
}
