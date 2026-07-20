//! Discover and rank nearby stations with working rain gauges.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::trails::Trail;

use super::{http_get_json, Auth, BASE_URL, TRAILS};

const DEFAULT_CLOSEST_LIMIT: u32 = 15;
const DEFAULT_QC_DAYS: u32 = 7;
const DEFAULT_QC_CANDIDATES: usize = 12;
/// Hard reject beyond this (except forced feed stations still get QC).
const MAX_DISTANCE_MI: f64 = 15.0;
/// Eligible for normal primary/secondary recommendation.
const PRIMARY_MAX_MI: f64 = 5.0;
/// Emergency fallback only — never auto-recommended as primary.
const BACKUP_MAX_MI: f64 = 10.0;
const TRACE_IN: f64 = 0.01;
const REF_WET_FOR_STUCK: usize = 2;
/// Known bad rain meters (always reject).
const BLOCKLIST: &[&str] = &["MID_D4511"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NetworkKind {
    Pws,
    Madis,
    Other,
}

impl NetworkKind {
    fn of(id: &str) -> Self {
        if id.starts_with("PWS_") {
            Self::Pws
        } else if id.starts_with("MID_") {
            Self::Madis
        } else {
            Self::Other
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pws => "pws",
            Self::Madis => "madis",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DistanceTier {
    Primary,
    Backup,
    Regional,
}

impl DistanceTier {
    fn of(distance_mi: f64) -> Self {
        let d = nan_last_distance(distance_mi);
        if d <= PRIMARY_MAX_MI {
            Self::Primary
        } else if d <= BACKUP_MAX_MI {
            Self::Backup
        } else {
            Self::Regional
        }
    }

}

#[derive(Debug, Deserialize)]
struct ClosestResponse {
    response: Option<Vec<ClosestStation>>,
    error: Option<super::ApiError>,
}

#[derive(Debug, Deserialize)]
struct ClosestStation {
    id: Option<String>,
    loc: Option<Loc>,
    place: Option<Place>,
    ob: Option<ClosestOb>,
    #[serde(rename = "relativeTo")]
    relative_to: Option<RelativeTo>,
}

#[derive(Debug, Deserialize)]
struct Loc {
    lat: Option<f64>,
    long: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Place {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClosestOb {
    #[serde(rename = "dateTimeISO")]
    date_time_iso: Option<String>,
    #[serde(rename = "trustFactor")]
    trust_factor: Option<f64>,
    #[serde(rename = "precipIN")]
    precip_in: Option<f64>,
    #[serde(rename = "precipSinceMidnightIN")]
    precip_since_midnight_in: Option<f64>,
    #[serde(rename = "precipSinceLastObIN")]
    precip_since_last_ob_in: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct RelativeTo {
    #[serde(rename = "distanceMI")]
    distance_mi: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct SummaryResponse {
    response: Option<serde_json::Value>,
    error: Option<super::ApiError>,
}

#[derive(Debug, Deserialize)]
struct ConditionsSummaryResponse {
    response: Option<serde_json::Value>,
    error: Option<super::ApiError>,
}

#[derive(Clone, Debug)]
struct Candidate {
    id: String,
    distance_mi: f64,
    place: String,
    trust: Option<f64>,
    last_ob: Option<String>,
    has_live_precip_field: bool,
}

#[derive(Clone, Debug)]
enum RejectReason {
    Blocklist,
    TooFar,
    NoPrecipField,
    NullPrecipAllDays,
    StuckZero { ref_wet: usize },
    FetchError(String),
    EmptySummary,
}

impl RejectReason {
    fn label(&self) -> String {
        match self {
            Self::Blocklist => "blocklist (known bad rain)".into(),
            Self::TooFar => format!("beyond {MAX_DISTANCE_MI:.0} mi"),
            Self::NoPrecipField => "no precip fields on latest ob".into(),
            Self::NullPrecipAllDays => "precip null/missing all summary days".into(),
            Self::StuckZero { ref_wet } => {
                format!("stuck at 0 while conditions wet {ref_wet} days")
            }
            Self::FetchError(e) => format!("fetch error: {e}"),
            Self::EmptySummary => "empty summary".into(),
        }
    }
}

#[derive(Clone, Debug)]
struct RankedStation {
    id: String,
    distance_mi: f64,
    place: String,
    trust: Option<f64>,
    last_ob: Option<String>,
    wet_days: usize,
    ref_wet_days: usize,
    mae_in: Option<f64>,
    max_day_in: f64,
    day_totals: Vec<(String, Option<f64>)>,
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut trail_filter: Option<String> = None;
    let mut limit = DEFAULT_CLOSEST_LIMIT;
    let mut days = DEFAULT_QC_DAYS;
    let mut qc_n = DEFAULT_QC_CANDIDATES;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--limit" => {
                limit = parse_u32(&args.next(), "--limit")?;
                if !(1..=50).contains(&limit) {
                    return Err("--limit must be 1..=50".into());
                }
            }
            "--days" => {
                days = parse_u32(&args.next(), "--days")?;
                if !(3..=14).contains(&days) {
                    return Err("--days must be 3..=14".into());
                }
            }
            "--candidates" => {
                qc_n = parse_u32(&args.next(), "--candidates")? as usize;
                if !(3..=30).contains(&qc_n) {
                    return Err("--candidates must be 3..=30".into());
                }
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other if other.starts_with('-') => {
                return Err(format!("unexpected argument {other:?}"));
            }
            other => {
                if trail_filter.is_some() {
                    return Err(format!("unexpected argument {other:?}"));
                }
                Trail::from_slug(other)
                    .ok_or_else(|| format!("unknown trail {other:?}"))?;
                trail_filter = Some(other.to_string());
            }
        }
    }

    let auth = Auth::from_env()?;
    let trails: Vec<Trail> = match trail_filter.as_deref() {
        Some(slug) => vec![Trail::from_slug(slug).unwrap()],
        None => Trail::ALL.to_vec(),
    };

    for trail in trails {
        rescan_trail(&auth, trail, limit, days, qc_n)?;
    }
    Ok(())
}

pub fn print_help() {
    eprintln!(
        "Usage:\n  jaycast xweather rescan [camp-murphy|markham|quiet-waters] [--limit N] [--days N] [--candidates N]\n\nFinds nearby PWS/mesonet stations, rejects bad/missing rain meters, ranks by distance tier.\nPrimary ≤{PRIMARY_MAX_MI:.0} mi, backup ≤{BACKUP_MAX_MI:.0} mi; conditions MAE is only a tie-break.\nDoes not change the feed station table (print-only).\n\nDefaults: --limit {DEFAULT_CLOSEST_LIMIT}  --days {DEFAULT_QC_DAYS}  --candidates {DEFAULT_QC_CANDIDATES}"
    );
}

fn parse_u32(value: &Option<String>, flag: &str) -> Result<u32, String> {
    let value = value
        .as_deref()
        .ok_or_else(|| format!("missing value after {flag}"))?;
    value
        .parse()
        .map_err(|_| format!("invalid {flag} {value:?}"))
}

fn rescan_trail(
    auth: &Auth,
    trail: Trail,
    closest_limit: u32,
    qc_days: u32,
    qc_candidates: usize,
) -> Result<(), String> {
    let lat = trail.latitude();
    let lon = trail.longitude();
    println!();
    println!(
        "=== {} ({})  {:.5},{:.5} ===",
        trail.short_name(),
        trail.slug(),
        lat,
        lon
    );

    let current: Vec<(&str, &str)> = TRAILS
        .iter()
        .find(|t| t.slug == trail.slug())
        .map(|t| t.stations.iter().map(|s| (s.id, s.role)).collect())
        .unwrap_or_default();
    if !current.is_empty() {
        print!("feed now: ");
        for (i, (id, role)) in current.iter().enumerate() {
            if i > 0 {
                print!(", ");
            }
            print!("{id} ({role})");
        }
        println!();
    }

    eprintln!("xweather rescan: conditions/summary {qc_days}d …");
    let ref_days = fetch_conditions_daily(auth, lat, lon, qc_days)?;
    let ref_wet = ref_days
        .iter()
        .filter(|(_, v)| v.map(|x| x >= TRACE_IN).unwrap_or(false))
        .count();
    print!("conditions ref: ");
    for (i, (ymd, v)) in ref_days.iter().enumerate() {
        if i > 0 {
            print!(" ");
        }
        match v {
            Some(x) => print!("{ymd}={x:.2}\""),
            None => print!("{ymd}=?"),
        }
    }
    println!("  (wet days {ref_wet})");

    let mut by_id: BTreeMap<String, Candidate> = BTreeMap::new();
    for filter in ["pws", "mesonet"] {
        eprintln!("xweather rescan: closest filter={filter} limit={closest_limit} …");
        let found = fetch_closest(auth, lat, lon, closest_limit, filter)?;
        for c in found {
            by_id.entry(c.id.clone()).or_insert(c);
        }
    }

    // Always include current feed stations even if outside closest / offline latest.
    let feed_ids: BTreeSet<String> = current
        .iter()
        .map(|(id, _)| (*id).to_string())
        .collect();
    for id in &feed_ids {
        if by_id.contains_key(id) {
            continue;
        }
        match fetch_one_meta(auth, id, lat, lon) {
            Ok(Some(c)) => {
                by_id.insert(c.id.clone(), c);
            }
            Ok(None) | Err(_) => {
                // Still contest them via summary QC; distance filled from summary if possible.
                eprintln!("xweather rescan: feed station {id} has no latest ob; forcing into contest");
                by_id.insert(
                    id.clone(),
                    Candidate {
                        id: id.clone(),
                        distance_mi: f64::NAN, // filled after summary if needed
                        place: String::from("(feed)"),
                        trust: None,
                        last_ob: None,
                        has_live_precip_field: true, // let summary decide precip health
                    },
                );
            }
        }
    }

    let mut candidates: Vec<Candidate> = by_id.into_values().collect();
    candidates.sort_by(|a, b| {
        nan_last_distance(a.distance_mi)
            .partial_cmp(&nan_last_distance(b.distance_mi))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut rejected: Vec<(Candidate, RejectReason)> = Vec::new();
    let mut to_qc: Vec<Candidate> = Vec::new();

    for c in candidates {
        if BLOCKLIST.contains(&c.id.as_str()) {
            rejected.push((c, RejectReason::Blocklist));
            continue;
        }
        // Feed stations always QC even if listed beyond the usual radius.
        let is_feed = feed_ids.contains(&c.id);
        if !is_feed && c.distance_mi > MAX_DISTANCE_MI {
            rejected.push((c, RejectReason::TooFar));
            continue;
        }
        to_qc.push(c);
    }

    // Always QC every current feed station; fill remaining slots by distance.
    let mut guaranteed: Vec<Candidate> = Vec::new();
    let mut others: Vec<Candidate> = Vec::new();
    for c in to_qc {
        if feed_ids.contains(&c.id) {
            guaranteed.push(c);
        } else {
            others.push(c);
        }
    }
    let other_slots = qc_candidates.saturating_sub(guaranteed.len());
    others.truncate(other_slots);
    let mut to_qc = guaranteed;
    to_qc.extend(others);

    let mut ranked: Vec<RankedStation> = Vec::new();
    for c in to_qc {
        eprintln!("xweather rescan: summary {} …", c.id);
        match evaluate_station(auth, &c, &ref_days, qc_days) {
            Ok(Ok(r)) => ranked.push(r),
            Ok(Err(reason)) => rejected.push((c, reason)),
            Err(e) => rejected.push((c, RejectReason::FetchError(e))),
        }
    }

    // Peer wet-day agreement among survivors within backup radius (not the grid).
    let peer_scores = peer_wet_agreement(&ranked);

    ranked.sort_by(|a, b| compare_stations(a, b, &peer_scores));

    let mut primary_ok = Vec::new();
    let mut backup_ok = Vec::new();
    let mut regional_ok = Vec::new();
    for r in &ranked {
        match DistanceTier::of(r.distance_mi) {
            DistanceTier::Primary => primary_ok.push(r),
            DistanceTier::Backup => backup_ok.push(r),
            DistanceTier::Regional => regional_ok.push(r),
        }
    }

    println!();
    println!("REJECTED ({}):", rejected.len());
    if rejected.is_empty() {
        println!("  (none)");
    } else {
        rejected.sort_by(|a, b| {
            nan_last_distance(a.0.distance_mi)
                .partial_cmp(&nan_last_distance(b.0.distance_mi))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (c, reason) in &rejected {
            println!(
                "  {:20} {:>6}  {}  — {}",
                c.id,
                fmt_mi(c.distance_mi),
                c.place,
                reason.label()
            );
        }
    }

    print_tier_table(
        &format!("PRIMARY ELIGIBLE (≤{PRIMARY_MAX_MI:.0} mi)"),
        &primary_ok,
        &peer_scores,
    );
    print_tier_table(
        &format!("BACKUPS ({PRIMARY_MAX_MI:.0}–{BACKUP_MAX_MI:.0} mi, not for normal primary)"),
        &backup_ok,
        &peer_scores,
    );
    if !regional_ok.is_empty() {
        print_tier_table(
            &format!("REGIONAL (>{BACKUP_MAX_MI:.0} mi, sanity only)"),
            &regional_ok,
            &peer_scores,
        );
    }

    println!();
    print_recommendation(&primary_ok, &backup_ok, &current);

    Ok(())
}

fn compare_stations(
    a: &RankedStation,
    b: &RankedStation,
    peer_scores: &BTreeMap<String, f64>,
) -> std::cmp::Ordering {
    // Distance tier first (primary before backup before regional).
    DistanceTier::of(a.distance_mi)
        .cmp(&DistanceTier::of(b.distance_mi))
        .then(
            nan_last_distance(a.distance_mi)
                .partial_cmp(&nan_last_distance(b.distance_mi))
                .unwrap_or(std::cmp::Ordering::Equal),
        )
        .then_with(|| {
            let pa = peer_scores.get(&a.id).copied().unwrap_or(0.0);
            let pb = peer_scores.get(&b.id).copied().unwrap_or(0.0);
            pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
        })
        .then(b.wet_days.cmp(&a.wet_days))
        .then_with(|| {
            // Conditions MAE is only a weak tie-break (not the main score).
            let mae_a = a.mae_in.unwrap_or(999.0);
            let mae_b = b.mae_in.unwrap_or(999.0);
            mae_a
                .partial_cmp(&mae_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| {
            let ta = a.trust.unwrap_or(0.0);
            let tb = b.trust.unwrap_or(0.0);
            tb.partial_cmp(&ta).unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// Fraction of days where this station's wet/dry matches the peer majority
/// among other OK gauges within BACKUP_MAX_MI. Higher is better.
fn peer_wet_agreement(stations: &[RankedStation]) -> BTreeMap<String, f64> {
    let peers: Vec<&RankedStation> = stations
        .iter()
        .filter(|s| nan_last_distance(s.distance_mi) <= BACKUP_MAX_MI)
        .collect();
    let mut out = BTreeMap::new();
    if peers.len() < 2 {
        return out;
    }

    // Collect union of dates.
    let mut dates: BTreeSet<String> = BTreeSet::new();
    for s in &peers {
        for (ymd, _) in &s.day_totals {
            dates.insert(ymd.clone());
        }
    }

    for s in &peers {
        let mut agree = 0usize;
        let mut compared = 0usize;
        let mine: BTreeMap<&str, bool> = s
            .day_totals
            .iter()
            .filter_map(|(ymd, v)| v.map(|x| (ymd.as_str(), x >= TRACE_IN)))
            .collect();
        for ymd in &dates {
            let Some(&my_wet) = mine.get(ymd.as_str()) else {
                continue;
            };
            let mut wet_votes = 0usize;
            let mut dry_votes = 0usize;
            for other in &peers {
                if other.id == s.id {
                    continue;
                }
                if let Some((_, Some(v))) = other.day_totals.iter().find(|(d, _)| d == ymd) {
                    if *v >= TRACE_IN {
                        wet_votes += 1;
                    } else {
                        dry_votes += 1;
                    }
                }
            }
            if wet_votes + dry_votes == 0 {
                continue;
            }
            let peer_wet = wet_votes >= dry_votes;
            compared += 1;
            if my_wet == peer_wet {
                agree += 1;
            }
        }
        if compared > 0 {
            out.insert(s.id.clone(), agree as f64 / compared as f64);
        }
    }
    out
}

fn print_tier_table(
    title: &str,
    rows: &[&RankedStation],
    peer_scores: &BTreeMap<String, f64>,
) {
    println!();
    println!("{title} ({}):", rows.len());
    if rows.is_empty() {
        println!("  (none)");
        return;
    }
    println!(
        "  {:3} {:20} {:>6}  {:>5}  {:>7}  {:>5}  {:>5}  {:>6}  {:>5}  {}",
        "#", "id", "mi", "net", "wet", "peer", "max", "mae", "trust", "place"
    );
    for (i, r) in rows.iter().enumerate() {
        let mae = r
            .mae_in
            .map(|m| format!("{m:.2}\""))
            .unwrap_or_else(|| "-".into());
        let trust = r
            .trust
            .map(|t| format!("{t:.0}"))
            .unwrap_or_else(|| "-".into());
        let peer = peer_scores
            .get(&r.id)
            .map(|p| format!("{:.0}%", p * 100.0))
            .unwrap_or_else(|| "-".into());
        println!(
            "  {:3} {:20} {:>6}  {:>5}  {:>3}/{:<3}  {:>5}  {:4.2}\"  {:>6}  {:>5}  {}",
            i + 1,
            r.id,
            fmt_mi(r.distance_mi),
            NetworkKind::of(&r.id).label(),
            r.wet_days,
            r.ref_wet_days,
            peer,
            r.max_day_in,
            mae,
            trust,
            r.place
        );
        print!(
            "       last_ob={}  days:",
            r.last_ob.as_deref().unwrap_or("-")
        );
        for (ymd, v) in &r.day_totals {
            match v {
                Some(x) => print!(" {ymd}={x:.2}\""),
                None => print!(" {ymd}=null"),
            }
        }
        println!();
    }
}

fn print_recommendation(
    primary_ok: &[&RankedStation],
    backup_ok: &[&RankedStation],
    current: &[(&str, &str)],
) {
    if primary_ok.is_empty() {
        println!("recommend: (no primary within {PRIMARY_MAX_MI:.0} mi)");
        if let Some(b) = backup_ok.first() {
            println!(
                "fallback only: {} ({}) — backup tier, not a normal feed primary",
                b.id,
                fmt_mi(b.distance_mi)
            );
        }
        return;
    }

    let primary = primary_ok[0];
    // Prefer a secondary with a different network (PWS vs MADIS) when possible.
    let primary_net = NetworkKind::of(&primary.id);
    let secondary = primary_ok
        .iter()
        .skip(1)
        .find(|s| NetworkKind::of(&s.id) != primary_net)
        .copied()
        .or_else(|| primary_ok.get(1).copied());

    print!(
        "recommend: primary={} ({}, {})",
        primary.id,
        fmt_mi(primary.distance_mi),
        NetworkKind::of(&primary.id).label()
    );
    if let Some(s) = secondary {
        print!(
            "  secondary={} ({}, {})",
            s.id,
            fmt_mi(s.distance_mi),
            NetworkKind::of(&s.id).label()
        );
    }
    println!();

    let feed_ids: BTreeSet<&str> = current.iter().map(|(id, _)| *id).collect();
    let rec_ids: BTreeSet<&str> = std::iter::once(primary.id.as_str())
        .chain(secondary.map(|s| s.id.as_str()))
        .collect();
    if feed_ids != rec_ids {
        println!(
            "note: differs from current feed — update TRAILS in src/xweather/mod.rs only after multi-event review."
        );
    } else {
        println!("note: matches current feed set (order may differ).");
    }
    if !backup_ok.is_empty() {
        println!(
            "note: {} backup-tier station(s) listed above are not recommended as primary.",
            backup_ok.len()
        );
    }
}

fn fmt_mi(distance_mi: f64) -> String {
    if distance_mi.is_finite() {
        format!("{distance_mi:.1} mi")
    } else {
        "? mi".into()
    }
}

fn evaluate_station(
    auth: &Auth,
    c: &Candidate,
    ref_days: &[(String, Option<f64>)],
    qc_days: u32,
) -> Result<Result<RankedStation, RejectReason>, String> {
    let summary = match fetch_station_daily(auth, &c.id, qc_days) {
        Ok(s) => s,
        Err(e) => return Ok(Err(RejectReason::FetchError(e))),
    };
    if summary.is_empty() {
        return Ok(Err(RejectReason::EmptySummary));
    }

    let mut any_precip_object = false;
    let mut any_non_null = false;
    let mut any_positive = false;
    let mut wet_days = 0usize;
    let mut max_day = 0.0f64;
    let mut pairs: Vec<(f64, f64)> = Vec::new();
    let ref_map: BTreeMap<&str, Option<f64>> = ref_days
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect();

    let mut day_totals = Vec::new();
    for (ymd, total) in &summary {
        day_totals.push((ymd.clone(), *total));
        match total {
            None => {}
            Some(v) => {
                any_precip_object = true;
                any_non_null = true;
                if *v >= TRACE_IN {
                    any_positive = true;
                    wet_days += 1;
                }
                max_day = max_day.max(*v);
                if let Some(Some(r)) = ref_map.get(ymd.as_str()) {
                    pairs.push((*v, *r));
                }
            }
        }
    }

    // Summary days present but every precip total missing.
    if !any_non_null {
        if !c.has_live_precip_field {
            return Ok(Err(RejectReason::NoPrecipField));
        }
        return Ok(Err(RejectReason::NullPrecipAllDays));
    }

    let ref_wet = ref_days
        .iter()
        .filter(|(_, v)| v.map(|x| x >= TRACE_IN).unwrap_or(false))
        .count();

    // Stuck gauge: regional rain on multiple days, station always zero.
    if ref_wet >= REF_WET_FOR_STUCK && !any_positive && any_precip_object {
        return Ok(Err(RejectReason::StuckZero { ref_wet }));
    }

    // No precip capability signals even with zeros only when ref is dry —
    // still allow if live fields exist and summary reports zeros (valid dry week).
    if !any_precip_object && !c.has_live_precip_field {
        return Ok(Err(RejectReason::NoPrecipField));
    }

    let mae_in = if pairs.len() >= 2 {
        let sum: f64 = pairs.iter().map(|(a, b)| (a - b).abs()).sum();
        Some(sum / pairs.len() as f64)
    } else {
        None
    };

    Ok(Ok(RankedStation {
        id: c.id.clone(),
        distance_mi: c.distance_mi,
        place: c.place.clone(),
        trust: c.trust,
        last_ob: c.last_ob.clone(),
        wet_days,
        ref_wet_days: ref_wet,
        mae_in,
        max_day_in: max_day,
        day_totals,
    }))
}

fn fetch_closest(
    auth: &Auth,
    lat: f64,
    lon: f64,
    limit: u32,
    filter: &str,
) -> Result<Vec<Candidate>, String> {
    let url = format!(
        "{BASE_URL}/observations/closest?p={lat},{lon}&limit={limit}&filter={filter}&{}",
        auth.query()
    );
    let body = http_get_json(&url, &format!("closest/{filter}"))?;
    let parsed: ClosestResponse = serde_json::from_str(&body)
        .map_err(|e| format!("closest/{filter} parse error: {e}"))?;
    if let Some(err) = parsed.error {
        let desc = err
            .description
            .unwrap_or_else(|| format!("{:?}", err.code));
        // empty results sometimes come as error — treat as empty
        if desc.to_lowercase().contains("no results") {
            return Ok(Vec::new());
        }
        return Err(format!("closest/{filter}: {desc}"));
    }
    let mut out = Vec::new();
    for s in parsed.response.unwrap_or_default() {
        let Some(id) = s.id else { continue };
        let lat_s = s.loc.as_ref().and_then(|l| l.lat);
        let lon_s = s.loc.as_ref().and_then(|l| l.long);
        let distance_mi = s
            .relative_to
            .as_ref()
            .and_then(|r| r.distance_mi)
            .or_else(|| match (lat_s, lon_s) {
                (Some(a), Some(b)) => Some(haversine_mi(lat, lon, a, b)),
                _ => None,
            })
            .unwrap_or(999.0);
        let place = s
            .place
            .as_ref()
            .and_then(|p| p.name.clone())
            .unwrap_or_default();
        let ob = s.ob.as_ref();
        let has_live_precip_field = ob
            .map(|o| {
                o.precip_in.is_some()
                    || o.precip_since_midnight_in.is_some()
                    || o.precip_since_last_ob_in.is_some()
            })
            .unwrap_or(false);
        out.push(Candidate {
            id,
            distance_mi,
            place,
            trust: ob.and_then(|o| o.trust_factor),
            last_ob: ob.and_then(|o| o.date_time_iso.clone()),
            has_live_precip_field,
        });
    }
    Ok(out)
}

fn fetch_one_meta(
    auth: &Auth,
    id: &str,
    trail_lat: f64,
    trail_lon: f64,
) -> Result<Option<Candidate>, String> {
    // Prefer latest obs; fall back to summary when the station is temporarily offline
    // (success + warn_no_data + response:[]), which is common for MID_C8019-class gauges.
    if let Some(c) = fetch_one_meta_latest(auth, id, trail_lat, trail_lon)? {
        return Ok(Some(c));
    }
    fetch_one_meta_from_summary(auth, id, trail_lat, trail_lon)
}

fn fetch_one_meta_latest(
    auth: &Auth,
    id: &str,
    trail_lat: f64,
    trail_lon: f64,
) -> Result<Option<Candidate>, String> {
    let url = format!("{BASE_URL}/observations/{id}?{}", auth.query());
    let body = match http_get_json(&url, id) {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let value: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if is_no_data_error(value.get("error")) {
        return Ok(None);
    }
    let response = value.get("response");
    let station_val = match response {
        Some(serde_json::Value::Object(_)) => response.cloned(),
        Some(serde_json::Value::Array(items)) => items.first().cloned(),
        _ => None,
    };
    let Some(station_val) = station_val else {
        return Ok(None);
    };
    let s: ClosestStation = match serde_json::from_value(station_val) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    Ok(Some(candidate_from_closest(s, id, trail_lat, trail_lon)))
}

fn fetch_one_meta_from_summary(
    auth: &Auth,
    id: &str,
    trail_lat: f64,
    trail_lon: f64,
) -> Result<Option<Candidate>, String> {
    let url = format!(
        "{BASE_URL}/observations/summary/{id}?from=-2days&to=now&plimit=1&{}",
        auth.query()
    );
    let body = match http_get_json(&url, &format!("summary-meta/{id}")) {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    let value: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if is_no_data_error(value.get("error"))
        && value.get("success") == Some(&serde_json::Value::Bool(false))
    {
        return Ok(None);
    }
    let obj = match value.get("response") {
        Some(serde_json::Value::Object(o)) => Some(o),
        Some(serde_json::Value::Array(items)) => items.first().and_then(|v| v.as_object()),
        _ => None,
    };
    let Some(obj) = obj else {
        return Ok(None);
    };
    let lat_s = obj
        .get("loc")
        .and_then(|l| l.get("lat"))
        .and_then(|v| v.as_f64())
        .unwrap_or(trail_lat);
    let lon_s = obj
        .get("loc")
        .and_then(|l| l.get("long"))
        .and_then(|v| v.as_f64())
        .unwrap_or(trail_lon);
    let place = obj
        .get("place")
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(Some(Candidate {
        id: id.to_string(),
        distance_mi: haversine_mi(trail_lat, trail_lon, lat_s, lon_s),
        place,
        trust: None,
        last_ob: None,
        has_live_precip_field: true,
    }))
}

fn candidate_from_closest(
    s: ClosestStation,
    fallback_id: &str,
    trail_lat: f64,
    trail_lon: f64,
) -> Candidate {
    let lat_s = s.loc.as_ref().and_then(|l| l.lat).unwrap_or(trail_lat);
    let lon_s = s.loc.as_ref().and_then(|l| l.long).unwrap_or(trail_lon);
    let ob = s.ob.as_ref();
    Candidate {
        id: s.id.unwrap_or_else(|| fallback_id.to_string()),
        distance_mi: haversine_mi(trail_lat, trail_lon, lat_s, lon_s),
        place: s
            .place
            .as_ref()
            .and_then(|p| p.name.clone())
            .unwrap_or_default(),
        trust: ob.and_then(|o| o.trust_factor),
        last_ob: ob.and_then(|o| o.date_time_iso.clone()),
        has_live_precip_field: ob
            .map(|o| {
                o.precip_in.is_some()
                    || o.precip_since_midnight_in.is_some()
                    || o.precip_since_last_ob_in.is_some()
            })
            .unwrap_or(false),
    }
}

fn is_no_data_error(error: Option<&serde_json::Value>) -> bool {
    let Some(err) = error.filter(|e| !e.is_null()) else {
        return false;
    };
    let code = err
        .get("code")
        .and_then(|c| c.as_str())
        .unwrap_or_default();
    let desc = err
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or_default();
    code == "warn_no_data" || desc.to_ascii_lowercase().contains("no results")
}

fn nan_last_distance(d: f64) -> f64 {
    if d.is_finite() {
        d
    } else {
        f64::MAX
    }
}

fn fetch_station_daily(
    auth: &Auth,
    id: &str,
    days: u32,
) -> Result<Vec<(String, Option<f64>)>, String> {
    let url = format!(
        "{BASE_URL}/observations/summary/{id}?from=-{days}days&to=now&plimit={days}&{}",
        auth.query()
    );
    let body = http_get_json(&url, &format!("summary/{id}"))?;
    let parsed: SummaryResponse =
        serde_json::from_str(&body).map_err(|e| format!("summary/{id} parse: {e}"))?;
    if let Some(err) = parsed.error {
        let desc = err
            .description
            .unwrap_or_else(|| format!("{:?}", err.code));
        return Err(desc);
    }
    Ok(parse_summary_periods(parsed.response.as_ref()))
}

fn fetch_conditions_daily(
    auth: &Auth,
    lat: f64,
    lon: f64,
    days: u32,
) -> Result<Vec<(String, Option<f64>)>, String> {
    let url = format!(
        "{BASE_URL}/conditions/summary/{lat},{lon}?from=-{days}days&to=now&plimit={days}&{}",
        auth.query()
    );
    let body = http_get_json(&url, "conditions/summary")?;
    let parsed: ConditionsSummaryResponse =
        serde_json::from_str(&body).map_err(|e| format!("conditions/summary parse: {e}"))?;
    if let Some(err) = parsed.error {
        let desc = err
            .description
            .unwrap_or_else(|| format!("{:?}", err.code));
        return Err(format!("conditions/summary: {desc}"));
    }
    Ok(parse_conditions_periods(parsed.response.as_ref()))
}

fn parse_summary_periods(response: Option<&serde_json::Value>) -> Vec<(String, Option<f64>)> {
    let mut out = Vec::new();
    let Some(response) = response else {
        return out;
    };
    let periods = if let Some(arr) = response.as_array() {
        arr.first()
            .and_then(|o| o.get("periods"))
            .and_then(|p| p.as_array())
    } else {
        response.get("periods").and_then(|p| p.as_array())
    };
    let Some(periods) = periods else {
        return out;
    };
    for p in periods {
        let summary = p.get("summary");
        let ymd = summary
            .and_then(|s| s.get("ymd"))
            .and_then(|v| match v {
                serde_json::Value::Number(n) => n.as_i64().map(|i| i.to_string()),
                serde_json::Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .or_else(|| {
                p.get("dateTimeISO")
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(10).filter(|c| *c != '-').collect())
            })
            .unwrap_or_else(|| "?".into());
        let ymd = normalize_ymd(&ymd);
        // precip may be missing entirely, or present with null totals
        let precip = summary.and_then(|s| s.get("precip"));
        let total = precip.and_then(|pr| {
            pr.get("totalIN")
                .and_then(|v| v.as_f64())
                .or_else(|| pr.get("totalMM").and_then(|v| v.as_f64()).map(|mm| mm / 25.4))
        });
        // If precip key missing entirely, still record as None
        if precip.is_none() {
            out.push((ymd, None));
        } else {
            out.push((ymd, total));
        }
    }
    out
}

fn parse_conditions_periods(response: Option<&serde_json::Value>) -> Vec<(String, Option<f64>)> {
    let mut out = Vec::new();
    let Some(response) = response else {
        return out;
    };
    let periods = if let Some(arr) = response.as_array() {
        arr.first()
            .and_then(|o| o.get("periods"))
            .and_then(|p| p.as_array())
    } else {
        response.get("periods").and_then(|p| p.as_array())
    };
    let Some(periods) = periods else {
        return out;
    };
    for p in periods {
        let ymd = p
            .get("dateTimeISO")
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(10).collect::<String>())
            .map(|s| normalize_ymd(&s))
            .or_else(|| {
                p.get("summary")
                    .and_then(|s| s.get("ymd"))
                    .and_then(|v| v.as_i64())
                    .map(|i| i.to_string())
            })
            .unwrap_or_else(|| "?".into());
        let total = p
            .get("precip")
            .and_then(|pr| pr.get("totalIN"))
            .and_then(|v| v.as_f64())
            .or_else(|| {
                p.get("summary")
                    .and_then(|s| s.get("precip"))
                    .and_then(|pr| pr.get("totalIN"))
                    .and_then(|v| v.as_f64())
            });
        out.push((ymd, total));
    }
    out
}

fn normalize_ymd(raw: &str) -> String {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 8 {
        return digits[..8].to_string();
    }
    // already YYYY-MM-DD
    if raw.len() >= 10 && raw.as_bytes().get(4) == Some(&b'-') {
        return format!("{}{}{}", &raw[0..4], &raw[5..7], &raw[8..10]);
    }
    raw.to_string()
}

fn haversine_mi(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 3958.8;
    let la1 = lat1.to_radians();
    let la2 = lat2.to_radians();
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * R * h.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_ymd_formats() {
        assert_eq!(normalize_ymd("20260718"), "20260718");
        assert_eq!(normalize_ymd("2026-07-18"), "20260718");
        assert_eq!(normalize_ymd("2026-07-18T12:00:00-04:00"), "20260718");
    }

    #[test]
    fn haversine_qw_to_pws() {
        // Quiet Waters -> PWS_363636363 ~2.4 mi
        let mi = haversine_mi(26.31012, -80.16113, 26.344482, -80.163688);
        assert!((2.0..3.0).contains(&mi), "got {mi}");
    }

    #[test]
    fn stuck_zero_logic_via_evaluate_shape() {
        // Unit-level: RejectReason label is stable
        let r = RejectReason::StuckZero { ref_wet: 3 };
        assert!(r.label().contains("stuck"));
    }

    #[test]
    fn distance_tiers_split_at_five_and_ten() {
        assert_eq!(DistanceTier::of(3.2), DistanceTier::Primary);
        assert_eq!(DistanceTier::of(5.0), DistanceTier::Primary);
        assert_eq!(DistanceTier::of(5.1), DistanceTier::Backup);
        assert_eq!(DistanceTier::of(7.5), DistanceTier::Backup);
        assert_eq!(DistanceTier::of(10.0), DistanceTier::Backup);
        assert_eq!(DistanceTier::of(10.1), DistanceTier::Regional);
    }

    #[test]
    fn network_kind_from_id_prefix() {
        assert_eq!(NetworkKind::of("PWS_W4RCT"), NetworkKind::Pws);
        assert_eq!(NetworkKind::of("MID_C8019"), NetworkKind::Madis);
        assert_eq!(NetworkKind::of("KFXE"), NetworkKind::Other);
    }

    #[test]
    fn closer_beats_better_mae_within_primary_tier() {
        let near = RankedStation {
            id: "NEAR".into(),
            distance_mi: 1.7,
            place: String::new(),
            trust: Some(100.0),
            last_ob: None,
            wet_days: 3,
            ref_wet_days: 6,
            mae_in: Some(0.20),
            max_day_in: 0.5,
            day_totals: vec![],
        };
        let far = RankedStation {
            id: "FAR".into(),
            distance_mi: 3.7,
            place: String::new(),
            trust: Some(100.0),
            last_ob: None,
            wet_days: 4,
            ref_wet_days: 6,
            mae_in: Some(0.08),
            max_day_in: 0.5,
            day_totals: vec![],
        };
        let peers = BTreeMap::new();
        assert_eq!(
            compare_stations(&near, &far, &peers),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn parse_summary_extracts_totals() {
        let v: serde_json::Value = serde_json::json!({
            "periods": [{
                "summary": {
                    "ymd": 20260718,
                    "precip": { "totalIN": 0.6, "method": "EOD24hr" }
                }
            }, {
                "summary": {
                    "ymd": 20260719,
                    "precip": { "totalIN": null }
                }
            }, {
                "summary": {
                    "ymd": 20260717
                }
            }]
        });
        let days = parse_summary_periods(Some(&v));
        assert_eq!(days.len(), 3);
        assert_eq!(days[0].0, "20260718");
        assert_eq!(days[0].1, Some(0.6));
        assert_eq!(days[1].1, None);
        assert_eq!(days[2].1, None);
    }
}
