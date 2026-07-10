use chrono::{Local, NaiveDate};
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::score::{score_color, score_days, DayForecast, Params};
use crate::weather::{self, WeatherModel, LOCATION_NAME, LOCATION_SUB, VIEW_DAYS};

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
    let grid_lat = RwSignal::new(0.0f64);
    let grid_lon = RwSignal::new(0.0f64);

    let is_first_load = RwSignal::new(true);

    let load = move || {
        let m = model.get_untracked();
        let first = is_first_load.get_untracked();
        state.set(LoadState::Loading);
        spawn_local(async move {
            match weather::fetch_forecast(m).await {
                Ok(resp) => {
                    grid_lat.set(resp.latitude);
                    grid_lon.set(resp.longitude);
                    let days = resp.days();
                    let today = Local::now().date_naive();
                    let scored = score_days(&days, today, &Params::default());

                    let today_idx = scored
                        .iter()
                        .position(|d| d.is_today)
                        .or_else(|| scored.iter().position(|d| !d.is_past))
                        .unwrap_or(0);

                    if first {
                        view_start.set(today_idx);
                    }

                    let prev_sel = selected.get_untracked();
                    if first
                        || (prev_sel.is_some() && !scored.iter().any(|d| Some(d.date) == prev_sel))
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
                Err(e) => state.set(LoadState::Error(e)),
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

    view! {
        <div id="app">
            <header class="header">
                <img
                    class="jay-mark"
                    src="/jaycast/jaycast-icon.png"
                    width="97"
                    height="110"
                    alt=""
                />
                <div class="header-text">
                    <h1>"jay" <span>"cast"</span></h1>
                    <span class="tagline">"scrub trail pack"</span>
                    <p class="location">
                        {LOCATION_NAME}
                        <br/>
                        {LOCATION_SUB}
                    </p>
                </div>
            </header>

            {move || match state.get() {
                LoadState::Loading => view! { <LoadingView /> }.into_any(),
                LoadState::Error(msg) => view! {
                    <ErrorView message=msg on_retry=Callback::new(move |_| {
                        weather::clear_cache(model.get_untracked());
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
                        grid_lat=grid_lat
                        grid_lon=grid_lon
                        on_switch=Callback::new(switch_model)
                    />
                }.into_any(),
            }}
        </div>
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
    grid_lat: RwSignal<f64>,
    grid_lon: RwSignal<f64>,
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
            grid_lat=grid_lat
            grid_lon=grid_lon
            on_switch=on_switch
        />
        <TimelineNav days=days_nav view_start=view_start selected=selected />
        <Timeline days=days_list view_start=view_start selected=selected />
        <footer class="footer">
            <p>
                "Heuristic for sandy dune trails that pack firm after rain. "
                "Not trail status - ride at your own judgment."
            </p>
            <p>
                {move || model.get().label()}
                " via "
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
    grid_lat: RwSignal<f64>,
    grid_lon: RwSignal<f64>,
    on_switch: Callback<WeatherModel>,
) -> impl IntoView {
    let best = days.iter().find(|d| d.best).cloned();

    view! {
        <section class="hero">
            <div class="hero-top-bar">
                <p class="label">"Best ride window"</p>
                <div class="hero-toggle">
                    <div class="model-toggle">
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
                    </div>
                    <p class="hero-distance">
                        {move || {
                            let lat = grid_lat.get();
                            let lon = grid_lon.get();
                            if lat == 0.0 && lon == 0.0 {
                                return String::new();
                            }
                            let km = haversine_km(
                                weather::LAT,
                                weather::LON,
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
                    let name = format_long(d.date);
                    let stars = stars_str(d.stars);
                    let blurb = d.blurb.clone();
                    let tint = score_style(d.score);
                    view! {
                        <h2 class="day-name">{name}</h2>
                        <div class="stars" style=tint>{stars}</div>
                        <p class="why">{blurb}</p>
                    }
                    .into_any()
                }
                None => view! {
                    <h2 class="day-name">"No forecast days"</h2>
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
                        view_start.set(today_idx.min(max_start));
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
                        let precip = format!("{:.2}\"", d.precip_in);
                        let temp = format!("{:.0}°/{:.0}°", d.temp_max_f, d.temp_min_f);
                        let date_s = format_short(date);
                        let tint = score_style(d.score);
                        let detail = d.clone();
                        let dow = if is_today {
                            "Today".to_string()
                        } else if is_past {
                            format!("{} · past", format_dow(date))
                        } else {
                            format_dow(date)
                        };
                        view! {
                            <div class="day-row" role="listitem">
                                <button
                                    type="button"
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
                                    on:click=move |_| {
                                        if selected.get() == Some(date) {
                                            selected.set(None);
                                        } else {
                                            selected.set(Some(date));
                                        }
                                    }
                                >
                                    <div class="date">
                                        {date_s}
                                        <span class="dow">{dow}</span>
                                    </div>
                                    <div class="mid">
                                        <div class="stars-sm">{stars}</div>
                                        <div class="blurb">{blurb}</div>
                                    </div>
                                    <div class="precip">
                                        {precip}
                                        <span class="temp">{temp}</span>
                                    </div>
                                </button>
                                {move || {
                                    (selected.get() == Some(date))
                                        .then(|| day_detail_view(detail.clone()))
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
    let score_line = format!(
        "score {:.0}% · {:.0}% rain chance",
        d.score * 100.0,
        d.precip_prob_max
    );
    let tint = score_style(d.score);
    view! {
        <section class="detail" style=tint>
            <p class="score-line">{score_line}</p>
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
