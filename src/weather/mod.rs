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

/// NOAA seamless stack via Open-Meteo (HRRR short-range + GFS longer range).
pub const WEATHER_MODEL: &str = "gfs_seamless";
pub const WEATHER_SOURCE_LABEL: &str = "NOAA GFS seamless (HRRR+GFS) via Open-Meteo";

const CACHE_KEY: &str = "jaycast:open-meteo:v3-gfs-seamless";
const CACHE_TTL_SECS: i64 = 90 * 60; // 1.5 hours

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    fetched_at: i64,
    payload: ForecastResponse,
}

pub async fn fetch_forecast() -> Result<ForecastResponse, String> {
    if let Some(cached) = load_cache() {
        return Ok(cached);
    }

    let url = build_url();
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

    save_cache(&payload);
    Ok(payload)
}

fn build_url() -> String {
    // /v1/gfs with gfs_seamless pins CONUS NOAA products (HRRR near-term + GFS out).
    format!(
        "https://api.open-meteo.com/v1/gfs?\
         latitude={LAT}&longitude={LON}\
         &timezone={TIMEZONE}\
         &past_days={PAST_DAYS}&forecast_days={FORECAST_DAYS}\
         &models={WEATHER_MODEL}\
         &daily=precipitation_sum,rain_sum,precipitation_hours,\
         precipitation_probability_max,temperature_2m_max,temperature_2m_min,\
         apparent_temperature_max,wind_speed_10m_max,wind_gusts_10m_max,\
         weather_code,et0_fao_evapotranspiration\
         &hourly=precipitation,soil_moisture_0_to_10cm,soil_moisture_10_to_40cm\
         &temperature_unit=fahrenheit&wind_speed_unit=mph&precipitation_unit=inch"
    )
}

fn load_cache() -> Option<ForecastResponse> {
    let storage = window()?.local_storage().ok()??;
    let raw = storage.get_item(CACHE_KEY).ok()??;
    let entry: CacheEntry = serde_json::from_str(&raw).ok()?;
    let now = chrono::Utc::now().timestamp();
    if now - entry.fetched_at > CACHE_TTL_SECS {
        return None;
    }
    Some(entry.payload)
}

fn save_cache(payload: &ForecastResponse) {
    let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) else {
        return;
    };
    let entry = CacheEntry {
        fetched_at: chrono::Utc::now().timestamp(),
        payload: payload.clone(),
    };
    if let Ok(raw) = serde_json::to_string(&entry) {
        let _ = storage.set_item(CACHE_KEY, &raw);
    }
}

pub fn clear_cache() {
    if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.remove_item(CACHE_KEY);
    }
}
