//! Xweather ground-truth gauge rain (native/CLI only).
//!
//! Fetches station archive observations and reduces them to hourly tip totals
//! (inches) for a static JSON feed the WASM app can load later.
//!
//! Completed past days are cached on disk; only today is always refetched.

mod rescan;

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://data.api.xweather.com";
const TIMEZONE: &str = "America/New_York";
const STALE_AFTER_SECS: i64 = 3 * 60 * 60;
const DEFAULT_DAYS: u32 = 2;
const SCHEMA_VERSION: u32 = 1;
const CACHE_SCHEMA: u32 = 1;
const CACHE_RETENTION_DAYS: i64 = 60;
const DEFAULT_CACHE_NAME: &str = ".jaycast-xweather-cache.json";

#[derive(Clone, Copy, Debug)]
struct StationSpec {
    id: &'static str,
    role: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct TrailSpec {
    slug: &'static str,
    stations: &'static [StationSpec],
}

const TRAILS: &[TrailSpec] = &[
    TrailSpec {
        slug: "markham",
        stations: &[
            StationSpec {
                id: "MID_E8181",
                role: "primary",
            },
            StationSpec {
                id: "PWS_W4RCT",
                role: "secondary",
            },
        ],
    },
    TrailSpec {
        slug: "camp-murphy",
        stations: &[
            StationSpec {
                id: "MID_C8019",
                role: "primary",
            },
            StationSpec {
                id: "PWS_JOE4SPEED",
                role: "primary",
            },
        ],
    },
    TrailSpec {
        slug: "quiet-waters",
        stations: &[StationSpec {
            id: "PWS_363636363",
            role: "primary",
        }],
    },
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Feed {
    pub schema: u32,
    pub generated_at: String,
    pub timezone: String,
    pub days: u32,
    pub day_start: String,
    pub day_end: String,
    pub trails: BTreeMap<String, TrailFeed>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TrailFeed {
    pub stations: Vec<StationFeed>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StationFeed {
    pub id: String,
    pub role: String,
    pub days: Vec<DayFeed>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DayFeed {
    pub date: String,
    pub hourly_tips_in: [f64; 24],
    pub day_total_in: f64,
    pub last_ob: Option<String>,
    pub stale: bool,
}

#[derive(Debug, Deserialize)]
struct ArchiveResponse {
    response: Option<ArchiveBody>,
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ArchiveBody {
    periods: Option<Vec<ArchivePeriod>>,
}

#[derive(Debug, Deserialize)]
struct ArchivePeriod {
    ob: Option<Observation>,
}

#[derive(Debug, Deserialize)]
struct Observation {
    #[serde(rename = "dateTimeISO")]
    date_time_iso: Option<String>,
    #[serde(rename = "precipSinceLastObIN")]
    precip_since_last_ob_in: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    description: Option<String>,
    code: Option<serde_json::Value>,
}

#[derive(Clone, Debug)]
struct Auth {
    client_id: String,
    client_secret: String,
}

impl Auth {
    fn from_env() -> Result<Self, String> {
        let key = std::env::var("XWEATHER_API_KEY").map_err(|_| {
            "XWEATHER_API_KEY is not set (client_id and client_secret joined with '_')"
                .to_string()
        })?;
        let (client_id, client_secret) = key.split_once('_').ok_or_else(|| {
            "XWEATHER_API_KEY must be client_id_client_secret (first '_' splits)".to_string()
        })?;
        if client_id.is_empty() || client_secret.is_empty() {
            return Err("XWEATHER_API_KEY client_id or client_secret is empty".into());
        }
        Ok(Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
        })
    }

    fn query(&self) -> String {
        format!(
            "client_id={}&client_secret={}",
            self.client_id, self.client_secret
        )
    }
}

/// CLI entry: `xweather publish|dump|rescan …`
pub fn run(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    match args.next().as_deref() {
        Some("publish") => {
            let opts = parse_opts(args, true)?;
            let out = opts.out.expect("--out required");
            let cache_path = opts.cache.unwrap_or_else(|| default_cache_path(Some(&out)));
            let feed = build_feed(opts.days, &cache_path)?;
            write_atomic(&out, &feed)?;
            Ok(())
        }
        Some("dump") => {
            let opts = parse_opts(args, false)?;
            let cache_path = opts.cache.unwrap_or_else(|| default_cache_path(None));
            let feed = build_feed(opts.days, &cache_path)?;
            let json = serde_json::to_string_pretty(&feed)
                .map_err(|e| format!("could not serialize feed: {e}"))?;
            println!("{json}");
            Ok(())
        }
        Some("rescan") => rescan::run(args),
        Some("--help" | "-h" | "help") | None => {
            print_help();
            Ok(())
        }
        Some(cmd) => Err(format!("unknown xweather command {cmd:?}")),
    }
}

pub fn print_help() {
    eprintln!(
        "Usage:\n  jaycast xweather publish --out <PATH> [--days N] [--cache PATH]\n  jaycast xweather dump [--days N] [--cache PATH]\n  jaycast xweather rescan [trail] [--limit N] [--days N] [--candidates N]\n\nEnvironment:\n  XWEATHER_API_KEY   client_id_client_secret\n\nDefaults:\n  --days {DEFAULT_DAYS}   full local days ending today (host local date)\n  --cache  beside --out, else ./{DEFAULT_CACHE_NAME}\n\nPast days are read from the cache when present; today is always fetched.\nrescan discovers nearby gauges, rejects bad rain meters, prints recommendations."
    );
}

struct Opts {
    out: Option<PathBuf>,
    days: u32,
    cache: Option<PathBuf>,
}

fn parse_opts(mut args: impl Iterator<Item = String>, require_out: bool) -> Result<Opts, String> {
    let mut out = None;
    let mut days = DEFAULT_DAYS;
    let mut cache = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => {
                let path = args
                    .next()
                    .ok_or_else(|| "missing path after --out".to_string())?;
                out = Some(PathBuf::from(path));
            }
            "--days" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value after --days".to_string())?;
                days = value
                    .parse::<u32>()
                    .map_err(|_| format!("invalid --days {value:?}"))?;
                if days == 0 {
                    return Err("--days must be at least 1".into());
                }
                if days > 31 {
                    return Err("--days must be at most 31".into());
                }
            }
            "--cache" => {
                let path = args
                    .next()
                    .ok_or_else(|| "missing path after --cache".to_string())?;
                cache = Some(PathBuf::from(path));
            }
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }
    if require_out && out.is_none() {
        return Err("publish requires --out <PATH>".into());
    }
    Ok(Opts { out, days, cache })
}

fn default_cache_path(out: Option<&Path>) -> PathBuf {
    match out.and_then(|p| p.parent()) {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(DEFAULT_CACHE_NAME),
        _ => PathBuf::from(DEFAULT_CACHE_NAME),
    }
}

/// station_id → date (YYYY-MM-DD) → day
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct DayCache {
    schema: u32,
    stations: BTreeMap<String, BTreeMap<String, DayFeed>>,
}

impl DayCache {
    fn load(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self {
                schema: CACHE_SCHEMA,
                stations: BTreeMap::new(),
            };
        };
        match serde_json::from_str::<DayCache>(&raw) {
            Ok(cache) if cache.schema == CACHE_SCHEMA => cache,
            _ => Self {
                schema: CACHE_SCHEMA,
                stations: BTreeMap::new(),
            },
        }
    }

    fn get(&self, station_id: &str, date: NaiveDate) -> Option<DayFeed> {
        self.stations
            .get(station_id)
            .and_then(|days| days.get(&date.to_string()))
            .cloned()
    }

    fn insert(&mut self, station_id: &str, day: DayFeed) {
        self.stations
            .entry(station_id.to_string())
            .or_default()
            .insert(day.date.clone(), day);
    }

    fn prune_before(&mut self, keep_from: NaiveDate) {
        let keep = keep_from.to_string();
        for days in self.stations.values_mut() {
            days.retain(|date, _| date.as_str() >= keep.as_str());
        }
        self.stations.retain(|_, days| !days.is_empty());
    }

    fn save(&self, path: &Path) -> Result<(), String> {
        write_json_atomic(path, self)
    }
}

fn build_feed(days: u32, cache_path: &Path) -> Result<Feed, String> {
    let auth = Auth::from_env()?;
    let today = Local::now().date_naive();
    let (day_start, day_end) = day_range(today, days);
    let generated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let now_ts = chrono::Utc::now().timestamp();

    let mut cache = DayCache::load(cache_path);
    let mut cache_dirty = false;
    eprintln!(
        "xweather: range {day_start}..={day_end}  cache={}",
        cache_path.display()
    );

    let mut trails = BTreeMap::new();
    for trail in TRAILS {
        let mut stations = Vec::with_capacity(trail.stations.len());
        for station in trail.stations {
            let mut day_feeds = Vec::with_capacity(days as usize);
            let mut date = day_start;
            loop {
                let day = if date < today {
                    if let Some(cached) = cache.get(station.id, date) {
                        eprintln!(
                            "xweather: cache  {} {}  total={:.2}\"  last_ob={}",
                            station.id,
                            date,
                            cached.day_total_in,
                            cached.last_ob.as_deref().unwrap_or("-"),
                        );
                        cached
                    } else {
                        eprintln!("xweather: api    {} {}  (past, uncached)", station.id, date);
                        let periods = fetch_archive(&auth, station.id, date)?;
                        let day = day_from_periods(date, &periods, now_ts);
                        eprintln!(
                            "xweather: api    {} {}  total={:.2}\"  obs={}  last_ob={}",
                            station.id,
                            date,
                            day.day_total_in,
                            periods.len(),
                            day.last_ob.as_deref().unwrap_or("-"),
                        );
                        cache.insert(station.id, day.clone());
                        cache_dirty = true;
                        day
                    }
                } else {
                    eprintln!("xweather: api    {} {}  (today)", station.id, date);
                    let periods = fetch_archive(&auth, station.id, date)?;
                    let day = day_from_periods(date, &periods, now_ts);
                    eprintln!(
                        "xweather: api    {} {}  total={:.2}\"  obs={}  last_ob={}  stale={}",
                        station.id,
                        date,
                        day.day_total_in,
                        periods.len(),
                        day.last_ob.as_deref().unwrap_or("-"),
                        day.stale,
                    );
                    day
                };
                day_feeds.push(day);
                if date == day_end {
                    break;
                }
                date = date
                    .succ_opt()
                    .ok_or_else(|| "date overflow".to_string())?;
            }
            stations.push(StationFeed {
                id: station.id.to_string(),
                role: station.role.to_string(),
                days: day_feeds,
            });
        }
        trails.insert(trail.slug.to_string(), TrailFeed { stations });
    }

    if cache_dirty {
        let keep_from = today - chrono::Duration::days(CACHE_RETENTION_DAYS);
        cache.prune_before(keep_from);
        cache.save(cache_path)?;
        eprintln!("xweather: cache written {}", cache_path.display());
    } else {
        eprintln!("xweather: cache unchanged {}", cache_path.display());
    }

    Ok(Feed {
        schema: SCHEMA_VERSION,
        generated_at,
        timezone: TIMEZONE.to_string(),
        days,
        day_start: day_start.to_string(),
        day_end: day_end.to_string(),
        trails,
    })
}

fn day_range(today: NaiveDate, days: u32) -> (NaiveDate, NaiveDate) {
    let start = today - chrono::Duration::days(i64::from(days.saturating_sub(1)));
    (start, today)
}

fn http_get_json(url: &str, label: &str) -> Result<String, String> {
    for attempt in 0..3u32 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_secs(1));
        }
        match ureq::get(url).call() {
            Ok(response) => {
                return response
                    .into_string()
                    .map_err(|e| format!("{label} response body error: {e}"));
            }
            Err(ureq::Error::Status(503, _)) if attempt < 2 => continue,
            Err(ureq::Error::Status(code, resp)) => {
                let detail = resp.into_string().unwrap_or_default();
                return Err(format!("{label} request failed: HTTP {code} {detail}"));
            }
            Err(error) => return Err(format!("{label} request failed: {error}")),
        }
    }
    unreachable!()
}

