//! Static Xweather gauge rain feed (`rain.json`) for day-card overlays
//! and past-hour rain replacement in scoring.

use std::collections::HashMap;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
use gloo_net::http::Request;
use serde::Deserialize;

use crate::trails::Trail;
use crate::weather::DayWeather;

const FEED_URL: &str = "/jaycast/rain.json";

/// Replace model precip with gauge tips for completed hours, then recompute
/// daily/window totals. No-op when the feed is unusable for `trail`.
///
/// - Past days: all 24 hours from gauge when present.
/// - Today: hours strictly before `current_hour` (previous complete hour).
/// - Future days: unchanged.
pub fn apply_gauge_to_days(
    days: &mut [DayWeather],
    gauge: &GaugeRain,
    trail: Trail,
    today: NaiveDate,
    current_hour: u32,
) {
    if !gauge.usable_for(trail) {
        return;
    }
    let hour_cap = (current_hour as usize).min(24);
    for day in days.iter_mut() {
        let Some(tips) = gauge.hourly(trail, day.date) else {
            continue;
        };
        if day.date < today {
            day.precip_hourly_in = tips;
            day.recompute_precip_from_hourly();
        } else if day.date == today && hour_cap > 0 {
            day.precip_hourly_in[..hour_cap].copy_from_slice(&tips[..hour_cap]);
            day.recompute_precip_from_hourly();
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GaugeRain {
    /// trail slug → date → 24 hourly tip totals (inches)
    by_trail: HashMap<String, HashMap<NaiveDate, [f64; 24]>>,
    /// trail slug → true when every station's today day is stale or missing
    today_all_stale: HashMap<String, bool>,
    /// trail slug → freshest today `last_ob` unix timestamp across stations
    last_ob_ts: HashMap<String, i64>,
    /// True when the HTTP fetch and parse succeeded (even if all gauges stale).
    loaded: bool,
}

impl GaugeRain {
    pub fn hourly(&self, trail: Trail, date: NaiveDate) -> Option<[f64; 24]> {
        if !self.usable_for(trail) {
            return None;
        }
        self.by_trail
            .get(trail.slug())
            .and_then(|days| days.get(&date).copied())
    }

    /// Feed is usable when loaded and at least one of today's stations is fresh.
    pub fn usable_for(&self, trail: Trail) -> bool {
        self.loaded && !self.today_all_stale.get(trail.slug()).copied().unwrap_or(true)
    }

    /// Loaded but unusable for this trail (footer should warn).
    pub fn stale_for(&self, trail: Trail) -> bool {
        self.loaded && self.today_all_stale.get(trail.slug()).copied().unwrap_or(true)
    }

    /// Seconds since the freshest today observation for this trail, if known.
    pub fn last_seen_secs_ago(&self, trail: Trail, now_ts: i64) -> Option<i64> {
        let ts = *self.last_ob_ts.get(trail.slug())?;
        Some((now_ts - ts).max(0))
    }
}

#[derive(Debug, Deserialize)]
struct FeedFile {
    trails: HashMap<String, TrailBlock>,
}

#[derive(Debug, Deserialize)]
struct TrailBlock {
    stations: Vec<StationBlock>,
}

#[derive(Debug, Deserialize)]
struct StationBlock {
    days: Vec<DayBlock>,
}

#[derive(Debug, Deserialize)]
struct DayBlock {
    date: String,
    hourly_tips_in: Vec<f64>,
    #[serde(default)]
    last_ob: Option<String>,
    #[serde(default)]
    stale: bool,
}

/// Fetch `/jaycast/rain.json`. Missing or invalid feed yields empty gauge data.
pub async fn fetch_gauge_rain(today: NaiveDate) -> GaugeRain {
    match fetch_inner(today).await {
        Ok(g) => g,
        Err(_) => GaugeRain::default(),
    }
}

async fn fetch_inner(today: NaiveDate) -> Result<GaugeRain, String> {
    let resp = Request::get(FEED_URL)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let file: FeedFile = resp.json().await.map_err(|e| e.to_string())?;
    Ok(index_feed(file, today))
}

fn index_feed(file: FeedFile, today: NaiveDate) -> GaugeRain {
    let mut by_trail = HashMap::new();
    let mut today_all_stale = HashMap::new();
    let mut last_ob_ts = HashMap::new();
    let today_s = today.to_string();

    for (slug, trail) in file.trails {
        let mut days: HashMap<NaiveDate, [f64; 24]> = HashMap::new();
        let mut today_fresh = false;
        let mut saw_today = false;
        let mut freshest_ob: Option<i64> = None;

        for station in &trail.stations {
            let today_block = station.days.iter().find(|d| d.date == today_s);
            if let Some(block) = today_block {
                saw_today = true;
                if !block.stale {
                    today_fresh = true;
                }
                if let Some(ts) = block.last_ob.as_deref().and_then(parse_iso_timestamp) {
                    freshest_ob = Some(freshest_ob.map_or(ts, |prev| prev.max(ts)));
                }
            }
        }

        // Past days: max across all stations. Today: max across non-stale only
        // when any station is fresh; otherwise leave today empty (unusable).
        for station in trail.stations {
            for day in station.days {
                let Ok(date) = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d") else {
                    continue;
                };
                if date == today && day.stale {
                    continue;
                }
                let tips = hourly_array(&day.hourly_tips_in);
                let entry = days.entry(date).or_insert([0.0; 24]);
                for (i, tip) in tips.iter().enumerate() {
                    if *tip > entry[i] {
                        entry[i] = *tip;
                    }
                }
            }
        }

        let all_stale = !saw_today || !today_fresh;
        today_all_stale.insert(slug.clone(), all_stale);
        if let Some(ts) = freshest_ob {
            last_ob_ts.insert(slug.clone(), ts);
        }
        by_trail.insert(slug, days);
    }

    GaugeRain {
        by_trail,
        today_all_stale,
        last_ob_ts,
        loaded: true,
    }
}

fn parse_iso_timestamp(iso: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|dt| dt.timestamp())
        .or_else(|| {
            NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S")
                .ok()
                .and_then(|ndt| Local.from_local_datetime(&ndt).single())
                .map(|dt| dt.timestamp())
        })
}

/// Human age for footer: "3 minutes" / "1 hour" / "2 hours".
pub fn format_seen_ago(secs: i64) -> String {
    let secs = secs.max(0);
    if secs < 60 {
        return "just now".into();
    }
    let mins = secs / 60;
    if mins < 60 {
        if mins == 1 {
            "1 minute".into()
        } else {
            format!("{mins} minutes")
        }
    } else {
        let hours = mins / 60;
        if hours == 1 {
            "1 hour".into()
        } else {
            format!("{hours} hours")
        }
    }
}

fn hourly_array(values: &[f64]) -> [f64; 24] {
    let mut out = [0.0; 24];
    for (i, v) in values.iter().take(24).enumerate() {
        if v.is_finite() && *v > 0.0 {
            out[i] = *v;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str, hour: usize, tip: f64, stale: bool) -> DayBlock {
        day_ob(date, hour, tip, stale, None)
    }

    fn day_ob(
        date: &str,
        hour: usize,
        tip: f64,
        stale: bool,
        last_ob: Option<&str>,
    ) -> DayBlock {
        let mut h = vec![0.0; 24];
        h[hour] = tip;
        DayBlock {
            date: date.into(),
            hourly_tips_in: h,
            last_ob: last_ob.map(str::to_string),
            stale,
        }
    }

    #[test]
    fn indexes_max_across_stations() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 18).unwrap();
        let file = FeedFile {
            trails: HashMap::from([(
                "markham".into(),
                TrailBlock {
                    stations: vec![
                        StationBlock {
                            days: vec![day("2026-07-18", 14, 0.36, false)],
                        },
                        StationBlock {
                            days: vec![DayBlock {
                                date: "2026-07-18".into(),
                                hourly_tips_in: {
                                    let mut h = vec![0.0; 24];
                                    h[11] = 0.24;
                                    h[14] = 0.20;
                                    h
                                },
                                last_ob: None,
                                stale: false,
                            }],
                        },
                    ],
                },
            )]),
        };
        let gauge = index_feed(file, today);
        assert!(gauge.usable_for(Trail::Markham));
        let tips = gauge
            .hourly(Trail::Markham, NaiveDate::from_ymd_opt(2026, 7, 18).unwrap())
            .unwrap();
        assert!((tips[11] - 0.24).abs() < 1e-9);
        assert!((tips[14] - 0.36).abs() < 1e-9);
    }

    #[test]
    fn all_today_stale_is_unusable() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
        let file = FeedFile {
            trails: HashMap::from([(
                "markham".into(),
                TrailBlock {
                    stations: vec![
                        StationBlock {
                            days: vec![
                                day("2026-07-22", 15, 0.50, true),
                                day("2026-07-23", 8, 0.10, true),
                            ],
                        },
                        StationBlock {
                            days: vec![day("2026-07-23", 9, 0.20, true)],
                        },
                    ],
                },
            )]),
        };
        let gauge = index_feed(file, today);
        assert!(gauge.stale_for(Trail::Markham));
        assert!(!gauge.usable_for(Trail::Markham));
        assert!(gauge
            .hourly(Trail::Markham, NaiveDate::from_ymd_opt(2026, 7, 22).unwrap())
            .is_none());
        assert!(gauge
            .hourly(Trail::Markham, today)
            .is_none());
    }

    #[test]
    fn one_fresh_station_keeps_feed_usable() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
        let file = FeedFile {
            trails: HashMap::from([(
                "camp-murphy".into(),
                TrailBlock {
                    stations: vec![
                        StationBlock {
                            days: vec![
                                day("2026-07-22", 12, 0.40, true),
                                day("2026-07-23", 10, 0.05, true),
                            ],
                        },
                        StationBlock {
                            days: vec![
                                day("2026-07-22", 12, 0.30, true),
                                day("2026-07-23", 10, 0.12, false),
                            ],
                        },
                    ],
                },
            )]),
        };
        let gauge = index_feed(file, today);
        assert!(gauge.usable_for(Trail::CampMurphy));
        // Today: only non-stale station contributes.
        let today_tips = gauge.hourly(Trail::CampMurphy, today).unwrap();
        assert!((today_tips[10] - 0.12).abs() < 1e-9);
        // Past: max across all stations.
        let past = gauge
            .hourly(
                Trail::CampMurphy,
                NaiveDate::from_ymd_opt(2026, 7, 22).unwrap(),
            )
            .unwrap();
        assert!((past[12] - 0.40).abs() < 1e-9);
    }

    #[test]
    fn missing_today_is_stale() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
        let file = FeedFile {
            trails: HashMap::from([(
                "quiet-waters".into(),
                TrailBlock {
                    stations: vec![StationBlock {
                        days: vec![day("2026-07-22", 14, 0.25, true)],
                    }],
                },
            )]),
        };
        let gauge = index_feed(file, today);
        assert!(gauge.stale_for(Trail::QuietWaters));
        assert!(!gauge.usable_for(Trail::QuietWaters));
    }

    fn model_day(date: &str, hourly: [f64; 24]) -> DayWeather {
        let mut d = DayWeather {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            precip_in: 0.0,
            precip_prob_max: 0.0,
            precip_prob_ride_max: 0.0,
            temp_max_f: 0.0,
            temp_min_f: 0.0,
            apparent_max_f: 0.0,
            apparent_am_f: 0.0,
            apparent_pm_f: 0.0,
            wind_max_mph: 0.0,
            gust_max_mph: 0.0,
            et0: 0.0,
            precip_ride_in: 0.0,
            precip_pm_in: 0.0,
            precip_hourly_in: hourly,
            precip_3h_in: [0.0; 8],
            cloud_3h_pct: [0.0; 8],
        };
        d.recompute_precip_from_hourly();
        d
    }

    #[test]
    fn apply_gauge_replaces_past_and_completed_today_hours() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
        let mut past_tips = [0.0; 24];
        past_tips[14] = 0.50;
        let mut today_tips = [0.0; 24];
        today_tips[8] = 0.10;
        today_tips[10] = 0.20;
        today_tips[15] = 0.99; // incomplete hour — must not apply at current_hour=11

        let file = FeedFile {
            trails: HashMap::from([(
                "markham".into(),
                TrailBlock {
                    stations: vec![StationBlock {
                        days: vec![
                            DayBlock {
                                date: "2026-07-22".into(),
                                hourly_tips_in: past_tips.to_vec(),
                                last_ob: None,
                                stale: true,
                            },
                            DayBlock {
                                date: "2026-07-23".into(),
                                hourly_tips_in: today_tips.to_vec(),
                                last_ob: Some("2026-07-23T10:30:00-04:00".into()),
                                stale: false,
                            },
                        ],
                    }],
                },
            )]),
        };
        let gauge = index_feed(file, today);

        let mut days = vec![
            model_day("2026-07-22", [0.1; 24]),
            model_day("2026-07-23", [0.05; 24]),
            model_day("2026-07-24", [0.02; 24]),
        ];

        apply_gauge_to_days(&mut days, &gauge, Trail::Markham, today, 11);

        assert!((days[0].precip_hourly_in[14] - 0.50).abs() < 1e-9);
        assert!((days[0].precip_in - 0.50).abs() < 1e-9);
        assert!((days[1].precip_hourly_in[8] - 0.10).abs() < 1e-9);
        assert!((days[1].precip_hourly_in[10] - 0.20).abs() < 1e-9);
        // Hour 15 and current hour 11 stay model.
        assert!((days[1].precip_hourly_in[15] - 0.05).abs() < 1e-9);
        assert!((days[1].precip_hourly_in[11] - 0.05).abs() < 1e-9);
        assert!((days[2].precip_hourly_in[0] - 0.02).abs() < 1e-9);
    }

    #[test]
    fn apply_gauge_skips_when_stale() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
        let file = FeedFile {
            trails: HashMap::from([(
                "markham".into(),
                TrailBlock {
                    stations: vec![StationBlock {
                        days: vec![day("2026-07-23", 8, 0.50, true)],
                    }],
                },
            )]),
        };
        let gauge = index_feed(file, today);
        let mut days = vec![model_day("2026-07-23", [0.05; 24])];
        apply_gauge_to_days(&mut days, &gauge, Trail::Markham, today, 12);
        assert!((days[0].precip_hourly_in[8] - 0.05).abs() < 1e-9);
    }

    #[test]
    fn tracks_freshest_last_ob() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
        let file = FeedFile {
            trails: HashMap::from([(
                "markham".into(),
                TrailBlock {
                    stations: vec![
                        StationBlock {
                            days: vec![day_ob(
                                "2026-07-23",
                                8,
                                0.1,
                                false,
                                Some("2026-07-23T09:00:00-04:00"),
                            )],
                        },
                        StationBlock {
                            days: vec![day_ob(
                                "2026-07-23",
                                9,
                                0.1,
                                false,
                                Some("2026-07-23T09:25:00-04:00"),
                            )],
                        },
                    ],
                },
            )]),
        };
        let gauge = index_feed(file, today);
        let now = parse_iso_timestamp("2026-07-23T09:40:00-04:00").unwrap();
        assert_eq!(gauge.last_seen_secs_ago(Trail::Markham, now), Some(15 * 60));
    }

    #[test]
    fn format_seen_ago_units() {
        assert_eq!(format_seen_ago(30), "just now");
        assert_eq!(format_seen_ago(60), "1 minute");
        assert_eq!(format_seen_ago(5 * 60), "5 minutes");
        assert_eq!(format_seen_ago(60 * 60), "1 hour");
        assert_eq!(format_seen_ago(3 * 60 * 60), "3 hours");
    }
}
