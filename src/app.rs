use std::collections::HashMap;

use chrono::{Duration, Local, NaiveDate, Timelike, TimeZone};
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::window;

use crate::rain_feed::{self, GaugeRain};
use crate::score::{score_color, score_days_as_of, DayForecast, Params};
use crate::theme::{
    apply_theme, apply_theme_color, detect_os_theme, load_theme_pref, save_theme_pref, Theme,
};
use crate::trails::{self, Trail};
use crate::weather::{self, WeatherModel, VIEW_DAYS};

const SELECTED_KEY_PREFIX: &str = "jaycast:selected";
const WEEKEND_PREF_KEY: &str = "jaycast:weekend-warrior";

fn load_selected_pref(trail: Trail) -> Option<NaiveDate> {
    window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|storage| {
            storage
                .get_item(&format!("{SELECTED_KEY_PREFIX}:{}", trail.slug()))
                .ok()
                .flatten()
        })
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
}

fn save_selected_pref(trail: Trail, date: Option<NaiveDate>) {
    if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
        let key = format!("{SELECTED_KEY_PREFIX}:{}", trail.slug());
        match date {
            Some(d) => {
                let _ = storage.set_item(&key, &d.format("%Y-%m-%d").to_string());
            }
            None => {
                let _ = storage.remove_item(&key);
            }
        }
    }
}

fn load_weekend_pref() -> bool {
    window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(WEEKEND_PREF_KEY).ok().flatten())
        .map(|s| s == "1")
        .unwrap_or(false)
}

fn save_weekend_pref(active: bool) {
    if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(WEEKEND_PREF_KEY, if active { "1" } else { "0" });
    }
}

fn url_has_screenshot_flag() -> bool {
    window()
        .and_then(|w| w.location().search().ok())
        .map(|s| s.contains("screenshot"))
        .unwrap_or(false)
}

#[derive(Clone)]
enum LoadState {
    Loading,
    Ready(Vec<DayForecast>),
    Error(String),
}

