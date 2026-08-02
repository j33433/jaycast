# CODEMAP

File-level map of the jaycast repository: source, assets, test data, and config.

```
jaycast/
  .gitignore
  Cargo.toml
  Cargo.lock
  LICENSE
  Trunk.toml
  index.html
  robots.txt
  sitemap.xml
  README.md
  about/
    index.html
  doc/
    CLI.md
    CODEMAP.md
    TRAILS.md
    XWEATHER.md
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
    rain_feed.rs
    score/
      mod.rs
      params.rs
      heuristic.rs
    weather/
      mod.rs
      types.rs
    xweather/
      mod.rs
      rescan.rs
    bin/
      jaycast.rs
  tests/
    fixtures/
      markham-2mo.json
      closures.txt
      qw.txt
```

## Root

| File | Description |
|------|-------------|
| `Cargo.toml` | Package manifest. Crate types `cdylib` + `rlib`. Deps: leptos 0.7 (csr), gloo-net, serde, serde_json, chrono, wasm-bindgen, web-sys, console_error_panic_hook. Native-only: ureq. Feature `cli` gates the binary (analyze, backtest, xweather). Release profile: `opt-level="z"`, lto, single codegen unit. |
| `Cargo.lock` | Dependency lockfile (auto-generated). |
| `LICENSE` | GPL-3.0-or-later. |
| `Trunk.toml` | Trunk build config. Target `index.html`, dist dir `dist/`, public URL `/jaycast/`. |
| `index.html` | App entry HTML. Inline JS applies saved theme before render. OpenGraph/Twitter meta, JSON-LD structured data (WebApplication + three SportsActivityLocation/Place). Trunk asset links for icon, CSS, WASM, and copy-file/copy-dir directives for SVGs, LICENSE, robots.txt, sitemap.xml, and the `about/` page. |
| `robots.txt` | Allows `/jaycast/`, declares sitemap URL. |
| `sitemap.xml` | URLs: home (daily) and the about page (monthly). |
| `about/index.html` | Static, JS-free landing page served at `/jaycast/about/`. Trail guides for the three parks, how scoring works, FAQ. JSON-LD: `AboutPage`, `FAQPage`, `WebSite`, three `SportsActivityLocation`/`Place` with geo. Copied to `dist/about/` via Trunk `copy-dir`. Deep links into the app with `?camp-murphy`/`?markham`/`?quiet-waters`. |
| `README.md` | Project description, trail profiles, weekend comparison grid, develop/test/build instructions, CLI usage, score model summary. |
| `doc/CLI.md` | CLI reference with examples for `analyze`, `backtest`, `xweather publish/dump/rescan`. |
| `doc/CODEMAP.md` | This file. It is the file-level map of the project. |
| `doc/TRAILS.md` | Per-trail surface character, rain response, score weights, gauge stations. |
| `doc/XWEATHER.md` | Xweather auth, station map, QC rules, feed CLI reference. |

## assets/

| File | Description |
|------|-------------|
| `style.css` | Application stylesheet. Florida scrub palette with dark (default) and light themes. CSS custom properties for jay blue, scrub green, sand, accent, warn, bad, star, rain. Styles for header, trail logo, location chooser dialog, help dialog (annotated demo day card with thought bubbles), hero, model toggle, theme toggle, weekend-warrior toggle, timeline nav, day cards (score-tinted gradients, AM/PM temp side borders, weekend/best/selected/past/today states), rain-wave and cloud-wave SVG backgrounds, detail panel with factor bars, vertical weekend comparison grid (day cards with trail rows, Best badge), footer, skeleton shimmer loader. Responsive breakpoint at 30rem. |
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

- Private modules: `app`, `theme`, `rain_feed`
- Public modules: `score`, `trails`, `weather`
- Native-only (`#[cfg(not(target_arch = "wasm32"))]`): `xweather`
- Re-exports: `apply_gauge_to_days`, `GaugeRain`. Native also re-exports `load_gauge_from_file`
- `#[wasm_bindgen(start)] pub fn main()` - entry point. Sets panic hook, mounts `App` to body

