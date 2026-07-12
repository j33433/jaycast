//! Trail-specific MTB rideability scoring.

mod heuristic;
mod params;

pub use heuristic::{score_color, score_days, DayForecast};
pub use params::{Params, RideabilityModel};
