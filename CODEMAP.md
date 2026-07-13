# CODEMAP

File-level map of the jaycast repository: source, assets, test data, and config.

```
jaycast/
  Cargo.toml
  Cargo.lock
  Trunk.toml
  index.html
  robots.txt
  sitemap.xml
  install.sh              # gitignored deploy script (trunk build + rsync)
  .gitignore
  LICENSE                 # GPL-3.0-or-later
  README.md
  CODEMAP.md
  assets/
    style.css
    jaycast-icon.png
  art/
    jaycast-detailed.svg
    jaycast-plain.svg
    gatorcast-plain.svg
    eaglecast-plain.svg
  src/
    lib.rs
    app.rs
    theme.rs
    trails.rs
    score/
      mod.rs
      params.rs
      heuristic.rs
    weather/
      mod.rs
      types.rs
    bin/
      jaycast.rs
  tests/
    fixtures/
      markham-2mo.json
      closures.txt
```

## Root

| File | Description |
|------|-------------|
| `Cargo.toml` | Package manifest. Crate types `cdylib` + `rlib`. Deps: leptos 0.7 (csr), gloo-net, serde, serde_json, chrono, wasm-bindgen, web-sys, console_error_panic_hook. Native-only: ureq. Feature `cli` gates the binary. Release profile: `opt-level="z"`, lto, single codegen unit. |
| `Cargo.lock` | Dependency lockfile (auto-generated). |
| `Trunk.toml` | Trunk build config. Target `index.html`, dist dir `dist/`, public URL `/jaycast/`. |
| `index.html` | App entry HTML. Inline JS applies saved theme before render. OpenGraph/Twitter meta, JSON-LD structured data. Trunk asset links for icon, CSS, WASM, and copy-file directives for SVGs, LICENSE, robots.txt, sitemap.xml. |
| `robots.txt` | Allows `/jaycast/`, declares sitemap URL. |
| `sitemap.xml` | Single URL entry for the deployed site. |
| `install.sh` | Deploy script (gitignored). Runs `trunk build --release` then rsyncs `dist/` to nginx. |
| `.gitignore` | Ignores `/target`, `/dist`, `.DS_Store`, `*.swp`, `install.sh`. |
| `LICENSE` | GPL-3.0-or-later full text. |
| `README.md` | Project description, trail profiles, develop/test/build instructions, CLI usage, score model summary. |

## assets/

| File | Description |
|------|-------------|
| `style.css` | Application stylesheet (1044 lines). Florida scrub palette with dark (default) and light themes. CSS custom properties for jay blue, scrub green, sand, accent, warn, bad, star, rain. Styles for header, trail logo, location chooser dialog, hero, model toggle, theme toggle, timeline nav, day cards (score-tinted gradients, weekend/best/selected/past/today states), rain-wave and cloud-wave SVG backgrounds, detail panel with factor bars, footer, skeleton shimmer loader. Responsive breakpoint at 30rem. |
| `jaycast-icon.png` | App icon / favicon / OG image. Referenced by `index.html` and `README.md`. |

## art/

| File | Description |
|------|-------------|
| `jaycast-detailed.svg` | Source artwork: detailed Camp Murphy scrub-jay logo (2048x2048, 62 linear gradients). Not deployed directly. |
| `jaycast-plain.svg` | Camp Murphy trail mark. Copied to dist, used as `/jaycast/jaycast-plain.svg`. |
| `gatorcast-plain.svg` | Markham trail mark (alligator). Copied to dist, used as `/jaycast/gatorcast-plain.svg`. |
| `eaglecast-plain.svg` | Quiet Waters trail mark (eagle). Copied to dist, used as `/jaycast/eaglecast-plain.svg`. |

## src/

### `src/lib.rs`

Crate root. Module doc: "weather-informed MTB trail rideability forecasts."

- Private modules: `app`, `theme`
- Public modules: `score`, `trails`, `weather`
- `#[wasm_bindgen(start)] pub fn main()` - entry point; sets panic hook, mounts `App` to body