### `src/app.rs`

Leptos UI component tree. All components and helpers are private.

**Types:**
- `enum LoadState { Loading, Ready(Vec<DayForecast>), Error(String) }`
- `struct WeekendGridData { dates, map, best_per_day }` - pure data prep for the multi-trail comparison grid. `build(all, today)` indexes scored days by trail/date and picks Best per day

**Components:**
- `App()` - root. Manages state signals (load state, selected day, view start, refreshed_at, model, trail, location/help dialogs, grid coords, theme, gauge_rain, first load, weekend_warrior, multi_days, multi_loading). Runs single-trail fetch+score effect + 15-min auto-refresh loop (timeline only. Grid is refreshed on toggle/model switch). `load_weekend` fetches history+forecast+gauge for all three trails. Trail switch exits weekend view. Model switch refreshes grid when open
- `LocationDialog(open, selected, on_change)` - modal trail chooser
- `HelpDialog(open)` - modal guide: short site explainer + static annotated day card (clouds, model rain, gauge rain, now marker, score, detail) with thought-bubble callouts
- `LoadingView()` - skeleton loading state
- `ErrorView(message, on_retry)` - error display with retry
- `ReadyView(days, selected, view_start, refreshed_at, model, trail, grid_lat, grid_lon, theme, gauge_rain, weekend_warrior, multi_days, multi_loading, on_switch, on_select_trail, on_toggle_weekend)` - composes Hero + either Timeline or WeekendWarriorView + footer. `on_select_day` jumps from grid cell to trail timeline with that day selected
- `Hero(days, refreshed_at, model, theme, weekend_warrior, on_switch, on_toggle_weekend)` - best ride window, GFS/ECMWF toggle, theme toggle (inline sun/moon SVG), weekend-warrior toggle (2×2 grid icon)
- `TimelineNav(days, view_start, selected)` - Older/Today/Newer scroll nav
- `Timeline(days, view_start, selected, trail, gauge_rain)` - day cards with rain/cloud wave SVG backgrounds, AM/PM temp border colors, blue gauge curve overlay, Markham Facebook status link
- `WeekendWarriorView(multi_days, multi_loading, trail, on_select_day)` - loading/empty/ready dispatch for multi-trail comparison
- `WeekendGrid(grid, today, trail, on_select_day)` - vertical day cards (today + next 5). Each card lists all three trails as touch-friendly rows with stars, blurb, and Best badge

**Helper functions:**
- `load_selected_pref(trail) / save_selected_pref(trail, date)` - per-trail expanded day card in localStorage
- `load_weekend_pref() / save_weekend_pref(active)` - weekend grid toggle in localStorage (`jaycast:weekend-warrior`)
- `day_detail_view(d)` - detail panel with factor breakdown bars
- `stars_str(n) -> String`
- `score_style(score) -> String`
- `day_card_style(score, am_vs_avg_f, pm_vs_avg_f) -> String` - score tint + AM/PM border colors
- `rain_wave_path(rain_3h_in) -> String`
- `cloud_wave_path(cloud_3h_pct) -> String`
- `smooth_wave_path(values, height) -> String` - Catmull-Rom spline path
- `format_long(d) -> String`
- `format_short(d) -> String`
- `format_dow(d) -> String`
- `is_weekend(d) -> bool`
- `haversine_km(lat1, lon1, lat2, lon2) -> f64`
- `source_distance_line(trail, grid_lat, grid_lon, gauge)` - footer: forecast distance + gauge distances + observation timestamps
- `format_weather_as_of(init_time, fallback_fetched_at) -> String` - shows model init time in local time (for example, "forecast as of 6:00 AM") with fallback to fetch time

### `src/theme.rs`

Light/dark theme preference with localStorage persistence.

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

Trail definitions and localStorage/URL persistence.