#[component]
pub fn App() -> impl IntoView {
    let state = RwSignal::new(LoadState::Loading);
    let selected = RwSignal::new(Option::<NaiveDate>::None);
    let view_start = RwSignal::new(0usize);
    let refreshed_at = RwSignal::new(String::new());
    let model = RwSignal::new(weather::load_model_pref());
    let trail = RwSignal::new(trails::load_trail_pref());
    let location_dialog_open = RwSignal::new(false);
    let help_dialog_open = RwSignal::new(false);
    let grid_lat = RwSignal::new(0.0f64);
    let grid_lon = RwSignal::new(0.0f64);
    let theme = RwSignal::new(load_theme_pref().unwrap_or_else(detect_os_theme));
    let gauge_rain = RwSignal::new(GaugeRain::default());

    let is_first_load = RwSignal::new(true);
    let weekend_warrior = RwSignal::new(load_weekend_pref());
    let multi_days = RwSignal::new(Vec::<(Trail, Vec<DayForecast>)>::new());
    let multi_loading = RwSignal::new(false);

    Effect::new(move |_| {
        let t = theme.get();
        apply_theme(t);
        apply_theme_color(t);
    });

    Effect::new(move |_| {
        let t = trail.get();
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.set_title(&format!("{}cast · {} trail forecast", t.brand(), t.short_name()));
        }
    });

    let load = move || {
        let m = model.get_untracked();
        let t = trail.get_untracked();
        let first = is_first_load.get_untracked();
        state.set(LoadState::Loading);
        spawn_local(async move {
            let now = Local::now();
            let today = now.date_naive();
            let current_hour = now.hour();
            let gauge = rain_feed::fetch_gauge_rain(today).await;
            match weather::fetch_forecast(m, t).await {
                Ok((forecast, forecast_at)) => {
                    let history_start = today - Duration::days(weather::PAST_DAYS.into());
                    let history_end = today - Duration::days(1);
                    match weather::fetch_historical_analysis(m, history_start, history_end, t)
                        .await
                    {
                        Ok((history, _)) => {
                            if model.get_untracked() != m || trail.get_untracked() != t {
                                return;
                            }
                            grid_lat.set(forecast.latitude);
                            grid_lon.set(forecast.longitude);
                            let mut days = weather::combine_history_and_forecast(
                                history.days(),
                                forecast.days(),
                                today,
                            );
                            rain_feed::apply_gauge_to_days(
                                &mut days,
                                &gauge,
                                t,
                                today,
                                current_hour,
                            );
                            let scored = score_days_as_of(
                                &days,
                                today,
                                &Params::for_trail(t),
                                Some(current_hour),
                            );

                            let today_idx = scored
                                .iter()
                                .position(|d| d.is_today)
                                .or_else(|| scored.iter().position(|d| !d.is_past))
                                .unwrap_or(0);

                            if first {
                                // Open on yesterday so the completed day sits above today.
                                view_start.set(today_idx.saturating_sub(1));
                            }

                            let prev_sel = selected.get_untracked();
                            if first {
                                let saved = load_selected_pref(t);
                                if let Some(date) = saved {
                                    if scored.iter().any(|d| d.date == date) {
                                        selected.set(Some(date));
                                    }
                                }
                                if url_has_screenshot_flag() {
                                    if let Some(today_day) = scored.iter().find(|d| d.is_today) {
                                        selected.set(Some(today_day.date));
                                        view_start.set(today_idx);
                                    }
                                }
                            } else if prev_sel.is_some()
                                && !scored.iter().any(|d| Some(d.date) == prev_sel)
                            {
                                selected.set(None);
                            }

                            is_first_load.set(false);
                            gauge_rain.set(gauge);
                            let init_time = weather::fetch_model_init_time(m).await;
                            refreshed_at.set(format_weather_as_of(init_time, forecast_at));
                            state.set(LoadState::Ready(scored));
                        }
                        Err(e) => {
                            if model.get_untracked() == m && trail.get_untracked() == t {
                                state.set(LoadState::Error(e));
                            }
                        }
                    }
                }
                Err(e) => {
                    if model.get_untracked() == m && trail.get_untracked() == t {
                        state.set(LoadState::Error(e));
                    }
                }
            }
        });
    };

    let load_weekend = {
        let model = model;
        let multi_days = multi_days;
        let multi_loading = multi_loading;
        move || {
            let m = model.get_untracked();
            multi_loading.set(true);
            spawn_local(async move {
                let today = Local::now().date_naive();
                let current_hour = Local::now().hour();
                let history_start = today - Duration::days(weather::PAST_DAYS.into());
                let history_end = today - Duration::days(1);
                let gauge = rain_feed::fetch_gauge_rain(today).await;
                let mut results = Vec::with_capacity(Trail::ALL.len());
                for t in Trail::ALL {
                    let fc_result = weather::fetch_forecast(m, t).await;
                    let hist_result = weather::fetch_historical_analysis(m, history_start, history_end, t).await;
                    match (fc_result, hist_result) {
                        (Ok((fc, _)), Ok((hist, _))) => {
                            let mut days = weather::combine_history_and_forecast(
                                hist.days(),
                                fc.days(),
                                today,
                            );
                            rain_feed::apply_gauge_to_days(
                                &mut days,
                                &gauge,
                                t,
                                today,
                                current_hour,
                            );
                            let scored = score_days_as_of(
                                &days,
                                today,
                                &Params::for_trail(t),
                                Some(current_hour),
                            );
                            results.push((t, scored));
                        }
                        (Ok((fc, _)), Err(_)) => {
                            let days = fc.days();
                            let scored = score_days_as_of(
                                &days,
                                today,
                                &Params::for_trail(t),
                                Some(current_hour),
                            );
                            results.push((t, scored));
                        }
                        _ => {}
                    }
                }
                multi_days.set(results);
                multi_loading.set(false);
            });
        }
    };

    Effect::new(move |_| {
        load();
    });

    // Kick off weekend grid fetch on cold load if the toggle was left on.
    if load_weekend_pref() {
        load_weekend();
    }

    spawn_local(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(15 * 60 * 1000).await;
            load();
        }
    });

    let switch_model = {
        let model = model;
        let weekend_warrior = weekend_warrior;
        let load_weekend = load_weekend;
        move |new_model: WeatherModel| {
            if model.get_untracked() == new_model {
                return;
            }
            let refresh_grid = weekend_warrior.get_untracked();
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(0).await;
                weather::save_model_pref(new_model);
                model.set(new_model);
                load();
                if refresh_grid {
                    load_weekend();
                }
            });
        }
    };

    let switch_trail = move |new_trail: Trail| {
        spawn_local(async move {
            // Yield past the DOM event before changing signals that unmount its target.
            gloo_timers::future::TimeoutFuture::new(0).await;
            if trail.get_untracked() == new_trail {
                location_dialog_open.set(false);
                return;
            }

            // Dispose the dialog's reactive owner before changing `trail`. Otherwise
            // its selected-option class can be queued, then invoked after disposal.
            location_dialog_open.set(false);
            gloo_timers::future::TimeoutFuture::new(0).await;

            trails::save_trail_pref(new_trail);
            trails::update_trail_url(new_trail);
            let keep_date = selected.get_untracked();
            save_selected_pref(trail.get_untracked(), None);
            if let Some(date) = keep_date {
                save_selected_pref(new_trail, Some(date));
            }
            trail.set(new_trail);
            selected.set(None);
            view_start.set(0);
            weekend_warrior.set(false);
            save_weekend_pref(false);
            is_first_load.set(true);
            load();
        });
    };

    view! {
        <div id="app">
            <header class="header">
                <button
                    type="button"
                    class="logo-change"
                    aria-label="Change trail location"
                    title="Change trail location"
                    on:click=move |_| location_dialog_open.set(true)
                >
                    <img
                        class="trail-logo"
                        src=move || trail.get().icon_src()
                        width="161"
                        height="161"
                        alt=""
                    />
                </button>
                <div class="header-text">
                    <h1>{move || trail.get().brand()} <span>"cast"</span></h1>
                    <span class="tagline">{move || trail.get().tagline()}</span>
                    <p class="location">
                        {move || trail.get().name()}
                        <br/>
                        {move || trail.get().location()}
                    </p>
                    <div class="header-actions">
                        <button
                            type="button"
                            class="location-change"
                            on:click=move |_| location_dialog_open.set(true)
                        >
                            "change location"
                        </button>
                        <span class="header-action-sep" aria-hidden="true">"·"</span>
                        <button
                            type="button"
                            class="help-open"
                            on:click=move |_| help_dialog_open.set(true)
                        >
                            "help"
                        </button>
                    </div>
                </div>
            </header>

            <LocationDialog
                open=location_dialog_open
                selected=trail
                on_change=Callback::new(switch_trail)
            />
            <HelpDialog open=help_dialog_open />

            {move || match state.get() {
                LoadState::Loading => view! { <LoadingView /> }.into_any(),
                LoadState::Error(msg) => view! {
                    <ErrorView message=msg on_retry=Callback::new(move |_| {
                        weather::clear_cache_for_trail(model.get_untracked(), trail.get_untracked());
                        spawn_local(async move {
                            gloo_timers::future::TimeoutFuture::new(0).await;
                            load();
                        });
                    }) />
                }.into_any(),
                LoadState::Ready(days) => view! {
                    <ReadyView
                        days=days
                        selected=selected
                        view_start=view_start
                        refreshed_at=refreshed_at
                        model=model
                        trail=trail
                        grid_lat=grid_lat
                        grid_lon=grid_lon
                        theme=theme
                        gauge_rain=gauge_rain
                        weekend_warrior=weekend_warrior
                        multi_days=multi_days
                        multi_loading=multi_loading
                        on_switch=Callback::new(switch_model)
                        on_select_trail_with_date=Callback::new(move |(t, d): (Trail, NaiveDate)| {
                            spawn_local(async move {
                                gloo_timers::future::TimeoutFuture::new(0).await;
                                if trail.get_untracked() == t {
                                    location_dialog_open.set(false);
                                    return;
                                }
                                location_dialog_open.set(false);
                                gloo_timers::future::TimeoutFuture::new(0).await;
                                trails::save_trail_pref(t);
                                trails::update_trail_url(t);
                                save_selected_pref(trail.get_untracked(), None);
                                save_selected_pref(t, Some(d));
                                trail.set(t);
                                selected.set(None);
                                view_start.set(0);
                                weekend_warrior.set(false);
                                save_weekend_pref(false);
                                is_first_load.set(true);
                                load();
                            });
                        })
                        on_toggle_weekend=Callback::new(move |_| {
                            let was = weekend_warrior.get_untracked();
                            let next = !was;
                            save_weekend_pref(next);
                            weekend_warrior.set(next);
                            if next {
                                load_weekend();
                            }
                        })
                    />
                }.into_any(),
            }}
        </div>
    }
}

#[component]
fn LocationDialog(
    open: RwSignal<bool>,
    selected: RwSignal<Trail>,
    on_change: Callback<Trail>,
) -> impl IntoView {
    view! {
        {move || open.get().then(|| {
            view! {
                <div class="location-backdrop" role="presentation" on:click=move |_| open.set(false)>
                    <section
                        class="location-dialog"
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="location-dialog-title"
                        on:click=move |event| event.stop_propagation()
                    >
                        <div class="location-dialog-head">
                            <div>
                                <p class="label">"Trail location"</p>
                                <h2 id="location-dialog-title">"Choose a trail"</h2>
                            </div>
                            <button
                                type="button"
                                class="dialog-close"
                                aria-label="Close location chooser"
                                on:click=move |_| open.set(false)
                            >
                                <svg class="dialog-close-icon" viewBox="0 0 24 24" aria-hidden="true">
                                    <g stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                        <line x1="6" y1="6" x2="18" y2="18"/>
                                        <line x1="18" y1="6" x2="6" y2="18"/>
                                    </g>
                                </svg>
                            </button>
                        </div>
                        <div class="location-options">
                            {Trail::ALL.into_iter().map(|trail| {
                                let name = trail.name();
                                let location = trail.location();
                                let icon_src = trail.icon_src();
                                // This dialog closes when a trail is chosen, so the class
                                // must not subscribe to `selected`: a queued class update
                                // could otherwise outlive the dialog's reactive owner.
                                let class = if selected.get_untracked() == trail {
                                    "location-option selected"
                                } else {
                                    "location-option"
                                };
                                view! {
                                    <button
                                        type="button"
                                        class=class
                                        on:click=move |event| {
                                            // Keep the same click from reaching the dialog
                                            // backdrop while the trail change is deferred.
                                            event.stop_propagation();
                                            on_change.run(trail);
                                        }
                                    >
                                        <img class="location-icon" src=icon_src alt=""/>
                                        <span class="location-option-copy">
                                            <strong>{name}</strong>
                                            <span>{location}</span>
                                        </span>
                                    </button>
                                }
                            }).collect_view()}
                        </div>
                    </section>
                </div>
            }
        })}
    }
}

