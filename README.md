![jaycast](assets/jaycast-icon.png)

# jaycast

**When should I ride?** Weather-aware trail scores for South Florida mountain bike parks.

**Live:** [https://upload.bike/jaycast/](https://upload.bike/jaycast/)

Pick a park, glance at the stars, and decide. No account, no app install — just open it in your browser.

---

## Trails

| Park | Vibe | After rain |
|------|------|------------|
| **Camp Murphy** (Jonathan Dickinson) | Sandy scrub | Firms up — often *better* once it packs |
| **Markham Park** (Weston) | Dirt / gravel | Can stay closed or sketchy until it drains |
| **Quiet Waters** (Deerfield Beach) | Mixed hardpack | Rarely a problem; usually rideable again soon |

Switch parks anytime. Your choice is remembered, and you can share a direct link with `?camp-murphy`, `?markham`, or `?quiet-waters`.

---

## What you see

- **Star score** (1.0–5.0) for each day — how rideable it looks, not official park status
- **Timeline** — yesterday through the next week, color-tinted by score
- **Rain & clouds** on each day card (midnight → evening)
- **Cooler / warmer** cues on the card edges vs the prior week
- **Why** — tap a day for a simple factor breakdown (surface, rain window, temp, wind…)
- **Weekend compare** — the grid icon stacks all three parks for the next several days and tags the **Best** pick; tap a row to jump straight to that day

Units are inches and °F. Light or dark theme sticks around.

Forecasts come from [Open-Meteo](https://open-meteo.com/) (GFS or ECMWF — you can flip between them). Optional nearby rain gauges improve “what already fell” when that feed is available. Still **not** official trail status — use judgment and local reports (e.g. Markham’s Facebook group when linked).

---

## Score model (short version)

Each trail has its own personality baked in:

- **Camp Murphy** cares a lot about sand pack after recent rain (and dings a wet morning ride).
- **Markham** estimates when dirt might reopen after meaningful rain (advisory only).
- **Quiet Waters** weights comfort weather more; surface usually stays pretty good.

Details and knobs live in `src/score/` and [doc/TRAILS.md](doc/TRAILS.md). Riders who want the full map of the code can peek at [doc/CODEMAP.md](doc/CODEMAP.md).

---

## Develop

```bash
# once
rustup target add wasm32-unknown-unknown
cargo install trunk   # or a trunk binary release

trunk serve           # http://127.0.0.1:8080
cargo test
trunk build --release # static site in dist/
```

Native helpers (same scorer as the site):

```bash
# score a day or range
cargo run --features cli --bin jaycast -- analyze markham 2026-07-08:2026-07-11 both

# ground-truth gauge rain feed (needs XWEATHER_API_KEY; see doc/XWEATHER.md)
cargo run --features cli --bin jaycast -- xweather publish --out assets/rain.json
```

More CLI detail: [doc/CLI.md](doc/CLI.md).

---

## License

GPL-3.0-or-later — see `LICENSE`.

Questions or trail notes: [upload.bike@gmail.com](mailto:upload.bike@gmail.com)
