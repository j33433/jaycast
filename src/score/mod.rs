//! Trail-specific MTB rideability scoring.

mod heuristic;
mod params;

pub use heuristic::{
    score_color, score_days, score_days_as_of, BlurbTag, ClosureStatus, DayForecast, TagTone,
};
pub use params::{Params, RideabilityModel};