#[component]
fn HelpDialog(open: RwSignal<bool>) -> impl IntoView {
    // Static demo paths: clouds morning-left, forecast rain peaks ~4:30am, gauge PM.
    // 8×3h cloud %; pin callout at ~9am on the curve (same mapping as cloud_wave_path).
    let cloud_3h = [85.0, 92.0, 78.0, 55.0, 28.0, 12.0, 8.0, 6.0];
    let cloud_path = cloud_wave_path(&cloud_3h);
    let cloud_pin_hour = 9.0_f64;
    let cloud_pin_x = cloud_pin_hour / 24.0 * 100.0;
    // Interpolate cloud % along the 8 control points for that hour.
    let cloud_t = (cloud_pin_hour / 24.0) * (cloud_3h.len() - 1) as f64;
    let cloud_i = cloud_t.floor() as usize;
    let cloud_f = cloud_t - cloud_i as f64;
    let cloud_pct = if cloud_i + 1 < cloud_3h.len() {
        cloud_3h[cloud_i] * (1.0 - cloud_f) + cloud_3h[cloud_i + 1] * cloud_f
    } else {
        cloud_3h[cloud_3h.len() - 1]
    };
    // Clouds fill from the top; aim mid-fill (above the lower curve edge).
    let cloud_edge_y = (cloud_pct.clamp(0.0, 100.0) / 100.0) * 52.0;
    let cloud_pin_y = cloud_edge_y * 0.45;
    let cloud_pin_style = format!("--pin-x:{cloud_pin_x:.2}%; --pin-y:{cloud_pin_y:.2}%");
    // 8×3h buckets; crest near 4:30am. Same mapping as rain_wave_path.
    // Pin x is layout-driven in CSS (midway date→stars); y tracks the curve.
    let rain_3h = [0.05, 0.2, 0.12, 0.02, 0.0, 0.0, 0.0, 0.0];
    let rain_path = rain_wave_path(&rain_3h);
    let rain_pin_hour = 4.5_f64;
    let rain_t = (rain_pin_hour / 24.0) * (rain_3h.len() - 1) as f64;
    let rain_i = rain_t.floor() as usize;
    let rain_f = rain_t - rain_i as f64;
    let rain_in = if rain_i + 1 < rain_3h.len() {
        rain_3h[rain_i] * (1.0 - rain_f) + rain_3h[rain_i + 1] * rain_f
    } else {
        rain_3h[rain_3h.len() - 1]
    };
    let rain_pin_y = 100.0 - (rain_in.max(0.0) / 0.25).clamp(0.0, 1.0) * 54.0;
    let rain_pin_style = format!("--pin-y:{rain_pin_y:.2}%");
    // Hourly tips; peak at hour 15 (3pm). Same mapping as rain_gauge_wave_path.
    let gauge_hourly = [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.03, 0.07, 0.12, 0.18, 0.15,
        0.08, 0.03, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];
    let gauge_peak_i = 15usize;
    let gauge_peak_in = gauge_hourly[gauge_peak_i];
    let gauge_path = rain_gauge_wave_path(&gauge_hourly);
    let gauge_pin_x = gauge_peak_i as f64 * 100.0 / (gauge_hourly.len() - 1) as f64;
    let gauge_pin_y = 100.0 - (gauge_peak_in.max(0.0) / 0.25).clamp(0.0, 1.0) * 54.0;
    let gauge_pin_style = format!("--pin-x:{gauge_pin_x:.2}%; --pin-y:{gauge_pin_y:.2}%");
    let tint = day_card_style(0.72, Some(-4.0), Some(3.0));

    view! {
        {move || open.get().then(|| {
            let cloud_path = cloud_path.clone();
            let rain_path = rain_path.clone();
            let gauge_path = gauge_path.clone();
            let tint = tint.clone();
            let cloud_pin_style = cloud_pin_style.clone();
            let rain_pin_style = rain_pin_style.clone();
            let gauge_pin_style = gauge_pin_style.clone();
            view! {
                <div class="help-backdrop" role="presentation" on:click=move |_| open.set(false)>
                    <section
                        class="help-dialog"
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="help-dialog-title"
                        on:click=move |event| event.stop_propagation()
                    >
                        <div class="help-dialog-head">
                            <div>
                                <p class="label">"Guide"</p>
                                <h2 id="help-dialog-title">"How to read a day"</h2>
                            </div>
                            <button
                                type="button"
                                class="dialog-close"
                                aria-label="Close help"
                                on:click=move |_| open.set(false)
                            >
                                <svg class="dialog-close-icon" viewBox="0 0 24 24" aria-hidden="true">
                                    <g stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                        <line x1="6" y1="6" x2="18" y2="18"/>
                                        <line x1="18" y1="6" x2="6" y2="18"/>
                                    </g>
                                </svg>
                            </button>
                        </div>
                        <div class="help-intro">
                            <p>
                                "Weather-aware trail scores for South Florida mountain bike parks. "
                                "Not official trail status."
                            </p>
                            <p>
                                "Stars estimate rideability from rain, surface model, temp, and wind. "
                                "Each card is midnight to evening, left to right."
                            </p>
                            <p class="help-edge-note">
                                "Card edges show feels-like vs the prior week: "
                                <span class="help-edge-cool">"blue"</span>" is cooler"
                                ", "
                                <span class="help-edge-warm">"red"</span>" is warmer"
                                ". Left is morning, right is afternoon. "
                                "This example shows an unusually cool morning."
                            </p>
                            <p>"Tap a day to open the details panel."</p>
                        </div>
                        <div class="help-card-stage">
                            <div class="help-demo">
                                    <div class="help-demo-card-wrap">
                                    <div class="day-card today selected help-demo-card" style=tint.clone()>
                                        <svg
                                            class="cloud-wave"
                                            viewBox="0 0 100 100"
                                            preserveAspectRatio="none"
                                            aria-hidden="true"
                                            focusable="false"
                                        >
                                            <path d=cloud_path />
                                        </svg>
                                        <svg
                                            class="rain-wave"
                                            viewBox="0 0 100 100"
                                            preserveAspectRatio="none"
                                            aria-hidden="true"
                                            focusable="false"
                                        >
                                            <path d=rain_path />
                                        </svg>
                                        <svg
                                            class="rain-gauge"
                                            viewBox="0 0 100 100"
                                            preserveAspectRatio="none"
                                            aria-hidden="true"
                                            focusable="false"
                                        >
                                            <path d=gauge_path />
                                        </svg>
                                        <div class="hourly-ticks" aria-hidden="true">
                                            {(0u32..24).map(|h| {
                                                let left = format!("{:.2}%", h as f64 * 100.0 / 24.0);
                                                let tall = h % 3 == 0;
                                                let cls = if tall { "htick tall" } else { "htick" };
                                                let label = match h {
                                                    3 => Some("3a"),
                                                    6 => Some("6a"),
                                                    9 => Some("9a"),
                                                    12 => Some("12p"),
                                                    15 => Some("3p"),
                                                    18 => Some("6p"),
                                                    21 => Some("9p"),
                                                    _ => None,
                                                };
                                                view! {
                                                    <span class=cls style=format!("left:{left}")></span>
                                                    {label.map(|l| view! {
                                                        <span class="hlabel" style=format!("left:{left}")>{l}</span>
                                                    })}
                                                }
                                            }).collect_view()}
                                            <span class="now-marker" style="left:45.83%"></span>
                                        </div>
                                        <div class="date">
                                            "Jul 18"
                                            <span class="dow">"Today"</span>
                                        </div>
                                        <div class="mid">
                                            <div class="stars-sm">"4.2 ★"</div>
                                            <div class="blurb">"sandy, light PM rain"</div>
                                        </div>
                                        <div class="precip">
                                            "0.41\""
                                            <div class="temp-row">
                                                <span class="temp">"78° / 91°"</span>
                                            </div>
                                        </div>
                                    </div>
                                    <div
                                        class="help-anno help-anno-pin help-anno-cloud-pin"
                                        style=cloud_pin_style
                                    >
                                        <span class="help-bubble">"Forecast cloud cover through the day"</span>
                                        <span class="help-stem" aria-hidden="true"></span>
                                        <span class="help-pin" aria-hidden="true"></span>
                                    </div>
                                    <div
                                        class="help-anno help-anno-pin help-anno-rain-pin"
                                        style=rain_pin_style
                                    >
                                        <span class="help-bubble">"Forecast rain (model)"</span>
                                        <span class="help-stem" aria-hidden="true"></span>
                                        <span class="help-pin" aria-hidden="true"></span>
                                    </div>
                                    <div
                                        class="help-anno help-anno-pin help-anno-gauge-pin"
                                        style=gauge_pin_style
                                    >
                                        <span class="help-bubble">"Measured rain from nearby gauges"</span>
                                        <span class="help-stem" aria-hidden="true"></span>
                                        <span class="help-pin" aria-hidden="true"></span>
                                    </div>
                                    </div>
                                    <section class="detail help-demo-detail" style=tint>
                                        <p class="score-line">
                                            <span>"rain 8 AM-noon · 4° below avg AM"</span>
                                            <span class="detail-meta">"partly cloudy +8%"</span>
                                        </p>
                                        <ul class="factors">
                                            <li class="factor">
                                                <span class="name">"Sand pack"</span>
                                                <span class="contrib pos">"+18%"</span>
                                                <span class="note">"0.4\" rain ~18h ago — good pack window"</span>
                                                <div class="bar-track">
                                                    <div class="bar-fill" style="width:82%"></div>
                                                </div>
                                            </li>
                                        </ul>
                                    </section>
                            </div>
                        </div>
                    </section>
                </div>
            }
        })}
    }
}

