use serde::Deserialize;

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
    pub subject: Vec<String>,
    #[serde(default)]
    pub publisher: Vec<String>,
    pub number_of_pages_median: Option<i64>,
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
    pub subjects: Vec<String>,
    pub publishers: Vec<String>,
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

    Some(OpenLibraryBookRecord {
        title: title.to_string(),
        edition_id,
        work_id,
        isbn13: first_isbn13(&doc.isbn),
        cover_ids: doc.cover_i.and_then(positive_cover_id).into_iter().collect(),
        cover_id: doc.cover_i.and_then(positive_cover_id),
        publish_year: doc.first_publish_year,
        description: None,
        pages: doc.number_of_pages_median.and_then(positive_u32),
        language: doc.language.first().cloned(),
        authors: doc.author_name.clone(),
        subjects: doc.subject.clone(),
        publishers: doc.publisher.clone(),
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
        subjects: vec![],
        publishers: response.publishers.clone(),
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
        subjects: response.subjects.clone(),
        publishers: vec![],
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
        cover_id: cover_ids.first().copied().or(edition.cover_id).or(work.cover_id),
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
    }
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
