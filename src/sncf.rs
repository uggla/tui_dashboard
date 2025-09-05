use crate::app::{AppConfig, JourneyRow, Place, PlacesResponse};
use jiff::{civil, Zoned};
use jiff::tz::TimeZone;

pub fn fetch_places(
    client: &reqwest::blocking::Client,
    api_key: &str,
    query: &str,
) -> Result<Vec<Place>, Box<dyn std::error::Error>> {
    let url = format!(
        "https://api.sncf.com/v1/coverage/sncf/places?q={}",
        urlencoding::encode(query)
    );
    let resp = client
        .get(url)
        .basic_auth(api_key, Some(""))
        .send()?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()).into());
    }
    let parsed: PlacesResponse = resp.json()?;
    let only_stop_areas = parsed
        .places
        .into_iter()
        .filter(|p| matches!(p.embedded_type.as_deref(), Some("stop_area")))
        .collect();
    Ok(only_stop_areas)
}

pub fn fetch_journeys(
    client: &reqwest::blocking::Client,
    api_key: &str,
    conf: &AppConfig,
) -> Result<Vec<JourneyRow>, Box<dyn std::error::Error>> {
    let url = build_journeys_url(&conf.start.id, &conf.destination.id);
    let resp = client.get(url).basic_auth(api_key, Some("")).send()?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()).into());
    }
    let parsed: JourneysResponse = resp.json()?;
    let rows = parsed
        .journeys
        .into_iter()
        .filter_map(|j| {
            let dep = parse_sncf_dt(&j.departure_date_time)?;
            let arr = parse_sncf_dt(&j.arrival_date_time)?;
            let dep_hm = super::ui::format_hm(&dep);
            let arr_hm = super::ui::format_hm(&arr);
            let date_str = super::ui::format_date(&dep);

            let dur = j.duration.unwrap_or_else(|| {
                let dep_sec = rfc3339z_to_epoch(&dep.timestamp().to_string()).unwrap_or(0);
                let arr_sec = rfc3339z_to_epoch(&arr.timestamp().to_string()).unwrap_or(dep_sec);
                (arr_sec - dep_sec).max(0)
            });

            Some(JourneyRow {
                dep,
                arr,
                dep_hm,
                arr_hm,
                date_str,
                duration_secs: dur,
                nb_transfers: j.nb_transfers.unwrap_or(0),
            })
        })
        .collect();
    Ok(rows)
}

#[derive(Debug, serde::Deserialize)]
struct JourneysResponse {
    #[serde(default)]
    journeys: Vec<JourneyItem>,
}

#[derive(Debug, serde::Deserialize)]
struct JourneyItem {
    #[serde(default)]
    departure_date_time: String,
    #[serde(default)]
    arrival_date_time: String,
    #[serde(default)]
    duration: Option<i64>,
    #[serde(default)]
    nb_transfers: Option<i64>,
}

fn build_journeys_url(from_id: &str, to_id: &str) -> String {
    let base = "https://api.sncf.com/v1/coverage/sncf/journeys";
    let from = urlencoding::encode(from_id);
    let to = urlencoding::encode(to_id);
    format!(
        "{base}?from={from}&to={to}&first_section_mode[]=walking&last_section_mode[]=walking&min_nb_transfers=0&direct_path=none&min_nb_journeys=25&is_journey_schedules=True"
    )
}

pub fn parse_sncf_dt(s: &str) -> Option<Zoned> {
    if s.len() < 15 { return None; }
    let y = s[0..4].parse().ok()?;
    let m = s[4..6].parse().ok()?;
    let d = s[6..8].parse().ok()?;
    let hh = s[9..11].parse().ok()?;
    let mm = s[11..13].parse().ok()?;
    let ss = s[13..15].parse().ok()?;
    let dt = civil::date(y, m, d).at(hh, mm, ss, 0);
    dt.to_zoned(TimeZone::system()).ok()
}

pub fn rfc3339z_to_epoch(s: &str) -> Option<i64> {
    if s.len() < 20 || !s.ends_with('Z') { return None; }
    let year: i64 = s[0..4].parse().ok()?;
    let month: i64 = s[5..7].parse().ok()?;
    let day: i64 = s[8..10].parse().ok()?;
    let hour: i64 = s[11..13].parse().ok()?;
    let minute: i64 = s[14..16].parse().ok()?;
    let second: i64 = s[17..19].parse().ok()?;
    fn is_leap(y: i64) -> bool { (y % 4 == 0) && ((y % 100 != 0) || (y % 400 == 0)) }
    fn days_before_year(y: i64) -> i64 { let y1=y-1; let leaps=y1/4 - y1/100 + y1/400; let base=1969; let leaps_base=base/4 - base/100 + base/400; (y1-1970+1)*365 + (leaps - leaps_base) }
    fn days_before_month(y: i64, m: i64) -> i64 { let md=[0,31,59,90,120,151,181,212,243,273,304,334]; let mut d=md[(m-1) as usize] as i64; if m>2 && is_leap(y){d+=1;} d }
    let days = days_before_year(year) + days_before_month(year, month) + (day - 1);
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

