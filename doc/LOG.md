# LOG

Most recent first.

| Date | Revisions | Summary |
| --- | --- | --- |
| 2026-08-02 | (uncommitted) | Hover tooltips on hero toggles, header buttons, timeline nav, and weekend grid rows. Styled CSS data-tip tips, themed, desktop only. Native title attributes replaced |
| 2026-08-02 | [43208b4](https://github.com/j33433/jaycast/commit/43208b4) | Screenshot capture folded into one script: --local serves the local dist/ build, --mobile keeps the 390x844 capture. Deletes scripts/screenshot.mjs. beta-install.sh ignored in git |
| 2026-08-02 | [0d105bd](https://github.com/j33433/jaycast/commit/0d105bd) | leptos 0.7.8 -> 0.8.20 (tachys 0.2.18); fixes the "closure invoked recursively or after being dropped" dialog bug. App resources switched to relative paths so one build serves any base path. Folds LEPTOSUPDATE.md into CODEMAP |
| 2026-08-02 | [e472ef8](https://github.com/j33433/jaycast/commit/e472ef8) | Per-trail favicon and home-screen icon swap; small 128px WebP icons for the location chooser and weekend grid; trail logos switch from SVG to WebP (1024px, q95). Same sharpness at every size, roughly 30% smaller on disk |
| 2026-08-02 | [6144de5](https://github.com/j33433/jaycast/commit/6144de5) | Day-card blurbs become short keyword lists ("firm, rain am", "cool"); Markham shows no "open" tag since status is unverified (see Facebook link instead) |
| 2026-08-01 | [5705672..fdd29ec](https://github.com/j33433/jaycast/compare/5705672...fdd29ec) | Add .gitignore and doc cleanup; UI copy simplified per Simple English; hero toggles stack on narrow views without dead space |
| 2026-07-31 | [03aaec7..b26a1f2](https://github.com/j33433/jaycast/compare/03aaec7...b26a1f2) | Static about page + trail SEO data; forecast trimmed to today +5 days; Quiet Waters mud window shortened to 10h; rescan avoids colocated PWS pairs |
| 2026-07-30 | [5d8bec5..11c437f](https://github.com/j33433/jaycast/compare/5d8bec5...11c437f) | Help dialog with annotated day-card guide; layout and copy polish |
| 2026-07-28 | [66ca348..e970b33](https://github.com/j33433/jaycast/compare/66ca348...e970b33) | Gauge specs moved to shared gauges.rs (single source of truth); screenshot flag for transparent/dark capture; README cleanup |
| 2026-07-27 | [36e56ec..788310e](https://github.com/j33433/jaycast/compare/36e56ec...788310e) | Weekend Warrior multi-trail comparison grid; vertical layout, selection persistence and cross-trail fixes; README rewrite |
| 2026-07-26 | [7c54efc..4dafc96](https://github.com/j33433/jaycast/compare/7c54efc...4dafc96) | Expanded day-card detail persists across refreshes; hourly tick markers with now-marker; CLI --gauge flag; stale rain no longer yields 'firm sand' |
| 2026-07-25 | [1bd3528..3299e63](https://github.com/j33433/jaycast/compare/1bd3528...3299e63) | Docs moved to doc/ directory |
| 2026-07-24 | [2be7f4b..d7b0b29](https://github.com/j33433/jaycast/compare/2be7f4b...d7b0b29) | CLI.md subcommand reference; CODEMAP/XWEATHER doc trim; auto-refresh every 15 min with cache-busted rain.json; timestamps for model/gauge times |
| 2026-07-23 | [3f3b7fd..9baeeaf](https://github.com/j33433/jaycast/compare/3f3b7fd...9baeeaf) | v0.3.0; gauge rain used for past hours; Markham drainage calibration and future-hour handling; no auto-expand of best day |
| 2026-07-20 | [3b594e5..7120e5c](https://github.com/j33433/jaycast/compare/3b594e5...7120e5c) | xweather rescan ranked by distance tier; empty archive days treated as zero rain |
| 2026-07-19 | [5614c26..96d35de](https://github.com/j33433/jaycast/compare/5614c26...96d35de) | xweather CLI publishes hourly gauge rain; gauge bins overlay; Quiet Waters gauge; rescan ranking docs; closure note for Jul 18 |
| 2026-07-18 | [46f4330](https://github.com/j33433/jaycast/commit/46f4330) | Hour-aware Quiet Waters mud window after PM storms |
| 2026-07-17 | [05afbb3..74c66e9](https://github.com/j33433/jaycast/compare/05afbb3...74c66e9) | ECMWF IFS HRES requested explicitly; selected model used for archive weather; forecast fetch time shown, cache cut to 30m |
| 2026-07-16 | [7ec4135..986f62e](https://github.com/j33433/jaycast/compare/7ec4135...986f62e) | Day-card side borders colored by feels-like temp delta; bare trail slugs in shareable URLs; tighter layout |
| 2026-07-15 | [afb896c..da90764](https://github.com/j33433/jaycast/compare/afb896c...da90764) | ECMWF becomes default model; apparent-temp outlier detection with cool-day badge; Quiet Waters scoring fix; day detail line simplified |
| 2026-07-14 | [531f404..5eed12a](https://github.com/j33433/jaycast/compare/531f404...5eed12a) | Weekend labels pill badge; Markham drainage window scaled by rain; 503 retry on weather fetch; document title updates per trail |
| 2026-07-13 | [a384a9d..b8dc24e](https://github.com/j33433/jaycast/compare/a384a9d...b8dc24e) | CODEMAP.md added; trace precipitation filtered from Markham closure advisory; start of manual closure observations |
| 2026-07-12 | [7eb16f5..f2a02c1](https://github.com/j33433/jaycast/compare/7eb16f5...f2a02c1) | Multi-trail rideability profiles; hourly Markham drainage model; closure advisory wording, sand notes, hero compacted; SEO trail names |
| 2026-07-11 | [6162bd8..8420118](https://github.com/j33433/jaycast/compare/6162bd8...8420118) | Forecast analysis CLI with historical analysis and model-switch caching; ride-window scoring refined |
| 2026-07-10 | [365391b..8af1718](https://github.com/j33433/jaycast/compare/365391b...8af1718) | v0.2.0; scoring rework (drop soil moisture, wind band, ET0 drying); light/dark theme toggle; weather curve backgrounds; hero/model toggle polish |
| 2026-07-09 | [5ae5baa..7025201](https://github.com/j33433/jaycast/compare/5ae5baa...7025201) | Initial release v0.1.0: browser-side rideability forecast for Camp Murphy, GFS/ECMWF toggle, 30-day archive, scrub-jay theming, day-card details |

## Updating this log

Future agents should add to this log when making user-visible changes. Rules:

- Add new entries at the top of the table, just below the header row.
- One row per clump, in this format: `| date | revision link | brief summary |`.
- Clump closely related commits from the same batch of work into a single row. Do not add one row per commit.
- Use the date of the most recent commit in the clump and the first..last short hashes.
- Link the revisions to GitHub: ranges as `[oldest..newest](https://github.com/j33433/jaycast/compare/oldest...newest)`, single commits as `[hash](https://github.com/j33433/jaycast/commit/hash)`.
- Summarize briefly what changed and why it matters. Avoid AI-style fluff and emdashes.
- You may omit trivial checkins (formatting-only, README tweaks, asset-only updates) or fold them into a related entry.
- If a release was cut (version bump) or a headline feature landed, lead the summary with it.
- Do not rewrite or reorder history entries.
