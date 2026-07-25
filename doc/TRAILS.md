# Trails

Three South Florida MTB trail systems, each with a distinct surface character and
weather response.

## Camp Murphy

**Location:** Jonathan Dickinson State Park, FL (27.01, -80.11)

**Surface:** Sandy scrub trails under light canopy. The soil is coarse sand that
turns loose and soft when bone-dry and firms up beautifully after rain. No mud
to speak of — this is Florida sugar sand over limestone.

**How rain affects it:**

Camp Murphy rides best after meaningful rain but before the sun bakes it back to
powder. The model scores this with **SandPack**:

- More rain always improves the pack — there is no "too much" for sand.
- The timing peak is at **8 hours** after rain ends. After a good soaking the
  sand is firm and fast. By roughly 3 days of sun it's back to soft.
- Sun dries this shadeless sand aggressively, so the ET0 (sunshine) forecast
  modulates the drying clock. Cloudy days keep the sand packed longer.
- A long dry spell (7+ days with no significant rain) bottoms out the pack score
  regardless of other conditions. Dry sand is slow, loose, and energy-sapping.

**Score weights:** 55% pack (surface+timing), 35% weather (temp/wind/sky), 10% confidence.

**Gauge stations:** MID_C8019, PWS_JOE4SPEED

---

## Markham Park

**Location:** Weston, FL (26.13, -80.35)

**Surface:** Dirt and gravel trails that drain slowly. When rain hits, the
trails close — not because of rangers, but because riding wet dirt chews up the
trail surface and gums up your drivetrain. The county may also post official
closures.

**How rain affects it:**

Markham uses a **Drainage** model that estimates when trails become rideable
again based on how much rain fell and when it stopped.

- **Significant rain threshold:** 0.10 in. Less than that and short showers are
  ignored.
- **Base drain time:** 8.5 hours after rain ends, plus 8 hours per additional
  inch of rain, capped at 18.5 hours total.
- Rain overnight (e.g. ending at 2 AM) will clear by midday. Heavy afternoon
  rain (1+ inch ending at 6 PM) pushes the reopening to the following midday.
- The model groups rain events across a 3-hour gap tolerance. Scattered showers
  within the same window count as one event.

**Closure statuses:**

| Status | Meaning |
|---|---|
| `Clear` | Likely open all day |
| `Possible` | Unsure for at least one window (AM, PM, or both) |
| `n/a` | No drainage constraint applies |

The score uses a **daylight fraction** gate instead of a wet-gate multiplier.
If the trail is likely open for 4 of 8 morning daylight hours, the pack
contribution is halved.

The app links to the Markham Park MTB Facebook group for ground-truth status.

**Gauge stations:** MID_E8181, PWS_W4RCT

---

## Quiet Waters Park

**Location:** Deerfield Beach, FL (26.31, -80.16)

**Surface:** Mixed hardpack and loose-over-hard. Unlike Camp Murphy's sand or
Markham's dirt, this is firm terrain that stays fast even when dry. It never
closes and degrades slowly after rain — a thin layer of loose material over a
hard base means puddles and slick roots are the worst of it.

**How rain affects it:**

Quiet Waters uses a **MixedSurface** model that puts less weight on surface
conditions and more on weather.

- The **dry baseline is high** (0.90). Even with no recent rain the surface
  quality starts near the top.
- Rain only **temporarily degrades** the surface. The mud penalty clears after
  14 hours.
- The timing peak is at **30 hours** after rain — a wider, gentler window than
  Camp Murphy's sharp 8h peak. Pack fade takes roughly 5 days.
- Ride-window rain thresholds are **more generous** (0.12 in soft, 0.70 in hard
  vs. Camp Murphy's 0.05/0.40). A light drizzle doesn't tank the score.

**Score weights:** 35% pack, 55% weather, 10% confidence. Weather (temp, wind,
comfort) matters more than surface here since surface quality is consistently
high.

**Field verification:** On July 18, 2026, an afternoon storm ending around 5 PM
left no mud by the following morning, confirming the 14-hour mud clear window.

**Gauge station:** PWS_363636363

---

## Summary

| | Camp Murphy | Markham | Quiet Waters |
|---|---|---|---|
| **Surface** | Sugar sand | Dirt/gravel | Mixed hardpack |
| **Worst condition** | Long dry spell | Rain within past 18h | Heavy ride-window rain |
| **Best condition** | Sun after rain (~8h) | 24h+ after rain | Any dry day |
| **Closes?** | Never | Rain-timed advisory | Never |
| **Pack weight** | 55% | 55% (drainage gate) | 35% |
| **Weather weight** | 35% | 35% | 55% |
