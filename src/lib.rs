//! jaycast — rideability forecasts for sandy dune MTB trails

mod app;
mod score;
mod weather;

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| {
        view! { <app::App /> }
    });
}
