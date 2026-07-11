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
    pub precip_prob_max: f64,
    pub blurb: String,
}

/// Score every day in the series. Antecedent rain uses earlier days in `days`.
/// `best` is set only among today and future days.
pub fn score_days(days: &[DayWeather], today: NaiveDate, params: &Params) -> Vec<DayForecast> {
    let mut forecasts = Vec::new();

    for (idx, _day) in days.iter().enumerate() {
        forecasts.push(score_one(days, idx, today, params));
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

    forecasts
}

fn score_one(days: &[DayWeather], idx: usize, today: NaiveDate, p: &Params) -> DayForecast {
    let day = &days[idx];

    let (pack_q, pack_factors) = pack_quality(days, idx, p);
    let (wx_q, wx_factors) = weather_quality(day, p);
    let (conf_q, conf_factor) = confidence(day.date, today);

    // Hard gate only for rain during the 8 AM-noon ride window. Overnight rain
    // has time to drain and should not be treated as riding in the rain.
    let wet_gate = if day.precip_ride_in >= p.ride_day_precip_hard {
        0.25
    } else if day.precip_ride_in > p.ride_day_precip_soft {
        let t = (day.precip_ride_in - p.ride_day_precip_soft)
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
        is_past: day.date < today,
        is_today: day.date == today,
        precip_in: day.precip_in,
        precip_3h_in: day.precip_3h_in,
        cloud_3h_pct: day.cloud_3h_pct,
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

    // Sun dries this shadeless sand fast; cloud slows it. Scale the drying clock
    // by ET0 accumulated since the rain: sunny stretches reach firm pack sooner
    // and dry out to soft faster; cloudy stretches keep the sand fresh longer.
    let dry_factor = drying_factor(days, idx, hours_since, p);
    let effective_hours = hours_since.map(|h| h * dry_factor);

    // Antecedent rain amount score: triangle around ideal.
    let amount_q = trap_score(
        antecedent,
        p.min_useful_rain_in * 0.5,
        p.min_useful_rain_in,
        p.ideal_antecedent_in,
        p.max_useful_rain_in,
    );

    // Timing: best near ideal_hours_since_rain, fades to 0 at pack_fade_hours.
    let timing_q = match effective_hours {
        None => 0.15,               // long dry spell / never — soft sand baseline
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

    // Rain during the ride. Wet ground is fine here (drains fast), so only rain
    // actually falling between 8 AM and noon is penalized; afternoon rain is
    // nearly ignored.
    let ride_rain = day.precip_ride_in + day.precip_pm_in * 0.15;
    let wet_q = if ride_rain <= p.ride_day_precip_soft {
        1.0 - (day.precip_prob_max / 100.0) * 0.1
    } else if ride_rain >= p.ride_day_precip_hard {
        0.1
    } else {
        let t = (ride_rain - p.ride_day_precip_soft)
            / (p.ride_day_precip_hard - p.ride_day_precip_soft);
        lerp(0.9, 0.1, t.clamp(0.0, 1.0))
    };

    // Combine pack sub-signals (soil moisture dropped — was modeled, not sensed).
    let pack = (0.45 * amount_q + 0.40 * timing_q + 0.15 * wet_q).clamp(0.0, 1.0);

    let timing_note = match effective_hours {
        None => "no significant rain in lookback — sand likely soft".into(),
        Some(h) if h < 12.0 => format!("~{h:.0}h drying since rain — still settling"),
        Some(h) if h <= 48.0 => format!("~{h:.0}h drying since rain — pack window"),
        Some(h) => format!("~{h:.0}h drying since rain — drying out"),
    };

    let amount_note = if antecedent < p.min_useful_rain_in {
        format!("{antecedent:.2} in prior rain (need more for firm pack)")
    } else if antecedent > p.max_useful_rain_in {
        format!("{antecedent:.2} in prior rain (heavy — may stay soft/puddled)")
    } else {
        format!(
            "{antecedent:.2} in rain in prior ~{:.0}h",
            p.pack_lookback_hours
        )
    };

    let wet_note = if day.precip_ride_in > p.ride_day_precip_soft {
        format!(
            "{:.2} in rain from 8 AM-noon ({:.0}% chance) — likely riding wet",
            day.precip_ride_in, day.precip_prob_max
        )
    } else if day.precip_pm_in > p.ride_day_precip_soft {
        format!(
            "{:.2} in afternoon rain — dry 8 AM-noon window",
            day.precip_pm_in
        )
    } else if day.precip_prob_max >= 40.0 {
        format!("{:.0}% rain chance, mostly dry", day.precip_prob_max)
    } else {
        format!("dry ride window ({:.0}% chance)", day.precip_prob_max)
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
            name: "Rain during ride",
            note: wet_note,
            contribution: wet_q * 2.0 - 1.0,
            quality: wet_q,
        },
    ];

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
            note: format!("{:.0}% max precip probability", day.precip_prob_max),
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
                name: "Data confidence",
                note: "observed weather (archive)".into(),
                contribution: 1.0,
                quality: 1.0,
            },
        );
    }

    let days_out = (date - today).num_days().max(0) as f64;
    // Full confidence today–day 3, then taper to ~0.45 by day 10.
    let q = if days_out <= 3.0 {
        1.0
    } else {
        lerp(1.0, 0.45, ((days_out - 3.0) / 7.0).clamp(0.0, 1.0))
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
            name: "Forecast confidence",
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
        // Default: precip split evenly-ish, sunny drying reference.
        DayWeather {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            precip_in: precip,
            precip_prob_max: if precip > 0.2 { 70.0 } else { 10.0 },
            temp_max_f: high,
            temp_min_f: high - 15.0,
            apparent_max_f: high + 2.0,
            wind_max_mph: 8.0,
            gust_max_mph: 14.0,
            et0: 0.20,
            // Assume rain falls in the afternoon by default (convective FL storms).
            precip_ride_in: 0.0,
            precip_pm_in: precip,
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
    fn stars_mapping_boundaries() {
        assert!((score_to_stars(1.0) - 5.0).abs() < 1e-9);
        assert!((score_to_stars(0.0) - 1.0).abs() < 1e-9);
        assert!((score_to_stars(0.875) - 4.5).abs() < 1e-9);
        assert!((score_to_stars(0.5) - 3.0).abs() < 1e-9);
    }
}
