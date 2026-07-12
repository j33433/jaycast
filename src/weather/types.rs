use serde::Deserialize;

/// Rain during the usual 8 AM-noon ride window affects rideability directly.
const RIDE_START_HOUR: u32 = 8;
const RIDE_END_HOUR: u32 = 12;
/// Park closes at sundown. Florida summer sunset is ~8 PM local; hours from this
/// point onward are after close and do not affect ride-day wetness scoring.
const PARK_CLOSE_HOUR: u32 = 20;
const HOURS_PER_DAY: usize = 24;
const THREE_HOUR_BUCKETS: usize = 8;

#[derive(Clone, Debug, Deserialize, serde::Serialize)]
pub struct ForecastResponse {
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: Option<String>,
    pub daily: DailyBlock,
    pub hourly: Option<HourlyBlock>,
}

#[derive(Clone, Debug, Deserialize, serde::Serialize)]
pub struct DailyBlock {
    pub time: Vec<String>,
    pub precipitation_sum: Vec<Option<f64>>,
    pub precipitation_probability_max: Vec<Option<f64>>,
    pub temperature_2m_max: Vec<Option<f64>>,
    pub temperature_2m_min: Vec<Option<f64>>,
    pub apparent_temperature_max: Vec<Option<f64>>,
    pub wind_speed_10m_max: Vec<Option<f64>>,
    pub wind_gusts_10m_max: Vec<Option<f64>>,
    pub et0_fao_evapotranspiration: Vec<Option<f64>>,
}

#[derive(Clone, Debug, Deserialize, serde::Serialize)]
pub struct HourlyBlock {
    pub time: Vec<String>,
    pub precipitation: Vec<Option<f64>>,
    /// Missing on some historical analysis series; treat as empty.
    #[serde(default)]
    pub precipitation_probability: Vec<Option<f64>>,
    pub cloud_cover: Vec<Option<f64>>,
}

/// One calendar day of inputs used by the scorer.
#[derive(Clone, Debug)]
pub struct DayWeather {
    pub date: chrono::NaiveDate,
    pub precip_in: f64,
    /// Daily maximum precip probability (%).
    pub precip_prob_max: f64,
    /// Maximum precip probability during the 8 AM-noon ride window (%).
    pub precip_prob_ride_max: f64,
    pub temp_max_f: f64,
    pub temp_min_f: f64,
    pub apparent_max_f: f64,
    pub wind_max_mph: f64,
    pub gust_max_mph: f64,
    /// Reference evapotranspiration for the day (inches). Drying-rate proxy:
    /// high under sun, low under cloud.
    pub et0: f64,
    /// Precip falling during the 8 AM-noon ride window, inches.
    pub precip_ride_in: f64,
    /// Precip while the park is still open after noon (noon until sundown), inches.
    pub precip_pm_in: f64,
    /// Rainfall in each local hour. Used for Markham's timed drainage model.
    pub precip_hourly_in: [f64; HOURS_PER_DAY],
    /// Rainfall in each three-hour period, from midnight through 9 PM.
    pub precip_3h_in: [f64; THREE_HOUR_BUCKETS],
    /// Average cloud cover in each three-hour period, from midnight through 9 PM.
    pub cloud_3h_pct: [f64; THREE_HOUR_BUCKETS],
}

