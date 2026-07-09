use serde::Deserialize;

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
    pub rain_sum: Vec<Option<f64>>,
    pub precipitation_hours: Vec<Option<f64>>,
    pub precipitation_probability_max: Vec<Option<f64>>,
    pub temperature_2m_max: Vec<Option<f64>>,
    pub temperature_2m_min: Vec<Option<f64>>,
    pub apparent_temperature_max: Vec<Option<f64>>,
    pub wind_speed_10m_max: Vec<Option<f64>>,
    pub wind_gusts_10m_max: Vec<Option<f64>>,
    pub weather_code: Vec<Option<i32>>,
    pub et0_fao_evapotranspiration: Vec<Option<f64>>,
}

#[derive(Clone, Debug, Deserialize, serde::Serialize)]
pub struct HourlyBlock {
    pub time: Vec<String>,
    pub precipitation: Vec<Option<f64>>,
    /// GFS soil layers (shallow cm depths used by gfs_seamless).
    #[serde(default)]
    pub soil_moisture_0_to_10cm: Vec<Option<f64>>,
    #[serde(default)]
    pub soil_moisture_10_to_40cm: Vec<Option<f64>>,
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
    /// Mean shallow soil moisture for the day (m³/m³), if available.
    pub soil_moisture: Option<f64>,
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

            let soil = self.mean_soil_for_date(&self.daily.time[i]);

            out.push(DayWeather {
                date,
                precip_in: opt(self.daily.precipitation_sum.get(i)),
                precip_prob_max: opt(self.daily.precipitation_probability_max.get(i)),
                temp_max_f: opt(self.daily.temperature_2m_max.get(i)),
                temp_min_f: opt(self.daily.temperature_2m_min.get(i)),
                apparent_max_f: opt(self.daily.apparent_temperature_max.get(i)),
                wind_max_mph: opt(self.daily.wind_speed_10m_max.get(i)),
                gust_max_mph: opt(self.daily.wind_gusts_10m_max.get(i)),
                soil_moisture: soil,
            });
        }

        out
    }

    fn mean_soil_for_date(&self, date_str: &str) -> Option<f64> {
        let hourly = self.hourly.as_ref()?;
        let mut sum = 0.0;
        let mut count = 0u32;

        for (i, t) in hourly.time.iter().enumerate() {
            if !t.starts_with(date_str) {
                continue;
            }
            let s0 = hourly.soil_moisture_0_to_10cm.get(i).and_then(|v| *v);
            let s1 = hourly.soil_moisture_10_to_40cm.get(i).and_then(|v| *v);
            let val = match (s0, s1) {
                (Some(a), Some(b)) => (a * 0.65) + (b * 0.35),
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => continue,
            };
            sum += val;
            count += 1;
        }

        if count == 0 {
            None
        } else {
            Some(sum / f64::from(count))
        }
    }
}

fn opt(v: Option<&Option<f64>>) -> f64 {
    v.and_then(|x| *x).unwrap_or(0.0)
}