**Types:**
- `enum Trail { CampMurphy, Markham, QuietWaters }` - derives `Clone, Copy, Debug, PartialEq, Eq, Hash`

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
- `pub fn rain_gauge_coords(self) -> &'static [(f64, f64)]` - gauge lat/lon pairs for footer distance display
- `pub fn from_slug(value: &str) -> Option<Self>`

**Functions:**
- `pub fn load_trail_pref() -> Trail` - reads from URL query then localStorage, defaults to Camp Murphy
- `pub fn save_trail_pref(trail: Trail)`
- `pub fn update_trail_url(trail: Trail)` - replaceState with `?<slug>`
- `fn trail_from_url() -> Option<Trail>` (private)
- `fn trail_from_query(query: &str) -> Option<Trail>` (private)

**Tests:** `parses_bookmarkable_trail_query`

## src/score/

### `src/score/mod.rs`

Module hub. Re-exports `score_color`, `score_days`, `ClosureStatus`, `DayForecast`, `BlurbTag`, `TagTone` from `heuristic`. It re-exports `Params`, `RideabilityModel` from `params`.

### `src/score/params.rs`

Tunable thresholds for the trail rideability heuristics.

**Types:**
- `enum RideabilityModel { SandPack, Drainage, MixedSurface }`
- `struct Params` - public trail tuning fields including rain thresholds, pack timing, `drainage_hours`, `drainage_hours_per_in`, `drainage_max_hours`, `mud_clear_hours`, ride-window rain, ET0, temperature, wind, and score weights

**`impl Default for Params`:** Camp Murphy baseline (pack 0.55 / weather 0.35 / confidence 0.10)

**`impl Params`:**
- `pub fn for_trail(trail: Trail) -> Self` - tuned params per trail:
  - Camp Murphy: SandPack, default
  - Markham: Drainage model, `significant_rain_in` 0.10. Drainage is 8.5 base hours + 8 hours/in, capped at 18.5 hours
  - Quiet Waters: MixedSurface, higher dry baseline, weather-weighted 0.55

### `src/score/heuristic.rs`

Heuristic rideability score for sandy trails that pack after rain.

**Constants (private):**
- `DAYLIGHT_START_HOUR` = 7.0
- `DAYLIGHT_END_HOUR` = 20.0
- `RAIN_EVENT_GAP_HOURS` = 3
- `TRACE_RAIN_IN` = 0.01
- `COMFORT_WINDOW` = 7, `COMFORT_THRESHOLD` = 4.0

**Types:**
- `struct Factor { name: &'static str, note: String, contribution: f64, quality: f64 }`
- `enum TagTone { Good, Bad, Neutral }` - tone of a blurb tag (green/red/neutral in the UI)
- `struct BlurbTag { text: &'static str, tone: TagTone }` - one short colored keyword in a day's blurb
- `enum ClosureStatus { NotApplicable, Clear, Possible }`
  - `pub fn is_possible(&self) -> bool`
- `struct DayForecast` - public fields: `date`, `stars`, `score`, `factors: Vec<Factor>`, `best`, `is_past`, `is_today`, `precip_in`, `precip_3h_in: [f64;8]`, `cloud_3h_pct: [f64;8]`, `temp_max_f`, `temp_min_f`, `apparent_am_f`, `apparent_pm_f`, `precip_prob_max`, `precip_prob_ride_max`, `closure_status`, `blurb` (tag texts joined with ", "), `tags: Vec<BlurbTag>`, `comfort_note` ("AM"/"PM" badge for cool outliers), `comfort_detail` ("6° below avg AM"), `am_vs_avg_f` (delta from 7-day AM avg), `pm_vs_avg_f`

**Private structs:**
- `DrainageStatus { quality, daylight_fraction, note, tag, closure_status }`
- `RainEvent { total_in, end_hour, start_hour }`

