//! Tunable thresholds for the sand-pack rideability heuristic.
//! Units: inches, °F, mph, hours/days.

#[derive(Clone, Debug)]
pub struct Params {
    /// Significant rain event threshold (inches/day).
    pub significant_rain_in: f64,
    /// Ideal antecedent rain sum over the pack window (inches).
    pub ideal_antecedent_in: f64,
    /// Soft floor: below this antecedent total, sand stays loose.
    pub min_useful_rain_in: f64,
    /// Too much recent rain (before ride day) starts to hurt.
    pub max_useful_rain_in: f64,
    /// Hours before ride day that count toward packing (lookback).
    pub pack_lookback_hours: f64,
    /// Hours after rain when pack is typically best.
    pub ideal_hours_since_rain: f64,
    /// After this many dry hours, pack benefit fades out.
    pub pack_fade_hours: f64,
    /// Ride-window rain amount that starts a real "rained-on ride" penalty (inches).
    pub ride_day_precip_soft: f64,
    /// Ride-window rain amount that fully tanks the ride (inches).
    pub ride_day_precip_hard: f64,
    /// Reference daily ET0 (inches) for "normal" drying. Sunny days exceed it
    /// (dry faster), cloudy days fall below (stay damp longer).
    pub et0_dry_ref: f64,
    /// Max fractional stretch/compression of the drying clock from ET0 (0..1).
    pub et0_modulation: f64,
    /// Comfortable high temp band (°F).
    pub temp_ideal_low: f64,
    pub temp_ideal_high: f64,
    pub temp_ok_low: f64,
    pub temp_ok_high: f64,
    /// Wind (mph) comfort: centered band, calm and gale both ding.
    pub wind_ideal_low: f64,
    pub wind_ideal_high: f64,
    /// Quality at dead calm (0 mph), 0..1.
    pub wind_calm_floor: f64,
    /// Wind (mph) where quality bottoms out.
    pub wind_bad: f64,
    /// Factor weights (should sum ~1).
    pub w_pack: f64,
    pub w_weather: f64,
    pub w_confidence: f64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            significant_rain_in: 0.25,
            ideal_antecedent_in: 1.0,
            min_useful_rain_in: 0.35,
            max_useful_rain_in: 3.0,
            pack_lookback_hours: 72.0,
            ideal_hours_since_rain: 24.0,
            pack_fade_hours: 120.0, // ~5 days
            ride_day_precip_soft: 0.05,
            ride_day_precip_hard: 0.4,
            et0_dry_ref: 0.20,
            et0_modulation: 0.30,
            temp_ideal_low: 65.0,
            temp_ideal_high: 85.0,
            temp_ok_low: 55.0,
            temp_ok_high: 95.0,
            wind_ideal_low: 5.0,
            wind_ideal_high: 12.0,
            wind_calm_floor: 0.7,
            wind_bad: 28.0,
            w_pack: 0.55,
            w_weather: 0.35,
            w_confidence: 0.10,
        }
    }
}