### `src/app.rs`

Leptos UI component tree (874 lines). All components and helpers are private.

**Types:**
- `enum LoadState { Loading, Ready(Vec<DayForecast>), Error(String) }`

**Components:**
- `App()` - root; manages state signals (load state, selected day, view start, refreshed_at, model, trail, dialog, grid coords, theme, first load), runs fetch+score effect, handles model/trail switching
- `LocationDialog(open, selected, on_change)` - modal trail chooser
- `LoadingView()` - skeleton loading state
- `ErrorView(message, on_retry)` - error display with retry
- `ReadyView(days, selected, view_start, refreshed_at, model, trail, grid_lat, grid_lon, theme, on_switch)` - composes Hero, TimelineNav, Timeline, footer
- `Hero(days, refreshed_at, model, trail, grid_lat, grid_lon, theme, on_switch)` - best ride window, GFS/ECMWF toggle, theme toggle (inline sun/moon SVG), distance display
- `TimelineNav(days, view_start, selected)` - Older/Today/Newer scroll nav
- `Timeline(days, view_start, selected, trail)` - day cards with rain/cloud wave SVG backgrounds, Markham Facebook status link

**Helper functions:**
- `day_detail_view(d, trail)` - detail panel with factor breakdown bars
- `stars_str(n) -> String`
- `score_style(score) -> String`
- `rain_wave_path(rain_3h_in) -> String`
- `cloud_wave_path(cloud_3h_pct) -> String`
- `smooth_wave_path(values, height) -> String` - Catmull-Rom spline path
- `format_long(d) -> String`
- `format_short(d) -> String`
- `format_dow(d) -> String`
- `is_weekend(d) -> bool`
- `haversine_km(lat1, lon1, lat2, lon2) -> f64`

### `src/theme.rs`

Light/dark theme preference with localStorage persistence (89 lines).

**Types:**
- `enum Theme { Light, Dark }`

**`impl Theme`:**
- `pub fn attr(self) -> &'static str`
- `pub fn toggle(self) -> Self`
- `pub fn theme_color(self) -> &'static str`
- `fn from_str(s: &str) -> Option<Self>` (private)

**Functions:**
- `pub fn load_theme_pref() -> Option<Theme>`
- `pub fn save_theme_pref(theme: Theme)`
- `pub fn detect_os_theme() -> Theme`
- `pub fn apply_theme(theme: Theme)`
- `pub fn apply_theme_color(theme: Theme)`

### `src/trails.rs`

Trail definitions and localStorage/URL persistence (149 lines).

**Types:**
- `enum Trail { CampMurphy, Markham, QuietWaters }`

**`impl Trail`:**
- `pub const ALL: [Self; 3]`
- `pub fn slug(self) -> &'static str` - `"camp-murphy"` / `"markham"` / `"quiet-waters"`
- `pub fn name(self) -> &'static str` - full trail name
- `pub fn location(self) -> &'static str` - park name and state
- `pub fn latitude(self) -> f64`
- `pub fn longitude(self) -> f64`
- `pub fn icon_src(self) -> &'static str` - SVG path
- `pub fn short_name(self) -> &'static str`
- `pub fn tagline(self) -> &'static str` - `"scrub trail pack"` / `"drainage advisory"` / `"mixed-surface forecast"`
- `pub fn brand(self) -> &'static str` - `"jay"` / `"gator"` / `"eagle"`
- `pub fn from_slug(value: &str) -> Option<Self>`

**Functions:**
- `pub fn load_trail_pref() -> Trail` - reads from URL query then localStorage, defaults to Camp Murphy
- `pub fn save_trail_pref(trail: Trail)`
- `pub fn update_trail_url(trail: Trail)` - replaceState with `?trail=<slug>`
- `fn trail_from_url() -> Option<Trail>` (private)
- `fn trail_from_query(query: &str) -> Option<Trail>` (private)

