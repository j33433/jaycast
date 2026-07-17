use chrono::{Duration, Local, NaiveDate};
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::score::{score_color, score_days, DayForecast, Params};
use crate::theme::{
    apply_theme, apply_theme_color, detect_os_theme, load_theme_pref, save_theme_pref, Theme,
};
use crate::trails::{self, Trail};
use crate::weather::{self, WeatherModel, VIEW_DAYS};

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
    let grid_lat = RwSignal::new(0.0f64);
    let grid_lon = RwSignal::new(0.0f64);
    let theme = RwSignal::new(load_theme_pref().unwrap_or_else(detect_os_theme));

    let is_first_load = RwSignal::new(true);

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
            match weather::fetch_forecast(m, t).await {
                Ok(forecast) => {
                    let today = Local::now().date_naive();
                    let history_start = today - Duration::days(weather::PAST_DAYS.into());
                    let history_end = today - Duration::days(1);
                    match weather::fetch_historical_analysis(m, history_start, history_end, t)
                        .await
                    {
                        Ok(history) => {
                            if model.get_untracked() != m || trail.get_untracked() != t {
                                return;
                            }
                            grid_lat.set(forecast.latitude);
                            grid_lon.set(forecast.longitude);
                            let days = weather::combine_history_and_forecast(
                                history.days(),
                                forecast.days(),
                                today,
                            );
                            let scored = score_days(&days, today, &Params::for_trail(t));

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
                            if first
                                || (prev_sel.is_some()
                                    && !scored.iter().any(|d| Some(d.date) == prev_sel))
                            {
                                let pick = scored
                                    .iter()
                                    .find(|d| d.best)
                                    .or_else(|| scored.get(today_idx))
                                    .or_else(|| scored.first())
                                    .map(|d| d.date);
                                selected.set(pick);
                            }

                            is_first_load.set(false);
                            refreshed_at.set(Local::now().format("%-I:%M %p").to_string());
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

    Effect::new(move |_| {
        load();
    });

    let switch_model = move |new_model: WeatherModel| {
        if model.get_untracked() == new_model {
            return;
        }
        weather::save_model_pref(new_model);
        model.set(new_model);
        load();
    };

    let switch_trail = move |new_trail: Trail| {
        if trail.get_untracked() == new_trail {
            location_dialog_open.set(false);
            return;
        }
        trails::save_trail_pref(new_trail);
        trails::update_trail_url(new_trail);
        trail.set(new_trail);
        selected.set(None);
        view_start.set(0);
        is_first_load.set(true);
        location_dialog_open.set(false);
        load();
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
                    <button
                        type="button"
                        class="location-change"
                        on:click=move |_| location_dialog_open.set(true)
                    >
                        "change location"
                    </button>
                </div>
            </header>

            <LocationDialog
                open=location_dialog_open
                selected=trail
                on_change=Callback::new(switch_trail)
            />

            {move || match state.get() {
                LoadState::Loading => view! { <LoadingView /> }.into_any(),
                LoadState::Error(msg) => view! {
                    <ErrorView message=msg on_retry=Callback::new(move |_| {
                        weather::clear_cache_for_trail(model.get_untracked(), trail.get_untracked());
                        load();
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
                        on_switch=Callback::new(switch_model)
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
                                "x"
                            </button>
                        </div>
                        <div class="location-options">
                            {Trail::ALL.into_iter().map(|trail| {
                                let name = trail.name();
                                let location = trail.location();
                                let icon_src = trail.icon_src();
                                view! {
                                    <button
                                        type="button"
                                        class=move || {
                                            if selected.get() == trail {
                                                "location-option selected"
                                            } else {
                                                "location-option"
                                            }
                                        }
                                        on:click=move |_| on_change.run(trail)
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
    on_switch: Callback<WeatherModel>,
) -> impl IntoView {
    let days_hero = days.clone();
    let days_nav = days.clone();
    let days_list = days;

    view! {
        <Hero
            days=days_hero
            refreshed_at=refreshed_at
            model=model
            trail=trail
            grid_lat=grid_lat
            grid_lon=grid_lon
            theme=theme
            on_switch=on_switch
        />
        <TimelineNav days=days_nav view_start=view_start selected=selected />
        <Timeline days=days_list view_start=view_start selected=selected trail=trail />
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
            <p>
                "Weather via "
                <a href="https://open-meteo.com/" target="_blank" rel="noopener">
                    "Open-Meteo"
                </a>
                " · "
                <a href="https://github.com/j33433/jaycast" target="_blank" rel="noopener">
                    "GitHub"
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
    trail: RwSignal<Trail>,
    grid_lat: RwSignal<f64>,
    grid_lon: RwSignal<f64>,
    theme: RwSignal<Theme>,
    on_switch: Callback<WeatherModel>,
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
                    </div>
                    <p class="hero-distance">
                        {move || {
                            let lat = grid_lat.get();
                            let lon = grid_lon.get();
                            if lat == 0.0 && lon == 0.0 {
                                return String::new();
                            }
                            let km = haversine_km(
                                trail.get().latitude(),
                                trail.get().longitude(),
                                lat,
                                lon,
                            );
                            let mi = km * 0.621371;
                            if mi < 0.1 {
                                "forecast at trailhead".to_string()
                            } else {
                                format!("forecast {mi:.1} miles away")
                            }
                        }}
                    </p>
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
                    if t.is_empty() {
                        "updated...".to_string()
                    } else {
                        format!("updated {t}")
                    }
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
) -> impl IntoView {
    view! {
        <div class="timeline" role="list">
            {move || {
                let n = days.len();
                let max_start = n.saturating_sub(VIEW_DAYS);
                let start = view_start.get().min(max_start);
                let end = (start + VIEW_DAYS).min(n);
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
                            <div class="day-row" role="listitem">
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
                                        } else {
                                            selected.set(Some(date));
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

fn day_detail_view(d: DayForecast) -> impl IntoView {
    let rain = format!("{:.0}% rain chance 8 AM-noon", d.precip_prob_ride_max);
    let score_line = match d.comfort_detail.as_deref() {
        Some(t) => format!("{rain} · {t}"),
        None => rain,
    };
    let tint = day_card_style(d.score, d.am_vs_avg_f, d.pm_vs_avg_f);
    view! {
        <section class="detail" style=tint>
            <p class="score-line">
                {score_line}
            </p>
            <ul class="factors">
                {d.factors
                    .into_iter()
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