**Functions (public):**
- `pub fn score_days(days: &[DayWeather], today: NaiveDate, params: &Params) -> Vec<DayForecast>` - scores every day, marks best among non-past, annotates comfort outliers
- `pub fn score_days_as_of(..., as_of_hour: Option<u32>)` - same, but Markham drainage on calendar today only scores rain before `as_of_hour`. Meaningful future PM rain adds a warning without lowering current drainage
- `pub fn score_to_stars(score: f64) -> f64` - maps 0..=1 to 1.0..=5.0 (one decimal)
- `pub fn score_color(score: f64) -> String` - HSL color: rust red to sand to scrub green

**Functions (private):**
- `score_one(days, idx, today, p) -> DayForecast` - combines pack/weather/confidence with wet gate, builds `tags`, joins them into `blurb`
- `annotate_comfort_outliers(forecasts)` - marks days whose AM/PM apparent temp is unusually cool vs trailing 7-day average. Sets `comfort_note`, `comfort_detail`, `am_vs_avg_f`, `pm_vs_avg_f`, and `cool`/`hot` tags. UI only. Does not affect scores.
- `pack_quality(days, idx, p) -> (f64, Vec<Factor>)` - antecedent rain amount + timing + ride-window wetness (SandPack/MixedSurface)
- `drainage_status(days, idx, p) -> DrainageStatus` - Markham hourly-rain closure model with amount-dependent drainage duration
- `latest_meaningful_rain_event(days, idx, p) -> Option<RainEvent>` - walks backward through hourly data, groups rain with 3h gap tolerance, ignores traces below `TRACE_RAIN_IN`
- `weather_quality(day, p) -> (f64, Vec<Factor>)` - temperature (with heat-index ding), wind (centered band), sky
- `confidence(date, today) -> (f64, Factor)` - full confidence today through day 3, tapers to 0.45 by day 7
- `drying_factor(days, idx, hours_since, p) -> f64` - ET0-based drying clock multiplier
- `hours_since_significant_rain(days, idx, threshold) -> Option<f64>`
- `make_tags(day, pack_q, factors, p) -> Vec<BlurbTag>` - surface + rain tags for non-drainage models
- `surface_tag(p, pack_q, factors) -> Option<BlurbTag>` - short surface keyword ("firm", "fast", "soft", "drying", ...)
- `ride_rain_tag(day) -> Option<BlurbTag>` - "rain am" when meaningful ride-window rain
- `build_drainage_tags(status, day, p) -> Vec<BlurbTag>` - Markham advisory tag only when `ClosureStatus::Possible`; clear days emit nothing (status is unverified, day card links to Facebook)
- `wet_period_tag(day) -> BlurbTag` - "rain am"/"rain pm"/"rainy" from dominant 3h rain period
- `wet_label_tag(label) -> BlurbTag` - "wet am"/"wet pm"/"wet" for MixedSurface trails
- `trap_score(x, a, b, c, d) -> f64` - trapezoid membership function
- `lerp(a, b, t) -> f64`

**Tests include:** `post_rain_dry_day_scores_high`, `long_dry_spell_scores_low_pack`, `ride_window_rain_penalized`, `afternoon_rain_tolerated`, `overnight_rain_does_not_penalize_the_ride_window`, `light_ride_window_rain_is_tolerated_on_packed_sand`, `cloudy_slows_drying_vs_sunny`, `dead_calm_dings_wind`, `markham_moderate_overnight_rain_reopens_around_midday`, `markham_heavy_pm_rain_carries_into_next_morning`, `markham_warns_about_future_pm_rain_without_tanking_morning`, `markham_afternoon_rain_open_am`, `markham_combines_rain_across_midnight`, `markham_ignores_short_showers`, `markham_trailing_trace_does_not_extend_closure`, `quiet_waters_keeps_a_higher_dry_surface_baseline`, `stars_mapping_boundaries`, `wet_blurb_names_a_single_wet_half`, `tags_carry_expected_tone`, `good_outlier_detected_when_cooler_than_trend`, `warm_morning_records_positive_am_delta`, `warm_afternoon_records_positive_pm_delta`, `no_outlier_when_within_trend`, `outlier_needs_trailing_data`