**Tests:** `parses_bookmarkable_trail_query`

## src/score/

### `src/score/mod.rs`

Module hub (7 lines). Re-exports `score_color`, `score_days`, `ClosureStatus`, `DayForecast` from `heuristic`; `Params`, `RideabilityModel` from `params`.

### `src/score/params.rs`

Tunable thresholds for the trail rideability heuristics (121 lines).

**Types:**
- `enum RideabilityModel { SandPack, Drainage, MixedSurface }`
- `struct Params` - 22 public fields: `model`, `significant_rain_in`, `ideal_antecedent_in`, `min_useful_rain_in`, `max_useful_rain_in`, `pack_lookback_hours`, `ideal_hours_since_rain`, `pack_fade_hours`, `dry_timing_floor`, `drainage_hours`, `ride_day_precip_soft`, `ride_day_precip_hard`, `et0_dry_ref`, `et0_modulation`, `temp_ideal_low`, `temp_ideal_high`, `temp_ok_low`, `temp_ok_high`, `wind_ideal_low`, `wind_ideal_high`, `wind_calm_floor`, `wind_bad`, `w_pack`, `w_weather`, `w_confidence`

**`impl Default for Params`:** Camp Murphy baseline (pack 0.55 / weather 0.35 / confidence 0.10)

**`impl Params`:**
- `pub fn for_trail(trail: Trail) -> Self` - tuned params per trail:
  - Camp Murphy: SandPack, default
  - Markham: Drainage model, `significant_rain_in` 0.10, `drainage_hours` 8.5
  - Quiet Waters: MixedSurface, higher dry baseline, weather-weighted 0.55

### `src/score/heuristic.rs`

Heuristic rideability score for sandy trails that pack after rain (1048 lines).

**Constants (private):**
- `DAYLIGHT_START_HOUR` = 7.0
- `DAYLIGHT_END_HOUR` = 20.0
- `RAIN_EVENT_GAP_HOURS` = 3
- `TRACE_RAIN_IN` = 0.01

**Types:**
- `struct Factor { name: &'static str, note: String, contribution: f64, quality: f64 }`
- `enum ClosureStatus { NotApplicable, Clear, Possible }`
  - `pub fn is_possible(&self) -> bool`
- `struct DayForecast` - 16 public fields: `date`, `stars`, `score`, `factors: Vec<Factor>`, `best`, `is_past`, `is_today`, `precip_in`, `precip_3h_in: [f64;8]`, `cloud_3h_pct: [f64;8]`, `temp_max_f`, `temp_min_f`, `precip_prob_max`, `precip_prob_ride_max`, `closure_status`, `blurb`

**Private structs:**
- `DrainageStatus { quality, daylight_fraction, note, blurb, closure_status }`
- `RainEvent { total_in, end_hour, start_hour }`

**Functions (public):**
- `pub fn score_days(days: &[DayWeather], today: NaiveDate, params: &Params) -> Vec<DayForecast>` - scores every day, marks best among non-past
- `pub fn score_to_stars(score: f64) -> f64` - maps 0..=1 to 1.0..=5.0 (one decimal)
- `pub fn score_color(score: f64) -> String` - HSL color: rust red to sand to scrub green

**Functions (private):**
- `score_one(days, idx, today, p) -> DayForecast` - combines pack/weather/confidence with wet gate
- `pack_quality(days, idx, p) -> (f64, Vec<Factor>)` - antecedent rain amount + timing + ride-window wetness (SandPack/MixedSurface)
- `drainage_status(days, idx, p) -> DrainageStatus` - Markham hourly-rain closure model
- `latest_meaningful_rain_event(days, idx, p) -> Option<RainEvent>` - walks backward through hourly data, groups rain with 3h gap tolerance, ignores traces below `TRACE_RAIN_IN`
- `weather_quality(day, p) -> (f64, Vec<Factor>)` - temperature (with heat-index ding), wind (centered band), sky
- `confidence(date, today) -> (f64, Factor)` - full confidence today through day 3, tapers to 0.45 by day 7
- `drying_factor(days, idx, hours_since, p) -> f64` - ET0-based drying clock multiplier
- `hours_since_significant_rain(days, idx, threshold) -> Option<f64>`
- `make_blurb(day, pack_q, factors, p) -> String`
- `wet_period_blurb(day) -> String` - names dominant rain period (morning/afternoon/evening)
- `trap_score(x, a, b, c, d) -> f64` - trapezoid membership function
- `lerp(a, b, t) -> f64`

