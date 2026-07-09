use chrono::{Local, NaiveDate};
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::score::{score_days, DayForecast, Params};
use crate::weather::{self, LOCATION_NAME, LOCATION_SUB};

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
    let refreshed_at = RwSignal::new(String::new());

    let load = move || {
        state.set(LoadState::Loading);
        spawn_local(async move {
            match weather::fetch_forecast().await {
                Ok(resp) => {
                    let days = resp.days();
                    let today = Local::now().date_naive();
                    let scored = score_days(&days, today, &Params::default());
                    if selected.get_untracked().is_none() {
                        let pick = scored
                            .iter()
                            .find(|d| d.best)
                            .or_else(|| scored.first())
                            .map(|d| d.date);
                        selected.set(pick);
                    }
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

    view! {
        <div id="app">
            <header class="header">
                <h1>"jay" <span>"cast"</span></h1>
                <div class="meta">
                    {move || {
                        let t = refreshed_at.get();
                        if t.is_empty() {
                            "loading...".to_string()
                        } else {
                            format!("updated {t}")
                        }
                    }}
                </div>
                <p class="location">{LOCATION_NAME} <br/> {LOCATION_SUB}</p>
            </header>

            {move || match state.get() {
                LoadState::Loading => view! { <LoadingView /> }.into_any(),
                LoadState::Error(msg) => view! {
                    <ErrorView message=msg on_retry=Callback::new(move |_| {
                        weather::clear_cache();
                        load();
                    }) />
                }.into_any(),
                LoadState::Ready(days) => view! {
                    <ReadyView
                        days=days
                        selected=selected
                        on_refresh=Callback::new(move |_| {
                            weather::clear_cache();
                            load();
                        })
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
    on_refresh: Callback<()>,
) -> impl IntoView {
    let days_hero = days.clone();
    let days_list = days.clone();
    let days_detail = days;

    view! {
        <Hero days=days_hero selected=selected />
        <p class="section-title">"Next 10 days"</p>
        <Timeline days=days_list selected=selected />
        <DayDetail days=days_detail selected=selected />
        <footer class="footer">
            <p>
                "Rideability is a heuristic for sandy dune trails that pack firm after rain. "
                "Stars blend prior rainfall, dry-out timing, ride-day wetness, and comfort weather. "
                "Not trail status - ride at your own judgment."
            </p>
            <p>
                "Weather: "
                <a href="https://open-meteo.com/" target="_blank" rel="noopener">
                    "Open-Meteo"
                </a>
                " · precip in inches · "
                <a href="LICENSE" target="_blank" rel="noopener">
                    "GPL-3.0"
                </a>
            </p>
            <p>
                <button
                    type="button"
                    class="btn"
                    style="margin-top:0.5rem;font-size:0.8rem;padding:0.35rem 0.8rem;"
                    on:click=move |_| on_refresh.run(())
                >
                    "Refresh weather"
                </button>
            </p>
        </footer>
    }
}

#[component]
fn Hero(days: Vec<DayForecast>, selected: RwSignal<Option<NaiveDate>>) -> impl IntoView {
    let best = days.iter().find(|d| d.best).cloned();

    view! {
        <section class="hero">
            <p class="label">"Best ride window"</p>
            {match best {
                Some(d) => {
                    let date = d.date;
                    let name = format_long(date);
                    let stars = stars_str(d.stars);
                    let blurb = d.blurb.clone();
                    view! {
                        <h2 class="day-name">{name}</h2>
                        <div class="stars">{stars}</div>
                        <p class="why">{blurb}</p>
                        <button
                            type="button"
                            class="btn"
                            style="margin-top:0.85rem;font-size:0.85rem;"
                            on:click=move |_| selected.set(Some(date))
                        >
                            "See factors"
                        </button>
                    }
                    .into_any()
                }
                None => view! {
                    <h2 class="day-name">"No forecast days"</h2>
                    <p class="why">"Try refreshing weather data."</p>
                }
                .into_any(),
            }}
        </section>
    }
}

#[component]
fn Timeline(days: Vec<DayForecast>, selected: RwSignal<Option<NaiveDate>>) -> impl IntoView {
    view! {
        <div class="timeline" role="list">
            {days
                .into_iter()
                .map(|d| {
                    let date = d.date;
                    let is_best = d.best;
                    let stars = stars_str(d.stars);
                    let blurb = d.blurb.clone();
                    let precip = format!("{:.2}\"", d.precip_in);
                    let temp = format!("{:.0}°/{:.0}°", d.temp_max_f, d.temp_min_f);
                    let date_s = format_short(date);
                    let dow = format_dow(date);
                    view! {
                        <button
                            type="button"
                            class=move || {
                                let mut c = String::from("day-card");
                                if is_best {
                                    c.push_str(" best");
                                }
                                if selected.get() == Some(date) {
                                    c.push_str(" selected");
                                }
                                c
                            }
                            role="listitem"
                            on:click=move |_| selected.set(Some(date))
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
                    }
                })
                .collect_view()}
        </div>
    }
}

#[component]
fn DayDetail(days: Vec<DayForecast>, selected: RwSignal<Option<NaiveDate>>) -> impl IntoView {
    view! {
        {move || {
            let sel = selected.get();
            let day = days.iter().find(|d| Some(d.date) == sel).cloned();
            match day {
                None => view! { <div></div> }.into_any(),
                Some(d) => {
                    let title = format!(
                        "{}{}",
                        format_long(d.date),
                        if d.best { " · best" } else { "" }
                    );
                    let score_line = format!(
                        "{} · score {:.0}% · {:.2}\" rain · {:.0}% chance",
                        stars_str(d.stars),
                        d.score * 100.0,
                        d.precip_in,
                        d.precip_prob_max
                    );
                    view! {
                        <section class="detail">
                            <h2>{title}</h2>
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
                    .into_any()
                }
            }
        }}
    }
}

fn stars_str(n: u8) -> String {
    let n = n.clamp(1, 5) as usize;
    format!("{}{}", "★".repeat(n), "☆".repeat(5 - n))
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