#[component]
fn LoadingView() -> impl IntoView {
    view! {
        <div class="status">
            <p>"Fetching weather..."</p>
            <div class="skeleton skeleton-card"></div>
            <div class="skeleton skeleton-card"></div>
            <div class="skeleton skeleton-card"></div>
        </div>
    }
}

#[component]
fn ErrorView(message: String, on_retry: Callback<()>) -> impl IntoView {
    view! {
        <div class="status error">
            <p>{message}</p>
            <button type="button" on:click=move |_| on_retry.run(())>
                "Retry"
            </button>
        </div>
    }
}

#[component]
fn ReadyView(
    days: Vec<DayForecast>,
    selected: RwSignal<Option<NaiveDate>>,
    view_start: RwSignal<usize>,
    refreshed_at: RwSignal<String>,
    model: RwSignal<WeatherModel>,
    trail: RwSignal<Trail>,
    grid_lat: RwSignal<f64>,
    grid_lon: RwSignal<f64>,
    theme: RwSignal<Theme>,
    gauge_rain: RwSignal<GaugeRain>,
    weekend_warrior: RwSignal<bool>,
    multi_days: RwSignal<Vec<(Trail, Vec<DayForecast>)>>,
    multi_loading: RwSignal<bool>,
    on_switch: Callback<WeatherModel>,
    on_select_trail_with_date: Callback<(Trail, NaiveDate)>,
    on_toggle_weekend: Callback<()>,
) -> impl IntoView {
    let days_hero = days.clone();
    let days_nav = days.clone();
    let days_list = days;

    let on_select_day = {
        let weekend_warrior = weekend_warrior;
        let on_select_trail_with_date = on_select_trail_with_date.clone();
        let trail = trail;
        let selected = selected;
        let view_start = view_start;
        let days = days_nav.clone();
        Callback::new(move |(t, date): (Trail, NaiveDate)| {
            weekend_warrior.set(false);
            save_weekend_pref(false);
            if trail.get_untracked() == t {
                selected.set(Some(date));
                save_selected_pref(t, Some(date));
                let today_idx = days.iter().position(|d| d.is_today).unwrap_or(0);
                let offset = (date - days[today_idx].date).num_days();
                let idx = (today_idx as i64 + offset).max(0) as usize;
                view_start.set(idx.saturating_sub(3));
            } else {
                save_selected_pref(t, Some(date));
                on_select_trail_with_date.run((t, date));
            }
        })
    };

    view! {
        <Hero
            days=days_hero
            refreshed_at=refreshed_at
            model=model
            theme=theme
            weekend_warrior=weekend_warrior
            on_switch=on_switch
            on_toggle_weekend=on_toggle_weekend
        />
        {move || {
            if weekend_warrior.get() {
                view! {
                    <WeekendWarriorView
                        multi_days=multi_days
                        multi_loading=multi_loading
                        trail=trail
                        on_select_day=on_select_day
                    />
                }.into_any()
            } else {
                view! {
                    <TimelineNav days=days_nav.clone() view_start=view_start selected=selected />
                    <Timeline
                        days=days_list.clone()
                        view_start=view_start
                        selected=selected
                        trail=trail
                        gauge_rain=gauge_rain
                    />
                }.into_any()
            }
        }}
        <footer class="footer">
            <p>
                {move || format!(
                    "Forecasts weather-informed rideability for {}. Not official trail status. Use your own judgment.",
                    trail.get().short_name()
                )}
            </p>
            <p>
                "Past and forecast days both use "
                {move || model.get().label()}
                "."
            </p>
            <p class="footer-distance">
                {move || {
                    source_distance_line(
                        trail.get(),
                        grid_lat.get(),
                        grid_lon.get(),
                        &gauge_rain.get(),
                    )
                }}
            </p>
            {move || {
                gauge_rain.get().stale_for(trail.get()).then(|| {
                    view! {
                        <p class="footer-gauge-stale">
                            "Rain gauges stale — using model rain only."
                        </p>
                    }
                })
            }}
            <p>
                "Weather via "
                <a href="https://open-meteo.com/" target="_blank" rel="noopener">
                    "Open-Meteo"
                </a>
                " · "
                <a href="https://github.com/j33433/jaycast" target="_blank" rel="noopener">
                    "GitHub"
                </a>
                " · "
                <a href="about/" rel="noopener">
                    "About"
                </a>
                {concat!(" · v", env!("CARGO_PKG_VERSION"), " · ")}
                <a href="mailto:upload.bike@gmail.com">"upload.bike@gmail.com"</a>
                " · "
                <a href="LICENSE" target="_blank" rel="noopener">
                    "GPL-3.0"
                </a>
            </p>
        </footer>
    }
}

