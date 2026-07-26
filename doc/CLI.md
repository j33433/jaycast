# CLI reference (jaycast)

Native-only commands for score analysis, historical backtesting, and Xweather
ground-truth rain feed management.

## Build

```sh
cargo build --features cli --bin jaycast
```

Output: `target/debug/jaycast`.

## Commands

### `analyze` — score one or more days

Scores a date or inclusive range against GFS, ECMWF, or both. Fetches historical
archive for past days and forecast for future ones.

```text
jaycast analyze [trail] [date|start:end] [gfs|ecmwf|both] [--gauge rain.json]
```

Defaults: trail=Camp Murphy, date=today, model=both.

**Trail slugs:** `camp-murphy` (or omit), `markham`, `quiet-waters`.

**`--gauge` flag:** Optionally provide an Xweather ground-truth rain.json feed
(see `xweather publish`). When present, gauge tips replace model precip for
completed hours so scores match the web UI. For today, only hours before the
current wall-clock hour are applied.

**Examples:**

```sh
# Today at Camp Murphy, both models (defaults)
cargo run --features cli --bin jaycast -- analyze

# Same, trail + model explicit
cargo run --features cli --bin jaycast -- analyze camp-murphy both

# Markham yesterday with GFS only
cargo run --features cli --bin jaycast -- analyze markham gfs

# 2-day range at Quiet Waters, ECMWF only
cargo run --features cli --bin jaycast -- analyze quiet-waters 2026-07-22:2026-07-23 ecmwf

# Past week at Markham, both models
cargo run --features cli --bin jaycast -- analyze markham 2026-07-17:2026-07-24 both

# With ground-truth gauge rain (matches web UI scores)
cargo run --features cli --bin jaycast -- analyze camp-murphy 2026-07-25:2026-07-26 both --gauge assets/rain.json
```

**Output (per day):**

```
2026-07-24 | NOAA GFS seamless (HRRR+GFS) | grid 27.022, -80.112
  score 2.7 stars (44%) | rain 0.00" total, 0.00" 8 AM-noon, 0.00" noon-sundown | 19% chance 8 AM-noon (daily max 19%)
  rain by 3h: 00: 0.00 | 03: 0.00 | 06: 0.00 | 09: 0.00 | 12: 0.00 | 15: 0.00 | 18: 0.00 | 21: 0.00
  cloud by 3h: 00: 6 | 03: 36 | 06: 98 | 09: 35 | 12: 28 | 15: 7 | 18: 2 | 21: 4
  feels like: AM 96°F / PM 98°F
  Recent rain: 0.00 in rain in prior ~48h
  Trail conditions: no recent rain - sand may be soft
  Rain during ride: dry ride window (19% chance 8 AM-noon)
  Temperature: high 88°F / low 80°F (feels 102°F)
  Wind: wind 12 mph, gusts 17 mph
  Sky: 19% highest rain chance
  Forecast reliability: today (near-term forecast)
```

When `both` models are requested, each model prints its own block for the same
date range so you can compare.

---

### `backtest` — roll `today` through a fixture

Loads a historical Open-Meteo archive response (the fixture), then rolls the
`today` pointer forward across every day in the file. At each step the scorer
sees only the data that would have been available on that day, so the result
mimics real-world daily scoring.

```text
jaycast backtest <fixture.json> [trail]
```

Default trail: Markham.

**Fixture:** a JSON `ForecastResponse` from the Open-Meteo archive API. The
repository includes one at `tests/fixtures/markham-2mo.json` (73 days, ECMWF IFS
2026-05-01 through 2026-07-12).

**Example:**

```sh
cargo run --features cli --bin jaycast -- backtest tests/fixtures/markham-2mo.json markham
```

**Output (last lines):**

```
2026-07-11  2.4  maybe closed         0.26 in rain; maybe closed  (0.21" rain)
2026-07-12  3.8  maybe closed         0.18 in rain; open AM, PM risk  (0.20" rain)
------------------------------------------------------------
summary: 36 likely open, 37 maybe closed (out of 73 days)
```

The table shows each date, star score, closure status (likely open / maybe
closed / n/a), trail-status factor note, and daily rain total. The summary line
at the bottom counts how many days fell into each closure bucket.

---

### `xweather` — ground-truth gauge rain feed

Xweather subcommands require the `XWEATHER_API_KEY` environment variable. See
[XWEATHER.md](XWEATHER.md) for setup and station details.

```text
jaycast xweather publish --out <PATH> [--days N] [--cache PATH]
jaycast xweather dump [--days N] [--cache PATH]
jaycast xweather rescan [trail] [--limit N] [--days N] [--candidates N]
```

#### `xweather publish`

Fetches hourly gauge tips for Markham, Camp Murphy, and Quiet Waters stations,
then atomically writes a static JSON feed that the WASM app loads at
`/jaycast/rain.json`.

| Flag | Default | Description |
|------|---------|-------------|
| `--out` | required | Output JSON path |
| `--days` | 2 | Full local days to include (yesterday + today) |
| `--cache` | beside --out or cwd | Past-day cache path |

**Examples:**

```sh
XWEATHER_API_KEY='CLIENTID_CLIENTSECRET' \
  cargo run --features cli --bin jaycast -- xweather publish \
    --out assets/rain.json --cache data/xweather-cache.json
```

Without `--cache` the cache lands beside `--out` (e.g. `assets/.jaycast-xweather-cache.json`). Past completed days are only fetched once then served from cache.

#### `xweather dump`

Same as publish but writes pretty-printed JSON to stdout instead of to a file. You almost always want `--cache` so the next run reuses past-day data.

```sh
XWEATHER_API_KEY='CLIENTID_CLIENTSECRET' \
  cargo run --features cli --bin jaycast -- xweather dump --cache data/xweather-cache.json
```

#### `xweather rescan`

Discovers nearby PWS and mesonet stations for a trail, ranks them by distance
tier and wet-day agreement, and prints a recommendation table. This is a
read-only audit — it does not modify the feed station table in
`src/xweather/mod.rs`.

| Flag | Default | Description |
|------|---------|-------------|
| `[trail]` | all trails | Slug to scan, or omit for all |
| `--limit` | 15 | Closest stations per filter |
| `--days` | 7 | Lookback days for QC |
| `--candidates` | 12 | QC candidates per trail |

**Example:**

```sh
XWEATHER_API_KEY='CLIENTID_CLIENTSECRET' \
  cargo run --features cli --bin jaycast -- xweather rescan camp-murphy --limit 20 --days 7
```

See [XWEATHER.md](XWEATHER.md) for the per-trail station table, API endpoints,
QC rules, and data interpretation.

---

## Environment

| Variable | Used by | Required |
|----------|---------|----------|
| `XWEATHER_API_KEY` | `xweather` subcommands | Yes (join client_id + `_` + client_secret) |

## Quick reference

```text
Build:              cargo build --features cli --bin jaycast
Run:                cargo run --features cli --bin jaycast -- <args>

Analyze today:      jaycast analyze
Analyze date:       jaycast analyze markham 2026-07-22 ecmwf
Analyze range:      jaycast analyze quiet-waters 2026-07-22:2026-07-23 both
Analyze w/ gauge:    jaycast analyze camp-murphy 2026-07-25:2026-07-26 both --gauge rain.json
Backtest fixture:   jaycast backtest tests/fixtures/markham-2mo.json markham
Publish rain feed:  jaycast xweather publish --out rain.json
Dump rain feed:     jaycast xweather dump
Rescan stations:    jaycast xweather rescan markham
```
