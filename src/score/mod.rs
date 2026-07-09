//! Rideability scoring for sandy dune MTB trails.

mod heuristic;
mod params;

pub use heuristic::{score_days, DayForecast};
pub use params::Params;