#[component]
fn Hero(
    days: Vec<DayForecast>,
    refreshed_at: RwSignal<String>,
    model: RwSignal<WeatherModel>,
    theme: RwSignal<Theme>,
    weekend_warrior: RwSignal<bool>,
    on_switch: Callback<WeatherModel>,
    on_toggle_weekend: Callback<()>,
) -> impl IntoView {
    let best = days.iter().find(|d| d.best).cloned();
    let day_name = best
        .as_ref()
        .map(|d| format_long(d.date))
        .unwrap_or_else(|| "No forecast days".to_string());

    view! {
        <section class="hero">
            <div class="hero-top-bar">
                <div class="hero-headline">
                    <p class="label">"Best ride window"</p>
                    <h2 class="day-name">{day_name}</h2>
                </div>
                <div class="hero-toggle">
                    <div class="hero-controls">
                        <div class="model-toggle">
                            <button
                                type="button"
                                class=move || {
                                    if model.get() == WeatherModel::Ecmwf {
                                        "model-btn active"
                                    } else {
                                        "model-btn"
                                    }
                                }
                                on:click=move |_| on_switch.run(WeatherModel::Ecmwf)
                            >
                                "ECMWF"
                            </button>
                            <button
                                type="button"
                                class=move || {
                                    if model.get() == WeatherModel::GfsSeamless {
                                        "model-btn active"
                                    } else {
                                        "model-btn"
                                    }
                                }
                                on:click=move |_| on_switch.run(WeatherModel::GfsSeamless)
                            >
                                "GFS"
                            </button>
                        </div>
                        <button
                            type="button"
                            class="theme-toggle"
                            aria-label=move || {
                                if theme.get() == Theme::Dark {
                                    "Switch to light theme"
                                } else {
                                    "Switch to dark theme"
                                }
                            }
                            title=move || {
                                if theme.get() == Theme::Dark {
                                    "Light theme"
                                } else {
                                    "Dark theme"
                                }
                            }
                            on:click=move |_| {
                                let next = theme.get_untracked().toggle();
                                save_theme_pref(next);
                                theme.set(next);
                            }
                        >
                            {move || {
                                if theme.get() == Theme::Dark {
                                    // Sun icon: switch to light
                                    view! {
                                        <svg class="theme-icon" viewBox="0 0 24 24" aria-hidden="true">
                                            <circle cx="12" cy="12" r="4" fill="currentColor"/>
                                            <g stroke="currentColor" stroke-width="1.75" stroke-linecap="round">
                                                <line x1="12" y1="2.5" x2="12" y2="5"/>
                                                <line x1="12" y1="19" x2="12" y2="21.5"/>
                                                <line x1="2.5" y1="12" x2="5" y2="12"/>
                                                <line x1="19" y1="12" x2="21.5" y2="12"/>
                                                <line x1="5.05" y1="5.05" x2="6.8" y2="6.8"/>
                                                <line x1="17.2" y1="17.2" x2="18.95" y2="18.95"/>
                                                <line x1="5.05" y1="18.95" x2="6.8" y2="17.2"/>
                                                <line x1="17.2" y1="6.8" x2="18.95" y2="5.05"/>
                                            </g>
                                        </svg>
                                    }.into_any()
                                } else {
                                    // Thick crescent moon: switch to dark
                                    view! {
                                        <svg class="theme-icon" viewBox="0 0 24 24" aria-hidden="true">
                                            <path
                                                fill="currentColor"
                                                d="M21 14.5A9 9 0 0 1 9.5 3 7.2 7.2 0 1 0 21 14.5z"
                                            />
                                        </svg>
                                    }.into_any()
                                }
                            }}
                        </button>
                        <button
                            type="button"
                            class=move || {
                                if weekend_warrior.get() {
                                    "weekend-toggle active"
                                } else {
                                    "weekend-toggle"
                                }
                            }
                            aria-label="Weekend warrior comparison grid"
                            title="Compare all trails"
                            on:click=move |_| on_toggle_weekend.run(())
                        >
                            <svg class="weekend-icon" viewBox="0 0 24 24" aria-hidden="true">
                                <rect x="3" y="3" width="7" height="7" rx="1" fill="currentColor"/>
                                <rect x="14" y="3" width="7" height="7" rx="1" fill="currentColor"/>
                                <rect x="3" y="14" width="7" height="7" rx="1" fill="currentColor"/>
                                <rect x="14" y="14" width="7" height="7" rx="1" fill="currentColor"/>
                            </svg>
                        </button>
                    </div>
                </div>
            </div>
            {match best {
                Some(d) => {
                    let stars = stars_str(d.stars);
                    let blurb = d.blurb.clone();
                    let tint = score_style(d.score);
                    view! {
                        <div class="stars" style=tint>{stars}</div>
                        <p class="why">{blurb}</p>
                    }
                    .into_any()
                }
                None => view! {
                    <p class="why">"Try refreshing weather data."</p>
                }
                .into_any(),
            }}
            <p class="hero-updated">
                {move || {
                    let t = refreshed_at.get();
                    if t.is_empty() { "forecast...".to_string() } else { t }
                }}
            </p>
        </section>
    }
}

#[component]
fn TimelineNav(
    days: Vec<DayForecast>,
    view_start: RwSignal<usize>,
    selected: RwSignal<Option<NaiveDate>>,
) -> impl IntoView {
    let n = days.len();
    let today_idx = days.iter().position(|d| d.is_today).unwrap_or(0);
    let max_start = n.saturating_sub(VIEW_DAYS);

    let range_label = {
        let days = days.clone();
        move || {
            let start = view_start.get().min(max_start);
            let end = (start + VIEW_DAYS).min(n).saturating_sub(1);
            match (days.get(start), days.get(end)) {
                (Some(a), Some(_)) if start == end => format_short(a.date),
                (Some(a), Some(b)) => {
                    format!("{} - {}", format_short(a.date), format_short(b.date))
                }
                _ => "No days".into(),
            }
        }
    };

    let step = VIEW_DAYS.saturating_sub(2).max(1);

    view! {
        <div class="timeline-nav">
            <button
                type="button"
                class="nav-btn"
                prop:disabled=move || { view_start.get() == 0 }
                on:click=move |_| {
                    let s = view_start.get();
                    view_start.set(s.saturating_sub(step));
                }
            >
                "Older"
            </button>
            <div class="nav-mid">
                <span class="nav-range">{range_label}</span>
                <button
                    type="button"
                    class="nav-today"
                    on:click=move |_| {
                        view_start.set(today_idx.saturating_sub(1).min(max_start));
                        if let Some(d) = days.get(today_idx) {
                            selected.set(Some(d.date));
                        }
                    }
                >
                    "Today"
                </button>
            </div>
            <button
                type="button"
                class="nav-btn"
                prop:disabled=move || { view_start.get() >= max_start }
                on:click=move |_| {
                    let s = view_start.get();
                    view_start.set((s + step).min(max_start));
                }
            >
                "Newer"
            </button>
        </div>
    }
}

