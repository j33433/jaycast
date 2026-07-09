//! Heuristic rideability score for sandy trails that pack after rain.

use chrono::NaiveDate;

use crate::weather::DayWeather;

use super::params::Params;

#[derive(Clone, Debug)]
pub struct Factor {
    pub name: &'static str,
    pub note: String,
    /// Contribution to final score roughly in [-1, 1] scaled by weight later.
    pub contribution: f64,
    /// Display bar 0..=1 for the raw subscore quality.
    pub quality: f64,
}

#[derive(Clone, Debug)]
pub struct DayForecast {
    pub date: NaiveDate,
    pub stars: u8,
    pub score: f64,
    pub factors: Vec<Factor>,
    pub best: bool,
    pub precip_in: f64,
    pub temp_max_f: f64,
    pub temp_min_f: f64,
    pub precip_prob_max: f64,
    pub blurb: String,
}

/// Score every day in the series. Only days on/after `today` are marked for display
/// ranking; history is used for antecedent rain.
pub fn score_days(days: &[DayWeather], today: NaiveDate, params: &Params) -> Vec<DayForecast> {
    let mut forecasts = Vec::new();

    for (idx, day) in days.iter().enumerate() {
        if day.date < today {
            continue;
        }
        let fc = score_one(days, idx, today, params);
        forecasts.push(fc);
    }

    if let Some(best_idx) = forecasts
        .iter()
        .enumerate()
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

    forecasts
}

