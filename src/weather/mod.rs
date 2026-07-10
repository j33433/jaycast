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
/// Forecast days to score and display.
pub const FORECAST_DAYS: u32 = 10;
/// Days shown in the timeline window at once.
pub const VIEW_DAYS: usize = 10;

const CACHE_TTL_SECS: i64 = 90 * 60; // 1.5 hours
const MODEL_PREF_KEY: &str = "jaycast:model-pref";

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
            WeatherModel::GfsSeamless => "jaycast:om:v6-gfs",
            WeatherModel::Ecmwf => "jaycast:om:v6-ecmwf",
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
    payload: ForecastResponse,
}

pub async fn fetch_forecast(model: WeatherModel) -> Result<ForecastResponse, String> {
    if let Some(cached) = load_cache(model) {
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

    save_cache(model, &payload);
    Ok(payload)
}

fn build_url(model: WeatherModel) -> String {
    let mut url = format!(
        "{}?latitude={LAT}&longitude={LON}\
         &timezone={TIMEZONE}\
         &past_days={PAST_DAYS}&forecast_days={FORECAST_DAYS}"
    , model.endpoint());

    if let Some(m) = model.models_param() {
        url.push_str(&format!("&models={m}"));
    }

    url.push_str(
        "&daily=precipitation_sum,rain_sum,precipitation_hours,\
         precipitation_probability_max,temperature_2m_max,temperature_2m_min,\
         apparent_temperature_max,wind_speed_10m_max,wind_gusts_10m_max,\
         weather_code,et0_fao_evapotranspiration",
    );
    url.push_str("&hourly=precipitation");
    url.push_str("&temperature_unit=fahrenheit&wind_speed_unit=mph&precipitation_unit=inch");
    url
}

fn load_cache(model: WeatherModel) -> Option<ForecastResponse> {
    let storage = window()?.local_storage().ok()??;
    let raw = storage.get_item(model.cache_key()).ok()??;
    let entry: CacheEntry = serde_json::from_str(&raw).ok()?;
    let now = chrono::Utc::now().timestamp();
    if now - entry.fetched_at > CACHE_TTL_SECS {
        return None;
    }
    Some(entry.payload)
}

fn save_cache(model: WeatherModel, payload: &ForecastResponse) {
    let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) else {
        return;
    };
    let entry = CacheEntry {
        fetched_at: chrono::Utc::now().timestamp(),
        payload: payload.clone(),
    };
    if let Ok(raw) = serde_json::to_string(&entry) {
        let _ = storage.set_item(model.cache_key(), &raw);
    }
}

pub fn clear_cache(model: WeatherModel) {
    if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.remove_item(model.cache_key());
    }
}