#[component]
fn Timeline(
    days: Vec<DayForecast>,
    view_start: RwSignal<usize>,
    selected: RwSignal<Option<NaiveDate>>,
    trail: RwSignal<Trail>,
    gauge_rain: RwSignal<GaugeRain>,
) -> impl IntoView {
    Effect::new(move |_| {
        let Some(date) = selected.get() else { return };
        let id = format!("day-{date}");
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(66).await;
            let Some(win) = window() else { return };
            let Ok(viewport_h) = win.inner_height() else { return };
            let viewport_h = viewport_h.as_f64().unwrap_or(0.0);
            let Ok(scroll_y) = win.scroll_y() else { return };
            let Some(doc) = win.document() else { return };
            let Some(el) = doc.get_element_by_id(&id) else { return };
            let rect = el.get_bounding_client_rect();
            let el_top = rect.top() + scroll_y;
            let el_h = rect.height();
            let el_mid = el_top + el_h / 2.0;
            let target = (el_mid - viewport_h / 2.0).max(0.0);
            let _ = win.scroll_to_with_x_and_y(0.0, target);
        });
    });

    view! {
        <div class="timeline" role="list">
            {move || {
                let n = days.len();
                let max_start = n.saturating_sub(VIEW_DAYS);
                let start = view_start.get().min(max_start);
                let end = (start + VIEW_DAYS).min(n);
                let t = trail.get();
                let gauge = gauge_rain.get();
                days[start..end]
                    .iter()
                    .map(|d| {
                        let date = d.date;
                        let is_best = d.best;
                        let is_past = d.is_past;
                        let is_today = d.is_today;
                        let is_weekend = is_weekend(date);
                        let stars = stars_str(d.stars);
                        let blurb = d.blurb.clone();
                        let comfort_note = d.comfort_note.clone();
                        let precip = format!("{:.2}\"", d.precip_in);
                        let temp = format!("{:.0}°/{:.0}°", d.temp_max_f, d.temp_min_f);
                        let rain_path = rain_wave_path(&d.precip_3h_in);
                        let cloud_path = cloud_wave_path(&d.cloud_3h_pct);
                        let gauge_path = gauge
                            .hourly(t, date)
                            .map(|h| rain_gauge_wave_path(&h));
                        let date_s = format_short(date);
                        let tint = day_card_style(d.score, d.am_vs_avg_f, d.pm_vs_avg_f);
                        let detail = d.clone();
                        let possible_closure = d.closure_status.is_possible();
                        let today = Local::now().date_naive();
                        let facebook_status_link = date == today
                            || (possible_closure && date == today + Duration::days(1));
                        let card_label = format!("Show details for {date_s}");
                        let dow = if is_today {
                            "Today".to_string()
                        } else {
                            format_dow(date)
                        };
                        view! {
                            <div class="day-row" role="listitem" id=format!("day-{date}")>
                                <div
                                    class=move || {
                                        let mut c = String::from("day-card");
                                        if is_best {
                                            c.push_str(" best");
                                        }
                                        if is_past {
                                            c.push_str(" past");
                                        }
                                        if is_today {
                                            c.push_str(" today");
                                        }
                                        if is_weekend {
                                            c.push_str(" weekend");
                                        }
                                        if selected.get() == Some(date) {
                                            c.push_str(" selected");
                                        }
                                        c
                                    }
                                    style=tint
                                >
                                    <button
                                        type="button"
                                        class="day-card-select"
                                        aria-label=card_label
                                    on:click=move |_| {
                                        if selected.get() == Some(date) {
                                            selected.set(None);
                                            save_selected_pref(trail.get_untracked(), None);
                                        } else {
                                            selected.set(Some(date));
                                            save_selected_pref(trail.get_untracked(), Some(date));
                                        }
                                    }
                                    ></button>
                                    <svg
                                        class="cloud-wave"
                                        viewBox="0 0 100 100"
                                        preserveAspectRatio="none"
                                        aria-hidden="true"
                                        focusable="false"
                                    >
                                        <path d=cloud_path />
                                    </svg>
                                    <svg
                                        class="rain-wave"
                                        viewBox="0 0 100 100"
                                        preserveAspectRatio="none"
                                        aria-hidden="true"
                                        focusable="false"
                                    >
                                        <path d=rain_path />
                                    </svg>
                                    {gauge_path.map(|path| {
                                        view! {
                                            <svg
                                                class="rain-gauge"
                                                viewBox="0 0 100 100"
                                                preserveAspectRatio="none"
                                                aria-hidden="true"
                                                focusable="false"
                                            >
                                                <path d=path />
                                            </svg>
                                        }
                                    })}
                                    <div class="hourly-ticks" aria-hidden="true">
                                        {(0u32..24).map(|h| {
                                            let left = format!("{:.2}%", h as f64 * 100.0 / 24.0);
                                            let tall = h % 3 == 0;
                                            let cls = if tall { "htick tall" } else { "htick" };
                                            let label = match h {
                                                3 => Some("3a"),
                                                6 => Some("6a"),
                                                9 => Some("9a"),
                                                12 => Some("12p"),
                                                15 => Some("3p"),
                                                18 => Some("6p"),
                                                21 => Some("9p"),
                                                _ => None,
                                            };
                                            view! {
                                                <span class=cls style=format!("left:{left}")></span>
                                                {label.map(|l| view! {
                                                    <span class="hlabel" style=format!("left:{left}")>{l}</span>
                                                })}
                                            }
                                        }).collect_view()}
                                        {if is_today {
                                            let now = Local::now();
                                            let pct = (now.hour() as f64 + now.minute() as f64 / 60.0) / 24.0 * 100.0;
                                            Some(view! {
                                                <span class="now-marker" style=format!("left:{pct:.2}%")></span>
                                            })
                                        } else {
                                            None
                                        }}
                                    </div>
                                    <div class="date">
                                        {date_s}
                                        <span class="dow">{dow}</span>
                                    </div>
                                    <div class="mid">
                                        <div class="stars-sm">{stars}</div>
                                        <div class="blurb">
                                            {blurb}
                                            {move || {
                                                (trail.get() == Trail::Markham && facebook_status_link).then(|| {
                                                    view! {
                                                        <span class="facebook-status-copy">
                                                            " · see "
                                                            <a
                                                                class="facebook-status-link"
                                                                href="https://www.facebook.com/groups/MarkhamParkMTB"
                                                                target="_blank"
                                                                rel="noopener"
                                                            >
                                                                "Facebook"
                                                            </a>
                                                        </span>
                                                    }
                                                })
                                            }}
                                        </div>
                                    </div>
                                    <div class="precip">
                                        {precip}
                                        <div class="temp-row">
                                            {comfort_note.as_ref().map(|n| view! {
                                                <span class="comfort-badge">
                                                    <svg class="comfort-icon" viewBox="0 0 24 24" aria-hidden="true">
                                                        <g stroke="currentColor" stroke-width="1.5" stroke-linecap="round" fill="none">
                                                            <line x1="12" y1="2" x2="12" y2="22"/>
                                                            <line x1="2" y1="12" x2="22" y2="12"/>
                                                            <line x1="5" y1="5" x2="19" y2="19"/>
                                                            <line x1="19" y1="5" x2="5" y2="19"/>
                                                        </g>
                                                        <path fill="currentColor" d="M12 4l2 2-2 2-2-2zM12 16l2 2-2 2-2-2zM4 12l2-2 2 2-2 2zM16 12l2-2 2 2-2 2z"/>
                                                    </svg>
                                                    {n.clone()}
                                                </span>
                                            })}
                                            <span class="temp">{temp}</span>
                                        </div>
                                    </div>
                                </div>
                                {move || {
                                    (selected.get() == Some(date))
                                        .then(|| {
                                            day_detail_view(detail.clone())
                                        })
                                }}
                            </div>
                        }
                    })
                    .collect_view()
            }}
        </div>
    }
}

