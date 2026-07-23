//! Heuristic rideability score for sandy trails that pack after rain.

use chrono::NaiveDate;

use crate::weather::DayWeather;

use super::params::{Params, RideabilityModel};

const DAYLIGHT_START_HOUR: f64 = 7.0;
const DAYLIGHT_END_HOUR: f64 = 20.0;
const RAIN_EVENT_GAP_HOURS: usize = 3;
const TRACE_RAIN_IN: f64 = 0.01;

#[derive(Clone, Debug)]
pub struct Factor {
    pub name: &'static str,
    pub note: String,
    /// Contribution to final score roughly in [-1, 1] scaled by weight later.
    pub contribution: f64,
    /// Display bar 0..=1 for the raw subscore quality.
    pub quality: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClosureStatus {
    NotApplicable,
    Clear,
    Possible,
}

impl ClosureStatus {
    pub fn is_possible(&self) -> bool {
        matches!(self, Self::Possible)
    }
}

#[derive(Clone, Debug)]
pub struct DayForecast {
    pub date: NaiveDate,
    /// Rideability in 1.0..=5.0, one decimal place.
    pub stars: f64,
    pub score: f64,
    pub factors: Vec<Factor>,
    pub best: bool,
    /// True when the day is strictly before local today (observed archive).
    pub is_past: bool,
    pub is_today: bool,
    pub precip_in: f64,
    pub precip_3h_in: [f64; 8],
    pub cloud_3h_pct: [f64; 8],
    pub temp_max_f: f64,
    pub temp_min_f: f64,
    pub apparent_am_f: f64,
    pub apparent_pm_f: f64,
    pub precip_prob_max: f64,
    pub precip_prob_ride_max: f64,
    pub closure_status: ClosureStatus,
    pub blurb: String,
    /// Short badge label ("AM"/"PM") when this day is an unusually cool outlier.
    pub comfort_note: Option<String>,
    /// Full detail line, e.g. "6° cooler than usual (AM)".
    pub comfort_detail: Option<String>,
    /// Morning apparent temp minus trailing 7-day AM avg (°F).
    /// Positive = warmer than recent mornings. None when insufficient history.
    pub am_vs_avg_f: Option<f64>,
    /// Afternoon apparent temp minus trailing 7-day PM avg (°F).
    /// Positive = warmer than recent afternoons. None when insufficient history.
    pub pm_vs_avg_f: Option<f64>,
}

/// Score every day in the series. Antecedent rain uses earlier days in `days`.
/// `best` is set only among today and future days.
///
/// Uses the full calendar day of precip (for tests, CLI, and future days).
pub fn score_days(days: &[DayWeather], today: NaiveDate, params: &Params) -> Vec<DayForecast> {
    score_days_as_of(days, today, params, None)
}

/// Like [`score_days`], but drainage on calendar `today` only counts hourly
/// rain strictly before `as_of_hour` (local). `None` means the full day.
pub fn score_days_as_of(
    days: &[DayWeather],
    today: NaiveDate,
    params: &Params,
    as_of_hour: Option<u32>,
) -> Vec<DayForecast> {
    let mut forecasts = Vec::new();

    for (idx, _day) in days.iter().enumerate() {
        forecasts.push(score_one(days, idx, today, params, as_of_hour));
    }

    if let Some(best_idx) = forecasts
        .iter()
        .enumerate()
        .filter(|(_, d)| !d.is_past)
        .max_by(|(_, a), (_, b)| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
    {
        if let Some(f) = forecasts.get_mut(best_idx) {
            f.best = true;
        }
    }

    annotate_comfort_outliers(&mut forecasts);

    forecasts
}

/// Mark days whose AM or PM apparent temp is unusually cool relative to the
/// trailing 7-day average, and record continuous AM/PM deltas for the side bands.
/// Purely a UI annotation — does not modify scores.
const COMFORT_WINDOW: usize = 7;
const COMFORT_THRESHOLD: f64 = 4.0;

fn annotate_comfort_outliers(forecasts: &mut [DayForecast]) {
    for i in 0..forecasts.len() {
        let start = i.saturating_sub(COMFORT_WINDOW);
        if i - start < COMFORT_WINDOW {
            continue;
        }
        let window = &forecasts[start..i];
        let am_vals: Vec<f64> = window.iter().map(|d| d.apparent_am_f).filter(|v| *v > 0.0).collect();
        let pm_vals: Vec<f64> = window.iter().map(|d| d.apparent_pm_f).filter(|v| *v > 0.0).collect();
        let apparent_am = forecasts[i].apparent_am_f;
        let apparent_pm = forecasts[i].apparent_pm_f;

        if am_vals.len() >= 3 && apparent_am > 0.0 {
            let avg_am = am_vals.iter().sum::<f64>() / am_vals.len() as f64;
            forecasts[i].am_vs_avg_f = Some(apparent_am - avg_am);
            if apparent_am <= avg_am - COMFORT_THRESHOLD {
                let delta = (avg_am - apparent_am).round();
                forecasts[i].comfort_note = Some("AM".into());
                forecasts[i].comfort_detail = Some(format!("{delta:.0}° below avg AM"));
            }
        }
        if pm_vals.len() >= 3 && apparent_pm > 0.0 {
            let avg_pm = pm_vals.iter().sum::<f64>() / pm_vals.len() as f64;
            forecasts[i].pm_vs_avg_f = Some(apparent_pm - avg_pm);
            if forecasts[i].comfort_note.is_none()
                && apparent_pm <= avg_pm - COMFORT_THRESHOLD
            {
                let delta = (avg_pm - apparent_pm).round();
                forecasts[i].comfort_note = Some("PM".into());
                forecasts[i].comfort_detail = Some(format!("{delta:.0}° below avg PM"));
            }
        }
    }
}

fn score_one(
    days: &[DayWeather],
    idx: usize,
    today: NaiveDate,
    p: &Params,
    as_of_hour: Option<u32>,
) -> DayForecast {
    let day = &days[idx];

    let drainage = (p.model == RideabilityModel::Drainage)
        .then(|| drainage_status(days, idx, today, p, as_of_hour));
    let (pack_q, pack_factors) = match drainage.as_ref() {
        Some(status) => (
            1.0,
            vec![Factor {
                name: "Trail status",
                note: status.note.clone(),
                contribution: status.quality * 2.0 - 1.0,
                quality: status.quality,
            }],
        ),
        None => pack_quality(days, idx, p),
    };
    let (wx_q, wx_factors) = weather_quality(day, p);
    let (conf_q, conf_factor) = confidence(day.date, today);

    // Hard gate only for rain during the 8 AM-noon ride window. Overnight rain
    // has time to drain and should not be treated as riding in the rain.
    let wet_gate = match p.model {
        RideabilityModel::Drainage => drainage
            .as_ref()
            .map(|status| status.daylight_fraction)
            .unwrap_or(1.0),
        RideabilityModel::SandPack => {
            if day.precip_ride_in >= p.ride_day_precip_hard {
                0.25
            } else if day.precip_ride_in > p.ride_day_precip_soft {
                let t = (day.precip_ride_in - p.ride_day_precip_soft)
                    / (p.ride_day_precip_hard - p.ride_day_precip_soft);
                lerp(0.9, 0.25, t.clamp(0.0, 1.0))
            } else {
                1.0
            }
        }
        RideabilityModel::MixedSurface => {
            // QW never closes and degrades slowly, so the floor is more generous.
            if day.precip_ride_in >= p.ride_day_precip_hard {
                0.45
            } else if day.precip_ride_in > p.ride_day_precip_soft {
                let t = (day.precip_ride_in - p.ride_day_precip_soft)
                    / (p.ride_day_precip_hard - p.ride_day_precip_soft);
                lerp(0.9, 0.45, t.clamp(0.0, 1.0))
            } else {
                1.0
            }
        }
    };

    // Wet gate for MixedSurface: temporary overall ding when trail is wet.
    let mud_gate = if p.model == RideabilityModel::MixedSurface {
        let trail_note = pack_factors
            .iter()
            .find(|f| f.name == "Trail conditions")
            .map(|f| f.note.as_str())
            .unwrap_or("");
        if trail_note.starts_with("wet") {
            0.65
        } else {
            1.0
        }
    } else {
        1.0
    };

    let score = ((p.w_pack * pack_q + p.w_weather * wx_q + p.w_confidence * conf_q)
        * wet_gate
        * mud_gate)
        .clamp(0.0, 1.0);

    let mut factors = Vec::new();
    for mut f in pack_factors {
        f.contribution *= p.w_pack;
        factors.push(f);
    }
    for mut f in wx_factors {
        f.contribution *= p.w_weather;
        factors.push(f);
    }
    let mut cf = conf_factor;
    cf.contribution *= p.w_confidence;
    factors.push(cf);

    let stars = score_to_stars(score);
    let blurb = drainage
        .as_ref()
        .map(|status| status.blurb.clone())
        .unwrap_or_else(|| make_blurb(day, pack_q, &factors, p));
    let closure_status = drainage
        .as_ref()
        .map(|status| status.closure_status.clone())
        .unwrap_or(ClosureStatus::NotApplicable);
    DayForecast {
        date: day.date,
        stars,
        score,
        factors,
        best: false,
        is_past: day.date < today,
        is_today: day.date == today,
        precip_in: day.precip_in,
        precip_3h_in: day.precip_3h_in,
        cloud_3h_pct: day.cloud_3h_pct,
        temp_max_f: day.temp_max_f,
        temp_min_f: day.temp_min_f,
        apparent_am_f: day.apparent_am_f,
        apparent_pm_f: day.apparent_pm_f,
        precip_prob_max: day.precip_prob_max,
        precip_prob_ride_max: day.precip_prob_ride_max,
        closure_status,
        blurb,
        comfort_note: None,
        comfort_detail: None,
        am_vs_avg_f: None,
        pm_vs_avg_f: None,
    }
}

fn pack_quality(days: &[DayWeather], idx: usize, p: &Params) -> (f64, Vec<Factor>) {
    let day = &days[idx];
    let lookback_days = (p.pack_lookback_hours / 24.0).ceil() as usize;

    // Rain ending before the ride day (antecedent packing rain).
    let mut antecedent = 0.0;
    let start = idx.saturating_sub(lookback_days);
    for d in &days[start..idx] {
        antecedent += d.precip_in;
    }

    // Hours since last significant rain day (midpoint of that day → ride day).
    let hours_since = hours_since_significant_rain(days, idx, p.significant_rain_in);

    // Sun dries this shadeless sand fast; cloud slows it. Scale the drying clock
    // by ET0 accumulated since the rain: sunny stretches reach firm pack sooner
    // and dry out to soft faster; cloudy stretches keep the sand fresh longer.
    let dry_factor = drying_factor(days, idx, hours_since, p);
    let effective_hours = hours_since.map(|h| h * dry_factor);

    // Antecedent rain amount score.
    // SandPack: triangle around ideal — rain packs loose sand; dry = soft.
    // MixedSurface: hardpack stays firm when dry; rain only temporarily degrades.
    let amount_q = if p.model == RideabilityModel::MixedSurface {
        (0.90 - (antecedent / p.max_useful_rain_in) * 0.60).clamp(0.30, 0.90)
    } else {
        trap_score(
            antecedent,
            p.min_useful_rain_in * 0.5,
            p.min_useful_rain_in,
            p.ideal_antecedent_in,
            p.max_useful_rain_in,
        )
    };

    // Timing quality.
    // SandPack: best near ideal_hours_since_rain, fades to dry_timing_floor.
    // MixedSurface: hour-aware mud window from last rain end to ride morning.
    let is_mixed = p.model == RideabilityModel::MixedSurface;
    let today_significant = day.precip_in >= p.significant_rain_in;
    let hours_since_end = hours_since_rain_end_by_morning(days, idx, p);
    let morning_mud = hours_since_end.map_or(false, |h| h < p.mud_clear_hours);
    let muddy = morning_mud || today_significant;
    let timing_q = if is_mixed {
        // MixedSurface uses raw hours since rain end (not ET0-adjusted).
        match hours_since_end {
            None if !today_significant => p.dry_timing_floor,
            _ if muddy => p.fresh_rain_floor,
            Some(h) if h < p.mud_clear_hours + 24.0 => lerp(
                p.fresh_rain_floor,
                p.dry_timing_floor,
                ((h - p.mud_clear_hours) / 24.0).clamp(0.0, 1.0),
            ),
            _ => p.dry_timing_floor,
        }
    } else {
        match effective_hours {
            None => p.dry_timing_floor.max(0.15),
            Some(h) if h < 6.0 => p.fresh_rain_floor,
            Some(h) => {
                let peak = p.ideal_hours_since_rain;
                let fade = p.pack_fade_hours;
                if h <= peak {
                    lerp(p.ramp_start_quality, 1.0, ((h - 6.0) / (peak - 6.0)).clamp(0.0, 1.0))
                } else if h >= fade {
                    p.dry_timing_floor
                } else {
                    lerp(
                        1.0,
                        p.dry_timing_floor,
                        ((h - peak) / (fade - peak)).clamp(0.0, 1.0),
                    )
                }
            }
        }
    };

    // Rain during the ride. Wet ground is fine here (drains fast), so only rain
    // actually falling between 8 AM and noon is heavily penalized. Afternoon rain
    // while the park is open (until sundown) is lightly weighted; rain after close
    // is ignored.
    let ride_rain = day.precip_ride_in + day.precip_pm_in * 0.15;
    let wet_q = if ride_rain <= p.ride_day_precip_soft {
        // Dry forecast amount: still ding a little when the morning chance is elevated.
        1.0 - (day.precip_prob_ride_max / 100.0) * 0.1
    } else if ride_rain >= p.ride_day_precip_hard {
        0.1
    } else {
        let t = (ride_rain - p.ride_day_precip_soft)
            / (p.ride_day_precip_hard - p.ride_day_precip_soft);
        lerp(0.9, 0.1, t.clamp(0.0, 1.0))
    };

    // Combine pack sub-signals (soil moisture dropped — was modeled, not sensed).
    let pack = (0.45 * amount_q + 0.40 * timing_q + 0.15 * wet_q).clamp(0.0, 1.0);

    let timing_note = if is_mixed {
        if muddy {
            let label = wet_period_label(days, idx, p, morning_mud);
            format!("{label} - let it drain")
        } else if hours_since_end.map_or(false, |h| h < p.mud_clear_hours + 24.0) {
            "drying, firming up".into()
        } else {
            "dry hardpack - fast and firm".into()
        }
    } else {
        match effective_hours {
            None => "no recent rain - sand may be soft".into(),
            Some(h) if h < 12.0 => format!("rain ended ~{h:.0}h ago - still settling"),
            Some(h) if h <= 48.0 => format!("rain ended ~{h:.0}h ago - best trail conditions"),
            Some(h) => format!("rain ended ~{h:.0}h ago - drying out"),
        }
    };

    let amount_note = if antecedent >= p.significant_rain_in && antecedent < p.min_useful_rain_in {
        "some recent rain".into()
    } else if antecedent > p.max_useful_rain_in {
        if p.model == RideabilityModel::MixedSurface {
            format!("{antecedent:.2} in recent rain (heavy - may be wet briefly)")
        } else {
            format!("{antecedent:.2} in recent rain (heavy - may stay soft or puddled)")
        }
    } else {
        format!(
            "{antecedent:.2} in rain in prior ~{:.0}h",
            p.pack_lookback_hours
        )
    };

    let wet_note = if day.precip_ride_in > p.ride_day_precip_soft {
        format!(
            "{:.2} in rain from 8 AM-noon ({:.0}% chance) — likely riding wet",
            day.precip_ride_in, day.precip_prob_ride_max
        )
    } else if day.precip_pm_in > p.ride_day_precip_soft {
        format!(
            "{:.2} in rain noon-sundown — dry 8 AM-noon window ({:.0}% morning chance)",
            day.precip_pm_in, day.precip_prob_ride_max
        )
    } else if day.precip_prob_ride_max >= 40.0 {
        format!(
            "{:.0}% rain chance 8 AM-noon, mostly dry forecast",
            day.precip_prob_ride_max
        )
    } else {
        format!(
            "dry ride window ({:.0}% chance 8 AM-noon)",
            day.precip_prob_ride_max
        )
    };

    let factors = vec![
        Factor {
            name: "Recent rain",
            note: amount_note,
            contribution: amount_q * 2.0 - 1.0,
            quality: amount_q,
        },
        Factor {
            name: "Trail conditions",
            note: timing_note,
            contribution: timing_q * 2.0 - 1.0,
            quality: timing_q,
        },
        Factor {
            name: "Rain during ride",
            note: wet_note,
            contribution: wet_q * 2.0 - 1.0,
            quality: wet_q,
        },
    ];

    (pack, factors)
}

struct DrainageStatus {
    quality: f64,
    daylight_fraction: f64,
    note: String,
    blurb: String,
    closure_status: ClosureStatus,
}

fn drainage_status(
    days: &[DayWeather],
    idx: usize,
    today: NaiveDate,
    p: &Params,
    as_of_hour: Option<u32>,
) -> DrainageStatus {
    let future_pm_rain = (days[idx].date == today)
        .then(|| as_of_hour.and_then(|hour| future_meaningful_rain_event(&days[idx], hour, p)))
        .flatten()
        .filter(|event| event.start_hour >= 12.0);

    let Some(rain_event) = latest_meaningful_rain_event(days, idx, today, p, as_of_hour) else {
        if let Some(event) = future_pm_rain {
            return DrainageStatus {
                quality: 1.0,
                daylight_fraction: 1.0,
                note: format!("{:.2} in forecast rain; open AM, PM risk", event.total_in),
                blurb: "maybe closed PM".into(),
                closure_status: ClosureStatus::Possible,
            };
        }
        return DrainageStatus {
            quality: 1.0,
            daylight_fraction: 1.0,
            note: "no recent heavy rain".into(),
            blurb: "likely open".into(),
            closure_status: ClosureStatus::Clear,
        };
    };
    let effective_drainage = (p.drainage_hours + rain_event.total_in * p.drainage_hours_per_in)
        .min(p.drainage_max_hours);
    let reopen_hour = rain_event.end_hour + effective_drainage;
    let rain_started_during_daylight = rain_event.start_hour >= DAYLIGHT_START_HOUR;

    if reopen_hour <= DAYLIGHT_START_HOUR {
        if let Some(event) = future_pm_rain {
            return DrainageStatus {
                quality: 1.0,
                daylight_fraction: 1.0,
                note: format!("{:.2} in forecast rain; open AM, PM risk", event.total_in),
                blurb: "maybe closed PM".into(),
                closure_status: ClosureStatus::Possible,
            };
        }
        return DrainageStatus {
            quality: 1.0,
            daylight_fraction: 1.0,
            note: format!("{:.2} in rain; likely open AM", rain_event.total_in),
            blurb: "likely open".into(),
            closure_status: ClosureStatus::Clear,
        };
    }
    if reopen_hour >= DAYLIGHT_END_HOUR {
        if rain_started_during_daylight {
            let daylight_fraction =
                ((rain_event.start_hour - DAYLIGHT_START_HOUR)
                    / (DAYLIGHT_END_HOUR - DAYLIGHT_START_HOUR))
                .clamp(0.0, 1.0);
            return DrainageStatus {
                quality: daylight_fraction,
                daylight_fraction,
                note: format!(
                    "{:.2} in rain; open AM, PM risk",
                    rain_event.total_in
                ),
                blurb: "maybe closed PM".into(),
                closure_status: ClosureStatus::Possible,
            };
        }
        return DrainageStatus {
            quality: 0.05,
            daylight_fraction: 0.0,
            note: format!("{:.2} in rain; maybe closed", rain_event.total_in),
            blurb: "maybe closed".into(),
            closure_status: ClosureStatus::Possible,
        };
    }

    let daylight_fraction = ((DAYLIGHT_END_HOUR - reopen_hour)
        / (DAYLIGHT_END_HOUR - DAYLIGHT_START_HOUR))
        .clamp(0.0, 1.0);
    DrainageStatus {
        quality: daylight_fraction,
        daylight_fraction,
        note: if reopen_hour <= 14.0 {
            format!(
                "{:.2} in rain; maybe closed AM, open PM",
                rain_event.total_in
            )
        } else {
            format!("{:.2} in rain; maybe closed", rain_event.total_in)
        },
        blurb: if reopen_hour <= 14.0 {
            "maybe closed AM".into()
        } else {
            "maybe closed".into()
        },
        closure_status: ClosureStatus::Possible,
    }
}

struct RainEvent {
    total_in: f64,
    /// End hour relative to the start of the scored day.
    end_hour: f64,
    /// Start hour relative to the start of the scored day.
    start_hour: f64,
}

/// First meaningful forecast rain event after the current completed hour.
fn future_meaningful_rain_event(day: &DayWeather, from_hour: u32, p: &Params) -> Option<RainEvent> {
    let mut total_in = 0.0;
    let mut start_hour = None;
    let mut end_hour = None;
    let mut dry_hours = 0usize;

    for hour in (from_hour as usize).min(24)..24 {
        let amount = day.precip_hourly_in[hour];
        if amount >= TRACE_RAIN_IN {
            if dry_hours > RAIN_EVENT_GAP_HOURS && start_hour.is_some() {
                if total_in >= p.significant_rain_in {
                    return Some(RainEvent {
                        total_in,
                        start_hour: start_hour.expect("rain event has a start hour"),
                        end_hour: end_hour.expect("rain event has an end hour"),
                    });
                }
                total_in = 0.0;
                start_hour = None;
            }
            start_hour.get_or_insert(hour as f64);
            end_hour = Some(hour as f64 + 1.0);
            total_in += amount;
            dry_hours = 0;
        } else if start_hour.is_some() {
            dry_hours += 1;
        }
    }

    (total_in >= p.significant_rain_in).then(|| RainEvent {
        total_in,
        start_hour: start_hour.expect("meaningful rain event has a start hour"),
        end_hour: end_hour.expect("meaningful rain event has an end hour"),
    })
}

fn latest_meaningful_rain_event(
    days: &[DayWeather],
    idx: usize,
    today: NaiveDate,
    p: &Params,
    as_of_hour: Option<u32>,
) -> Option<RainEvent> {
    let mut total_in = 0.0;
    let mut end_hour = None;
    let mut start_hour = None;
    let mut dry_hours = 0usize;

    for day_idx in (0..=idx).rev() {
        let day = &days[day_idx];
        // On calendar today, ignore hours that have not completed yet so a
        // forecast afternoon storm does not close the morning.
        let hour_end = if day.date == today {
            as_of_hour.map(|h| (h as usize).min(24)).unwrap_or(24)
        } else {
            24
        };
        if hour_end == 0 {
            continue;
        }
        // Prefer real hourly tips anywhere on the day so a PM-only forecast is
        // not collapsed onto midday when scanning morning hours only.
        let has_hourly_data = day.precip_hourly_in.iter().any(|amount| *amount > 0.0);
        for hour in (0..hour_end).rev() {
            let amount = if has_hourly_data {
                day.precip_hourly_in[hour]
            } else if day.precip_in >= p.significant_rain_in && hour == 11 {
                // Hourly data can be absent in an API response; place the daily
                // total at midday as a conservative fallback.
                day.precip_in
            } else {
                0.0
            };

            if amount >= TRACE_RAIN_IN {
                if dry_hours > RAIN_EVENT_GAP_HOURS && end_hour.is_some() {
                    if total_in >= p.significant_rain_in {
                        return Some(RainEvent {
                            total_in,
                            end_hour: end_hour.expect("rain event has an end hour"),
                            start_hour: start_hour.expect("rain event has a start hour"),
                        });
                    }
                    total_in = 0.0;
                    end_hour = None;
                }
                total_in += amount;
                let rel_hour = hour as f64 - (idx - day_idx) as f64 * 24.0;
                end_hour.get_or_insert(rel_hour + 1.0);
                // Walking backward, so the last non-zero hour seen is the start.
                start_hour = Some(rel_hour);
                dry_hours = 0;
            } else if end_hour.is_some() {
                dry_hours += 1;
            }
        }
    }

    (total_in >= p.significant_rain_in).then(|| RainEvent {
        total_in,
        end_hour: end_hour.expect("meaningful rain event has an end hour"),
        start_hour: start_hour.expect("meaningful rain event has a start hour"),
    })
}

fn weather_quality(day: &DayWeather, p: &Params) -> (f64, Vec<Factor>) {
    let high = day.temp_max_f;
    let temp_q = if high >= p.temp_ideal_low && high <= p.temp_ideal_high {
        1.0
    } else if high < p.temp_ok_low || high > p.temp_ok_high {
        0.15
    } else if high < p.temp_ideal_low {
        lerp(
            0.35,
            1.0,
            ((high - p.temp_ok_low) / (p.temp_ideal_low - p.temp_ok_low)).clamp(0.0, 1.0),
        )
    } else {
        lerp(
            1.0,
            0.25,
            ((high - p.temp_ideal_high) / (p.temp_ok_high - p.temp_ideal_high)).clamp(0.0, 1.0),
        )
    };

    // Heat index-ish via apparent temp.
    let apparent_pen = if day.apparent_max_f > 95.0 {
        ((day.apparent_max_f - 95.0) / 15.0).clamp(0.0, 0.4)
    } else {
        0.0
    };
    let temp_q = (temp_q - apparent_pen).clamp(0.0, 1.0);

    let wind = day.wind_max_mph.max(day.gust_max_mph * 0.7);
    // Centered band: a light breeze is ideal; dead calm dings (hot, buggy, still)
    // and gales ding harder.
    let wind_q = if wind >= p.wind_ideal_low && wind <= p.wind_ideal_high {
        1.0
    } else if wind < p.wind_ideal_low {
        lerp(
            p.wind_calm_floor,
            1.0,
            (wind / p.wind_ideal_low).clamp(0.0, 1.0),
        )
    } else if wind >= p.wind_bad {
        0.2
    } else {
        lerp(
            1.0,
            0.2,
            ((wind - p.wind_ideal_high) / (p.wind_bad - p.wind_ideal_high)).clamp(0.0, 1.0),
        )
    };

    // Light use of precip probability already in pack; small comfort ding here.
    let sky_q = 1.0 - (day.precip_prob_max / 100.0) * 0.35;

    let wx = (0.55 * temp_q + 0.30 * wind_q + 0.15 * sky_q).clamp(0.0, 1.0);

    let temp_note = format!(
        "high {:.0}°F / low {:.0}°F (feels {:.0}°F)",
        day.temp_max_f, day.temp_min_f, day.apparent_max_f
    );
    let wind_note = format!(
        "wind {:.0} mph, gusts {:.0} mph",
        day.wind_max_mph, day.gust_max_mph
    );

    let factors = vec![
        Factor {
            name: "Temperature",
            note: temp_note,
            contribution: temp_q * 2.0 - 1.0,
            quality: temp_q,
        },
        Factor {
            name: "Wind",
            note: wind_note,
            contribution: wind_q * 2.0 - 1.0,
            quality: wind_q,
        },
        Factor {
            name: "Sky",
            note: format!("{:.0}% highest rain chance", day.precip_prob_max),
            contribution: sky_q * 2.0 - 1.0,
            quality: sky_q,
        },
    ];

    (wx, factors)
}

fn confidence(date: NaiveDate, today: NaiveDate) -> (f64, Factor) {
    if date < today {
        return (
            1.0,
            Factor {
                name: "Weather data",
                note: "observed weather".into(),
                contribution: 1.0,
                quality: 1.0,
            },
        );
    }

    let days_out = (date - today).num_days().max(0) as f64;
    // Full confidence today–day 3, then taper to ~0.45 by day 7.
    let q = if days_out <= 3.0 {
        1.0
    } else {
        lerp(1.0, 0.45, ((days_out - 3.0) / 4.0).clamp(0.0, 1.0))
    };

    let note = if days_out == 0.0 {
        "today (near-term forecast)".into()
    } else if days_out <= 1.0 {
        "near-term forecast".into()
    } else if days_out <= 4.0 {
        format!("+{days_out:.0} days out - solid confidence")
    } else {
        format!("+{days_out:.0} days out - lower confidence")
    };

    (
        q,
        Factor {
            name: "Forecast reliability",
            note,
            contribution: q * 2.0 - 1.0,
            quality: q,
        },
    )
}

/// Drying-clock multiplier from ET0 (sun) since the last significant rain.
/// Mean ET0 above the reference dries faster (>1, up to 1+modulation); below
/// reference (cloudy) dries slower (<1, down to 1-modulation). Returns 1.0 when
/// there is no rain reference or no ET0 data.
fn drying_factor(days: &[DayWeather], idx: usize, hours_since: Option<f64>, p: &Params) -> f64 {
    let Some(h) = hours_since else {
        return 1.0;
    };
    // Days since rain to average ET0 over (at least the ride day itself).
    let span = ((h / 24.0).round() as usize).max(1);
    let start = idx.saturating_sub(span);
    let slice = &days[start..=idx];
    if slice.is_empty() {
        return 1.0;
    }
    let mean_et0: f64 = slice.iter().map(|d| d.et0).sum::<f64>() / slice.len() as f64;
    if p.et0_dry_ref <= 0.0 {
        return 1.0;
    }
    let ratio = mean_et0 / p.et0_dry_ref;
    ratio.clamp(1.0 - p.et0_modulation, 1.0 + p.et0_modulation)
}

fn hours_since_significant_rain(days: &[DayWeather], idx: usize, threshold: f64) -> Option<f64> {
    // Walk backward from day before ride day.
    for back in 1..=idx {
        let i = idx - back;
        if days[i].precip_in >= threshold {
            // Approximate: rain day midpoint → ride day noon = back * 24 hours.
            return Some(back as f64 * 24.0);
        }
    }
    None
}

/// Map continuous score 0..=1 to stars 1.0..=5.0 (one decimal).
pub fn score_to_stars(score: f64) -> f64 {
    let s = (1.0 + score.clamp(0.0, 1.0) * 4.0).clamp(1.0, 5.0);
    (s * 10.0).round() / 10.0
}

/// CSS color for a day card from score 0..=1 (red → sand → scrub green).
pub fn score_color(score: f64) -> String {
    let t = score.clamp(0.0, 1.0);
    // Hue: ~8 (rust red) → 42 (sand/gold) → 118 (scrub green)
    let h = if t < 0.5 {
        8.0 + (t / 0.5) * 34.0
    } else {
        42.0 + ((t - 0.5) / 0.5) * 76.0
    };
    let s = 48.0 + t * 12.0;
    let l = 38.0 + t * 8.0;
    format!("hsl({h:.0} {s:.0}% {l:.0}%)")
}

fn make_blurb(day: &DayWeather, pack_q: f64, factors: &[Factor], p: &Params) -> String {
    if p.model == RideabilityModel::MixedSurface {
        if let Some(f) = factors.iter().find(|f| f.name == "Trail conditions") {
            if f.note.starts_with("wet") {
                return f.note.split(" - ").next().unwrap_or("wet").into();
            }
        }
    }
    if day.precip_in >= 0.25 {
        return wet_period_blurb(day);
    }
    if pack_q >= 0.7 {
        return if p.model == RideabilityModel::MixedSurface {
            "good".into()
        } else {
            "firm sand".into()
        };
    }
    if pack_q <= 0.35 {
        return if p.model == RideabilityModel::MixedSurface {
            "some loose terrain likely".into()
        } else {
            "likely soft sand".into()
        };
    }
    // Fall back to strongest named factor note snippet.
    factors
        .first()
        .map(|f| f.note.clone())
        .unwrap_or_else(|| format!("high {:.0}°F", day.temp_max_f))
}

/// Prefer a timed wet label when one part of the day holds most of the rain.
fn wet_period_blurb(day: &DayWeather) -> String {
    let morning =
        day.precip_3h_in[0] + day.precip_3h_in[1] + day.precip_3h_in[2] + day.precip_3h_in[3];
    let afternoon = day.precip_3h_in[4] + day.precip_3h_in[5];
    let evening = day.precip_3h_in[6] + day.precip_3h_in[7];
    let total = morning + afternoon + evening;
    let period = if total <= 0.0 {
        "day"
    } else {
        let am = morning;
        let pm = afternoon + evening;
        let (name, amount) = [
            ("AM", am),
            ("PM", pm),
        ]
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(("day", 0.0));
        // Spread across both halves stays "rainy day".
        if amount / total >= 0.55 {
            name
        } else {
            "rainy day"
        }
    };
    if period == "rainy day" {
        "rainy day".into()
    } else {
        format!("rain {period}")
    }
}

/// Wet period label for MixedSurface trails: "wet AM", "wet PM", or "wet".
///
/// Prefer today's rain timing when the ride day itself is wet. Otherwise a
/// still-wet trail at ride morning is wet AM (hour-aware overnight carryover).
fn wet_period_label(
    days: &[DayWeather],
    idx: usize,
    p: &Params,
    morning_mud: bool,
) -> String {
    let today = &days[idx];
    if today.precip_in >= p.significant_rain_in {
        return wet_label_from_3h(&today.precip_3h_in);
    }
    if morning_mud {
        return "wet AM".into();
    }
    "wet".into()
}

/// Hours from last meaningful rain end to ride morning (8 AM), counting only
/// rain that has already ended by then. Future/afternoon forecast rain on the
/// ride day is ignored so it does not fake a muddy morning.
fn hours_since_rain_end_by_morning(
    days: &[DayWeather],
    idx: usize,
    p: &Params,
) -> Option<f64> {
    const RIDE_MORNING_HOUR: f64 = 8.0;
    let event = latest_rain_event_ending_by(days, idx, p, RIDE_MORNING_HOUR)?;
    Some(RIDE_MORNING_HOUR - event.end_hour)
}

/// Latest meaningful rain event whose end is at or before `by_hour` (relative
/// to the start of the scored day). Uses hourly precip when present, else
/// spreads 3h buckets, else places daily total at midday.
fn latest_rain_event_ending_by(
    days: &[DayWeather],
    idx: usize,
    p: &Params,
    by_hour: f64,
) -> Option<RainEvent> {
    let mut total_in = 0.0;
    let mut end_hour = None;
    let mut start_hour = None;
    let mut dry_hours = 0usize;

    for day_idx in (0..=idx).rev() {
        let day = &days[day_idx];
        let hourly = precip_hourly_for_event(day, p);
        for hour in (0..24).rev() {
            let rel_hour = hour as f64 - (idx - day_idx) as f64 * 24.0;
            let hour_end = rel_hour + 1.0;
            if hour_end > by_hour {
                continue;
            }
            let amount = hourly[hour];
            if amount >= TRACE_RAIN_IN {
                if dry_hours > RAIN_EVENT_GAP_HOURS && end_hour.is_some() {
                    if total_in >= p.significant_rain_in {
                        return Some(RainEvent {
                            total_in,
                            end_hour: end_hour.expect("rain event has an end hour"),
                            start_hour: start_hour.expect("rain event has a start hour"),
                        });
                    }
                    total_in = 0.0;
                    end_hour = None;
                }
                total_in += amount;
                end_hour.get_or_insert(hour_end);
                start_hour = Some(rel_hour);
                dry_hours = 0;
            } else if end_hour.is_some() {
                dry_hours += 1;
            }
        }
    }

    (total_in >= p.significant_rain_in).then(|| RainEvent {
        total_in,
        end_hour: end_hour.expect("meaningful rain event has an end hour"),
        start_hour: start_hour.expect("meaningful rain event has a start hour"),
    })
}

fn precip_hourly_for_event(day: &DayWeather, p: &Params) -> [f64; 24] {
    if day.precip_hourly_in.iter().any(|amount| *amount > 0.0) {
        return day.precip_hourly_in;
    }
    if day.precip_3h_in.iter().any(|amount| *amount > 0.0) {
        let mut hourly = [0.0; 24];
        for (bucket, amount) in day.precip_3h_in.iter().enumerate() {
            for offset in 0..3 {
                hourly[bucket * 3 + offset] = amount / 3.0;
            }
        }
        return hourly;
    }
    let mut hourly = [0.0; 24];
    if day.precip_in >= p.significant_rain_in {
        hourly[11] = day.precip_in;
    }
    hourly
}

fn wet_label_from_3h(buckets: &[f64; 8]) -> String {
    let morning = buckets[0] + buckets[1] + buckets[2] + buckets[3];
    let afternoon = buckets[4] + buckets[5];
    let evening = buckets[6] + buckets[7];
    let total = morning + afternoon + evening;
    if total <= 0.0 {
        return "wet".into();
    }
    let pm = afternoon + evening;
    let (name, amount) = [("AM", morning), ("PM", pm)]
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(("day", 0.0));
    if amount / total >= 0.55 {
        format!("wet {name}")
    } else {
        "wet".into()
    }
}

fn trap_score(x: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    // Trapezoid membership: 0 outside [a,d], ramp a→b, 1 on [b,c], ramp c→d.
    if x <= a || x >= d {
        return 0.0;
    }
    if x >= b && x <= c {
        return 1.0;
    }
    if x < b {
        return ((x - a) / (b - a)).clamp(0.0, 1.0);
    }
    ((d - x) / (d - c)).clamp(0.0, 1.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn day(date: &str, precip: f64, high: f64) -> DayWeather {
        // Default: precip split evenly-ish, sunny drying reference.
        DayWeather {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            precip_in: precip,
            precip_prob_max: if precip > 0.2 { 70.0 } else { 10.0 },
            precip_prob_ride_max: if precip > 0.2 { 70.0 } else { 10.0 },
            temp_max_f: high,
            temp_min_f: high - 15.0,
            apparent_max_f: high + 2.0,
            apparent_am_f: high - 5.0,
            apparent_pm_f: high + 2.0,
            wind_max_mph: 8.0,
            gust_max_mph: 14.0,
            et0: 0.20,
            // Assume rain falls in the afternoon by default (convective FL storms).
            precip_ride_in: 0.0,
            precip_pm_in: precip,
            precip_hourly_in: [0.0; 24],
            precip_3h_in: [0.0; 8],
            cloud_3h_pct: [0.0; 8],
        }
    }

    #[test]
    fn post_rain_dry_day_scores_high() {
        let days = vec![
            day("2026-07-01", 0.1, 88.0),
            day("2026-07-02", 1.2, 82.0), // heavy rain
            day("2026-07-03", 0.0, 84.0), // pack day
            day("2026-07-04", 0.0, 86.0),
        ];
        let today = NaiveDate::from_ymd_opt(2026, 7, 3).unwrap();
        let scored = score_days(&days, today, &Params::default());
        assert_eq!(scored.len(), 4, "scores past and future days");
        assert!(scored[0].is_past);
        assert!(scored[2].is_today);
        let d3 = scored.iter().find(|d| d.date == today).unwrap();
        assert!(
            d3.stars >= 4.0,
            "expected high stars after rain, got {:.1} (score {:.2})",
            d3.stars,
            d3.score
        );
        assert!(
            scored.iter().filter(|d| d.best).count() == 1,
            "exactly one best among non-past"
        );
        assert!(!scored[0].best && !scored[1].best, "past days are not best");
    }

    #[test]
    fn long_dry_spell_scores_low_pack() {
        let mut days = Vec::new();
        for i in 1..=8 {
            days.push(day(&format!("2026-07-{i:02}"), 0.0, 90.0));
        }
        let today = NaiveDate::from_ymd_opt(2026, 7, 8).unwrap();
        let scored = score_days(&days, today, &Params::default());
        let d = scored.iter().find(|d| d.date == today).unwrap();
        assert!(
            d.stars <= 3.5,
            "dry sand should not be great, got {:.1} (score {:.2})",
            d.stars,
            d.score
        );
    }

    #[test]
    fn ride_window_rain_penalized() {
        // Same daily total as afternoon case, but falling while the park is open.
        let mut d1 = day("2026-07-01", 1.0, 80.0);
        d1.precip_ride_in = 0.0;
        d1.precip_pm_in = 1.0;
        let mut d2 = day("2026-07-02", 0.8, 78.0);
        d2.precip_ride_in = 0.8; // rains while the park is open
        d2.precip_pm_in = 0.0;
        let days = vec![d1, d2];
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        let scored = score_days(&days, today, &Params::default());
        let d = &scored[1];
        assert!(
            d.stars <= 3.5,
            "ride-window rain should tank the ride, got {:.1} (score {:.2})",
            d.stars,
            d.score
        );
    }

    #[test]
    fn afternoon_rain_tolerated() {
        // Rain arrives only in the afternoon after a good packing rain.
        let mut prior = day("2026-07-01", 1.0, 80.0);
        prior.precip_ride_in = 0.0;
        prior.precip_pm_in = 1.0;
        let mut ride = day("2026-07-02", 0.5, 82.0);
        ride.precip_ride_in = 0.0; // dry ride window
        ride.precip_pm_in = 0.5; // afternoon storm
        let days = vec![prior, ride];
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        let scored = score_days(&days, today, &Params::default());
        let d = &scored[1];
        assert!(
            d.stars >= 3.5,
            "afternoon-only rain should stay rideable, got {:.1} (score {:.2})",
            d.stars,
            d.score
        );
    }

    #[test]
    fn overnight_rain_does_not_penalize_the_ride_window() {
        let prior = day("2026-07-01", 1.0, 80.0);
        let dry_ride = day("2026-07-02", 0.0, 82.0);
        let mut overnight_rain = day("2026-07-02", 0.25, 82.0);
        overnight_rain.precip_ride_in = 0.0;
        overnight_rain.precip_pm_in = 0.0;
        overnight_rain.precip_prob_max = dry_ride.precip_prob_max;
        overnight_rain.precip_prob_ride_max = dry_ride.precip_prob_ride_max;
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();

        let dry_score = score_days(&[prior.clone(), dry_ride], today, &Params::default())[1].score;
        let overnight_score =
            score_days(&[prior, overnight_rain], today, &Params::default())[1].score;
        assert!(
            (dry_score - overnight_score).abs() < 1e-9,
            "overnight rain should not lower the ride score: dry {dry_score:.3} vs overnight {overnight_score:.3}"
        );
    }

    #[test]
    fn light_ride_window_rain_is_tolerated_on_packed_sand() {
        let prior = day("2026-07-01", 1.0, 80.0);
        let dry_ride = day("2026-07-02", 0.0, 82.0);
        let mut light_rain = day("2026-07-02", 0.04, 82.0);
        light_rain.precip_ride_in = 0.04;
        light_rain.precip_pm_in = 0.0;
        light_rain.precip_prob_max = dry_ride.precip_prob_max;
        light_rain.precip_prob_ride_max = dry_ride.precip_prob_ride_max;
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();

        let dry_score = score_days(&[prior.clone(), dry_ride], today, &Params::default())[1].score;
        let light_rain_score = score_days(&[prior, light_rain], today, &Params::default())[1].score;
        assert!(
            (dry_score - light_rain_score).abs() < 1e-9,
            "light ride-window rain should be tolerated: dry {dry_score:.3} vs light {light_rain_score:.3}"
        );
    }

    #[test]
    fn cloudy_slows_drying_vs_sunny() {
        // Identical rain history; one week sunny (high ET0), one cloudy (low ET0),
        // riding a few days out so the drying window matters.
        let mk = |et0: f64| {
            let mut v = Vec::new();
            for i in 1..=5 {
                let mut dd = day(&format!("2026-07-{i:02}"), 0.0, 84.0);
                dd.et0 = et0;
                v.push(dd);
            }
            // Big packing rain 3 days before ride.
            v[1].precip_in = 1.0;
            v[1].precip_pm_in = 1.0;
            v
        };
        let today = NaiveDate::from_ymd_opt(2026, 7, 5).unwrap();
        let sunny = score_days(&mk(0.30), today, &Params::default());
        let cloudy = score_days(&mk(0.08), today, &Params::default());
        let s = sunny.iter().find(|d| d.date == today).unwrap();
        let c = cloudy.iter().find(|d| d.date == today).unwrap();
        // Sunny dried further past peak (lower timing) OR cloudy held fresher.
        // Either way the two should differ, proving ET0 modulates timing.
        assert!(
            (s.score - c.score).abs() > 1e-3,
            "ET0 should change the score: sunny {:.3} vs cloudy {:.3}",
            s.score,
            c.score
        );
    }

    #[test]
    fn dead_calm_dings_wind() {
        let mut calm = day("2026-07-02", 0.0, 78.0);
        calm.wind_max_mph = 0.0;
        calm.gust_max_mph = 0.0;
        let mut breeze = day("2026-07-02", 0.0, 78.0);
        breeze.wind_max_mph = 8.0;
        breeze.gust_max_mph = 12.0;
        let (calm_q, _) = weather_quality(&calm, &Params::default());
        let (breeze_q, _) = weather_quality(&breeze, &Params::default());
        assert!(
            breeze_q > calm_q,
            "light breeze should beat dead calm: {breeze_q:.3} vs {calm_q:.3}"
        );
    }

    #[test]
    fn markham_moderate_overnight_rain_reopens_around_midday() {
        // Jul 10-11 gauge event: 0.23 in ending around 2 AM; park reopened
        // around 12:30 PM.
        let mut before_midnight = day("2026-07-10", 0.06, 80.0);
        before_midnight.precip_hourly_in[23] = 0.06;
        let mut after_midnight = day("2026-07-11", 0.17, 80.0);
        after_midnight.precip_prob_max = 10.0;
        after_midnight.precip_prob_ride_max = 10.0;
        after_midnight.precip_hourly_in[0] = 0.15;
        after_midnight.precip_hourly_in[1] = 0.02;
        let days = vec![
            before_midnight,
            after_midnight,
            day("2026-07-12", 0.0, 80.0),
        ];
        let today = NaiveDate::from_ymd_opt(2026, 7, 11).unwrap();
        let scored = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::Markham),
        );

        assert_eq!(scored[1].blurb, "maybe closed AM");
        assert_eq!(scored[1].closure_status, ClosureStatus::Possible);
        assert_eq!(scored[2].blurb, "likely open");
        assert_eq!(scored[2].closure_status, ClosureStatus::Clear);
        assert!(scored[1].score < scored[2].score);
    }

    #[test]
    fn markham_heavy_pm_rain_carries_into_next_morning() {
        // Jul 15-16 gauge event: 1.25 in ending around 6 PM; park reopened
        // the next day around 12:30 PM.
        let mut rain = day("2026-07-15", 1.25, 90.0);
        rain.precip_hourly_in[15] = 0.02;
        rain.precip_hourly_in[16] = 0.72;
        rain.precip_hourly_in[17] = 0.51;
        let dry_next_day = day("2026-07-16", 0.0, 88.0);
        let today = NaiveDate::from_ymd_opt(2026, 7, 16).unwrap();
        let scored = score_days(
            &[rain, dry_next_day],
            today,
            &Params::for_trail(crate::trails::Trail::Markham),
        );

        assert_eq!(scored[1].blurb, "maybe closed AM");
        assert_eq!(scored[1].closure_status, ClosureStatus::Possible);
    }

    #[test]
    fn markham_afternoon_rain_open_am() {
        // Afternoon storm: rain at hours 15-16, trail open all morning.
        let mut rain = day("2026-07-02", 0.52, 90.0);
        rain.precip_prob_max = 22.0;
        rain.precip_prob_ride_max = 4.0;
        rain.precip_hourly_in[15] = 0.40;
        rain.precip_hourly_in[16] = 0.12;
        let days = vec![rain, day("2026-07-03", 0.0, 80.0)];
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        let scored = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::Markham),
        );

