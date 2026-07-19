# Xweather ground-truth rain notes (jaycast)

Ground-truth rain from **real stations** (PWS, MADIS/CWOP mesonet, ASOS) via the [Xweather Weather API](https://www.xweather.com/docs/weather-api), as a complement to Open-Meteo models. Browser jaycast stays model-only (no API keys). This doc is for **local queries** — Xweather already stores multi-day history, so you do **not** need a poll/archive loop like aprs.fi.

Supersedes the aprs.fi approach in `APRS.md` for the same purpose.

## Why Xweather (vs aprs.fi)

| Need | aprs.fi | Xweather |
|------|---------|----------|
| Latest station wx | `what=wx` only | `/observations` |
| Multi-hour / multi-day history | **None** → must poll + store | `/observations/archive`, `/observations/summary` |
| Find stations near trail | Must know callsigns | `closest?p=lat,lon` |
| Same CWOP IDs | `EW8181`, `CW8019`, … | MADIS: `MID_E8181`, `MID_C8019`, `MID_D4511` |
| Daily rain totals | DIY from `rain_1h` / `rain_mn` | `summary.precip.totalIN` |

## Auth

1. Register an app at [Xweather](https://www.xweather.com/) → client ID + client secret.
2. Put credentials in the environment only — **never commit them**:

   ```bash
   # client_id and client_secret joined with a single underscore
   export XWEATHER_API_KEY='CLIENTID_CLIENTSECRET'
   ```

3. Split for requests (first `_` only):

   ```bash
   CID="${XWEATHER_API_KEY%%_*}"
   CSEC="${XWEATHER_API_KEY#*_}"
   AUTH="client_id=${CID}&client_secret=${CSEC}"
   ```

4. HTTPS only:

   `https://data.api.xweather.com/...`

5. Local / server use only. Do not ship the secret into the WASM browser build.

## Endpoints

Base: `https://data.api.xweather.com`

### Discover stations near a trailhead

```text
GET /observations/closest?p={lat},{lon}&limit=N&filter=pws|mesonet|metar|allstations&{AUTH}
```

Useful filters:

| `filter=` | What you get |
|-----------|----------------|
| `pws` | Personal weather stations |
| `mesonet` | MADIS mesonet (includes CWOP-style gauges) |
| `metar` | Airport ASOS/METAR |
| `allstations` | Mixed, closest first |

### Daily gauge totals (preferred for day scores)

```text
GET /observations/summary/{id}?from=-7days&to=now&plimit=7&{AUTH}
```

- **`plimit` is required for multi-day** — without it you often get a single day.
- Each period’s `summary.precip` has `totalIN` / `totalMM`, `method` (e.g. `EOD24hr`, `sum`), and QC fields.
- `from` / `to` accept relative forms (`-7days`, `now`) or `YYYY-MM-DD`.

### Full local day of observations

```text
GET /observations/archive/{id}?from=YYYY-MM-DD&{AUTH}
```

- One **local calendar day** per request (midnight–23:59 at the station). `to` is not used like other endpoints.
- Precip fields vary by station; common ones:
  - `ob.precipIN` / `ob.precipMM` — period precip (definition varies: hour, last 60 min, or since last ob)
  - `ob.precipSinceMidnightIN` — running since local midnight
  - `ob.precipSinceLastObIN` — tip since previous sample
- For daily totals, prefer `/observations/summary` over summing archive yourself.

### Trailhead analyzed precip (not a tipping bucket)

Gridded / analyzed conditions at lat/lon — useful context next to gauges, **not** pure ground truth.

```text
GET /conditions/{lat},{lon}?from=-48hours&to=now&plimit=48&{AUTH}
GET /conditions/summary/{lat},{lon}?from=-7days&to=now&plimit=7&{AUTH}
```

Hourly periods expose `precipIN` / `precipMM`. Daily summary exposes `precip.totalIN` / `totalMM`.

### One-shot checks

```bash
CID="${XWEATHER_API_KEY%%_*}"
CSEC="${XWEATHER_API_KEY#*_}"
AUTH="client_id=${CID}&client_secret=${CSEC}"

# Closest PWS + mesonet near Markham
curl -sS "https://data.api.xweather.com/observations/closest?p=26.12983,-80.35090&limit=10&filter=pws&${AUTH}" | jq .
curl -sS "https://data.api.xweather.com/observations/closest?p=26.12983,-80.35090&limit=10&filter=mesonet&${AUTH}" | jq .

# 7-day daily rain for primary Markham CWOP
curl -sS "https://data.api.xweather.com/observations/summary/MID_E8181?from=-7days&to=now&plimit=7&${AUTH}" \
  | jq '.response[0].periods[] | {ymd: .summary.ymd, precip: .summary.precip}'

# Full day archive
curl -sS "https://data.api.xweather.com/observations/archive/MID_E8181?from=2026-07-18&${AUTH}" \
  | jq '.response.periods[-5:] | .[] | {t: .ob.dateTimeISO, p: .ob.precipIN, mid: .ob.precipSinceMidnightIN}'

# Analyzed daily totals at trailhead
curl -sS "https://data.api.xweather.com/conditions/summary/26.12983,-80.35090?from=-7days&to=now&plimit=7&${AUTH}" \
  | jq '.response[0].periods[] | {day: .dateTimeISO[0:10], in: .precip.totalIN}'
```

## Station map (by trail)

Trail coordinates match `src/trails.rs`.

| Trail | App lat/lon | Primary rain | Secondary / notes |
|-------|-------------|--------------|-------------------|
| **Markham Park** | 26.12983, -80.35090 | **MID_E8181** | **PWS_W4RCT** nearby; ignore **MID_D4511** rain; ASOS **KFXE** ~11 mi |
| **Camp Murphy** | 27.01226, -80.11082 | **MID_C8019** | **PWS_JOE4SPEED** co-primary (~same distance); closer mesonets often lack precip |
| **Quiet Waters** | 26.31012, -80.16113 | **PWS_363636363** | ~2.4 mi (boca pointe); MID_C6162 ~3.9 mi alt; WU KFLDEERF75 closer but not on Xweather; MID_SSNVV no precip |

### CWOP / MADIS ID mapping

aprs.fi-style callsigns appear under MADIS mesonet as `MID_` + callsign without the leading letter class prefix pattern used on RF — verified:

| aprs.fi / CWOP | Xweather ID | ~Location | Role |
|----------------|-------------|-----------|------|
| **EW8181** | **MID_E8181** | 26.1212, -80.4093 (~3.7 mi Markham) | **Primary Markham rain** |
| **DW4511** | **MID_D4511** | ~1.7 mi Markham | **Rain unreliable** — daily totals stuck at 0 |
| **CW8019** | **MID_C8019** | ~3.2 mi Camp Murphy | **Primary Camp Murphy rain** |
| W4RCT | **PWS_W4RCT** | ~1.7 mi Markham | **Usable on Xweather** (not on aprs.fi); good near-trail PWS |

### Other nearby sources

| ID | Type | ~Distance | Notes |
|----|------|-----------|--------|
| **KFXE** | METAR | ~11 mi Markham | Better ASOS precip than KHWO in practice |
| **KHWO** | METAR | nearer Markham area | Often empty `precip` in summary — do not rely on alone |
| **MID_1529W** | MADIS | ~0.8 mi Markham | Very close; often **no precip** field |
| **PWS_TEQUES007** | PWS | ~2.2 mi Markham | Alt after QC |
| **PWS_JOE4SPEED** | PWS | ~3.2 mi Camp Murphy | Co-primary with MID_C8019; tracks well |
| **PWS_363636363** | PWS | ~2.4 mi Quiet Waters | Candidate primary after multi-day QC |
| **MID_C6162** | MADIS | ~3.9 mi Quiet Waters | Candidate mesonet |

Re-run `closest` periodically; PWS IDs and trust can change.

## Working with precip (no DIY 48h poll loop)

### Daily totals

Use `/observations/summary/{id}?plimit=…`. Example shape:

```json
"precip": {
  "totalMM": 13.21,
  "totalIN": 0.52,
  "method": "EOD24hr",
  "qc": 10,
  "QC": "O"
}
```

Jaycast UI units are **inches** — prefer `totalIN` when comparing to the app.

### Intraday / “when did it rain”

1. **Archive + `precipSinceMidnightIN`** — plot the midnight-reset curve; last non-null of the day ≈ daily total when the gauge is healthy.
2. **Archive + `precipSinceLastObIN`** — sum positive tips (watch for gaps and resets).
3. **`conditions` hourly `precipIN`** — timing at the trailhead coordinate (analyzed, not gauge).

### Cross-checks

- Latest summary day for **MID_E8181** should move on wet days; if stuck at 0 while **PWS_W4RCT** and **conditions/summary** show rain → gauge fault (same class of failure as DW4511).
- ASOS (**KFXE**) is airport tipping-bucket truth but **not** trail-yard; use as regional sanity check.
- Large persistent zeros on one station while neighbors report rain → drop that station’s precip.

## QC rules

- **Markham precip:** use **MID_E8181** and/or **PWS_W4RCT**. Flag or ignore **MID_D4511** rain until the gauge clearly reports non-zero rain in real events.
- **Camp Murphy precip:** use **MID_C8019** and/or **PWS_JOE4SPEED** (both ~3.2 mi; prefer either over closer no-precip mesonets).
- **Quiet Waters precip:** use **PWS_363636363** (~2.4 mi). MID_C6162 is a farther alt; MID_SSNVV has no precip; WU KFLDEERF75 is closer but not on Xweather.
- Prefer `ob.trustFactor` ≥ 80 and `QCcode` 10 when present.
- If latest `ob.dateTimeISO` is stale (e.g. >2–3 hours for a normally frequent PWS), treat as offline.
- Sparse packets / `precip: null` → incomplete; don’t invent missing rain.
- `conditions/*` is **not** a substitute for a local gauge when validating drainage heuristics.

## Example 7-day snapshot (illustrative)

Captured during doc authoring (local use only; numbers change):

| Day (approx) | MID_E8181 (in) | PWS_W4RCT (in) | MID_D4511 (in) | MID_C8019 (in) | conditions Markham (in) |
|--------------|----------------|----------------|----------------|----------------|-------------------------|
| wet sample | 1.23 | 0.53 | 0 | … | 0.35 |
| another wet | 0.52 | 0.60 | 0 | … | 0.52 |

Takeaway: **E8181** and **W4RCT** track rain; **D4511** does not; analyzed conditions are in the same ballpark on heavier days.

### Hourly example: PWS_W4RCT on 2026-07-18 (ET)

From `/observations/archive/PWS_W4RCT?from=2026-07-18`. Per hour: **tips** = sum of `precipSinceLastObIN`; **mid_end** = max `precipSinceMidnightIN` in that hour; **max_rate** = max rolling `precipIN` (not an hourly total).

| hour | tips (in) | mid_end (in) | max_rate |
|------|-----------|--------------|----------|
| 00:00–10:00 | 0.00 | 0.00 | 0 |
| **11:00** | **0.24** | **0.24** | 1.44 |
| 12:00 | 0.00 | 0.24 | 0 |
| 13:00 | 0.00 | 0.24 | 0 |
| **14:00** | **0.36** | **0.60** | 1.39 |
| 15:00–23:00 | 0.00 | 0.60 | 0 |
| **day** | **0.60** | **0.60** | — |

Two cells only: late morning (**0.24"**) and mid-afternoon (**0.36"**). Matches summary daily total **0.60"**. ~1-minute obs cadence (1439 samples that day).

### Best stations: Camp Murphy

Closest sites with a **working rain gauge** (closer MADIS like MID_1527W ~1.9 mi often have `precip: null`):

| Rank | Station | Dist | Notes |
|------|---------|------|-------|
| 1 | **MID_C8019** (CW8019) | 3.2 mi | MADIS/CWOP; rain most days in sample week |
| 2 | **PWS_JOE4SPEED** | 3.2 mi | Closest healthy PWS; tracks C8019 on convective days |

### Hourly example: Camp Murphy 2026-07-16 … 19 (ET)

From `/observations/archive/{id}`. **tips** = sum of `precipSinceLastObIN` that hour; dry stretches collapsed. Jul 19 through ~07:00 only (stations briefly stale that morning).

#### MID_C8019

| period | tips (in) | mid_end |
|--------|-----------|---------|
| 07-16 00:00–21:00 | 0 | dry ×22h |
| **07-16 22:00** | **0.69** | 0.69 |
| **07-16 23:00** | **0.04** | **0.73** |
| **07-17 00:00** | **0.07** | 0.07 |
| 07-17 01:00–23:00 | 0 | dry ×23h |
| **07-18 00:00** | **0.02** | 0.02 |
| 07-18 01:00–11:00 | 0 | dry ×11h |
| **07-18 12:00** | **0.02** | 0.04 |
| 07-18 13:00 → 07-19 07:00 | 0 | dry |

**Day tip sums:** 16th **0.73"** · 17th **0.07"** · 18th **0.04"** · 19th **0** (matches summary).

#### PWS_JOE4SPEED

| period | tips (in) | mid_end |
|--------|-----------|---------|
| 07-16 00:00–21:00 | 0 | dry ×22h |
| **07-16 22:00** | **0.66** | 0.66 |
| **07-16 23:00** | **0.06** | **0.72** |
| **07-17 00:00** | **0.02** | 0.02 |
| **07-17 01:00** | **0.01** | 0.03 |
| 07-17 02:00 → 07-18 11:00 | 0 | dry ×34h |
| **07-18 12:00** | **0.04** | 0.04 |
| 07-18 13:00 → 07-19 07:00 | 0 | dry |

**Day tip sums:** 16th **0.72"** · 17th **0.03"** · 18th **0.04"** · 19th **0**.

Main event: **~0.7" late evening Jul 16** (22:00–23:00), small spill past midnight into the 17th, then a light noon tip on the 18th. Both gauges agree; Markham’s heavier Jul 18 afternoon cells did not hit Camp Murphy the same way.

## Recipe: per-trail daily rain table

```bash
CID="${XWEATHER_API_KEY%%_*}"
CSEC="${XWEATHER_API_KEY#*_}"
AUTH="client_id=${CID}&client_secret=${CSEC}"

# Markham + Camp Murphy primaries
for id in MID_E8181 PWS_W4RCT MID_D4511 KFXE MID_C8019 PWS_JOE4SPEED; do
  echo "=== $id ==="
  curl -sS "https://data.api.xweather.com/observations/summary/${id}?from=-7days&to=now&plimit=7&${AUTH}" \
    | jq -r --arg id "$id" '
        .response[0].periods[]?
        | "\($id) \(.summary.ymd) \(.summary.precip.totalIN // "null") \(.summary.precip.method // "")"
      '
done
```

Then offline: join to Open-Meteo historical days (CLI `jaycast analyze …`) when tuning score params. Still local-only; no key in the web app.

## Feed CLI (hourly tips JSON)

Native-only subcommand builds a static JSON file of hourly gauge tips for Markham, Camp Murphy, and Quiet Waters. Key stays on the host; WASM can fetch the file later without credentials.

```bash
export XWEATHER_API_KEY='CLIENTID_CLIENTSECRET'

# pretty-print to stdout
cargo run --features cli --bin jaycast -- xweather dump [--days N]

# atomic write to a path you choose
cargo run --features cli --bin jaycast -- xweather publish --out /path/to/xweather.json [--days N]
```

- `--days` defaults to **2** (yesterday + today in host local date; label timezone is `America/New_York`).
- Stations: Markham `MID_E8181` / `PWS_W4RCT`; Camp Murphy `MID_C8019` / `PWS_JOE4SPEED`; Quiet Waters `PWS_363636363`.
- Each day has `hourly_tips_in` (24 floats, inches) = sum of `precipSinceLastObIN` by local hour from `/observations/archive/{id}`.
- **Cache:** completed past days are stored in `--cache` (default: `.jaycast-xweather-cache.json` beside `--out`, else cwd). Today is always fetched; past days hit the cache after the first pull. Retention 60 days.
- Install/cron placement is left to the operator.

## What this is not

- Not a drop-in replacement for Open-Meteo inside the WASM app (no secret key in the browser).
- Not official trail status or park closure data.
- Not identical to aprs.fi field names (`rain_1h` / `rain_24h` / `rain_mn`) — use Xweather `precip*` / summary totals instead.
- Not a guarantee every PWS is calibrated; always QC against neighbors.

## Quick reference

```text
Markham:      MID_E8181 (rain primary), PWS_W4RCT (near-trail PWS), MID_D4511 (rain suspect)
Camp Murphy:  MID_C8019, PWS_JOE4SPEED
Quiet Waters: PWS_363636363 (rain primary)

Auth:   XWEATHER_API_KEY = client_id + '_' + client_secret
Base:   https://data.api.xweather.com
Daily:  /observations/summary/{id}?from=-7days&to=now&plimit=7
Day:    /observations/archive/{id}?from=YYYY-MM-DD
Near:   /observations/closest?p=lat,lon&filter=pws|mesonet|metar
Grid:   /conditions/summary/{lat},{lon}?from=-7days&plimit=7
Docs:   https://www.xweather.com/docs/weather-api
```
