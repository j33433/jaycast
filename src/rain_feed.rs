//! Static Xweather gauge rain feed (`rain.json`) for day-card overlays.

use std::collections::HashMap;

use chrono::NaiveDate;
use gloo_net::http::Request;
use serde::Deserialize;

use crate::trails::Trail;

const FEED_URL: &str = "/jaycast/rain.json";

#[derive(Clone, Debug, Default)]
pub struct GaugeRain {
    /// trail slug → date → 24 hourly tip totals (inches)
    by_trail: HashMap<String, HashMap<NaiveDate, [f64; 24]>>,
}

impl GaugeRain {
    pub fn hourly(&self, trail: Trail, date: NaiveDate) -> Option<[f64; 24]> {
        self.by_trail
            .get(trail.slug())
            .and_then(|days| days.get(&date).copied())
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
}

/// Fetch `/jaycast/rain.json`. Missing or invalid feed yields empty gauge data.
pub async fn fetch_gauge_rain() -> GaugeRain {
    match fetch_inner().await {
        Ok(g) => g,
        Err(_) => GaugeRain::default(),
    }
}

async fn fetch_inner() -> Result<GaugeRain, String> {
    let resp = Request::get(FEED_URL)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let file: FeedFile = resp.json().await.map_err(|e| e.to_string())?;
    Ok(index_feed(file))
}

fn index_feed(file: FeedFile) -> GaugeRain {
    let mut by_trail = HashMap::new();
    for (slug, trail) in file.trails {
        let mut days: HashMap<NaiveDate, [f64; 24]> = HashMap::new();
        for station in trail.stations {
            for day in station.days {
                let Ok(date) = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d") else {
                    continue;
                };
                let tips = hourly_array(&day.hourly_tips_in);
                let entry = days.entry(date).or_insert([0.0; 24]);
                // Max across stations so a working gauge still shows rain.
                for (i, tip) in tips.iter().enumerate() {
                    if *tip > entry[i] {
                        entry[i] = *tip;
                    }
                }
            }
        }
        by_trail.insert(slug, days);
    }
    GaugeRain { by_trail }
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

    #[test]
    fn indexes_max_across_stations() {
        let file = FeedFile {
            trails: HashMap::from([(
                "markham".into(),
                TrailBlock {
                    stations: vec![
                        StationBlock {
                            days: vec![DayBlock {
                                date: "2026-07-18".into(),
                                hourly_tips_in: {
                                    let mut h = vec![0.0; 24];
                                    h[14] = 0.36;
                                    h
                                },
                            }],
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
                            }],
                        },
                    ],
                },
            )]),
        };
        let gauge = index_feed(file);
        let tips = gauge
            .hourly(Trail::Markham, NaiveDate::from_ymd_opt(2026, 7, 18).unwrap())
            .unwrap();
        assert!((tips[11] - 0.24).abs() < 1e-9);
        assert!((tips[14] - 0.36).abs() < 1e-9);
    }
}
