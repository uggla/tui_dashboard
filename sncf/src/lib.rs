pub mod client;
mod fake;

use jiff::tz::TimeZone;
use jiff::{Unit, Zoned, civil};
use thiserror::Error;

use crate::client::HTTPClient;

#[derive(Error, Debug)]
pub enum SncfAPIError {
    #[error("HTTP request failed: {0}")]
    HttpRequest(#[from] reqwest::Error),
    #[error("Deserialization failed: {0}")]
    Deserialization(#[from] serde_json::Error),
}
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Place {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub embedded_type: Option<String>,
}
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PlacesResponse {
    pub places: Vec<Place>,
}

#[derive(Debug, Clone)]
pub struct JourneyRow {
    pub dep: Zoned,
    pub arr: Zoned,
    pub dep_hm: String,
    pub arr_hm: String,
    pub date_str: String,
    pub duration_secs: i64,
    pub nb_transfers: i64,
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

pub async fn fetch_places(
    client: &impl HTTPClient,
    api_key: &str,
    query: &str,
) -> Result<Vec<Place>, SncfAPIError> {
    let url = format!(
        "https://api.sncf.com/v1/coverage/sncf/places?q={}",
        urlencoding::encode(query)
    );
    let parsed: PlacesResponse = client.get(&url, api_key, Some("")).await?;
    Ok(parsed
        .places
        .into_iter()
        .filter(|p| matches!(p.embedded_type.as_deref(), Some("stop_area")))
        .collect())
}

pub async fn fetch_journeys(
    client: &impl HTTPClient,
    api_key: &str,
    from_id: &str,
    to_id: &str,
) -> Result<Vec<JourneyRow>, Box<dyn std::error::Error>> {
    let url = build_journeys_url(from_id, to_id);
    let parsed: JourneysResponse = client.get(&url, api_key, Some("")).await?;
    let rows = parsed
        .journeys
        .into_iter()
        .filter_map(|j| {
            let dep = parse_sncf_dt(&j.departure_date_time)?;
            let arr = parse_sncf_dt(&j.arrival_date_time)?;
            let dep_hm = format_hm(&dep);
            let arr_hm = format_hm(&arr);
            let date_str = format_date(&dep);
            let dur = &arr - &dep;
            Some(JourneyRow {
                dep,
                arr,
                dep_hm,
                arr_hm,
                date_str,
                duration_secs: dur.total(Unit::Second).unwrap() as i64,
                nb_transfers: j.nb_transfers.unwrap_or(0),
            })
        })
        .collect();
    Ok(rows)
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
    if s.len() < 15 {
        return None;
    }
    let y = s[0..4].parse().ok()?;
    let m = s[4..6].parse().ok()?;
    let d = s[6..8].parse().ok()?;
    let hh = s[9..11].parse().ok()?;
    let mm = s[11..13].parse().ok()?;
    let ss = s[13..15].parse().ok()?;
    let dt = civil::date(y, m, d).at(hh, mm, ss, 0);
    dt.to_zoned(TimeZone::system()).ok()
}

pub fn format_hm(z: &Zoned) -> String {
    let s = z.to_string();
    if s.len() >= 16 {
        s[11..16].to_string()
    } else {
        s
    }
}
pub fn format_date(z: &Zoned) -> String {
    let s = z.to_string();
    if s.len() >= 10 {
        s[0..10].to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use crate::client::FakeClient;

    use super::*;

    #[tokio::test]
    async fn fetch_places_filters_stop_area() {
        let client = FakeClient::new();
        let results = fetch_places(&client, "key", "Grenoble").await.unwrap();

        assert_eq!(results.len(), 8, "only stop_area should remain");
        assert_eq!(results[0].id, "stop_area:SNCF:87747006");
        assert_eq!(results[7].id, "stop_area:SNCF:87335521");
    }
}