/// Compact multi-trail comparison grid: today + next 5 days × all 3 trails.
#[component]
fn WeekendWarriorView(
    multi_days: RwSignal<Vec<(Trail, Vec<DayForecast>)>>,
    multi_loading: RwSignal<bool>,
    trail: RwSignal<Trail>,
    on_select_day: Callback<(Trail, NaiveDate)>,
) -> impl IntoView {
    view! {
        <div class="weekend-warrior">
            {move || {
                if multi_loading.get() {
                    view! {
                        <div class="status">
                            <p>"Crunching trail comparisons..."</p>
                            <div class="skeleton skeleton-card"></div>
                            <div class="skeleton skeleton-card"></div>
                        </div>
                    }.into_any()
                } else {
                    let all = multi_days.get();
                    if all.is_empty() {
                        view! {
                            <div class="status"><p>"No trail data loaded."</p></div>
                        }.into_any()
                    } else {
                        let today = all.first()
                            .and_then(|(_, days)| days.iter().find(|d| d.is_today).map(|d| d.date))
                            .unwrap_or_else(|| Local::now().date_naive());
                        let grid = WeekendGridData::build(&all, today);
                        view! { <WeekendGrid grid=grid today=today trail=trail on_select_day=on_select_day /> }.into_any()
                    }
                }
            }}
        </div>
    }
}

struct WeekendGridData {
    dates: Vec<NaiveDate>,
    map: HashMap<Trail, HashMap<NaiveDate, DayForecast>>,
    best_per_day: HashMap<NaiveDate, Trail>,
}

impl WeekendGridData {
    const GRID_DAYS: usize = 6;

    fn build(all: &[(Trail, Vec<DayForecast>)], today: NaiveDate) -> Self {
        let dates: Vec<NaiveDate> = (0..Self::GRID_DAYS)
            .map(|i| today + Duration::days(i as i64))
            .collect();

        let mut map: HashMap<Trail, HashMap<NaiveDate, DayForecast>> = HashMap::new();
        for (t, days) in all {
            let m: HashMap<NaiveDate, DayForecast> = days
                .iter()
                .filter(|d| dates.contains(&d.date))
                .map(|d| (d.date, d.clone()))
                .collect();
            map.insert(*t, m);
        }

        let best_per_day: HashMap<NaiveDate, Trail> = dates
            .iter()
            .filter_map(|date| {
                Trail::ALL
                    .iter()
                    .filter_map(|t| {
                        map.get(t)
                            .and_then(|m| m.get(date))
                            .filter(|d| !d.is_past)
                            .map(|d| (*t, d.score))
                    })
                    .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(t, _)| (*date, t))
            })
            .collect();

        Self { dates, map, best_per_day }
    }
}

#[component]
fn WeekendGrid(
    grid: WeekendGridData,
    today: NaiveDate,
    trail: RwSignal<Trail>,
    on_select_day: Callback<(Trail, NaiveDate)>,
) -> impl IntoView {
    let dates = grid.dates;
    let map = grid.map;
    let best_per_day = grid.best_per_day;

    let cards: Vec<_> = dates.into_iter().map(|date| {
        let is_today = date == today;
        let dow = if is_today { "Today".to_string() } else { format_dow(date) };
        let short = format_short(date);
        let best_trail = best_per_day.get(&date).copied();

        let trail_rows: Vec<_> = Trail::ALL.iter().map(|t| {
            let cell = map.get(t).and_then(|m| m.get(&date));
            let is_best = best_trail == Some(*t);
            let on_select_day = on_select_day.clone();
            let is_selected = move || trail.get() == *t;
            view! {
                <button
                    type="button"
                    class=move || {
                        let mut c = String::from("weekend-trail-row");
                        if is_selected() { c.push_str(" selected"); }
                        if is_best { c.push_str(" best-bet"); }
                        c
                    }
                    on:click=move |_| on_select_day.run((*t, date))
                >
                    <div class="weekend-trail-row-main">
                        <img class="weekend-trail-icon" src=t.icon_src() alt=""/>
                        <span class="weekend-trail-name">{t.short_name()}</span>
                    </div>
                    {match cell {
                        Some(d) => {
                            let stars = stars_str(d.stars);
                            let blurb = d.blurb.clone();
                            let tint = score_style(d.score);
                            view! {
                                <div class="weekend-trail-score" style=tint>
                                    <span class="weekend-stars">{stars}</span>
                                    <span class="weekend-blurb">{blurb}</span>
                                    {if is_best {
                                        view! { <span class="best-bet-badge">"Best"</span> }.into_any()
                                    } else {
                                        ().into_any()
                                    }}
                                </div>
                            }.into_any()
                        }
                        None => view! {
                            <div class="weekend-trail-score">
                                <span class="weekend-na">"-"</span>
                            </div>
                        }.into_any(),
                    }}
                </button>
            }
        }).collect();

        view! {
            <div class=if is_today { "weekend-day-card today" } else { "weekend-day-card" }>
                <div class="weekend-day-head">
                    <span class="weekend-dow">{dow}</span>
                    <span class="weekend-date">{short}</span>
                </div>
                <div class="weekend-day-trails">
                    {trail_rows}
                </div>
            </div>
        }
    }).collect();

    view! {
        <div class="weekend-grid">{cards}</div>
    }
}

fn day_detail_view(d: DayForecast) -> impl IntoView {
    let rain = format!("{:.0}% rain chance 8 AM-noon", d.precip_prob_ride_max);
    let score_line = match d.comfort_detail.as_deref() {
        Some(t) => format!("{rain} · {t}"),
        None => rain,
    };

    let sky = d.factors.iter().find(|f| f.name == "Sky");

    let ride_clouds_avg: f64 = d.cloud_3h_pct[2..5].iter().sum::<f64>() / 3.0;
    let cloud_word = match ride_clouds_avg as u32 {
        0..=19 => "sunny",
        20..=39 => "mostly sunny",
        40..=59 => "partly cloudy",
        60..=79 => "cloudy",
        _ => "overcast",
    };

    let sky_pct = sky.map_or("0%".into(), |f| format!("{:+.0}%", f.contribution * 50.0));
    let meta = format!("{cloud_word} {sky_pct}");

    let tint = day_card_style(d.score, d.am_vs_avg_f, d.pm_vs_avg_f);
    view! {
        <section class="detail" style=tint>
            <p class="score-line">
                <span>{score_line}</span>
                <span class="detail-meta">{meta}</span>
            </p>
            <ul class="factors">
                {d.factors
                    .into_iter()
                    .filter(|f| f.name != "Sky" && f.name != "Forecast reliability")
                    .map(|f| {
                        let cls = if f.contribution > 0.08 {
                            "contrib pos"
                        } else if f.contribution < -0.08 {
                            "contrib neg"
                        } else {
                            "contrib neu"
                        };
                        let bar_cls = if f.quality >= 0.65 {
                            "bar-fill"
                        } else if f.quality >= 0.4 {
                            "bar-fill warn"
                        } else {
                            "bar-fill bad"
                        };
                        let width = format!("width:{:.0}%", f.quality * 100.0);
                        let contrib = format!("{:+.0}%", f.contribution * 50.0);
                        let name = f.name;
                        let note = f.note;
                        view! {
                            <li class="factor">
                                <span class="name">{name}</span>
                                <span class=cls>{contrib}</span>
                                <span class="note">{note}</span>
                                <div class="bar-track">
                                    <div class=bar_cls style=width></div>
                                </div>
                            </li>
                        }
                    })
                    .collect_view()}
            </ul>
        </section>
    }
}