## src/weather/

### `src/weather/mod.rs`

Open-Meteo weather client. Private module `types` re-exported.

**Constants:**
- `pub const TIMEZONE: &str = "America/New_York"`
- `pub const PAST_DAYS: u32 = 30` - archive history depth
- `pub const FORECAST_DAYS: u32 = 6` - today + next 5
- `pub const VIEW_DAYS: usize = 7` - yesterday + today + next 5

**Types:**
- `enum WeatherModel { GfsSeamless, Ecmwf }`
  - `pub fn label(self) -> &'static str`
  - `pub fn short(self) -> &'static str`
  - `fn endpoint(self) -> &'static str` - base API URL
  - `fn models_param(self) -> Option<&'static str>` - `"gfs_seamless"` / `"ecmwf_ifs"`
  - `fn cache_key(self, trail) -> String` - localStorage cache key
  - `fn meta_domain(self) -> &'static str` - `"ncep_hrrr_conus"` / `"ecmwf_ifs"` for metadata API
- `type WeatherFetch = (ForecastResponse, i64)`

**Functions (public):**
- `pub fn load_model_pref() -> WeatherModel` - localStorage, defaults to ECMWF
- `pub fn save_model_pref(model: WeatherModel)`
- `pub async fn fetch_model_init_time(model) -> Option<i64>` - fetches `last_run_initialisation_time` from Open-Meteo metadata API (`/data/{domain}/static/meta.json`), 30-min localStorage cache
- `pub async fn fetch_forecast(model, trail) -> Result<WeatherFetch, String>` - checks cache, fetches via gloo-net, saves cache
- `pub async fn fetch_historical_analysis(model, start, end, trail) -> Result<WeatherFetch, String>` - archive API for completed days using the selected model
- `pub fn combine_history_and_forecast(history, forecast, today) -> Vec<DayWeather>` - retains past days from history, future days from forecast
- `pub fn build_date_range_url(model, start, end, trail) -> String` - forecast API URL for a fixed date range
- `pub fn build_historical_url(model, start, end, trail) -> String` - archive API URL (`gfs_seamless` or `ecmwf_ifs`)
- `pub fn clear_cache_for_trail(model, trail)`

**Functions (private):** `build_url`, `append_weather_fields`, `load_cache`, `save_cache`, `history_cache_key`

**Tests (3):** `historical_analysis_replaces_completed_forecast_days`, `trail_requests_and_caches_are_location_specific`, `historical_archive_follows_selected_model`

### `src/weather/types.rs`

Open-Meteo API response types and day-window extraction.

**Constants (private):**
- `RIDE_START_HOUR` = 8, `RIDE_END_HOUR` = 12, `PARK_CLOSE_HOUR` = 20
- `HOURS_PER_DAY` = 24, `THREE_HOUR_BUCKETS` = 8

**Types (Deserialize + Serialize):**
- `struct ForecastResponse { latitude: f64, longitude: f64, timezone: Option<String>, daily: DailyBlock, hourly: Option<HourlyBlock> }`
- `struct DailyBlock { time: Vec<String>, precipitation_sum, precipitation_probability_max, temperature_2m_max, temperature_2m_min, apparent_temperature_max, wind_speed_10m_max, wind_gusts_10m_max, et0_fao_evapotranspiration }` (all `Vec<Option<f64>>` except time)
- `struct HourlyBlock { time: Vec<String>, precipitation, precipitation_probability (serde default), cloud_cover, apparent_temperature }` (all `Vec<Option<f64>>`)
- `struct DayWeather` (Clone, Debug) - fields: `date: NaiveDate`, `precip_in`, `precip_prob_max`, `precip_prob_ride_max`, `temp_max_f`, `temp_min_f`, `apparent_max_f`, `apparent_am_f`, `apparent_pm_f`, `wind_max_mph`, `gust_max_mph`, `et0`, `precip_ride_in`, `precip_pm_in`, `precip_hourly_in: [f64;24]`, `precip_3h_in: [f64;8]`, `cloud_3h_pct: [f64;8]`