fn score_one(days: &[DayWeather], idx: usize, today: NaiveDate, p: &Params) -> DayForecast {
    let day = &days[idx];

    let (pack_q, pack_factors) = pack_quality(days, idx, p);
    let (wx_q, wx_factors) = weather_quality(day, p);
    let (conf_q, conf_factor) = confidence(day.date, today);

    // Hard gate: heavy rain on the ride day tanks the whole score.
    let wet_gate = if day.precip_in >= p.ride_day_precip_hard {
        0.25
    } else if day.precip_in > p.ride_day_precip_soft {
        let t = (day.precip_in - p.ride_day_precip_soft)
            / (p.ride_day_precip_hard - p.ride_day_precip_soft);
        lerp(0.9, 0.25, t.clamp(0.0, 1.0))
    } else {
        1.0
    };

    let score = ((p.w_pack * pack_q + p.w_weather * wx_q + p.w_confidence * conf_q) * wet_gate)
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
    let blurb = make_blurb(day, pack_q, &factors);

    DayForecast {
        date: day.date,
        stars,
        score,
        factors,
        best: false,
        precip_in: day.precip_in,
        temp_max_f: day.temp_max_f,
        temp_min_f: day.temp_min_f,
        precip_prob_max: day.precip_prob_max,
        blurb,
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

    // Antecedent rain amount score: triangle around ideal.
    let amount_q = trap_score(
        antecedent,
        p.min_useful_rain_in * 0.5,
        p.min_useful_rain_in,
        p.ideal_antecedent_in,
        p.max_useful_rain_in,
    );

    // Timing: best near ideal_hours_since_rain, fades to 0 at pack_fade_hours.
    let timing_q = match hours_since {
        None => 0.15, // long dry spell / never — soft sand baseline
        Some(h) if h < 6.0 => 0.35, // still very fresh / maybe wet
        Some(h) => {
            let peak = p.ideal_hours_since_rain;
            let fade = p.pack_fade_hours;
            if h <= peak {
                // ramp from 6h → peak
                lerp(0.55, 1.0, ((h - 6.0) / (peak - 6.0)).clamp(0.0, 1.0))
            } else if h >= fade {
                0.1
            } else {
                lerp(1.0, 0.1, ((h - peak) / (fade - peak)).clamp(0.0, 1.0))
            }
        }
    };

    // Ride-day wetness penalty (applied inside pack as "surface condition").
    let wet_q = if day.precip_in <= p.ride_day_precip_soft {
        1.0 - (day.precip_prob_max / 100.0) * 0.25
    } else if day.precip_in >= p.ride_day_precip_hard {
        0.05
    } else {
        let t = (day.precip_in - p.ride_day_precip_soft)
            / (p.ride_day_precip_hard - p.ride_day_precip_soft);
        lerp(0.85, 0.05, t.clamp(0.0, 1.0)) * (1.0 - (day.precip_prob_max / 100.0) * 0.15)
    };

    // Soil moisture bonus (secondary).
    let (soil_q, soil_note) = match day.soil_moisture {
        Some(sm) if sm >= p.soil_ideal_low && sm <= p.soil_ideal_high => {
            (1.0, format!("soil moisture {sm:.2} m³/m³ in firm band"))
        }
        Some(sm) if sm > p.soil_ideal_high => {
            let over = ((sm - p.soil_ideal_high) / 0.15).clamp(0.0, 1.0);
            (
                lerp(0.7, 0.25, over),
                format!("soil moisture {sm:.2} m³/m³ (wet side)"),
            )
        }
        Some(sm) => (
            lerp(0.35, 0.7, (sm / p.soil_ideal_low).clamp(0.0, 1.0)),
            format!("soil moisture {sm:.2} m³/m³ (dry side)"),
        ),
        None => (0.55, "soil moisture unavailable".into()),
    };

    // Combine pack sub-signals.
    let pack = (0.40 * amount_q + 0.35 * timing_q + 0.20 * wet_q + 0.05 * soil_q).clamp(0.0, 1.0);

    let timing_note = match hours_since {
        None => "no significant rain in lookback — sand likely soft".into(),
        Some(h) if h < 12.0 => format!("~{h:.0}h since last solid rain — still settling"),
        Some(h) if h <= 48.0 => format!("~{h:.0}h since last solid rain — pack window"),
        Some(h) => format!("~{h:.0}h since last solid rain — drying out"),
    };

    let amount_note = if antecedent < p.min_useful_rain_in {
        format!("{antecedent:.2} in prior rain (need more for firm pack)")
    } else if antecedent > p.max_useful_rain_in {
        format!("{antecedent:.2} in prior rain (heavy — may stay soft/puddled)")
    } else {
        format!("{antecedent:.2} in rain in prior ~{:.0}h", p.pack_lookback_hours)
    };

    let wet_note = if day.precip_in > p.ride_day_precip_soft {
        format!(
            "{:.2} in expected on ride day ({:.0}% chance)",
            day.precip_in, day.precip_prob_max
        )
    } else if day.precip_prob_max >= 40.0 {
        format!(
            "{:.0}% rain chance, {:.2} in forecast",
            day.precip_prob_max, day.precip_in
        )
    } else {
        format!(
            "mostly dry day ({:.2} in, {:.0}% chance)",
            day.precip_in, day.precip_prob_max
        )
    };

    let factors = vec![
        Factor {
            name: "Prior rain",
            note: amount_note,
            contribution: amount_q * 2.0 - 1.0,
            quality: amount_q,
        },
        Factor {
            name: "Pack timing",
            note: timing_note,
            contribution: timing_q * 2.0 - 1.0,
            quality: timing_q,
        },
        Factor {
            name: "Ride-day wetness",
            note: wet_note,
            contribution: wet_q * 2.0 - 1.0,
            quality: wet_q,
        },
        Factor {
            name: "Soil moisture",
            note: soil_note,
            contribution: (soil_q * 2.0 - 1.0) * 0.4,
            quality: soil_q,
        },
    ];

    // Recompute pack for return; factors already set.
    let _ = pack;
    let pack = (0.40 * amount_q + 0.35 * timing_q + 0.20 * wet_q + 0.05 * soil_q).clamp(0.0, 1.0);
    (pack, factors)
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
    let wind_q = if wind <= p.wind_ok {
        1.0
    } else if wind >= p.wind_bad {
        0.2
    } else {
        lerp(
            1.0,
            0.2,
            ((wind - p.wind_ok) / (p.wind_bad - p.wind_ok)).clamp(0.0, 1.0),
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
            note: format!("{:.0}% max precip probability", day.precip_prob_max),
            contribution: sky_q * 2.0 - 1.0,
            quality: sky_q,
        },
    ];

    (wx, factors)
}

fn confidence(date: NaiveDate, today: NaiveDate) -> (f64, Factor) {
    let days_out = (date - today).num_days().max(0) as f64;
    // Full confidence today–day 3, then taper to ~0.45 by day 10.
    let q = if days_out <= 3.0 {
        1.0
    } else {
        lerp(1.0, 0.45, ((days_out - 3.0) / 7.0).clamp(0.0, 1.0))
    };

    let note = if days_out <= 1.0 {
        "near-term forecast".into()
    } else if days_out <= 4.0 {
        format!("+{days_out:.0} days out — solid confidence")
    } else {
        format!("+{days_out:.0} days out — lower confidence")
    };

    (
        q,
        Factor {
            name: "Forecast confidence",
            note,
            contribution: q * 2.0 - 1.0,
            quality: q,
        },
    )
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

fn score_to_stars(score: f64) -> u8 {
    if score >= 0.85 {
        5
    } else if score >= 0.70 {
        4
    } else if score >= 0.50 {
        3
    } else if score >= 0.30 {
        2
    } else {
        1
    }
}

fn make_blurb(day: &DayWeather, pack_q: f64, factors: &[Factor]) -> String {
    if day.precip_in >= 0.25 {
        return format!("wet day · {:.2} in rain", day.precip_in);
    }
    if pack_q >= 0.7 {
        return "firm sand window".into();
    }
    if pack_q <= 0.35 {
        return "likely soft sand".into();
    }
    // Fall back to strongest named factor note snippet.
    factors
        .first()
        .map(|f| f.note.clone())
        .unwrap_or_else(|| format!("high {:.0}°F", day.temp_max_f))
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
        DayWeather {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            precip_in: precip,
            precip_prob_max: if precip > 0.2 { 70.0 } else { 10.0 },
            temp_max_f: high,
            temp_min_f: high - 15.0,
            apparent_max_f: high + 2.0,
            wind_max_mph: 8.0,
            gust_max_mph: 14.0,
            soil_moisture: Some(0.18),
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
        let d3 = scored.iter().find(|d| d.date == today).unwrap();
        assert!(
            d3.stars >= 4,
            "expected high stars after rain, got {} (score {:.2})",
            d3.stars,
            d3.score
        );
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
            d.stars <= 3,
            "dry sand should not be great, got {} (score {:.2})",
            d.stars,
            d.score
        );
    }

    #[test]
    fn wet_ride_day_penalized() {
        let days = vec![
            day("2026-07-01", 1.0, 80.0),
            day("2026-07-02", 0.8, 78.0), // raining on ride day
        ];
        let today = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        let scored = score_days(&days, today, &Params::default());
        let d = &scored[0];
        assert!(
            d.stars <= 3,
            "wet ride day should be mediocre at best, got {} (score {:.2})",
            d.stars,
            d.score
        );
    }

    #[test]
    fn stars_mapping_boundaries() {
        assert_eq!(score_to_stars(0.9), 5);
        assert_eq!(score_to_stars(0.7), 4);
        assert_eq!(score_to_stars(0.5), 3);
        assert_eq!(score_to_stars(0.3), 2);
        assert_eq!(score_to_stars(0.1), 1);
    }
}