fn stars_str(n: f64) -> String {
    format!("{:.1} ★", n.clamp(1.0, 5.0))
}

fn score_style(score: f64) -> String {
    format!("--score-color: {}", score_color(score))
}

/// Score tint plus optional AM/PM temp colors for the side borders.
fn day_card_style(score: f64, am_vs_avg_f: Option<f64>, pm_vs_avg_f: Option<f64>) -> String {
    let mut style = score_style(score);
    if let Some(delta) = am_vs_avg_f {
        style.push_str(&format!("; --am-temp-color: {}", temp_delta_color(delta)));
    }
    if let Some(delta) = pm_vs_avg_f {
        style.push_str(&format!("; --pm-temp-color: {}", temp_delta_color(delta)));
    }
    style
}

fn temp_delta_color(delta: f64) -> String {
    let t = (delta / 5.0).clamp(-1.0, 1.0);
    // blue (#4a9fd4) at t=-1 → red (#c46b5a) at t=+1
    let u = (t + 1.0) / 2.0;
    let r = (0x4a as f64 + u * (0xc4 - 0x4a) as f64).round() as u8;
    let g = (0x9f as f64 + u * (0x6b - 0x9f) as f64).round() as u8;
    let b = (0xd4 as f64 + u * (0x5a - 0xd4) as f64).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn rain_wave_path(rain_3h_in: &[f64]) -> String {
    let curve = smooth_wave_path(rain_3h_in, |inches| {
        // A quarter inch in a three-hour period fills the full visual range.
        100.0 - (inches.max(0.0) / 0.25).clamp(0.0, 1.0) * 54.0
    });
    format!("{curve} L 100 100 L 0 100 Z")
}

/// Hourly gauge tips as a filled curve, same vertical scale as the model rain wave.
fn rain_gauge_wave_path(hourly_tips_in: &[f64; 24]) -> String {
    let curve = smooth_wave_path(hourly_tips_in, |inches| {
        100.0 - (inches.max(0.0) / 0.25).clamp(0.0, 1.0) * 54.0
    });
    format!("{curve} L 100 100 L 0 100 Z")
}

fn cloud_wave_path(cloud_3h_pct: &[f64]) -> String {
    let curve = smooth_wave_path(cloud_3h_pct, |pct| (pct.clamp(0.0, 100.0) / 100.0) * 52.0);
    format!("{curve} L 100 0 L 0 0 Z")
}

fn smooth_wave_path(values: &[f64], height: impl Fn(f64) -> f64) -> String {
    let points: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(i, value)| {
            let x = if values.len() > 1 {
                i as f64 * 100.0 / (values.len() - 1) as f64
            } else {
                0.0
            };
            (x, height(*value).clamp(0.0, 100.0))
        })
        .collect();
    let Some(&(first_x, first_y)) = points.first() else {
        return "M 0 100".to_string();
    };

    let mut path = format!("M {first_x:.1} {first_y:.1}");
    for i in 0..points.len().saturating_sub(1) {
        let previous = points[i.saturating_sub(1)];
        let current = points[i];
        let next = points[i + 1];
        let following = points[(i + 2).min(points.len() - 1)];
        let control_1 = (
            current.0 + (next.0 - previous.0) / 6.0,
            (current.1 + (next.1 - previous.1) / 6.0).clamp(0.0, 100.0),
        );
        let control_2 = (
            next.0 - (following.0 - current.0) / 6.0,
            (next.1 - (following.1 - current.1) / 6.0).clamp(0.0, 100.0),
        );
        path.push_str(&format!(
            " C {0:.1} {1:.1}, {2:.1} {3:.1}, {4:.1} {5:.1}",
            control_1.0, control_1.1, control_2.0, control_2.1, next.0, next.1
        ));
    }
    path
}

/// Formatted "forecast as of" string using model init time, falling back to fetch time.
fn format_weather_as_of(init_time: Option<i64>, fallback_fetched_at: i64) -> String {
    let ts = init_time.unwrap_or(fallback_fetched_at);
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|t| format!("forecast as of {}", t.format("%-I:%M %p")))
        .unwrap_or_else(|| String::new())
}

fn format_long(d: NaiveDate) -> String {
    d.format("%A, %b %-d").to_string()
}

fn format_short(d: NaiveDate) -> String {
    d.format("%b %-d").to_string()
}

fn format_dow(d: NaiveDate) -> String {
    d.format("%a").to_string()
}

fn is_weekend(d: NaiveDate) -> bool {
    use chrono::Datelike;
    matches!(d.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun)
}

/// Footer line: forecast grid distance plus unnamed rain-gauge distances.
fn source_distance_line(
    trail: Trail,
    grid_lat: f64,
    grid_lon: f64,
    gauge: &GaugeRain,
) -> String {
    let t_lat = trail.latitude();
    let t_lon = trail.longitude();
    let mut parts = Vec::new();

    if grid_lat != 0.0 || grid_lon != 0.0 {
        let mi = haversine_km(t_lat, t_lon, grid_lat, grid_lon) * 0.621_371;
        if mi < 0.1 {
            parts.push("Forecast at trailhead".to_string());
        } else {
            parts.push(format!("Forecast {mi:.1} miles away"));
        }
    }

    let mut gauge_mi: Vec<f64> = trail
        .rain_gauge_coords()
        .iter()
        .map(|(lat, lon)| haversine_km(t_lat, t_lon, *lat, *lon) * 0.621_371)
        .filter(|mi| mi.is_finite())
        .collect();
    gauge_mi.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // Dedupe near-identical co-located gauges (e.g. Camp Murphy pair).
    gauge_mi.dedup_by(|a, b| (*a - *b).abs() < 0.05);

    if !gauge_mi.is_empty() {
        let mut gauges = match gauge_mi.as_slice() {
            [one] => {
                if *one < 0.1 {
                    "rain gauge at trailhead".to_string()
                } else {
                    format!("rain gauge {one:.1} miles away")
                }
            }
            many => {
                let list = many
                    .iter()
                    .map(|mi| format!("{mi:.1}"))
                    .collect::<Vec<_>>()
                    .join(" and ");
                format!("rain gauge {list} miles away")
            }
        };
        if let Some(ts) = gauge.last_seen_ts(trail) {
            let seen = Local
                .timestamp_opt(ts, 0)
                .single()
                .map(|dt| dt.format("%-I:%M %p").to_string())
                .unwrap_or_else(|| "recently".into());
            gauges.push_str(&format!(" at {seen}"));
        }
        parts.push(gauges);
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!("{}.", parts.join(" · "))
    }
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0_f64;
    let la1 = lat1.to_radians();
    let la2 = lat2.to_radians();
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    r * c
}