**`impl ForecastResponse`:**
- `pub fn days(&self) -> Vec<DayWeather>` - parses daily + hourly into per-day `DayWeather`
- `fn hourly_precip_for_date(&self, date_str) -> [f64;24]` (private)
- `fn precip_windows_for_date(&self, date_str) -> (f64, f64)` (private) - ride window (8AM-noon) and PM (noon-sundown) totals
- `fn apparent_windows_for_date(&self, date_str) -> (f64, f64)` (private) - average apparent temp for AM and PM windows
- `fn prob_ride_max_for_date(&self, date_str, daily_fallback) -> f64` (private)
- `fn three_hour_weather_for_date(&self, date_str) -> ([f64;8], [f64;8])` (private) - 3h rain and cloud summaries

**Functions (private):**
- `fn hour_of(ts: &str) -> Option<u32>` - parses local hour from ISO8601 timestamp
- `fn opt(v: Option<&Option<f64>>) -> f64` - unwraps nested Option

**Tests (2):** `rain_windows_start_when_the_park_opens`, `apparent_windows_averaged_correctly`

### `src/rain_feed.rs`

Fetches static `/jaycast/rain.json` (Xweather hourly gauge tips). Indexes max tip per hour across stations per trail/day. Unusable when every station's today day is `stale` (footer warning. No overlay or scoring blend). When usable, past completed hours replace model precip before scoring. Today uses only non-stale stations. Day cards draw blue gauge curve over the model rain wave when usable.

Exposes `last_seen_ts(trail) -> Option<i64>` for displaying observation timestamps in the footer.

### `src/xweather/mod.rs`

Native-only Xweather hourly rain feed builder. Auth via `XWEATHER_API_KEY`. Fetches `/observations/archive/{id}` for Markham, Camp Murphy, and Quiet Waters stations. Buckets `precipSinceLastObIN` into 24 hourly tip totals (inches). Writes schema-versioned JSON. Past-day cache. `rescan` ranks nearby gauges and rejects bad rain meters.

**Types:** `Feed`, `TrailFeed`, `StationFeed`, `DayFeed` - JSON serialization types for the rain feed.

**Private module:** `rescan` - station discovery and QC ranking.

**CLI (via bin):** `publish`, `dump`, `rescan [trail]`.

**Tests:** day range, local hour parse, tip bucketing, stale detection, cache.

### `src/xweather/rescan.rs`

Discovers nearby PWS and mesonet stations via Xweather `/observations/closest`, rejects blocklisted / no-precip / stuck-zero gauges, ranks by distance tier (primary ≤5 mi, backup 5–10 mi). Print-only audit. Does not modify the feed station table.

**Tests:** distance tiers, network kind parsing, stuck-zero detection, evaluation shape, haversine.

## src/bin/

### `src/bin/jaycast.rs`

Native CLI binary (requires `cli` feature). Uses `ureq` for HTTP.

**Functions (private):**
- `fn main()` - dispatches to `run()`, prints help on error, exits 2
- `fn run() -> Result<(), String>` - subcommands: `analyze`, `backtest`, `xweather`, `--help`/`-h`/`help`
- `fn analyze(args) -> Result<(), String>` - scores a date or inclusive range against gfs/ecmwf/both. Fetches historical archive + forecast. Merges and prints per-day analysis
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
| `markham-2mo.json` | Open-Meteo archive API response for Markham Park (ecmwf_ifs, 2026-05-01 to 2026-07-12, 73 days, 57KB). Contains daily and hourly precipitation, probability, cloud cover, temperature, wind, and ET0 fields. Used by `cargo run --features cli --bin jaycast -- backtest tests/fixtures/markham-2mo.json markham`. |
| `closures.txt` | Notes of closures gathered from Facebook and likely missing close/open events |
| `qw.txt` | Notes on condition of Quiet Waters |