fn fetch_archive(
    auth: &Auth,
    station_id: &str,
    date: NaiveDate,
) -> Result<Vec<ArchivePeriod>, String> {
    let url = format!(
        "{BASE_URL}/observations/archive/{station_id}?from={date}&{}",
        auth.query()
    );
    let body = http_get_json(&url, station_id)?;
    let parsed: ArchiveResponse = serde_json::from_str(&body)
        .map_err(|e| format!("{station_id} archive for {date} parse error: {e}"))?;
    if let Some(err) = parsed.error {
        let desc = err
            .description
            .unwrap_or_else(|| format!("{:?}", err.code));
        return Err(format!("{station_id} archive for {date}: {desc}"));
    }
    Ok(parsed
        .response
        .and_then(|r| r.periods)
        .unwrap_or_default())
}

fn day_from_periods(date: NaiveDate, periods: &[ArchivePeriod], now_ts: i64) -> DayFeed {
    let mut hourly = [0.0_f64; 24];
    let mut last_ob: Option<String> = None;
    let mut last_ob_ts: Option<i64> = None;

    for period in periods {
        let Some(ob) = period.ob.as_ref() else {
            continue;
        };
        let Some(iso) = ob.date_time_iso.as_deref() else {
            continue;
        };
        if let Some(hour) = local_hour(iso) {
            if let Some(tip) = ob.precip_since_last_ob_in {
                if tip.is_finite() && tip > 0.0 {
                    hourly[hour as usize] += tip;
                }
            }
        }
        last_ob = Some(iso.to_string());
        if let Some(ts) = parse_iso_timestamp(iso) {
            last_ob_ts = Some(ts);
        }
    }

    // Round tips to hundredths for stable JSON.
    for v in &mut hourly {
        *v = (*v * 100.0).round() / 100.0;
    }
    let day_total_in = (hourly.iter().sum::<f64>() * 100.0).round() / 100.0;
    let stale = match last_ob_ts {
        Some(ts) => now_ts - ts > STALE_AFTER_SECS,
        None => true,
    };

    DayFeed {
        date: date.to_string(),
        hourly_tips_in: hourly,
        day_total_in,
        last_ob,
        stale,
    }
}

