use std::{env, process};

use chrono::{Duration, Local, NaiveDate};
use jaycast::{
    score::{score_days, DayForecast, Params},
    weather::{
        build_date_range_url, build_historical_url, DayWeather, ForecastResponse, WeatherModel,
    },
};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        print_help();
        process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("analyze") => analyze(args),
        Some("--help" | "-h" | "help") | None => {
            print_help();
            Ok(())
        }
        Some(command) => Err(format!("unknown command {command:?}")),
    }
}

fn analyze(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let first = args.next();
    let today = Local::now().date_naive();
    let (start, end, model) = match first {
        Some(value) if matches!(value.as_str(), "gfs" | "ecmwf" | "both") => (today, today, value),
        Some(value) => {
            let (start, end) = parse_date_range(&value)?;
            (start, end, args.next().unwrap_or_else(|| "both".into()))
        }
        None => (today, today, "both".into()),
    };
    if let Some(extra) = args.next() {
        return Err(format!("unexpected argument {extra:?}"));
    }

    let models = match model.as_str() {
        "gfs" => vec![WeatherModel::GfsSeamless],
        "ecmwf" => vec![WeatherModel::Ecmwf],
        "both" => vec![WeatherModel::GfsSeamless, WeatherModel::Ecmwf],
        _ => return Err(format!("unknown model {model:?}; use gfs, ecmwf, or both")),
    };
    let fetch_start = start - Duration::days(3);
    let historical = (fetch_start < today)
        .then(|| {
            let history_end = today - Duration::days(1);
            fetch_forecast(
                build_historical_url(fetch_start, history_end),
                "historical IFS analysis",
            )
        })
        .transpose()?;
    let historical_days = historical
        .as_ref()
        .map(ForecastResponse::days)
        .unwrap_or_default();

    if start < today {
        let history_end = end.min(today - Duration::days(1));
        let response = historical
            .as_ref()
            .ok_or_else(|| "historical data was not loaded".to_string())?;
        print_range_analysis(
            "Historical ECMWF IFS analysis",
            response.latitude,
            response.longitude,
            start,
            history_end,
            &historical_days,
            today,
        )?;
    }

    if end >= today {
        let forecast_start = fetch_start.max(today);
        for model in models {
            let response = fetch_forecast(
                build_date_range_url(model, forecast_start, end),
                model.short(),
            )?;
            let mut days = historical_days.clone();
            days.extend(response.days());
            days.sort_by_key(|day| day.date);
            print_range_analysis(
                model.label(),
                response.latitude,
                response.longitude,
                start.max(today),
                end,
                &days,
                today,
            )?;
        }
    }
    Ok(())
}

fn fetch_forecast(url: String, source: &str) -> Result<ForecastResponse, String> {
    ureq::get(&url)
        .call()
        .map_err(|error| format!("{source} request failed: {error}"))?
        .into_json()
        .map_err(|error| format!("{source} response could not be parsed: {error}"))
}

fn print_range_analysis(
    source: &str,
    latitude: f64,
    longitude: f64,
    start: NaiveDate,
    end: NaiveDate,
    days: &[DayWeather],
    today: NaiveDate,
) -> Result<(), String> {
    let scores = score_days(days, today, &Params::default());

    let mut date = start;
    loop {
        let weather = days
            .iter()
            .find(|day| day.date == date)
            .ok_or_else(|| format!("{source} returned no data for {date}"))?;
        let score = scores
            .iter()
            .find(|day| day.date == date)
            .ok_or_else(|| format!("{source} could not score {date}"))?;
        print_analysis(date, source, latitude, longitude, weather, score);
        if date == end {
            break;
        }
        date += Duration::days(1);
    }
    Ok(())
}

fn print_analysis(
    date: NaiveDate,
    source: &str,
    latitude: f64,
    longitude: f64,
    weather: &DayWeather,
    score: &DayForecast,
) {
    println!("{date} | {source} | grid {latitude:.3}, {longitude:.3}");
    println!(
        "  score {:.1} stars ({:.0}%) | rain {:.2}\" total, {:.2}\" 8 AM-noon, {:.2}\" noon-sundown | {:.0}% chance 8 AM-noon (daily max {:.0}%)",
        score.stars,
        score.score * 100.0,
        weather.precip_in,
        weather.precip_ride_in,
        weather.precip_pm_in,
        weather.precip_prob_ride_max,
        weather.precip_prob_max,
    );
    println!(
        "  rain by 3h: {}",
        format_three_hour(&weather.precip_3h_in, 2)
    );
    println!(
        "  cloud by 3h: {}",
        format_three_hour(&weather.cloud_3h_pct, 0)
    );
    for factor in &score.factors {
        println!("  {}: {}", factor.name, factor.note);
    }
    println!();
}

fn parse_date_range(value: &str) -> Result<(NaiveDate, NaiveDate), String> {
    let parse = |date: &str| {
        NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| format!("invalid date {date:?}; use YYYY-MM-DD"))
    };
    let (start, end) = match value.split_once(':') {
        Some((start, end)) => (parse(start)?, parse(end)?),
        None => {
            let date = parse(value)?;
            (date, date)
        }
    };
    if start > end {
        return Err(format!("date range starts after it ends: {value:?}"));
    }
    Ok((start, end))
}

fn format_three_hour(values: &[f64; 8], precision: usize) -> String {
    values
        .iter()
        .enumerate()
        .map(|(bucket, value)| format!("{:02}: {value:.precision$}", bucket * 3))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn print_help() {
    eprintln!("Usage: jaycast analyze [YYYY-MM-DD[:YYYY-MM-DD]] [gfs|ecmwf|both]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inclusive_date_ranges() {
        let (start, end) = parse_date_range("2026-07-08:2026-07-11").unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 7, 8).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 7, 11).unwrap());
    }
}
