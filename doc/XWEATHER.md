# Xweather ground-truth rain feed

Ground-truth rain from PWS, MADIS/CWOP mesonet, and ASOS via the
[Xweather Weather API](https://www.xweather.com/docs/weather-api). The feed
replaces model precip for completed hours on the day cards and in scoring. The
browser app fetches the static feed at `/jaycast/rain.json`. No API key ships
to the client.

## Auth

1. Register at [Xweather](https://www.xweather.com/) → client ID + client secret.
2. Store it in the environment. Never commit it:

   ```bash
   export XWEATHER_API_KEY='CLIENTID_CLIENTSECRET'
   ```

   The key is `client_id` + `_` + `client_secret`. Local / server use only.

## CLI

See [CLI.md](CLI.md) for `xweather publish`, `xweather dump`, and
`xweather rescan`.

```sh
# hourly tips JSON for the WASM app
XWEATHER_API_KEY='...' cargo run --features cli --bin jaycast -- \
  xweather publish --out assets/rain.json --cache data/xweather-cache.json
```

- `--days` defaults to 2 (yesterday + today, host-local `America/New_York`).
- Past days cache at `--cache` (default: beside `--out` or cwd). Retention
  60 days. Only today is re-fetched each run.

## Station map

| Trail | Primary | Secondary / notes |
|-------|---------|-------------------|
| **Markham Park** | **MID_E8181** | **PWS_W4RCT** nearby. Ignore **MID_D4511** rain. ASOS **KFXE** ~11 mi |
| **Camp Murphy** | **MID_C8019** | **PWS_JOE4SPEED** co-primary (~same distance) |
| **Quiet Waters** | **PWS_363636363** | ~2.4 mi. MID_C6162 ~3.9 mi alt. MID_SSNVV no precip |

### ID mapping

| CWOP | Xweather ID | Role |
|------|-------------|------|
| EW8181 | **MID_E8181** | Primary Markham rain |
| DW4511 | **MID_D4511** | Rain unreliable. Daily totals stuck at 0 |
| CW8019 | **MID_C8019** | Primary Camp Murphy rain |
| W4RCT | **PWS_W4RCT** | Near-trail Markham PWS |

### Other nearby sources

| ID | Type | Distance | Notes |
|----|------|----------|-------|
| KFXE | METAR | ~11 mi Markham | Better ASOS precip than KHWO |
| KHWO | METAR | nearer Markham | Often empty `precip`. Do not rely on it alone |
| MID_1529W | MADIS | ~0.8 mi Markham | Very close but often no precip field |
| PWS_TEQUES007 | PWS | ~2.2 mi Markham | Alt after QC |
| PWS_JOE4SPEED | PWS | ~3.2 mi Camp Murphy | Co-primary with MID_C8019 |
| PWS_363636363 | PWS | ~2.4 mi Quiet Waters | Primary after multi-day QC |
| MID_C6162 | MADIS | ~3.9 mi Quiet Waters | Candidate mesonet |

## QC rules

- **Markham:** use MID_E8181, PWS_W4RCT, or both. Flag/ignore MID_D4511 rain.
- **Camp Murphy:** use MID_C8019, PWS_JOE4SPEED, or both.
- **Quiet Waters:** use PWS_363636363. MID_C6162 is a farther alt.
- Prefer `ob.trustFactor` ≥ 80 and `QCcode` 10 when present.
- If `ob.dateTimeISO` is >2–3 hours stale, treat as offline.
- `precip: null` → incomplete. Do not invent missing rain.
- Secondary must not be colocated with primary (same site / dual MADIS+PWS IDs).
- Re-run `xweather rescan` periodically. PWS IDs and trust can change.

## Quick reference

```text
Auth:   XWEATHER_API_KEY = client_id + '_' + client_secret
Base:   https://data.api.xweather.com
Docs:   https://www.xweather.com/docs/weather-api
```