/// Parse local hour (0–23) from an ISO-8601 timestamp, preferring the wall-clock
/// hour in the string (station-local) over UTC conversion.
fn local_hour(iso: &str) -> Option<u32> {
    // "2026-07-18T14:05:00-04:00" or "2026-07-18T14:05:00"
    let time = iso.split('T').nth(1)?;
    let hour: u32 = time.get(0..2)?.parse().ok()?;
    (hour < 24).then_some(hour)
}

fn parse_iso_timestamp(iso: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|dt| dt.timestamp())
        .or_else(|| {
            // Fallback: date-time without offset → treat as local.
            chrono::NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S")
                .ok()
                .and_then(|ndt| Local.from_local_datetime(&ndt).single())
                .map(|dt| dt.timestamp())
        })
}

fn write_atomic(path: &Path, feed: &Feed) -> Result<(), String> {
    write_json_atomic(path, feed)
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("could not serialize {}: {e}", path.display()))?;
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(dir) = parent {
        fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    let tmp = match parent {
        Some(dir) => dir.join(format!(
            ".{}.tmp",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("xweather.json")
        )),
        None => PathBuf::from(format!(
            ".{}.tmp",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("xweather.json")
        )),
    };
    {
        let mut file = fs::File::create(&tmp)
            .map_err(|e| format!("could not create temp {}: {e}", tmp.display()))?;
        file.write_all(json.as_bytes())
            .map_err(|e| format!("could not write temp {}: {e}", tmp.display()))?;
        file.write_all(b"\n")
            .map_err(|e| format!("could not write temp {}: {e}", tmp.display()))?;
        file.sync_all()
            .map_err(|e| format!("could not sync temp {}: {e}", tmp.display()))?;
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!(
            "could not rename {} -> {}: {e}",
            tmp.display(),
            path.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_range_includes_today_and_lookback() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 19).unwrap();
        let (start, end) = day_range(today, 2);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 7, 18).unwrap());
        assert_eq!(end, today);
        let (start, end) = day_range(today, 1);
        assert_eq!(start, today);
        assert_eq!(end, today);
    }

    #[test]
    fn local_hour_reads_wall_clock() {
        assert_eq!(local_hour("2026-07-18T14:05:00-04:00"), Some(14));
        assert_eq!(local_hour("2026-07-18T00:01:00-04:00"), Some(0));
        assert_eq!(local_hour("2026-07-18T23:59:59Z"), Some(23));
        assert_eq!(local_hour("not-a-date"), None);
    }

    #[test]
    fn buckets_tips_by_hour() {
        let periods = vec![
            period("2026-07-18T11:01:00-04:00", Some(0.10)),
            period("2026-07-18T11:15:00-04:00", Some(0.14)),
            period("2026-07-18T14:00:00-04:00", Some(0.36)),
            period("2026-07-18T14:30:00-04:00", Some(0.0)),
            period("2026-07-18T15:00:00-04:00", None),
        ];
        let date = NaiveDate::from_ymd_opt(2026, 7, 18).unwrap();
        let day = day_from_periods(date, &periods, 0);
        assert_eq!(day.hourly_tips_in[11], 0.24);
        assert_eq!(day.hourly_tips_in[14], 0.36);
        assert_eq!(day.day_total_in, 0.60);
        assert_eq!(day.last_ob.as_deref(), Some("2026-07-18T15:00:00-04:00"));
    }

    #[test]
    fn marks_stale_when_last_ob_old() {
        let periods = vec![period("2026-07-18T10:00:00-04:00", Some(0.1))];
        let date = NaiveDate::from_ymd_opt(2026, 7, 18).unwrap();
        let ts = parse_iso_timestamp("2026-07-18T10:00:00-04:00").unwrap();
        let fresh = day_from_periods(date, &periods, ts + 60);
        assert!(!fresh.stale);
        let stale = day_from_periods(date, &periods, ts + STALE_AFTER_SECS + 1);
        assert!(stale.stale);
        let empty = day_from_periods(date, &[], ts);
        assert!(empty.stale);
    }

    fn period(iso: &str, tip: Option<f64>) -> ArchivePeriod {
        ArchivePeriod {
            ob: Some(Observation {
                date_time_iso: Some(iso.into()),
                precip_since_last_ob_in: tip,
            }),
        }
    }

    #[test]
    fn cache_returns_inserted_past_day() {
        let mut cache = DayCache {
            schema: CACHE_SCHEMA,
            stations: BTreeMap::new(),
        };
        let date = NaiveDate::from_ymd_opt(2026, 7, 18).unwrap();
        let day = DayFeed {
            date: date.to_string(),
            hourly_tips_in: [0.0; 24],
            day_total_in: 0.5,
            last_ob: Some("2026-07-18T23:59:00-04:00".into()),
            stale: true,
        };
        cache.insert("MID_E8181", day.clone());
        assert_eq!(cache.get("MID_E8181", date), Some(day));
        assert!(cache.get("MID_E8181", date.succ_opt().unwrap()).is_none());
    }

    #[test]
    fn cache_prunes_old_days() {
        let mut cache = DayCache {
            schema: CACHE_SCHEMA,
            stations: BTreeMap::new(),
        };
        for (date, total) in [
            ("2026-05-01", 0.1),
            ("2026-07-10", 0.2),
            ("2026-07-18", 0.3),
        ] {
            cache.insert(
                "MID_E8181",
                DayFeed {
                    date: date.into(),
                    hourly_tips_in: [0.0; 24],
                    day_total_in: total,
                    last_ob: None,
                    stale: true,
                },
            );
        }
        cache.prune_before(NaiveDate::from_ymd_opt(2026, 7, 1).unwrap());
        let days = cache.stations.get("MID_E8181").unwrap();
        assert!(!days.contains_key("2026-05-01"));
        assert!(days.contains_key("2026-07-10"));
        assert!(days.contains_key("2026-07-18"));
    }

    #[test]
    fn default_cache_sits_beside_out() {
        assert_eq!(
            default_cache_path(Some(Path::new("/var/www/rain.json"))),
            PathBuf::from(format!("/var/www/{DEFAULT_CACHE_NAME}"))
        );
        assert_eq!(
            default_cache_path(None),
            PathBuf::from(DEFAULT_CACHE_NAME)
        );
    }
}
