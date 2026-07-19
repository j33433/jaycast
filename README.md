![jaycast](assets/jaycast-icon.png)

# jaycast

Forecast weather-informed rideability for South Florida MTB trails.

**Live:** [https://upload.bike/jaycast/](https://upload.bike/jaycast/)

Browser-only (Rust → WASM via Leptos). Weather from [Open-Meteo](https://open-meteo.com/) with a **GFS seamless** / **ECMWF** model toggle. No API keys, no backend.

## Idea

Choose Camp Murphy, Markham Park, or Quiet Waters Park from the location chooser. The choice persists in the browser and can be shared with `?camp-murphy`, `?markham`, or `?quiet-waters`.

Each profile interprets weather according to its terrain:

1. Camp Murphy: sandy, lightly shaded trails firm up after rain and dry quickly under sun.
2. Markham: meaningful rain uses its hourly end time to estimate a drainage reopening. This is an advisory model, not an official status feed.
3. Quiet Waters: mixed hardpack and loose-over-hard terrain never closes, is less sand-dependent, degrades more slowly after rain, and gets more generous ride-window rain thresholds and a gentler timing curve.

Each day in a **30-day archive + 7-day forecast** gets a **1.0–5.0 star** score (one decimal) plus a factor breakdown. The default timeline shows yesterday, today, and the next 7 days. Day cards are tinted by score. Their subtle background curves show rain rising from the bottom and gray cloud cover descending from the top in three-hour periods, from midnight on the left through late evening on the right. Day-card side borders encode feels-like vs the prior week: left = morning, right = afternoon (blue cooler, red warmer). Use **Older / Today / Newer** to scroll the timeline and check scores against days you rode. Units are **inches** and °F. Light/dark theme persists in the browser.

## Develop

```bash
# once
rustup target add wasm32-unknown-unknown
cargo install trunk   # or use a trunk binary release

trunk serve           # http://127.0.0.1:8080
cargo test            # heuristic unit tests
trunk build --release # static site in dist/
```

Analyze a date or inclusive range with the same scorer: `cargo run --features cli --bin jaycast -- analyze markham 2026-07-08:2026-07-11 both`. The trail slug is optional and defaults to Camp Murphy; omit the date for today.

Ground-truth hourly gauge rain (Xweather, server key only): `cargo run --features cli --bin jaycast -- xweather publish --out xweather.json` (see `XWEATHER.md`).

## Score model

Heuristic weights and trail profiles live in `src/score/params.rs`, `src/score/heuristic.rs`, and `src/trails.rs`.

| Factor | Role |
|--------|------|
| Surface/drainage | Trail-specific sand-pack, drainage-risk, or mixed-surface behavior |
| Rain during ride | Camp Murphy and Quiet Waters penalize rain from 8 AM-noon; Markham uses daily rain and a drainage advisory |
| Temperature | Florida MTB comfort band, with heat-index ding |
| Wind | Ideal light breeze ~5–12 mph; dead calm and gales both ding |
| Forecast confidence | Tapers for farther days |

Camp Murphy uses roughly **pack 55% / weather 35% / confidence 10%**. Quiet Waters weights weather more heavily and, since it never closes, uses higher ride-window rain thresholds, a more generous wet-gate floor (0.45 vs 0.25), and gentler fresh-rain timing. Markham estimates reopening from hourly rain after at least 0.10 in. Wet-day blurbs name the dominant period: **rain AM**, **rain PM**, or **rainy day**. Tune constants in `params.rs` after real rides. This is not official trail status.

## License

GPL-3.0-or-later (see `LICENSE`).

See [CODEMAP.md](CODEMAP.md) for a file-level map of the project.
