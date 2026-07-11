//! Open-Meteo weather client for Camp Murphy.

mod types;

pub use types::*;

use gloo_net::http::Request;
use web_sys::window;

/// Camp Murphy MTB Trails, Jonathan Dickinson State Park
pub const LAT: f64 = 27.012260963502648;
pub const LON: f64 = -80.11081837620299;
pub const TIMEZONE: &str = "America/New_York";
pub const LOCATION_NAME: &str = "Camp Murphy MTB Trails";
pub const LOCATION_SUB: &str = "Jonathan Dickinson State Park, FL";

/// Past days of history (pack model lookback + browseable archive).
pub const PAST_DAYS: u32 = 30;
/// Forecast days to score and display (today + next 7).
pub const FORECAST_DAYS: u32 = 8;
/// Days shown in the timeline window at once (yesterday + today + next 7).
pub const VIEW_DAYS: usize = 9;

const CACHE_TTL_SECS: i64 = 90 * 60; // 1.5 hours
const MODEL_PREF_KEY: &str = "jaycast:model-pref";
const HISTORY_CACHE_KEY: &str = "jaycast:om:v1-history";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherModel {
    GfsSeamless,
    Ecmwf,
}

impl WeatherModel {
    pub fn label(self) -> &'static str {
        match self {
            WeatherModel::GfsSeamless => "NOAA GFS seamless (HRRR+GFS)",
            WeatherModel::Ecmwf => "ECMWF IFS HRES 9km",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            WeatherModel::GfsSeamless => "GFS",
            WeatherModel::Ecmwf => "ECMWF",
        }
    }

    fn endpoint(self) -> &'static str {
        match self {
            WeatherModel::GfsSeamless => "https://api.open-meteo.com/v1/gfs",
            WeatherModel::Ecmwf => "https://api.open-meteo.com/v1/ecmwf",
        }
    }

    fn models_param(self) -> Option<&'static str> {
        match self {
            WeatherModel::GfsSeamless => Some("gfs_seamless"),
            WeatherModel::Ecmwf => None,
        }
    }

    fn cache_key(self) -> &'static str {
        match self {
            WeatherModel::GfsSeamless => "jaycast:om:v8-gfs",
            WeatherModel::Ecmwf => "jaycast:om:v8-ecmwf",
        }
    }
}

/// Read saved model preference from localStorage (default GFS).
pub fn load_model_pref() -> WeatherModel {
    if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
        if let Ok(Some(val)) = storage.get_item(MODEL_PREF_KEY) {
            if val == "ecmwf" {
                return WeatherModel::Ecmwf;
            }
        }
    }
    WeatherModel::GfsSeamless
}

/// Persist model preference to localStorage.
pub fn save_model_pref(model: WeatherModel) {
    if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(MODEL_PREF_KEY, model.short().to_lowercase().as_str());
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    fetched_at: i64,
    /// Inclusive date range for historical analysis cache entries.
    start_date: Option<String>,
    end_date: Option<String>,
    payload: ForecastResponse,
}

pub async fn fetch_forecast(model: WeatherModel) -> Result<ForecastResponse, String> {
    if let Some(cached) = load_cache(model.cache_key(), None, None) {
        return Ok(cached);
    }

    let url = build_url(model);
    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.ok() {
        return Err(format!("Weather API returned HTTP {}", resp.status()));
    }

    let payload: ForecastResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse weather data: {e}"))?;

    save_cache(model.cache_key(), None, None, &payload);
    Ok(payload)
}

/// Fetch observation-informed ECMWF IFS analysis for completed days.
pub async fn fetch_historical_analysis(
    start: chrono::NaiveDate,
    end: chrono::NaiveDate,
) -> Result<ForecastResponse, String> {
    let start_s = start.to_string();
    let end_s = end.to_string();
    if let Some(cached) = load_cache(HISTORY_CACHE_KEY, Some(&start_s), Some(&end_s)) {
        return Ok(cached);
    }

    let resp = Request::get(&build_historical_url(start, end))
        .send()
        .await
        .map_err(|e| format!("Historical weather network error: {e}"))?;

    if !resp.ok() {
        return Err(format!(
            "Historical weather API returned HTTP {}",
            resp.status()
        ));
    }

    let payload: ForecastResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse historical weather data: {e}"))?;

    save_cache(HISTORY_CACHE_KEY, Some(&start_s), Some(&end_s), &payload);
    Ok(payload)
}