**Tests (16):** `post_rain_dry_day_scores_high`, `long_dry_spell_scores_low_pack`, `ride_window_rain_penalized`, `afternoon_rain_tolerated`, `overnight_rain_does_not_penalize_the_ride_window`, `light_ride_window_rain_is_tolerated_on_packed_sand`, `cloudy_slows_drying_vs_sunny`, `dead_calm_dings_wind`, `markham_uses_hourly_rain_for_same_day_closure`, `markham_afternoon_rain_open_am`, `markham_combines_rain_across_midnight`, `markham_ignores_short_showers`, `markham_trailing_trace_does_not_extend_closure`, `quiet_waters_keeps_a_higher_dry_surface_baseline`, `stars_mapping_boundaries`, `wet_blurb_names_the_dominant_period`

## src/weather/

### `src/weather/mod.rs`

Open-Meteo weather client (336 lines). Private module `types` re-exported.

**Constants:**
- `pub const TIMEZONE: &str = "America/New_York"`
- `pub const PAST_DAYS: u32 = 30` - archive history depth
- `pub const FORECAST_DAYS: u32 = 8` - today + next 7
- `pub const VIEW_DAYS: usize = 9` - yesterday + today + next 7

**Types:**
- `enum WeatherModel { GfsSeamless, Ecmwf }`
  - `pub fn label(self) -> &'static str` - `"NOAA GFS seamless (HRRR+GFS)"` / `"ECMWF IFS HRES 9km"`
  - `pub fn short(self) -> &'static str` - `"GFS"` / `"ECMWF"`
- `struct CacheEntry { fetched_at, start_date, end_date, payload }` (private, Serialize/Deserialize)

**Functions (public):**
- `pub fn load_model_pref() -> WeatherModel` - localStorage, defaults to GFS
- `pub fn save_model_pref(model: WeatherModel)`
- `pub async fn fetch_forecast(model, trail) -> Result<ForecastResponse, String>` - checks cache, fetches via gloo-net, saves cache
- `pub async fn fetch_historical_analysis(start, end, trail) -> Result<ForecastResponse, String>` - archive API for completed days
- `pub fn combine_history_and_forecast(history, forecast, today) -> Vec<DayWeather>` - retains past days from history, future days from forecast
- `pub fn build_date_range_url(model, start, end, trail) -> String` - forecast API URL for a fixed date range
- `pub fn build_historical_url(start, end, trail) -> String` - archive API URL (ecmwf_ifs model)
- `pub fn clear_cache_for_trail(model, trail)`

**Functions (private):** `build_url`, `append_weather_fields`, `load_cache`, `save_cache`, `history_cache_key`

**Tests (2):** `historical_analysis_replaces_completed_forecast_days`, `trail_requests_and_caches_are_location_specific`

### `src/weather/types.rs`

Open-Meteo API response types and day-window extraction (302 lines).

**Constants (private):**
- `RIDE_START_HOUR` = 8, `RIDE_END_HOUR` = 12, `PARK_CLOSE_HOUR` = 20
- `HOURS_PER_DAY` = 24, `THREE_HOUR_BUCKETS` = 8

