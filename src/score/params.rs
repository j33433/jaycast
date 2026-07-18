//! Tunable thresholds for the trail rideability heuristics.
//! Units: inches, °F, mph, hours/days.

use crate::trails::Trail;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RideabilityModel {
    SandPack,
    Drainage,
    MixedSurface,
}

#[derive(Clone, Debug)]
pub struct Params {
    pub model: RideabilityModel,
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
    /// Baseline timing quality after a long dry spell.
    pub dry_timing_floor: f64,
    /// Hours after meaningful rain that Markham may remain closed while draining.
    pub drainage_hours: f64,
    /// Rain total (inches) at which the full drainage_hours window applies.
    /// Lighter rain scales the window down proportionally.
    pub drainage_ref_rain_in: f64,
    /// Minimum fraction of drainage_hours for trace-level rain (0..1).
    pub drainage_scale_floor: f64,
    /// Hours after rain ends before MixedSurface is no longer muddy at ride morning.
    /// Afternoon storms that finish by ~4 PM clear by the next 8 AM ride window.
    pub mud_clear_hours: f64,
    /// Ride-window rain amount that starts a real "rained-on ride" penalty (inches).
    pub ride_day_precip_soft: f64,
    /// Ride-window rain amount that fully tanks the ride (inches).
    pub ride_day_precip_hard: f64,
    /// Minimum timing quality when rain just ended (< 6h, still settling).
    pub fresh_rain_floor: f64,
    /// Quality at the start of the 6h→peak ramp (rises to 1.0 at peak).
    pub ramp_start_quality: f64,
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
            model: RideabilityModel::SandPack,
            significant_rain_in: 0.25,
            ideal_antecedent_in: 1.0,
            min_useful_rain_in: 0.35,
            max_useful_rain_in: 3.0,
            pack_lookback_hours: 48.0,
            ideal_hours_since_rain: 18.0,
            pack_fade_hours: 72.0, // ~3 days
            dry_timing_floor: 0.1,
            drainage_hours: 8.5,
            drainage_ref_rain_in: 0.70,
            drainage_scale_floor: 0.35,
            mud_clear_hours: 14.0,
            ride_day_precip_soft: 0.05,
            ride_day_precip_hard: 0.4,
            fresh_rain_floor: 0.35,
            ramp_start_quality: 0.55,
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

impl Params {
    pub fn for_trail(trail: Trail) -> Self {
        let mut params = Self::default();
        match trail {
            Trail::CampMurphy => {}
            Trail::Markham => {
                params.model = RideabilityModel::Drainage;
                params.significant_rain_in = 0.10;
                // July 11 observation: 0.21 in ending around 4 AM reopened at 12:30 PM.
                params.drainage_hours = 8.5;
                params.drainage_ref_rain_in = 0.70;
                params.drainage_scale_floor = 0.35;
                params.w_pack = 0.55;
                params.w_weather = 0.35;
            }
            Trail::QuietWaters => {
                params.model = RideabilityModel::MixedSurface;
                params.min_useful_rain_in = 0.55;
                params.ideal_antecedent_in = 1.25;
                params.max_useful_rain_in = 4.0;
                params.pack_lookback_hours = 72.0;
                params.ideal_hours_since_rain = 30.0;
                params.pack_fade_hours = 120.0;
                params.dry_timing_floor = 0.90;
                // Jul 18 2026: prior-day afternoon storm (rain end ~5 PM) was not
                // muddy by morning. 14h clear window drops typical PM convection.
                params.mud_clear_hours = 14.0;
                // QW never closes and degrades slowly, so be more generous with
                // ride-window rain thresholds and the fresh-rain timing curve.
                params.ride_day_precip_soft = 0.12;
                params.ride_day_precip_hard = 0.70;
                params.fresh_rain_floor = 0.35;
                params.ramp_start_quality = 0.65;
                params.w_pack = 0.35;
                params.w_weather = 0.55;
            }
        }
        params
    }
}