        assert_eq!(scored[0].blurb, "maybe closed PM");
        assert_eq!(scored[0].closure_status, ClosureStatus::Possible);
        // Morning was rideable, so score should reflect partial daylight.
        assert!(
            scored[0].score > 0.0,
            "afternoon rain should not zero out the score: got {:.3}",
            scored[0].score
        );
        // Day after is clear.
        assert_eq!(scored[1].blurb, "likely open");
        assert_eq!(scored[1].closure_status, ClosureStatus::Clear);
    }

    #[test]
    fn markham_warns_about_future_pm_rain_without_tanking_morning() {
        // Light PM storm that previously collapsed the whole day to 1★ when
        // scored with the full forecast at 10 AM.
        let mut rain = day("2026-07-23", 0.10, 90.0);
        rain.precip_prob_max = 70.0;
        rain.precip_prob_ride_max = 20.0;
        rain.precip_hourly_in[15] = 0.024;
        rain.precip_hourly_in[16] = 0.079;
        let days = vec![day("2026-07-22", 0.0, 88.0), rain];
        let today = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
        let p = Params::for_trail(crate::trails::Trail::Markham);

        let morning = score_days_as_of(&days, today, &p, Some(10));
        assert_eq!(morning[1].blurb, "maybe closed PM");
        assert_eq!(morning[1].closure_status, ClosureStatus::Possible);
        let status = morning[1]
            .factors
            .iter()
            .find(|factor| factor.name == "Trail status")
            .unwrap();
        assert_eq!(
            status.quality, 1.0,
            "forecast PM risk must not lower current drainage"
        );
        assert!(
            morning[1].stars >= 3.0,
            "morning should not be 1★ for forecast PM rain: got {:.1}",
            morning[1].stars
        );

        let evening = score_days_as_of(&days, today, &p, Some(18));
        assert_ne!(
            evening[1].blurb, "likely open",
            "after the storm, drainage should apply"
        );
        assert_eq!(evening[1].closure_status, ClosureStatus::Possible);
    }

    #[test]
    fn markham_combines_rain_across_midnight() {
        let mut before_midnight = day("2026-07-01", 0.25, 80.0);
        before_midnight.precip_hourly_in[23] = 0.25;
        let mut after_midnight = day("2026-07-02", 0.25, 80.0);
        after_midnight.precip_hourly_in[1] = 0.25;
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        let scored = score_days(
            &[before_midnight, after_midnight],
            today,
            &Params::for_trail(crate::trails::Trail::Markham),
        );

        assert_eq!(scored[1].closure_status, ClosureStatus::Possible);
    }

    #[test]
    fn markham_ignores_short_showers() {
        let days = vec![day("2026-07-02", 0.06, 80.0)];
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        let scored = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::Markham),
        );

        assert_eq!(scored[0].blurb, "likely open");
        assert_eq!(scored[0].closure_status, ClosureStatus::Clear);
    }

    #[test]
    fn markham_trailing_trace_does_not_extend_closure() {
        // 0.185" rain at hours 19-20, then a 0.004" trace at hour 22.
        // Without the trace filter, the trace extends end_hour to 23:00,
        // pushing reopen to 7:30 AM and falsely flagging "maybe closed AM".
        let mut rain = day("2026-07-12", 0.197, 92.0);
        rain.precip_prob_max = 49.0;
        rain.precip_prob_ride_max = 0.0;
        rain.precip_hourly_in[19] = 0.039;
        rain.precip_hourly_in[20] = 0.146;
        rain.precip_hourly_in[22] = 0.004;
        let next = day("2026-07-13", 0.0, 90.0);
        let days = vec![rain, next];
        let today = NaiveDate::from_ymd_opt(2026, 7, 13).unwrap();
        let scored = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::Markham),
        );

        assert_eq!(scored[1].blurb, "likely open");
        assert_eq!(scored[1].closure_status, ClosureStatus::Clear);
    }

    #[test]
    fn quiet_waters_keeps_a_higher_dry_surface_baseline() {
        let days = vec![day("2026-07-01", 0.0, 80.0), day("2026-07-02", 0.0, 80.0)];
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        let camp = score_days(&days, today, &Params::default())[1].score;
        let quiet = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::QuietWaters),
        )[1]
        .score;

        assert!(
            quiet > camp,
            "Quiet Waters should degrade less in dry weather"
        );
        let quiet_stars = score_to_stars(quiet);
        assert!(
            quiet_stars >= 4.0,
            "dry hardpack should score well, got {:.1} stars",
            quiet_stars
        );
    }

    #[test]
    fn quiet_waters_dry_day_is_fast_and_firm() {
        // Field observation Jul 15, 2026: "fast with some dust" on a dry day.
        // The model should score a dry hardpack day high, not penalize it as soft sand.
        let days = vec![
            day("2026-07-13", 0.0, 92.0),
            day("2026-07-14", 0.0, 94.0),
            day("2026-07-15", 0.0, 94.0),
        ];
        let today = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap();
        let scored = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::QuietWaters),
        );
        let d = scored.iter().find(|d| d.date == today).unwrap();
        assert!(
            d.stars >= 3.5,
            "dry hardpack should score well, got {:.1} stars (score {:.2})",
            d.stars,
            d.score
        );
        assert!(
            !d.blurb.contains("soft"),
            "dry hardpack blurb should not say soft: got '{}'",
            d.blurb
        );
    }

    #[test]
    fn quiet_waters_tolerates_ride_window_rain_better_than_camp() {
        let prior = day("2026-07-01", 1.0, 80.0);
        let mut wet_ride = day("2026-07-02", 0.50, 82.0);
        wet_ride.precip_ride_in = 0.50;
        wet_ride.precip_pm_in = 0.0;
        let days = vec![prior, wet_ride];
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();

        let camp_score = score_days(&days, today, &Params::default())[1].score;
        let quiet_score = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::QuietWaters),
        )[1]
        .score;

        assert!(
            quiet_score > camp_score,
            "Quiet Waters should tolerate ride-window rain better than Camp Murphy: \
             quiet {quiet_score:.3} vs camp {camp_score:.3}"
        );
    }

    #[test]
    fn quiet_waters_wet_am() {
        let mut rain = day("2026-07-02", 0.50, 88.0);
        rain.precip_3h_in = [0.0, 0.20, 0.20, 0.10, 0.0, 0.0, 0.0, 0.0];
        rain.precip_ride_in = 0.30;
        rain.precip_pm_in = 0.0;
        let days = vec![day("2026-07-01", 0.0, 88.0), rain];
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        let scored = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::QuietWaters),
        );
        let d = &scored[1];
        assert_eq!(d.blurb, "wet AM");
        assert!(
            d.stars < 3.5,
            "wet AM should ding the score, got {:.1} stars",
            d.stars
        );
    }

    #[test]
    fn quiet_waters_wet_pm() {
        let mut rain = day("2026-07-02", 0.50, 88.0);
        rain.precip_3h_in = [0.0, 0.0, 0.0, 0.0, 0.20, 0.20, 0.10, 0.0];
        rain.precip_ride_in = 0.0;
        rain.precip_pm_in = 0.40;
        let days = vec![day("2026-07-01", 0.0, 88.0), rain];
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        let scored = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::QuietWaters),
        );
        let d = &scored[1];
        assert_eq!(d.blurb, "wet PM");
        assert!(
            d.stars < 3.5,
            "wet PM should ding the score, got {:.1} stars",
            d.stars
        );
    }

    #[test]
    fn quiet_waters_wet_spread() {
        let mut rain = day("2026-07-02", 0.60, 88.0);
        rain.precip_3h_in = [0.0, 0.15, 0.0, 0.0, 0.15, 0.0, 0.0, 0.0];
        rain.precip_ride_in = 0.15;
        rain.precip_pm_in = 0.15;
        let days = vec![day("2026-07-01", 0.0, 88.0), rain];
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        let scored = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::QuietWaters),
        );
        let d = &scored[1];
        assert_eq!(d.blurb, "wet");
        assert!(
            d.stars < 3.5,
            "wet (spread) should ding the score, got {:.1} stars",
            d.stars
        );
    }

    #[test]
    fn quiet_waters_wet_am_from_late_evening() {
        // Late evening rain yesterday still wet at 8 AM (end ~11 PM → ~9h).
        let mut prev = day("2026-07-01", 0.50, 88.0);
        prev.precip_3h_in = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.10, 0.40];
        prev.precip_pm_in = 0.50;
        prev.precip_hourly_in[21] = 0.10;
        prev.precip_hourly_in[22] = 0.40;
        let dry_today = day("2026-07-02", 0.0, 88.0);
        let days = vec![prev, dry_today];
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        let scored = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::QuietWaters),
        );
        let d = &scored[1];
        assert_eq!(d.blurb, "wet AM");
        assert!(
            d.stars < 3.5,
            "late-evening wet AM should ding the score, got {:.1} stars",
            d.stars
        );
    }

    #[test]
    fn quiet_waters_afternoon_storm_clears_by_next_morning() {
        // Jul 18 2026 observation: prior ~3 PM storm was not wet by morning.
        let mut prev = day("2026-07-17", 0.42, 95.0);
        prev.precip_3h_in = [0.0, 0.0, 0.0, 0.0, 0.0, 0.42, 0.0, 0.0];
        prev.precip_pm_in = 0.42;
        prev.precip_hourly_in[15] = 0.42;
        let dry_today = day("2026-07-18", 0.0, 92.0);
        let days = vec![prev, dry_today];
        let today = NaiveDate::from_ymd_opt(2026, 7, 18).unwrap();
        let scored = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::QuietWaters),
        );
        let d = &scored[1];
        assert_ne!(d.blurb, "wet AM", "afternoon storm should clear overnight");
        assert!(
            !d.blurb.starts_with("wet"),
            "expected recovered trail, got '{}'",
            d.blurb
        );
        assert!(
            d.stars >= 3.5,
            "cleared overnight should score decently, got {:.1} stars",
            d.stars
        );
    }

    #[test]
    fn quiet_waters_recovered_after_rain() {
        // Rain 2+ days ago — trail should be dry and firm again.
        let mut rain = day("2026-07-01", 1.0, 82.0);
        rain.precip_pm_in = 1.0;
        let days = vec![
            rain,
            day("2026-07-02", 0.0, 88.0),
            day("2026-07-03", 0.0, 88.0),
        ];
        let today = NaiveDate::from_ymd_opt(2026, 7, 3).unwrap();
        let scored = score_days(
            &days,
            today,
            &Params::for_trail(crate::trails::Trail::QuietWaters),
        );
        let d = &scored[2];
        assert_eq!(d.blurb, "good");
        assert!(
            d.stars >= 3.5,
            "recovered trail should score well, got {:.1} stars",
            d.stars
        );
    }

    #[test]
    fn stars_mapping_boundaries() {
        assert!((score_to_stars(1.0) - 5.0).abs() < 1e-9);
        assert!((score_to_stars(0.0) - 1.0).abs() < 1e-9);
        assert!((score_to_stars(0.875) - 4.5).abs() < 1e-9);
        assert!((score_to_stars(0.5) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn wet_blurb_names_the_dominant_period() {
        let mut evening = day("2026-07-02", 1.0, 88.0);
        evening.precip_3h_in = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        assert_eq!(wet_period_blurb(&evening), "rain PM");

        let mut morning = day("2026-07-02", 0.50, 88.0);
        morning.precip_3h_in = [0.0, 0.0, 0.40, 0.10, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(wet_period_blurb(&morning), "rain AM");

        let mut spread = day("2026-07-02", 0.60, 88.0);
        spread.precip_3h_in = [0.0, 0.0, 0.30, 0.0, 0.30, 0.0, 0.0, 0.0];
        assert_eq!(wet_period_blurb(&spread), "rainy day");
    }

    #[test]
    fn good_outlier_detected_when_cooler_than_trend() {
        let mut days = Vec::new();
        for i in 1..=7 {
            days.push(day(&format!("2026-07-{i:02}"), 0.0, 95.0));
        }
        // Day 8: unusually cool AM (trailing avg is 90.0, threshold 86.0)
        let mut cool = day("2026-07-08", 0.0, 90.0);
        cool.apparent_am_f = 85.0;
        cool.apparent_pm_f = 98.0;
        days.push(cool);
        let today = NaiveDate::from_ymd_opt(2026, 7, 8).unwrap();
        let scored = score_days(&days, today, &Params::default());
        let d = scored.iter().find(|d| d.date == today).unwrap();
        assert_eq!(
            d.comfort_note.as_deref(),
            Some("AM"),
            "expected comfort note for cool AM"
        );
        // Trailing avg 90.0, day 85.0 → 5° below avg.
        assert_eq!(d.comfort_detail.as_deref(), Some("5° below avg AM"));
        assert!(
            (d.am_vs_avg_f.unwrap() - (-5.0)).abs() < 1e-9,
            "expected am_vs_avg_f ≈ -5"
        );
    }

    #[test]
    fn warm_morning_records_positive_am_delta() {
        let mut days = Vec::new();
        for i in 1..=7 {
            days.push(day(&format!("2026-07-{i:02}"), 0.0, 95.0));
        }
        // day() sets apparent_am_f = high - 5 = 90 for prior days
        let mut warm = day("2026-07-08", 0.0, 95.0);
        warm.apparent_am_f = 95.0;
        days.push(warm);
        let today = NaiveDate::from_ymd_opt(2026, 7, 8).unwrap();
        let scored = score_days(&days, today, &Params::default());
        let d = scored.iter().find(|d| d.date == today).unwrap();
        assert!(d.comfort_note.is_none());
        assert!(
            (d.am_vs_avg_f.unwrap() - 5.0).abs() < 1e-9,
            "expected am_vs_avg_f ≈ +5"
        );
    }

    #[test]
    fn warm_afternoon_records_positive_pm_delta() {
        let mut days = Vec::new();
        for i in 1..=7 {
            days.push(day(&format!("2026-07-{i:02}"), 0.0, 95.0));
        }
        // day() sets apparent_pm_f = high + 2 = 97 for prior days
        let mut warm = day("2026-07-08", 0.0, 95.0);
        warm.apparent_pm_f = 102.0;
        days.push(warm);
        let today = NaiveDate::from_ymd_opt(2026, 7, 8).unwrap();
        let scored = score_days(&days, today, &Params::default());
        let d = scored.iter().find(|d| d.date == today).unwrap();
        assert!(
            (d.pm_vs_avg_f.unwrap() - 5.0).abs() < 1e-9,
            "expected pm_vs_avg_f ≈ +5"
        );
    }

    #[test]
    fn no_outlier_when_within_trend() {
        let mut days = Vec::new();
        for i in 1..=8 {
            days.push(day(&format!("2026-07-{i:02}"), 0.0, 92.0));
        }
        let today = NaiveDate::from_ymd_opt(2026, 7, 8).unwrap();
        let scored = score_days(&days, today, &Params::default());
        let d = scored.iter().find(|d| d.date == today).unwrap();
        assert!(
            d.comfort_note.is_none(),
            "no outlier expected when temps are steady"
        );
        assert!(
            d.am_vs_avg_f.unwrap().abs() < 1e-9,
            "steady temps should yield ~0 AM delta"
        );
        assert!(
            d.pm_vs_avg_f.unwrap().abs() < 1e-9,
            "steady temps should yield ~0 PM delta"
        );
    }

    #[test]
    fn outlier_needs_trailing_data() {
        let days = vec![
            day("2026-07-01", 0.0, 90.0),
            day("2026-07-02", 0.0, 90.0),
        ];
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        let scored = score_days(&days, today, &Params::default());
        let d = scored.iter().find(|d| d.date == today).unwrap();
        assert!(
            d.comfort_note.is_none(),
            "no outlier expected with insufficient trailing data"
        );
        assert!(
            d.am_vs_avg_f.is_none(),
            "no AM delta without full trailing window"
        );
        assert!(
            d.pm_vs_avg_f.is_none(),
            "no PM delta without full trailing window"
        );
    }
}
