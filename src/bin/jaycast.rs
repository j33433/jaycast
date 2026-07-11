use std::{env, process};

use chrono::{Duration, Local, NaiveDate};
use jaycast::{
    score::{score_days, DayForecast, Params},
    weather::{build_date_range_url, DayWeather, ForecastResponse, WeatherModel},
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

    for model in models {
        let url = build_date_range_url(model, fetch_start, end);
        let response: ForecastResponse = ureq::get(&url)
            .call()
            .map_err(|error| format!("{} request failed: {error}", model.short()))?
            .into_json()
            .map_err(|error| format!("{} response could not be parsed: {error}", model.short()))?;
        let days = response.days();
        let scores = score_days(&days, today, &Params::default());
        println!(
            "{} | grid {:.3}, {:.3}",
            model.label(),
            response.latitude,
            response.longitude
        );

        let mut date = start;
        loop {
            let weather = days
                .iter()
                .find(|day| day.date == date)
                .ok_or_else(|| format!("{} returned no data for {date}", model.short()))?;
            let score = scores
                .iter()
                .find(|day| day.date == date)
                .ok_or_else(|| format!("{} could not score {date}", model.short()))?;
            print_analysis(date, weather, score);
            if date == end {
                break;
            }
            date += Duration::days(1);
        }
    }
    Ok(())
}

fn print_analysis(date: NaiveDate, weather: &DayWeather, score: &DayForecast) {
    println!("  {date}");
    println!(
        "  score {:.1} stars ({:.0}%) | rain {:.2}\" total, {:.2}\" 8 AM-noon, {:.2}\" afternoon",
        score.stars,
        score.score * 100.0,
        weather.precip_in,
        weather.precip_ride_in,
        weather.precip_pm_in,
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