impl ForecastResponse {
    pub fn days(&self) -> Vec<DayWeather> {
        let n = self.daily.time.len();
        let mut out = Vec::with_capacity(n);

        for i in 0..n {
            let date = match chrono::NaiveDate::parse_from_str(&self.daily.time[i], "%Y-%m-%d") {
                Ok(d) => d,
                Err(_) => continue,
            };

            let (precip_ride_in, precip_pm_in) = self.precip_windows_for_date(&self.daily.time[i]);
            let precip_prob_max = opt(self.daily.precipitation_probability_max.get(i));
            let precip_prob_ride_max =
                self.prob_ride_max_for_date(&self.daily.time[i], precip_prob_max);
            let (precip_3h_in, cloud_3h_pct) =
                self.three_hour_weather_for_date(&self.daily.time[i]);
            let precip_hourly_in = self.hourly_precip_for_date(&self.daily.time[i]);

            out.push(DayWeather {
                date,
                precip_in: opt(self.daily.precipitation_sum.get(i)),
                precip_prob_max,
                precip_prob_ride_max,
                temp_max_f: opt(self.daily.temperature_2m_max.get(i)),
                temp_min_f: opt(self.daily.temperature_2m_min.get(i)),
                apparent_max_f: opt(self.daily.apparent_temperature_max.get(i)),
                wind_max_mph: opt(self.daily.wind_speed_10m_max.get(i)),
                gust_max_mph: opt(self.daily.wind_gusts_10m_max.get(i)),
                et0: opt(self.daily.et0_fao_evapotranspiration.get(i)),
                precip_ride_in,
                precip_pm_in,
                precip_hourly_in,
                precip_3h_in,
                cloud_3h_pct,
            });
        }

        out
    }

    fn hourly_precip_for_date(&self, date_str: &str) -> [f64; HOURS_PER_DAY] {
        let Some(hourly) = self.hourly.as_ref() else {
            return [0.0; HOURS_PER_DAY];
        };
        let mut rain = [0.0; HOURS_PER_DAY];
        for (i, timestamp) in hourly.time.iter().enumerate() {
            if !timestamp.starts_with(date_str) {
                continue;
            }
            let Some(hour) = hour_of(timestamp) else {
                continue;
            };
            if let Some(value) = hourly.precipitation.get(i).and_then(|value| *value) {
                if let Some(total) = rain.get_mut(hour as usize) {
                    *total += value;
                }
            }
        }
        rain
    }

    /// Sum hourly precip for a date into (morning ride window, open afternoon).
    /// Hourly timestamps are local (timezone param set), so the hour is read
    /// directly from the `THH` portion of the ISO string.
    /// Rain after park close (sundown) is omitted from both totals.
    fn precip_windows_for_date(&self, date_str: &str) -> (f64, f64) {
        let Some(hourly) = self.hourly.as_ref() else {
            return (0.0, 0.0);
        };
        let mut ride = 0.0;
        let mut pm = 0.0;

        for (i, t) in hourly.time.iter().enumerate() {
            if !t.starts_with(date_str) {
                continue;
            }
            let Some(p) = hourly.precipitation.get(i).and_then(|v| *v) else {
                continue;
            };
            match hour_of(t) {
                Some(h) if (RIDE_START_HOUR..RIDE_END_HOUR).contains(&h) => ride += p,
                Some(h) if (RIDE_END_HOUR..PARK_CLOSE_HOUR).contains(&h) => pm += p,
                Some(_) => {}
                None => {}
            }
        }

        (ride, pm)
    }

    /// Max hourly precip probability in the 8 AM-noon ride window.
    /// Falls back to the daily max when hourly probability is unavailable.
    fn prob_ride_max_for_date(&self, date_str: &str, daily_fallback: f64) -> f64 {
        let Some(hourly) = self.hourly.as_ref() else {
            return daily_fallback;
        };
        if hourly.precipitation_probability.is_empty() {
            return daily_fallback;
        }

        let mut max_prob = None;
        for (i, t) in hourly.time.iter().enumerate() {
            if !t.starts_with(date_str) {
                continue;
            }
            let Some(h) = hour_of(t) else {
                continue;
            };
            if !(RIDE_START_HOUR..RIDE_END_HOUR).contains(&h) {
                continue;
            }
            if let Some(p) = hourly.precipitation_probability.get(i).and_then(|v| *v) {
                max_prob = Some(max_prob.map_or(p, |m: f64| m.max(p)));
            }
        }
        max_prob.unwrap_or(daily_fallback)
    }

