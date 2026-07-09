# jaycast

Rideability forecasts for sandy dune MTB trails that pack firm after rain.

**Camp Murphy MTB Trails** · Jonathan Dickinson State Park, FL

Browser-only app (Rust → WASM via Leptos). Weather from [Open-Meteo](https://open-meteo.com/). No API keys, no backend.

## Idea

Sandy trails firm up after sustained rain, then loosen as they dry. Ideal ride day:

1. Meaningful rain in the prior ~1–3 days
2. Dry (or nearly dry) on the day you ride
3. Comfortable temperature and wind

Each day in a **30-day archive + 10-day forecast** gets a **1.0–5.0 star** score (one decimal) plus a factor breakdown (prior rain, pack timing, ride-day wetness, soil moisture, temp, wind, forecast confidence). Day cards are tinted by score. Use **Older / Today / Newer** to scroll the timeline and verify scores against days you rode. Units are **inches** and °F.

## Develop

```bash
# once
rustup target add wasm32-unknown-unknown
cargo install trunk   # or use a trunk binary release

trunk serve           # http://127.0.0.1:8080
cargo test            # heuristic unit tests
trunk build --release # static site in dist/
```

## Score model

Heuristic weights (see `src/score/params.rs`):

| Factor | Role |
|--------|------|
| Prior rain | Antecedent precip over ~72h; sweet spot ~0.35–3.0 in, ideal ~1.0 in |
| Pack timing | Best ~24h after a solid rain day; fades over ~5 dry days |
| Ride-day wetness | Soft penalty for light rain; hard gate if ≥0.4 in that day |
| Soil moisture | Secondary Open-Meteo soil signal |
| Temperature / wind | Florida MTB comfort band |
| Forecast confidence | Tapers for farther days |

Tune constants in `params.rs` after real rides. This is not official trail status.

## License

GPL-3.0-or-later (see `LICENSE`).
