//! jaycast - weather-informed MTB trail rideability forecasts

mod app;
pub mod gauges;
mod rain_feed;
pub mod score;
mod theme;
pub mod trails;
pub mod weather;
#[cfg(not(target_arch = "wasm32"))]
pub mod xweather;

pub use rain_feed::{apply_gauge_to_days, GaugeRain};
#[cfg(not(target_arch = "wasm32"))]
pub use rain_feed::load_gauge_from_file;

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| {
        view! { <app::App /> }
    });
}