/// Completed days use historical analysis; today and future use the selected forecast model.
pub fn combine_history_and_forecast(
    mut history: Vec<DayWeather>,
    forecast: Vec<DayWeather>,
    today: chrono::NaiveDate,
) -> Vec<DayWeather> {
    history.retain(|day| day.date < today);
    history.extend(forecast.into_iter().filter(|day| day.date >= today));
    history.sort_by_key(|day| day.date);
    history
}

fn build_url(model: WeatherModel) -> String {
    // Completed days come from historical analysis, so only future days are needed here.
    let mut url = format!(
        "{}?latitude={LAT}&longitude={LON}\
         &timezone={TIMEZONE}\
         &past_days=1&forecast_days={FORECAST_DAYS}",
        model.endpoint()
    );

    if let Some(m) = model.models_param() {
        url.push_str(&format!("&models={m}"));
    }

    append_weather_fields(&mut url);
    url
}

/// Build an Open-Meteo request for a fixed local date range.
/// Used by the native analysis CLI to score a specific past or future day.
pub fn build_date_range_url(
    model: WeatherModel,
    start: chrono::NaiveDate,
    end: chrono::NaiveDate,
) -> String {
    let mut url = format!(
        "{}?latitude={LAT}&longitude={LON}&timezone={TIMEZONE}&start_date={start}&end_date={end}",
        model.endpoint()
    );

    if let Some(m) = model.models_param() {
        url.push_str(&format!("&models={m}"));
    }

    append_weather_fields(&mut url);
    url
}

/// Build an Open-Meteo historical analysis request for completed past days.
pub fn build_historical_url(start: chrono::NaiveDate, end: chrono::NaiveDate) -> String {
    let mut url = format!(
        "https://archive-api.open-meteo.com/v1/archive?latitude={LAT}&longitude={LON}\
         &timezone={TIMEZONE}&start_date={start}&end_date={end}&models=ecmwf_ifs"
    );
    append_weather_fields(&mut url);
    url
}

fn append_weather_fields(url: &mut String) {
    url.push_str(
        "&daily=precipitation_sum,precipitation_probability_max,\
         temperature_2m_max,temperature_2m_min,apparent_temperature_max,\
         wind_speed_10m_max,wind_gusts_10m_max,et0_fao_evapotranspiration",
    );
    url.push_str("&hourly=precipitation,cloud_cover");
    url.push_str("&temperature_unit=fahrenheit&wind_speed_unit=mph&precipitation_unit=inch");
}

fn load_cache(
    key: &str,
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Option<ForecastResponse> {
    let storage = window()?.local_storage().ok()??;
    let raw = storage.get_item(key).ok()??;
    let entry: CacheEntry = serde_json::from_str(&raw).ok()?;
    let now = chrono::Utc::now().timestamp();
    if now - entry.fetched_at > CACHE_TTL_SECS {
        return None;
    }
    if entry.start_date.as_deref() != start_date || entry.end_date.as_deref() != end_date {
        return None;
    }
    Some(entry.payload)
}

fn save_cache(
    key: &str,
    start_date: Option<&str>,
    end_date: Option<&str>,
    payload: &ForecastResponse,
) {
    let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) else {
        return;
    };
    let entry = CacheEntry {
        fetched_at: chrono::Utc::now().timestamp(),
        start_date: start_date.map(str::to_string),
        end_date: end_date.map(str::to_string),
        payload: payload.clone(),
    };
    if let Ok(raw) = serde_json::to_string(&entry) {
        let _ = storage.set_item(key, &raw);
    }
}

pub fn clear_cache(model: WeatherModel) {
    if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.remove_item(model.cache_key());
        let _ = storage.remove_item(HISTORY_CACHE_KEY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str, precip_in: f64) -> DayWeather {
        DayWeather {
            date: chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            precip_in,
            precip_prob_max: 0.0,
            temp_max_f: 0.0,
            temp_min_f: 0.0,
            apparent_max_f: 0.0,
            wind_max_mph: 0.0,
            gust_max_mph: 0.0,
            et0: 0.0,
            precip_ride_in: 0.0,
            precip_pm_in: 0.0,
            precip_3h_in: [0.0; 8],
            cloud_3h_pct: [0.0; 8],
        }
    }

    #[test]
    fn historical_analysis_replaces_completed_forecast_days() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 11).unwrap();
        let days = combine_history_and_forecast(
            vec![day("2026-07-09", 0.1), day("2026-07-10", 0.4)],
            vec![day("2026-07-10", 0.0), day("2026-07-11", 0.2)],
            today,
        );

        assert_eq!(days.len(), 3);
        assert_eq!(days[1].precip_in, 0.4);
        assert_eq!(days[2].precip_in, 0.2);
    }
}