**Types (Deserialize + Serialize):**
- `struct ForecastResponse { latitude: f64, longitude: f64, timezone: Option<String>, daily: DailyBlock, hourly: Option<HourlyBlock> }`
- `struct DailyBlock { time: Vec<String>, precipitation_sum, precipitation_probability_max, temperature_2m_max, temperature_2m_min, apparent_temperature_max, wind_speed_10m_max, wind_gusts_10m_max, et0_fao_evapotranspiration }` (all `Vec<Option<f64>>` except time)
- `struct HourlyBlock { time: Vec<String>, precipitation, precipitation_probability (serde default), cloud_cover }` (all `Vec<Option<f64>>`)
- `struct DayWeather` (Clone, Debug) - 14 fields: `date: NaiveDate`, `precip_in`, `precip_prob_max`, `precip_prob_ride_max`, `temp_max_f`, `temp_min_f`, `apparent_max_f`, `wind_max_mph`, `gust_max_mph`, `et0`, `precip_ride_in`, `precip_pm_in`, `precip_hourly_in: [f64;24]`, `precip_3h_in: [f64;8]`, `cloud_3h_pct: [f64;8]`

**`impl ForecastResponse`:**
- `pub fn days(&self) -> Vec<DayWeather>` - parses daily + hourly into per-day `DayWeather`
- `fn hourly_precip_for_date(&self, date_str) -> [f64;24]` (private)
- `fn precip_windows_for_date(&self, date_str) -> (f64, f64)` (private) - ride window (8AM-noon) and PM (noon-sundown) totals
- `fn prob_ride_max_for_date(&self, date_str, daily_fallback) -> f64` (private)
- `fn three_hour_weather_for_date(&self, date_str) -> ([f64;8], [f64;8])` (private) - 3h rain and cloud summaries

**Functions (private):**
- `fn hour_of(ts: &str) -> Option<u32>` - parses local hour from ISO8601 timestamp
- `fn opt(v: Option<&Option<f64>>) -> f64` - unwraps nested Option

**Tests (1):** `rain_windows_start_when_the_park_opens`

## src/bin/

### `src/bin/jaycast.rs`

Native CLI binary (324 lines, requires `cli` feature). Uses `ureq` for HTTP.

**Functions (private):**
- `fn main()` - dispatches to `run()`, prints help on error, exits 2
- `fn run() -> Result<(), String>` - subcommands: `analyze`, `backtest`, `--help`/`-h`/`help`
- `fn analyze(args) -> Result<(), String>` - scores a date or inclusive range against gfs/ecmwf/both; fetches historical archive + forecast, merges, prints per-day analysis
- `fn fetch_forecast(url, source) -> Result<ForecastResponse, String>` - ureq GET + JSON deserialize
- `fn print_range_analysis(source, lat, lon, start, end, days, today, trail) -> Result<(), String>`
- `fn print_analysis(date, source, lat, lon, weather, score)` - formatted stdout: stars, rain totals, 3h breakdown, factor notes
- `fn parse_date_range(value) -> Result<(NaiveDate, NaiveDate), String>` - supports `YYYY-MM-DD` or `YYYY-MM-DD:YYYY-MM-DD`
- `fn format_three_hour(values: &[f64;8], precision: usize) -> String`
- `fn backtest(args) -> Result<(), String>` - loads a JSON fixture, rolls `today` across all dates, runs `score_days`, prints per-day table with closure status and summary counts
- `fn print_help()`

**Tests (1):** `parses_inclusive_date_ranges`

## tests/

### `tests/fixtures/`

| File | Description |
|------|-------------|
| `markham-2mo.json` | Open-Meteo archive API response for Markham Park (ecmwf_ifs, 2026-05-01 to 2026-07-12, 73 days, 1752 hourly points, 57KB). Contains daily and hourly precipitation, probability, cloud cover, temperature, wind, and ET0 fields. Used by `cargo run --features cli --bin jaycast -- backtest tests/fixtures/markham-2mo.json markham`. |
| `closures.txt` | Notes of closures gathered from Facebook and likely missing close/open events |
