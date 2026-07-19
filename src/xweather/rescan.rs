//! Discover and rank nearby stations with working rain gauges.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::trails::Trail;

use super::{http_get_json, Auth, BASE_URL, TRAILS};

const DEFAULT_CLOSEST_LIMIT: u32 = 15;
const DEFAULT_QC_DAYS: u32 = 7;
const DEFAULT_QC_CANDIDATES: usize = 12;
const MAX_DISTANCE_MI: f64 = 15.0;
const TRACE_IN: f64 = 0.01;
const REF_WET_FOR_STUCK: usize = 2;
/// Known bad rain meters (always reject).
const BLOCKLIST: &[&str] = &["MID_D4511"];

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
        "Usage:\n  jaycast xweather rescan [camp-murphy|markham|quiet-waters] [--limit N] [--days N] [--candidates N]\n\nFinds nearby PWS/mesonet stations, rejects bad/missing rain meters, ranks survivors.\nDoes not change the feed station table (print-only).\n\nDefaults: --limit {DEFAULT_CLOSEST_LIMIT}  --days {DEFAULT_QC_DAYS}  --candidates {DEFAULT_QC_CANDIDATES}"
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

    // Always include current feed stations even if outside closest page.
    for (id, _) in &current {
        if by_id.contains_key(*id) {
            continue;
        }
        if let Ok(Some(c)) = fetch_one_meta(auth, id, lat, lon) {
            by_id.insert(c.id.clone(), c);
        }
    }

    let mut candidates: Vec<Candidate> = by_id.into_values().collect();
    candidates.sort_by(|a, b| {
        a.distance_mi
            .partial_cmp(&b.distance_mi)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut rejected: Vec<(Candidate, RejectReason)> = Vec::new();
    let mut to_qc: Vec<Candidate> = Vec::new();

    for c in candidates {
        if BLOCKLIST.contains(&c.id.as_str()) {
            rejected.push((c, RejectReason::Blocklist));
            continue;
        }
        if c.distance_mi > MAX_DISTANCE_MI {
            rejected.push((c, RejectReason::TooFar));
            continue;
        }
        to_qc.push(c);
    }

    // QC nearest first, cap API calls.
    to_qc.truncate(qc_candidates);

    let mut ranked: Vec<RankedStation> = Vec::new();
    for c in to_qc {
        eprintln!("xweather rescan: summary {} …", c.id);
        match evaluate_station(auth, &c, &ref_days, qc_days) {
            Ok(Ok(r)) => ranked.push(r),
            Ok(Err(reason)) => rejected.push((c, reason)),
            Err(e) => rejected.push((c, RejectReason::FetchError(e))),
        }
    }

    ranked.sort_by(|a, b| {
        // Prefer lower MAE when both have it, then closer, then more wet days.
        let mae_a = a.mae_in.unwrap_or(999.0);
        let mae_b = b.mae_in.unwrap_or(999.0);
        mae_a
            .partial_cmp(&mae_b)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.distance_mi
                    .partial_cmp(&b.distance_mi)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(b.wet_days.cmp(&a.wet_days))
    });

    println!();
    println!("REJECTED ({}):", rejected.len());
    if rejected.is_empty() {
        println!("  (none)");
    } else {
        rejected.sort_by(|a, b| {
            a.0.distance_mi
                .partial_cmp(&b.0.distance_mi)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (c, reason) in &rejected {
            println!(
                "  {:20} {:5.1} mi  {}  — {}",
                c.id,
                c.distance_mi,
                c.place,
                reason.label()
            );
        }
    }

    println!();
    println!("OK ranked ({}):", ranked.len());
    if ranked.is_empty() {
        println!("  (none with working rain)");
    } else {
        println!(
            "  {:3} {:20} {:>6}  {:>7}  {:>8}  {:>6}  {}",
            "#", "id", "mi", "wet", "mae", "max", "place"
        );
        for (i, r) in ranked.iter().enumerate() {
            let mae = r
                .mae_in
                .map(|m| format!("{m:.2}\""))
                .unwrap_or_else(|| "-".into());
            let trust = r
                .trust
                .map(|t| format!("{t:.0}"))
                .unwrap_or_else(|| "-".into());
            println!(
                "  {:3} {:20} {:5.1}  {:>3}/{:<3}  {:>8}  {:5.2}\"  {}  trust={}",
                i + 1,
                r.id,
                r.distance_mi,
                r.wet_days,
                r.ref_wet_days,
                mae,
                r.max_day_in,
                r.place,
                trust
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

    println!();
    if ranked.is_empty() {
        println!("recommend: (no usable rain stations)");
    } else {
        let primary = &ranked[0];
        let secondary = ranked.get(1);
        print!("recommend: primary={} ({:.1} mi)", primary.id, primary.distance_mi);
        if let Some(s) = secondary {
            print!(
                "  secondary={} ({:.1} mi)",
                s.id, s.distance_mi
            );
        }
        println!();
        // Note if feed differs.
        let feed_ids: BTreeSet<&str> = current.iter().map(|(id, _)| *id).collect();
        let rec_ids: BTreeSet<&str> = std::iter::once(primary.id.as_str())
            .chain(secondary.map(|s| s.id.as_str()))
            .collect();
        if feed_ids != rec_ids {
            println!("note: differs from current feed station set — update TRAILS in src/xweather/mod.rs if you want to switch.");
        } else {
            println!("note: matches current feed set (order may differ).");
        }
    }

    Ok(())
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

fn fetch_one_meta(auth: &Auth, id: &str, trail_lat: f64, trail_lon: f64) -> Result<Option<Candidate>, String> {
    let url = format!("{BASE_URL}/observations/{id}?{}", auth.query());
    let body = match http_get_json(&url, id) {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    #[derive(Deserialize)]
    struct One {
        response: Option<ClosestStation>,
        error: Option<super::ApiError>,
    }
    let parsed: One = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    if parsed.error.is_some() {
        return Ok(None);
    }
    let Some(s) = parsed.response else {
        return Ok(None);
    };
    let lat_s = s.loc.as_ref().and_then(|l| l.lat).unwrap_or(trail_lat);
    let lon_s = s.loc.as_ref().and_then(|l| l.long).unwrap_or(trail_lon);
    let ob = s.ob.as_ref();
    Ok(Some(Candidate {
        id: s.id.unwrap_or_else(|| id.to_string()),
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
    }))
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
