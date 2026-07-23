//! Trail-specific MTB rideability scoring.

mod heuristic;
mod params;

pub use heuristic::{score_color, score_days, score_days_as_of, ClosureStatus, DayForecast};
pub use params::{Params, RideabilityModel};
