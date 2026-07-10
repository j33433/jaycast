use serde::Deserialize;

/// Local hour (0-23) that splits morning rain (penalized) from afternoon.
const AM_RAIN_CUTOFF_HOUR: u32 = 12;

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
}

/// One calendar day of inputs used by the scorer.
#[derive(Clone, Debug)]
pub struct DayWeather {
    pub date: chrono::NaiveDate,
    pub precip_in: f64,
    pub precip_prob_max: f64,
    pub temp_max_f: f64,
    pub temp_min_f: f64,
    pub apparent_max_f: f64,
    pub wind_max_mph: f64,
    pub gust_max_mph: f64,
    /// Reference evapotranspiration for the day (inches). Drying-rate proxy:
    /// high under sun, low under cloud.
    pub et0: f64,
    /// Precip falling in the morning ride window (before noon local), inches.
    pub precip_am_in: f64,
    /// Precip falling in the afternoon (noon local onward), inches.
    pub precip_pm_in: f64,
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

            let (precip_am_in, precip_pm_in) = self.precip_split_for_date(&self.daily.time[i]);

            out.push(DayWeather {
                date,
                precip_in: opt(self.daily.precipitation_sum.get(i)),
                precip_prob_max: opt(self.daily.precipitation_probability_max.get(i)),
                temp_max_f: opt(self.daily.temperature_2m_max.get(i)),
                temp_min_f: opt(self.daily.temperature_2m_min.get(i)),
                apparent_max_f: opt(self.daily.apparent_temperature_max.get(i)),
                wind_max_mph: opt(self.daily.wind_speed_10m_max.get(i)),
                gust_max_mph: opt(self.daily.wind_gusts_10m_max.get(i)),
                et0: opt(self.daily.et0_fao_evapotranspiration.get(i)),
                precip_am_in,
                precip_pm_in,
            });
        }

        out
    }

    /// Sum hourly precip for a date into (morning, afternoon) inches.
    /// Hourly timestamps are local (timezone param set), so the hour is read
    /// directly from the `THH` portion of the ISO string.
    fn precip_split_for_date(&self, date_str: &str) -> (f64, f64) {
        let Some(hourly) = self.hourly.as_ref() else {
            return (0.0, 0.0);
        };
        let mut am = 0.0;
        let mut pm = 0.0;

        for (i, t) in hourly.time.iter().enumerate() {
            if !t.starts_with(date_str) {
                continue;
            }
            let Some(p) = hourly.precipitation.get(i).and_then(|v| *v) else {
                continue;
            };
            match hour_of(t) {
                Some(h) if h < AM_RAIN_CUTOFF_HOUR => am += p,
                Some(_) => pm += p,
                None => pm += p,
            }
        }

        (am, pm)
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
