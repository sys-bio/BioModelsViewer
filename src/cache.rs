//! The BioModels metadata index (sys-bio/biomodels_cache) and the SBML store
//! (sys-bio/BiomodelsStore), both read straight from GitHub.

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

/// Metadata for every curated model, one JSON object keyed by model id.
pub const INDEX_URL: &str =
    "https://raw.githubusercontent.com/sys-bio/biomodels_cache/main/cache/biomodels_cache.json";

/// The folder holding a model's SBML file. Each folder holds one file, whose
/// name varies: usually `<id>_url.xml`, but also `<id>.xml`, `Wang2016_1.xml`...
fn store_folder(id: &str) -> String {
    format!("https://raw.githubusercontent.com/sys-bio/BiomodelsStore/main/biomodels/{id}")
}

/// Where the published site is, for the files the deploy workflow writes
/// beside the page when they are not beside it (under `trunk serve`).
const PUBLISHED_SITE: &str = "https://sys-bio.github.io/BioModelsViewer";

/// A JSON file the deploy workflow publishes beside the page (made by the
/// `model_stats` tool): read from beside the page, else from the published
/// site. Under `trunk serve` the first answer is the page itself, which does
/// not parse, so the second is used.
async fn fetch_published<T: serde::de::DeserializeOwned>(name: &str) -> Option<T> {
    for url in [name.to_string(), format!("{PUBLISHED_SITE}/{name}")] {
        if let Ok(text) = fetch_text(&url).await
            && let Ok(value) = serde_json::from_str(&text)
        {
            return Some(value);
        }
    }
    None
}

/// Model id to SBML file name (the names vary).
pub type Manifest = HashMap<String, String>;

pub async fn fetch_manifest() -> Option<Manifest> {
    fetch_published("sbml_files.json").await
}

/// Every model's statistics, by model id, for filtering the search.
pub type StatsTable = HashMap<String, crate::stats::Summary>;

pub async fn fetch_model_stats() -> Option<StatsTable> {
    fetch_published("model_stats.json").await
}

/// Downloads a model's SBML, returning the text and the URL it came from.
/// The file is named by the manifest if it is there, else `<id>_url.xml` is
/// tried, and last the GitHub API is asked (unauthenticated, it allows 60
/// calls an hour, so it is the last resort).
pub async fn fetch_sbml(id: &str, manifest: Option<&Manifest>) -> Result<(String, String), String> {
    if let Some(file) = manifest.and_then(|m| m.get(id)) {
        let url = format!("{}/{file}", store_folder(id));
        return fetch_text(&url).await.map(|xml| (xml, url));
    }
    let guess = format!("{}/{id}_url.xml", store_folder(id));
    if let Ok(xml) = fetch_text(&guess).await {
        return Ok((xml, guess));
    }
    #[derive(Deserialize)]
    struct Listed {
        name: String,
        download_url: Option<String>,
    }
    let api = format!("https://api.github.com/repos/sys-bio/BiomodelsStore/contents/biomodels/{id}");
    let listing: Vec<Listed> = serde_json::from_str(&fetch_text(&api).await?)
        .map_err(|e| format!("The store's file list for {id} could not be read: {e}"))?;
    let url = listing
        .into_iter()
        .find(|f| f.name.ends_with(".xml"))
        .and_then(|f| f.download_url)
        .ok_or_else(|| format!("The store has no SBML file for {id}."))?;
    fetch_text(&url).await.map(|xml| (xml, url))
}

/// The model's page on the EBI BioModels site.
pub fn biomodels_page(id: &str) -> String {
    format!("https://www.ebi.ac.uk/biomodels/{id}")
}

/// One model's metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub title: String,
    pub authors: Vec<String>,
    pub journal: String,
    /// As written in the index, usually a year.
    pub date: String,
    pub year: Option<i32>,
    pub synopsis: String,
    pub citation: Option<String>,
    /// Usually an identifiers.org link to the publication.
    pub url: Option<String>,
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    journal: Option<String>,
    /// A string in all but one entry, which has a number.
    #[serde(default)]
    date: serde_json::Value,
    #[serde(default)]
    synopsis: Option<String>,
    #[serde(default)]
    citation: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

/// Parses the index, sorted by model id.
pub fn parse_index(text: &str) -> Result<Vec<ModelEntry>, String> {
    let raw: BTreeMap<String, RawEntry> =
        serde_json::from_str(text).map_err(|e| format!("The model index could not be read: {e}"))?;
    Ok(raw
        .into_iter()
        .map(|(key, r)| {
            let date = match r.date {
                serde_json::Value::String(s) => s,
                serde_json::Value::Number(n) => n.to_string(),
                _ => String::new(),
            };
            ModelEntry {
                id: r.model_id.filter(|s| !s.is_empty()).unwrap_or(key),
                name: r.name.unwrap_or_default(),
                title: r.title.unwrap_or_default(),
                authors: r.authors,
                journal: r.journal.unwrap_or_default(),
                year: first_year(&date),
                date,
                synopsis: r.synopsis.unwrap_or_default(),
                citation: r.citation.filter(|s| !s.is_empty()),
                url: r.url.filter(|s| !s.is_empty()),
            }
        })
        .collect())
}

/// The first run of four digits in a date, as a year.
fn first_year(date: &str) -> Option<i32> {
    let bytes = date.as_bytes();
    bytes
        .windows(4)
        .position(|w| w.iter().all(u8::is_ascii_digit))
        .and_then(|i| date[i..i + 4].parse().ok())
}

/// Downloads a text file.
pub async fn fetch_text(url: &str) -> Result<String, String> {
    let response = gloo_net::http::Request::get(url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;
    if !response.ok() {
        return Err(format!("Download failed: HTTP {} for {url}", response.status()));
    }
    response.text().await.map_err(|e| format!("Download failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_string_and_number_dates_and_null_citations() {
        let json = r#"{
            "BIOMD0000000002": {"model_id": "BIOMD0000000002", "name": "B", "authors": ["X Y"],
                "title": "t", "synopsis": "s", "citation": null, "date": 1997, "journal": "J", "url": "u"},
            "BIOMD0000000001": {"model_id": "BIOMD0000000001", "name": "A", "authors": [],
                "title": "t", "synopsis": "s", "citation": "c", "date": "1996-05", "journal": "J", "url": "u"}
        }"#;
        let entries = parse_index(json).unwrap();
        assert_eq!(entries[0].id, "BIOMD0000000001");
        assert_eq!(entries[0].year, Some(1996));
        assert_eq!(entries[1].date, "1997");
        assert_eq!(entries[1].citation, None);
    }
}
