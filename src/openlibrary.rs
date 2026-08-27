use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone)]
pub struct OpenLibrarySearchResponse {
    #[serde(default)]
    pub docs: Vec<OpenLibrarySearchDoc>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpenLibrarySearchDoc {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub edition_key: Vec<String>,
    #[serde(default)]
    pub isbn: Vec<String>,
    pub cover_i: Option<i64>,
    pub first_publish_year: Option<u16>,
    #[serde(default)]
    pub language: Vec<String>,
    #[serde(default)]
    pub author_name: Vec<String>,
    #[serde(default)]
    pub author_key: Vec<String>,
    #[serde(default)]
    pub subject: Vec<String>,
    #[serde(default)]
    pub publisher: Vec<String>,
    pub number_of_pages_median: Option<i64>,
    #[serde(default)]
    pub series_key: Vec<String>,
    #[serde(default)]
    pub series_name: Vec<String>,
    #[serde(default)]
    pub series_position: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpenLibraryWorkResponse {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub title: String,
    pub description: Option<OpenLibraryDescription>,
    #[serde(default)]
    pub covers: Vec<i64>,
    #[serde(default)]
    pub subjects: Vec<String>,
    pub first_publish_date: Option<String>,
    #[serde(default)]
    pub series: Vec<OpenLibraryWorkSeriesEdge>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpenLibraryWorkSeriesEdge {
    pub series: OpenLibrarySeriesRef,
    pub position: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpenLibrarySeriesRef {
    #[serde(default)]
    pub key: String,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpenLibrarySeriesResponse {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpenLibraryEditionResponse {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub title: String,
    pub description: Option<OpenLibraryDescription>,
    #[serde(default)]
    pub works: Vec<OpenLibraryKeyRef>,
    #[serde(default)]
    pub isbn_13: Vec<String>,
    #[serde(default)]
    pub covers: Vec<i64>,
    pub number_of_pages: Option<i64>,
    pub publish_date: Option<String>,
    #[serde(default)]
    pub languages: Vec<OpenLibraryKeyRef>,
    #[serde(default)]
    pub publishers: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpenLibraryWorkEditionsResponse {
    #[serde(default)]
    pub entries: Vec<OpenLibraryEditionResponse>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpenLibraryKeyRef {
    pub key: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum OpenLibraryDescription {
    Text(String),
    Value { value: Option<String> },
}

impl OpenLibraryDescription {
    pub fn as_text(&self) -> Option<String> {
        match self {
            OpenLibraryDescription::Text(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            OpenLibraryDescription::Value { value } => value.as_ref().and_then(|text| {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
        }
    }
}

#[derive(Debug, Serialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenLibrarySeriesRecord {
    pub id: Option<String>,
    pub name: Option<String>,
    pub position: Option<String>,
}

impl OpenLibrarySeriesRecord {
    pub fn numeric_position(&self) -> Option<f64> {
        self.position
            .as_deref()
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite())
    }
}

#[derive(Debug, Clone, Default)]
pub struct OpenLibraryBookRecord {
    pub title: String,
    pub edition_id: Option<String>,
    pub work_id: Option<String>,
    pub isbn13: Option<String>,
    pub cover_ids: Vec<u64>,
    pub cover_id: Option<u64>,
    pub publish_year: Option<u16>,
    pub description: Option<String>,
    pub pages: Option<u32>,
    pub language: Option<String>,
    pub authors: Vec<String>,
    pub author_keys: Vec<String>,
    pub subjects: Vec<String>,
    pub publishers: Vec<String>,
    pub series: Vec<OpenLibrarySeriesRecord>,
}

impl OpenLibraryBookRecord {
    pub fn dedup_key(&self) -> String {
        if let Some(work_id) = &self.work_id {
            return format!("work:{work_id}");
        }
        if let Some(edition_id) = &self.edition_id {
            return format!("edition:{edition_id}");
        }
        if let Some(isbn13) = &self.isbn13 {
            return format!("isbn13:{isbn13}");
        }
        format!("title:{}", self.title.to_ascii_lowercase())
    }

    pub fn primary_series(&self) -> Option<&OpenLibrarySeriesRecord> {
        self.series.iter().find(|series| {
            series
                .name
                .as_deref()
                .map(str::trim)
                .is_some_and(|name| !name.is_empty())
        })
    }
}

pub fn normalize_openlibrary_id(value: &str, prefix: &str) -> Option<String> {
    let trimmed = value.trim().trim_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    if !trimmed.contains('/') {
        return Some(trimmed.to_string());
    }

    let candidate = trimmed
        .strip_prefix(prefix)
        .or_else(|| trimmed.strip_prefix(&format!("{prefix}/")))
        .or_else(|| trimmed.rsplit('/').next())
        .unwrap_or(trimmed)
        .trim_matches('/');

    if candidate.is_empty() {
        None
    } else {
        Some(candidate.to_string())
    }
}

pub fn extract_year_from_text(value: &str) -> Option<u16> {
    let bytes = value.as_bytes();
    for idx in 0..bytes.len().saturating_sub(3) {
        let chunk = &bytes[idx..idx + 4];
        if chunk.iter().all(|b| b.is_ascii_digit()) {
            if let Ok(year) = std::str::from_utf8(chunk).ok()?.parse::<u16>() {
                if (1000..=2999).contains(&year) {
                    return Some(year);
                }
            }
        }
    }
    None
}

pub fn normalize_isbn13(value: &str) -> Option<String> {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 13 {
        Some(digits)
    } else {
        None
    }
}

pub fn first_isbn13(values: &[String]) -> Option<String> {
    values.iter().find_map(|value| normalize_isbn13(value))
}

pub fn language_from_key(value: &str) -> Option<String> {
    let last = value
        .trim()
        .trim_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    if last.is_empty() {
        None
    } else {
        Some(last.to_string())
    }
}

pub fn encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for b in value.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*b as char)
            }
            b' ' => encoded.push_str("%20"),
            _ => encoded.push_str(&format!("%{:02X}", b)),
        }
    }
    encoded
}

pub fn build_search_url(search: &str) -> String {
    format!(
        "https://openlibrary.org/search.json?q={query}&limit=25",
        query = encode_query_component(search)
    )
}

pub fn build_isbn_url(isbn13: &str) -> String {
    format!("https://openlibrary.org/isbn/{isbn13}.json")
}

pub fn build_edition_url(edition_id: &str) -> String {
    format!("https://openlibrary.org/books/{edition_id}.json")
}

pub fn build_work_url(work_id: &str) -> String {
    format!("https://openlibrary.org/works/{work_id}.json")
}

pub fn build_work_editions_url(work_id: &str) -> String {
    format!("https://openlibrary.org/works/{work_id}/editions.json?limit=1")
}

pub fn build_series_url(series_id: &str) -> String {
    format!("https://openlibrary.org/series/{series_id}.json")
}

pub fn build_cover_url_from_id(cover_id: u64) -> String {
    format!("https://covers.openlibrary.org/b/id/{cover_id}-L.jpg")
}

pub fn build_cover_url_from_olid(olid: &str) -> String {
    format!("https://covers.openlibrary.org/b/olid/{olid}-L.jpg")
}

pub fn book_record_from_search_doc(doc: &OpenLibrarySearchDoc) -> Option<OpenLibraryBookRecord> {
    let title = doc.title.trim();
    if title.is_empty() {
        return None;
    }

    let edition_id = doc
        .edition_key
        .first()
        .and_then(|value| normalize_openlibrary_id(value, "books"));

    let work_id = normalize_openlibrary_id(&doc.key, "works");
    let series = series_records_from_search_doc(doc);

    Some(OpenLibraryBookRecord {
        title: title.to_string(),
        edition_id,
        work_id,
        isbn13: first_isbn13(&doc.isbn),
        cover_ids: doc
            .cover_i
            .and_then(positive_cover_id)
            .into_iter()
            .collect(),
        cover_id: doc.cover_i.and_then(positive_cover_id),
        publish_year: doc.first_publish_year,
        description: None,
        pages: doc.number_of_pages_median.and_then(positive_u32),
        language: doc.language.first().cloned(),
        authors: doc.author_name.clone(),
        author_keys: doc.author_key.clone(),
        subjects: doc.subject.clone(),
        publishers: doc.publisher.clone(),
        series,
    })
}

pub fn book_record_from_edition_response(
    response: &OpenLibraryEditionResponse,
) -> OpenLibraryBookRecord {
    let description = response
        .description
        .as_ref()
        .and_then(OpenLibraryDescription::as_text);

    let publish_year = response
        .publish_date
        .as_deref()
        .and_then(extract_year_from_text);

    let cover_ids = extract_cover_ids(&response.covers);

    OpenLibraryBookRecord {
        title: response.title.trim().to_string(),
        edition_id: normalize_openlibrary_id(&response.key, "books"),
        work_id: response
            .works
            .first()
            .and_then(|work| normalize_openlibrary_id(&work.key, "works")),
        isbn13: first_isbn13(&response.isbn_13),
        cover_id: cover_ids.first().copied(),
        cover_ids,
        publish_year,
        description,
        pages: response.number_of_pages.and_then(positive_u32),
        language: response
            .languages
            .first()
            .and_then(|language| language_from_key(&language.key)),
        authors: vec![],
        author_keys: vec![],
        subjects: vec![],
        publishers: response.publishers.clone(),
        series: vec![],
    }
}

pub fn book_record_from_work_response(response: &OpenLibraryWorkResponse) -> OpenLibraryBookRecord {
    let cover_ids = extract_cover_ids(&response.covers);

    OpenLibraryBookRecord {
        title: response.title.trim().to_string(),
        edition_id: None,
        work_id: normalize_openlibrary_id(&response.key, "works"),
        isbn13: None,
        cover_id: cover_ids.first().copied(),
        cover_ids,
        publish_year: response
            .first_publish_date
            .as_deref()
            .and_then(extract_year_from_text),
        description: response
            .description
            .as_ref()
            .and_then(OpenLibraryDescription::as_text),
        pages: None,
        language: None,
        authors: vec![],
        author_keys: vec![],
        subjects: response.subjects.clone(),
        publishers: vec![],
        series: response
            .series
            .iter()
            .filter_map(|edge| {
                let id = normalize_openlibrary_id(&edge.series.key, "series");
                let name = non_empty_string(edge.series.name.as_deref());
                let position = non_empty_string(edge.position.as_deref());
                if id.is_none() && name.is_none() {
                    None
                } else {
                    Some(OpenLibrarySeriesRecord { id, name, position })
                }
            })
            .collect(),
    }
}

pub fn first_record_from_work_editions(
    response: &OpenLibraryWorkEditionsResponse,
) -> Option<OpenLibraryBookRecord> {
    response
        .entries
        .first()
        .map(book_record_from_edition_response)
}

pub fn merge_work_with_edition(
    work: OpenLibraryBookRecord,
    edition: Option<OpenLibraryBookRecord>,
) -> OpenLibraryBookRecord {
    let Some(edition) = edition else {
        return work;
    };

    let mut cover_ids = work.cover_ids.clone();
    for cover_id in edition.cover_ids.iter().copied() {
        if !cover_ids.contains(&cover_id) {
            cover_ids.push(cover_id);
        }
    }
    if cover_ids.is_empty() {
        cover_ids.extend(work.cover_id);
        cover_ids.extend(edition.cover_id);
    }

    OpenLibraryBookRecord {
        title: if work.title.is_empty() {
            edition.title
        } else {
            work.title
        },
        edition_id: edition.edition_id.or(work.edition_id),
        work_id: work.work_id.or(edition.work_id),
        isbn13: edition.isbn13.or(work.isbn13),
        cover_id: cover_ids
            .first()
            .copied()
            .or(edition.cover_id)
            .or(work.cover_id),
        cover_ids,
        publish_year: edition.publish_year.or(work.publish_year),
        description: work.description.or(edition.description),
        pages: edition.pages.or(work.pages),
        language: edition.language.or(work.language),
        authors: if work.authors.is_empty() {
            edition.authors
        } else {
            work.authors
        },
        author_keys: if work.author_keys.is_empty() {
            edition.author_keys
        } else {
            work.author_keys
        },
        subjects: if work.subjects.is_empty() {
            edition.subjects
        } else {
            work.subjects
        },
        publishers: if edition.publishers.is_empty() {
            work.publishers
        } else {
            edition.publishers
        },
        series: if work.series.is_empty() {
            edition.series
        } else {
            work.series
        },
    }
}

pub fn merge_work_series_into_edition(
    mut edition: OpenLibraryBookRecord,
    work: OpenLibraryBookRecord,
) -> OpenLibraryBookRecord {
    if !work.series.is_empty() {
        edition.series = work.series;
    }
    edition
}

fn non_empty_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn series_records_from_search_doc(doc: &OpenLibrarySearchDoc) -> Vec<OpenLibrarySeriesRecord> {
    let count = doc
        .series_key
        .len()
        .max(doc.series_name.len())
        .max(doc.series_position.len());
    let mut records = Vec::new();

    for index in 0..count {
        let id = doc
            .series_key
            .get(index)
            .and_then(|value| normalize_openlibrary_id(value, "series"));
        let name = doc
            .series_name
            .get(index)
            .and_then(|value| non_empty_string(Some(value)));
        let position = doc
            .series_position
            .get(index)
            .and_then(|value| non_empty_string(Some(value)));

        if id.is_some() || name.is_some() {
            records.push(OpenLibrarySeriesRecord { id, name, position });
        }
    }

    records
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_ids_from_paths() {
        assert_eq!(
            normalize_openlibrary_id("/works/OL45804W", "works"),
            Some("OL45804W".to_string())
        );
        assert_eq!(
            normalize_openlibrary_id("books/OL7353617M", "books"),
            Some("OL7353617M".to_string())
        );
    }

    #[test]
    fn encode_query_component_encodes_spaces() {
        assert_eq!(encode_query_component("The Hobbit"), "The%20Hobbit");
    }

    #[test]
    fn extract_year_from_publish_date() {
        assert_eq!(extract_year_from_text("September 21, 1937"), Some(1937));
    }

    #[test]
    fn first_isbn13_prefers_normalized_13_digit() {
        let values = vec!["978-0-14-032872-1".to_string(), "0140328726".to_string()];
        assert_eq!(first_isbn13(&values), Some("9780140328721".to_string()));
    }

    #[test]
    fn search_doc_maps_author_keys() {
        let doc = OpenLibrarySearchDoc {
            key: "/works/OL45804W".to_string(),
            title: "The Hobbit".to_string(),
            edition_key: vec!["OL7353617M".to_string()],
            isbn: vec!["9780140328721".to_string()],
            cover_i: None,
            first_publish_year: Some(1937),
            language: vec!["eng".to_string()],
            author_name: vec!["J.R.R. Tolkien".to_string()],
            author_key: vec!["OL26320A".to_string()],
            subject: vec!["Fantasy".to_string()],
            publisher: vec!["Allen & Unwin".to_string()],
            number_of_pages_median: None,
            series_key: vec![],
            series_name: vec![],
            series_position: vec![],
        };

        let record = book_record_from_search_doc(&doc).expect("Expected mapped record");
        assert_eq!(record.authors, vec!["J.R.R. Tolkien".to_string()]);
        assert_eq!(record.author_keys, vec!["OL26320A".to_string()]);
    }

    #[test]
    fn search_doc_maps_aligned_series_metadata() {
        let doc: OpenLibrarySearchDoc = serde_json::from_value(serde_json::json!({
            "key": "/works/OL27513W",
            "title": "The Fellowship of the Ring",
            "series_key": ["OL330052L"],
            "series_name": ["The Lord of the Rings"],
            "series_position": ["1"]
        }))
        .unwrap();

        let record = book_record_from_search_doc(&doc).expect("Expected mapped record");
        assert_eq!(
            record.series,
            vec![OpenLibrarySeriesRecord {
                id: Some("OL330052L".to_string()),
                name: Some("The Lord of the Rings".to_string()),
                position: Some("1".to_string()),
            }]
        );
        assert_eq!(
            record.primary_series().unwrap().numeric_position(),
            Some(1.0)
        );
    }

    #[test]
    fn search_doc_handles_mismatched_series_arrays() {
        let doc: OpenLibrarySearchDoc = serde_json::from_value(serde_json::json!({
            "key": "/works/OL27513W",
            "title": "The Fellowship of the Ring",
            "series_key": ["OL330052L", "OL999L"],
            "series_name": ["The Lord of the Rings"],
            "series_position": ["1", ""]
        }))
        .unwrap();

        let record = book_record_from_search_doc(&doc).expect("Expected mapped record");
        assert_eq!(record.series.len(), 2);
        assert_eq!(
            record.series[0].name.as_deref(),
            Some("The Lord of the Rings")
        );
        assert_eq!(record.series[1].id.as_deref(), Some("OL999L"));
        assert_eq!(record.series[1].name, None);
    }

    #[test]
    fn work_response_maps_nested_series_edge() {
        let response: OpenLibraryWorkResponse = serde_json::from_value(serde_json::json!({
            "key": "/works/OL27513W",
            "title": "The Fellowship of the Ring",
            "series": [{
                "series": { "key": "/series/OL330052L" },
                "position": "1"
            }]
        }))
        .unwrap();

        let record = book_record_from_work_response(&response);
        assert_eq!(
            record.series,
            vec![OpenLibrarySeriesRecord {
                id: Some("OL330052L".to_string()),
                name: None,
                position: Some("1".to_string()),
            }]
        );
    }

    #[test]
    fn series_positions_only_parse_plain_finite_numbers() {
        let numeric = OpenLibrarySeriesRecord {
            position: Some("2.5".to_string()),
            ..Default::default()
        };
        let range = OpenLibrarySeriesRecord {
            position: Some("1-3".to_string()),
            ..Default::default()
        };
        let infinite = OpenLibrarySeriesRecord {
            position: Some("inf".to_string()),
            ..Default::default()
        };

        assert_eq!(numeric.numeric_position(), Some(2.5));
        assert_eq!(range.numeric_position(), None);
        assert_eq!(infinite.numeric_position(), None);
    }

    #[test]
    fn edition_response_maps_all_positive_cover_ids() {
        let response = OpenLibraryEditionResponse {
            key: "/books/OL7353617M".to_string(),
            title: "The Hobbit".to_string(),
            description: None,
            works: vec![],
            isbn_13: vec![],
            covers: vec![12345, 0, -1, 67890, 12345],
            number_of_pages: None,
            publish_date: None,
            languages: vec![],
            publishers: vec![],
        };

        let record = book_record_from_edition_response(&response);
        assert_eq!(record.cover_ids, vec![12345, 67890]);
        assert_eq!(record.cover_id, Some(12345));
    }

    #[test]
    fn merge_work_with_edition_keeps_all_cover_ids() {
        let work = OpenLibraryBookRecord {
            title: "The Hobbit".to_string(),
            work_id: Some("OL45804W".to_string()),
            cover_ids: vec![2701529, 2701530, 6307679],
            cover_id: Some(2701529),
            series: vec![OpenLibrarySeriesRecord {
                id: Some("OL330052L".to_string()),
                name: Some("The Lord of the Rings".to_string()),
                position: Some("1".to_string()),
            }],
            ..Default::default()
        };

        let edition = OpenLibraryBookRecord {
            title: "The Hobbit".to_string(),
            edition_id: Some("OL7353617M".to_string()),
            cover_ids: vec![2701530, 9999999],
            cover_id: Some(2701530),
            ..Default::default()
        };

        let merged = merge_work_with_edition(work, Some(edition));
        assert_eq!(merged.cover_ids, vec![2701529, 2701530, 6307679, 9999999]);
        assert_eq!(merged.cover_id, Some(2701529));
        assert_eq!(merged.series.len(), 1);
        assert_eq!(merged.series[0].position.as_deref(), Some("1"));
    }

    #[test]
    fn series_enrichment_preserves_edition_metadata() {
        let edition = OpenLibraryBookRecord {
            title: "La Communauté de l'anneau".to_string(),
            description: Some("French edition description".to_string()),
            pages: Some(544),
            edition_id: Some("OL123M".to_string()),
            ..Default::default()
        };
        let work = OpenLibraryBookRecord {
            title: "The Fellowship of the Ring".to_string(),
            description: Some("Generic work description".to_string()),
            pages: Some(423),
            series: vec![OpenLibrarySeriesRecord {
                id: Some("OL330052L".to_string()),
                name: Some("The Lord of the Rings".to_string()),
                position: Some("1".to_string()),
            }],
            ..Default::default()
        };

        let enriched = merge_work_series_into_edition(edition, work);
        assert_eq!(enriched.title, "La Communauté de l'anneau");
        assert_eq!(
            enriched.description.as_deref(),
            Some("French edition description")
        );
        assert_eq!(enriched.pages, Some(544));
        assert_eq!(enriched.edition_id.as_deref(), Some("OL123M"));
        assert_eq!(enriched.series.len(), 1);
        assert_eq!(enriched.series[0].position.as_deref(), Some("1"));
    }
}
fn positive_cover_id(value: i64) -> Option<u64> {
    if value > 0 {
        Some(value as u64)
    } else {
        None
    }
}

fn extract_cover_ids(values: &[i64]) -> Vec<u64> {
    let mut cover_ids = Vec::new();
    for value in values {
        if let Some(cover_id) = positive_cover_id(*value) {
            if !cover_ids.contains(&cover_id) {
                cover_ids.push(cover_id);
            }
        }
    }
    cover_ids
}

fn positive_u32(value: i64) -> Option<u32> {
    if value > 0 && value <= u32::MAX as i64 {
        Some(value as u32)
    } else {
        None
    }
}
