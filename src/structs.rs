use crate::cycle::Cycle;
use crate::errorcode::ErrorMask;
use crate::instruments::InstrumentType;
use chrono::prelude::DateTime;
use chrono::Utc;
use rusqlite::Error;
use rusqlite::{params, Connection, Result};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::ops::{BitOr, BitOrAssign};
use std::{thread, time};

use csv::StringRecord;
// use std::error::Error;

// use crate::errorcode::EqualLen;
use crate::gas_plot;
use crate::instruments::GasType;
use crate::stats;

pub const ERROR_INT: i64 = -9999;
pub const ERROR_FLOAT: f64 = -9999.;

// pub trait EqualLen {
//     fn validate_lengths(&self) -> bool;
// }

// fn nan_exceeds_threshold(measurement_length: f64, threshold: f64) -> bool {
//     let total_count = values.len();
//     let nan_count = values.iter().filter(|&&x| x.is_nan()).count();
//
//     // Check if NaN count exceeds the threshold percentage
//     (nan_count as f64) / (total_count as f64) > threshold
// }