    /// Summarize each three-hour period for the timeline background curves.
    fn three_hour_weather_for_date(
        &self,
        date_str: &str,
    ) -> ([f64; THREE_HOUR_BUCKETS], [f64; THREE_HOUR_BUCKETS]) {
        let Some(hourly) = self.hourly.as_ref() else {
            return ([0.0; THREE_HOUR_BUCKETS], [0.0; THREE_HOUR_BUCKETS]);
        };
        let mut rain = [0.0; THREE_HOUR_BUCKETS];
        let mut cloud_total = [0.0; THREE_HOUR_BUCKETS];
        let mut cloud_count = [0u32; THREE_HOUR_BUCKETS];

        for (i, t) in hourly.time.iter().enumerate() {
            if !t.starts_with(date_str) {
                continue;
            }
            let Some(hour) = hour_of(t) else {
                continue;
            };
            let bucket = (hour / 3) as usize;
            if bucket >= THREE_HOUR_BUCKETS {
                continue;
            }

            rain[bucket] += hourly.precipitation.get(i).and_then(|v| *v).unwrap_or(0.0);
            if let Some(cloud_cover) = hourly.cloud_cover.get(i).and_then(|v| *v) {
                cloud_total[bucket] += cloud_cover;
                cloud_count[bucket] += 1;
            }
        }

        let mut cloud = [0.0; THREE_HOUR_BUCKETS];
        for bucket in 0..THREE_HOUR_BUCKETS {
            if cloud_count[bucket] > 0 {
                cloud[bucket] = cloud_total[bucket] / f64::from(cloud_count[bucket]);
            }
        }
        (rain, cloud)
    }
}

/// Parse the local hour from an ISO8601 local timestamp like `2026-07-10T13:00`.
fn hour_of(ts: &str) -> Option<u32> {
    let time_part = ts.split('T').nth(1)?;
    let hh = time_part.split(':').next()?;
    hh.parse::<u32>().ok()
}

fn opt(v: Option<&Option<f64>>) -> f64 {
    v.and_then(|x| *x).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rain_windows_start_when_the_park_opens() {
        let response = ForecastResponse {
            latitude: 0.0,
            longitude: 0.0,
            timezone: None,
            daily: DailyBlock {
                time: vec!["2026-07-11".into()],
                precipitation_sum: vec![Some(0.35)],
                precipitation_probability_max: vec![],
                temperature_2m_max: vec![],
                temperature_2m_min: vec![],
                apparent_temperature_max: vec![],
                wind_speed_10m_max: vec![],
                wind_gusts_10m_max: vec![],
                et0_fao_evapotranspiration: vec![],
            },
            hourly: Some(HourlyBlock {
                time: vec![
                    "2026-07-11T07:00".into(),
                    "2026-07-11T08:00".into(),
                    "2026-07-11T11:00".into(),
                    "2026-07-11T12:00".into(),
                    "2026-07-11T19:00".into(),
                    "2026-07-11T20:00".into(),
                ],
                precipitation: vec![
                    Some(0.20),
                    Some(0.01),
                    Some(0.02),
                    Some(0.04),
                    Some(0.05),
                    Some(0.50),
                ],
                precipitation_probability: vec![
                    Some(10.0),
                    Some(40.0),
                    Some(55.0),
                    Some(80.0),
                    Some(70.0),
                    Some(90.0),
                ],
                cloud_cover: vec![Some(0.0); 6],
            }),
        };

        let day = response.days().pop().unwrap();
        assert!((day.precip_ride_in - 0.03).abs() < 1e-9);
        // Noon + 7 PM count; 8 PM is after sundown close and is ignored.
        assert!((day.precip_pm_in - 0.09).abs() < 1e-9);
        assert!((day.precip_hourly_in[8] - 0.01).abs() < 1e-9);
        // Ride-window chance is max of 8 AM and 11 AM, not noon/evening peaks.
        assert!((day.precip_prob_ride_max - 55.0).abs() < 1e-9);
    }
}
