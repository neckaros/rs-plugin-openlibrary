use extism_pdk::{http, log, plugin_fn, FnResult, HttpRequest, Json, LogLevel, WithReturnCode};
use std::collections::HashSet;

use rs_plugin_common_interfaces::{
    domain::{external_images::ExternalImage, person::PersonType, rs_ids::RsIds},
    lookup::{
        RsLookupBook, RsLookupMatchType, RsLookupMetadataResultWrapper, RsLookupMetadataResults,
        RsLookupQuery, RsLookupWrapper,
    },
    PluginInformation, PluginType,
};

mod convert;
mod openlibrary;

use convert::{openlibrary_book_to_images, openlibrary_book_to_result};
use openlibrary::{
    book_record_from_edition_response, book_record_from_search_doc, book_record_from_work_response,
    build_edition_url, build_isbn_url, build_search_url, build_series_url, build_work_editions_url,
    build_work_url, first_record_from_work_editions, merge_work_series_into_edition,
    merge_work_with_edition, normalize_isbn13, normalize_openlibrary_id, OpenLibraryBookRecord,
    OpenLibraryEditionResponse, OpenLibrarySearchResponse, OpenLibrarySeriesResponse,
    OpenLibraryWorkEditionsResponse, OpenLibraryWorkResponse,
};
use serde::de::DeserializeOwned;

#[plugin_fn]
pub fn infos() -> FnResult<Json<PluginInformation>> {
    Ok(Json(PluginInformation {
        name: "openlibrary_metadata".into(),
        capabilities: vec![PluginType::LookupMetadata],
        version: env!("CARGO_PKG_VERSION_MINOR").parse()?,
        interface_version: 1,
        repo: Some("https://github.com/neckaros/rs-plugin-openlibrary".into()),
        publisher: "neckaros".into(),
        description: "Look up book metadata from OpenLibrary".into(),
        credential_kind: None,
        settings: vec![],
        ..Default::default()
    }))
}

#[derive(Debug, Default)]
struct BookIds {
    isbn13: Option<String>,
    edition_id: Option<String>,
    work_id: Option<String>,
}

fn extract_book_ids(query: &RsLookupQuery) -> Option<BookIds> {
    match query {
        RsLookupQuery::Book(book) => {
            let ids = book.ids.as_ref();
            Some(BookIds {
                isbn13: ids
                    .and_then(|ids| ids.isbn13())
                    .and_then(|value| normalize_isbn13(value)),
                edition_id: ids
                    .and_then(|ids| ids.openlibrary_edition_id())
                    .and_then(|value| normalize_openlibrary_id(value, "books")),
                work_id: ids
                    .and_then(|ids| ids.openlibrary_work_id())
                    .and_then(|value| normalize_openlibrary_id(value, "works")),
            })
        }
        _ => None,
    }
}

fn build_http_request(url: String) -> HttpRequest {
    let mut request = HttpRequest {
        url,
        headers: Default::default(),
        method: Some("GET".into()),
    };

    request
        .headers
        .insert("Accept".to_string(), "application/json".to_string());

    request
}

fn execute_get<T: DeserializeOwned>(url: String) -> FnResult<T> {
    let request = build_http_request(url);
    let res = http::request::<Vec<u8>>(&request, None);

    match res {
        Ok(res) if res.status_code() >= 200 && res.status_code() < 300 => match res.json::<T>() {
            Ok(parsed) => Ok(parsed),
            Err(e) => {
                log!(LogLevel::Error, "OpenLibrary JSON parse error: {}", e);
                Err(WithReturnCode::new(e, 500))
            }
        },
        Ok(res) => {
            log!(
                LogLevel::Error,
                "OpenLibrary HTTP error {}: {}",
                res.status_code(),
                String::from_utf8_lossy(&res.body())
            );
            Err(WithReturnCode::new(
                extism_pdk::Error::msg(format!("HTTP error: {}", res.status_code())),
                res.status_code() as i32,
            ))
        }
        Err(e) => {
            log!(LogLevel::Error, "OpenLibrary request failed: {}", e);
            Err(WithReturnCode(e, 500))
        }
    }
}

fn fetch_by_isbn(isbn13: &str, include_series: bool) -> FnResult<Vec<OpenLibraryBookRecord>> {
    let edition: OpenLibraryEditionResponse = execute_get(build_isbn_url(isbn13))?;
    Ok(vec![book_record_with_optional_work(
        edition,
        include_series,
    )])
}

fn fetch_by_edition(
    edition_id: &str,
    include_series: bool,
) -> FnResult<Vec<OpenLibraryBookRecord>> {
    let edition: OpenLibraryEditionResponse = execute_get(build_edition_url(edition_id))?;
    Ok(vec![book_record_with_optional_work(
        edition,
        include_series,
    )])
}

fn fetch_by_work(work_id: &str, include_series: bool) -> FnResult<Vec<OpenLibraryBookRecord>> {
    let work: OpenLibraryWorkResponse = execute_get(build_work_url(work_id))?;
    let editions: OpenLibraryWorkEditionsResponse = execute_get(build_work_editions_url(work_id))?;
    let mut work_record = book_record_from_work_response(&work);
    if include_series {
        enrich_series_names(&mut work_record);
    } else {
        work_record.series.clear();
    }
    let merged = merge_work_with_edition(work_record, first_record_from_work_editions(&editions));
    Ok(vec![merged])
}

fn book_record_with_optional_work(
    edition: OpenLibraryEditionResponse,
    include_series: bool,
) -> OpenLibraryBookRecord {
    let edition_record = book_record_from_edition_response(&edition);
    if !include_series {
        return edition_record;
    }
    let Some(work_id) = edition_record.work_id.clone() else {
        return edition_record;
    };

    let work = match execute_get::<OpenLibraryWorkResponse>(build_work_url(&work_id)) {
        Ok(work) => work,
        Err(error) => {
            log!(
                LogLevel::Warn,
                "Unable to enrich OpenLibrary edition with work {}: {:?}",
                work_id,
                error
            );
            return edition_record;
        }
    };

    let mut work_record = book_record_from_work_response(&work);
    enrich_series_names(&mut work_record);
    merge_work_series_into_edition(edition_record, work_record)
}

fn enrich_series_names(record: &mut OpenLibraryBookRecord) {
    for series in &mut record.series {
        if series.name.is_some() {
            continue;
        }
        let Some(series_id) = series.id.as_deref() else {
            continue;
        };

        match execute_get::<OpenLibrarySeriesResponse>(build_series_url(series_id)) {
            Ok(response) => {
                let name = response.name.trim();
                if !name.is_empty() {
                    series.name = Some(name.to_string());
                }
            }
            Err(error) => log!(
                LogLevel::Warn,
                "Unable to enrich OpenLibrary series {}: {:?}",
                series_id,
                error
            ),
        }
    }
}

fn fetch_by_search(search: &str) -> FnResult<Vec<OpenLibraryBookRecord>> {
    let response: OpenLibrarySearchResponse = execute_get(build_search_url(search))?;
    Ok(response
        .docs
        .iter()
        .filter_map(book_record_from_search_doc)
        .collect())
}

fn normalize_exact_isbn_search(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let compact: String = trimmed
        .chars()
        .filter(|c| *c != '-' && !c.is_ascii_whitespace())
        .collect();

    if compact.len() == 13 && compact.chars().all(|c| c.is_ascii_digit()) {
        return Some(compact);
    }

    if compact.len() != 10 {
        return None;
    }

    let mut chars = compact.chars();
    let last = chars.next_back()?;
    let body = chars.as_str();
    if body.chars().all(|c| c.is_ascii_digit())
        && (last.is_ascii_digit() || last == 'X' || last == 'x')
    {
        return Some(format!("{body}{}", last.to_ascii_uppercase()));
    }

    None
}

fn deduplicate_records(records: Vec<OpenLibraryBookRecord>) -> Vec<OpenLibraryBookRecord> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();

    for record in records {
        if seen.insert(record.dedup_key()) {
            deduped.push(record);
        }
    }

    deduped
}

fn deduplicate_images(images: Vec<ExternalImage>) -> Vec<ExternalImage> {
    let mut seen_urls = HashSet::new();
    let mut deduped = Vec::new();

    for image in images {
        if seen_urls.insert(image.url.url.clone()) {
            deduped.push(image);
        }
    }

    deduped
}

fn build_book_search_query(book: &RsLookupBook) -> Option<String> {
    let mut clauses = Vec::new();

    if let Some(name) = non_empty(book.name.as_deref()) {
        if let Some(isbn) = normalize_exact_isbn_search(name) {
            clauses.push(field_clause("isbn", &isbn));
        } else {
            clauses.push(field_clause("title", name));
        }
    }
    if let Some(author) = non_empty(book.author.as_deref()) {
        clauses.push(field_clause("author", author));
    }
    if let Some(ids) = book.ids.as_ref() {
        if let Some(isbn) = ids.isbn13().and_then(normalize_isbn13) {
            clauses.push(field_clause("isbn", &isbn));
        }
        if let Some(edition_id) = ids
            .openlibrary_edition_id()
            .and_then(|value| normalize_openlibrary_id(value, "books"))
        {
            clauses.push(field_clause("edition_key", &edition_id));
        }
        if let Some(work_id) = ids
            .openlibrary_work_id()
            .and_then(|value| normalize_openlibrary_id(value, "works"))
        {
            clauses.push(field_clause("key", &work_id));
        }
    }

    for person in book.people.as_deref().unwrap_or_default() {
        if person
            .role
            .as_ref()
            .is_some_and(|role| role != &PersonType::Author)
        {
            return None;
        }

        let clause = person
            .ids
            .as_ref()
            .and_then(|ids| find_olid(ids, 'A'))
            .map(|id| field_clause("author_key", &id))
            .or_else(|| {
                non_empty(person.name.as_deref()).map(|name| field_clause("author", name))
            })?;
        clauses.push(clause);
    }

    for series in book.series.as_deref().unwrap_or_default() {
        let clause = series
            .ids
            .as_ref()
            .and_then(|ids| find_olid(ids, 'L'))
            .map(|id| field_clause("series_key", &id))
            .or_else(|| {
                non_empty(series.name.as_deref()).map(|name| field_clause("series_name", name))
            })?;
        clauses.push(clause);
    }

    for tag in book.tags.as_deref().unwrap_or_default() {
        let clause = tag
            .ids
            .as_ref()
            .and_then(openlibrary_subject_key)
            .map(|key| field_clause("subject_key", &key))
            .or_else(|| non_empty(tag.name.as_deref()).map(|name| field_clause("subject", name)))?;
        clauses.push(clause);
    }

    (!clauses.is_empty()).then(|| clauses.join(" AND "))
}

fn book_matches_filters(book: &RsLookupBook, record: &OpenLibraryBookRecord) -> bool {
    for person in book.people.as_deref().unwrap_or_default() {
        if person
            .role
            .as_ref()
            .is_some_and(|role| role != &PersonType::Author)
        {
            return false;
        }
        let matches_name = person.name.as_deref().is_some_and(|name| {
            record
                .authors
                .iter()
                .any(|author| normalized(author) == normalized(name))
        });
        let matches_id = person
            .ids
            .as_ref()
            .and_then(|ids| find_olid(ids, 'A'))
            .is_some_and(|id| {
                record
                    .author_keys
                    .iter()
                    .any(|author_id| author_id.eq_ignore_ascii_case(&id))
            });
        if !matches_name && !matches_id {
            return false;
        }
    }

    for series in book.series.as_deref().unwrap_or_default() {
        let matches_name = series.name.as_deref().is_some_and(|name| {
            record.series.iter().any(|series| {
                series
                    .name
                    .as_deref()
                    .is_some_and(|candidate| normalized(candidate) == normalized(name))
            })
        });
        let matches_id = series
            .ids
            .as_ref()
            .and_then(|ids| find_olid(ids, 'L'))
            .is_some_and(|id| {
                record.series.iter().any(|series| {
                    series
                        .id
                        .as_deref()
                        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(&id))
                })
            });
        if !matches_name && !matches_id {
            return false;
        }
    }

    for tag in book.tags.as_deref().unwrap_or_default() {
        let matches_name = tag.name.as_deref().is_some_and(|name| {
            record
                .subjects
                .iter()
                .any(|subject| normalized(subject) == normalized(name))
        });
        let matches_id = tag
            .ids
            .as_ref()
            .and_then(openlibrary_subject_key)
            .is_some_and(|key| {
                record
                    .subjects
                    .iter()
                    .any(|subject| subject_key(subject) == key)
            });
        if !matches_name && !matches_id {
            return false;
        }
    }

    true
}

fn has_relation_filters(book: &RsLookupBook) -> bool {
    book.people
        .as_ref()
        .is_some_and(|values| !values.is_empty())
        || book
            .series
            .as_ref()
            .is_some_and(|values| !values.is_empty())
        || book.tags.as_ref().is_some_and(|values| !values.is_empty())
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn field_clause(field: &str, value: &str) -> String {
    format!("{field}:\"{}\"", escape_query_phrase(value))
}

fn escape_query_phrase(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn find_olid(ids: &RsIds, suffix: char) -> Option<String> {
    ids.iter()
        .find_map(|(_, value)| extract_olid(value, suffix))
}

fn extract_olid(value: &str, suffix: char) -> Option<String> {
    let upper = value.to_ascii_uppercase();
    for (start, _) in upper.match_indices("OL") {
        let candidate = &upper[start..];
        let digits = candidate[2..]
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .count();
        if digits > 0 && candidate.chars().nth(2 + digits) == Some(suffix) {
            return Some(candidate[..3 + digits].to_string());
        }
    }
    None
}

fn openlibrary_subject_key(ids: &RsIds) -> Option<String> {
    ids.iter().find_map(|(source, value)| {
        matches!(
            source.as_str(),
            "openlib-tag" | "openlibrary-subject" | "subject" | "subject-key"
        )
        .then(|| subject_key(value))
        .filter(|value| !value.is_empty())
    })
}

fn normalized(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn subject_key(value: &str) -> String {
    value
        .trim()
        .trim_matches('/')
        .strip_prefix("subjects/")
        .unwrap_or(value.trim().trim_matches('/'))
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '(' | ')') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn lookup_book_records(
    lookup: &RsLookupWrapper,
) -> FnResult<(Vec<OpenLibraryBookRecord>, Option<RsLookupMatchType>)> {
    let Some(mut ids) = extract_book_ids(&lookup.query) else {
        return Ok((vec![], None));
    };

    if ids.isbn13.is_none() {
        if let RsLookupQuery::Book(book) = &lookup.query {
            if let Some(name) = book.name.as_deref() {
                ids.isbn13 = normalize_exact_isbn_search(name);
            }
        }
    }

    let validate_exact_match_with_search = match &lookup.query {
        RsLookupQuery::Book(book) => {
            has_relation_filters(book)
                && (ids.isbn13.is_some() || ids.edition_id.is_some() || ids.work_id.is_some())
        }
        _ => false,
    };

    let (records, match_type) = if validate_exact_match_with_search {
        let search = match &lookup.query {
            RsLookupQuery::Book(book) => build_book_search_query(book),
            _ => None,
        }
        .ok_or_else(|| WithReturnCode::new(extism_pdk::Error::msg("Not supported"), 404))?;
        (fetch_by_search(&search)?, Some(RsLookupMatchType::ExactId))
    } else if let Some(isbn13) = ids.isbn13 {
        (
            fetch_by_isbn(&isbn13, true)?,
            Some(RsLookupMatchType::ExactId),
        )
    } else if let Some(edition_id) = ids.edition_id {
        (
            fetch_by_edition(&edition_id, true)?,
            Some(RsLookupMatchType::ExactId),
        )
    } else if let Some(work_id) = ids.work_id {
        (
            fetch_by_work(&work_id, true)?,
            Some(RsLookupMatchType::ExactId),
        )
    } else {
        let search = match &lookup.query {
            RsLookupQuery::Book(book) => build_book_search_query(book),
            _ => None,
        };

        match search {
            Some(search) => (
                fetch_by_search(&search)?,
                Some(RsLookupMatchType::ExactText),
            ),
            _ => {
                return Err(WithReturnCode::new(
                    extism_pdk::Error::msg("Not supported"),
                    404,
                ));
            }
        }
    };

    let records = match &lookup.query {
        RsLookupQuery::Book(book) => records
            .into_iter()
            .filter(|record| book_matches_filters(book, record))
            .collect(),
        _ => records,
    };

    Ok((deduplicate_records(records), match_type))
}

fn lookup_book_records_for_images(
    lookup: &RsLookupWrapper,
) -> FnResult<(Vec<OpenLibraryBookRecord>, Option<RsLookupMatchType>)> {
    let Some(mut ids) = extract_book_ids(&lookup.query) else {
        return Ok((vec![], None));
    };

    if ids.isbn13.is_none() {
        if let RsLookupQuery::Book(book) = &lookup.query {
            if let Some(name) = book.name.as_deref() {
                ids.isbn13 = normalize_exact_isbn_search(name);
            }
        }
    }

    let has_filters = match &lookup.query {
        RsLookupQuery::Book(book) => has_relation_filters(book),
        _ => false,
    };

    if (ids.isbn13.is_some() || ids.edition_id.is_some() || ids.work_id.is_some()) && !has_filters {
        let mut records = Vec::new();

        if let Some(isbn13) = ids.isbn13.as_deref() {
            records.extend(fetch_by_isbn(isbn13, false)?);
        }
        if let Some(edition_id) = ids.edition_id.as_deref() {
            records.extend(fetch_by_edition(edition_id, false)?);
        }
        if let Some(work_id) = ids.work_id.as_deref() {
            records.extend(fetch_by_work(work_id, false)?);
        }

        if let RsLookupQuery::Book(book) = &lookup.query {
            records.retain(|record| book_matches_filters(book, record));
        }

        return Ok((records, Some(RsLookupMatchType::ExactId)));
    }

    lookup_book_records(lookup)
}

#[plugin_fn]
pub fn lookup_metadata(
    Json(lookup): Json<RsLookupWrapper>,
) -> FnResult<Json<RsLookupMetadataResults>> {
    let (all_books, match_type) = lookup_book_records(&lookup)?;

    let results: Vec<RsLookupMetadataResultWrapper> = all_books
        .into_iter()
        .map(|book| openlibrary_book_to_result(book, match_type.clone()))
        .collect();

    Ok(Json(RsLookupMetadataResults {
        results,
        next_page_key: None,
    }))
}

#[plugin_fn]
pub fn lookup_metadata_images(
    Json(lookup): Json<RsLookupWrapper>,
) -> FnResult<Json<Vec<ExternalImage>>> {
    let (all_books, match_type) = lookup_book_records_for_images(&lookup)?;

    let mut images: Vec<ExternalImage> = all_books
        .into_iter()
        .flat_map(|book| openlibrary_book_to_images(&book))
        .collect();

    for img in &mut images {
        img.match_type = match_type.clone();
    }

    Ok(Json(deduplicate_images(images)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rs_plugin_common_interfaces::lookup::{
        RsLookupPersonFilter, RsLookupSerieFilter, RsLookupTagFilter,
    };

    #[test]
    fn book_query_extracts_ids() {
        let query = RsLookupQuery::Book(RsLookupBook {
            name: None,
            ids: Some({
                let mut ids = RsIds::default();
                ids.set("isbn13", "9780140328721");
                ids.set("openlibrary_edition_id", "/books/OL7353617M");
                ids.set("openlibrary_work_id", "works/OL45804W");
                ids
            }),
            ..Default::default()
        });

        let ids = extract_book_ids(&query).expect("Expected ids");
        assert_eq!(ids.isbn13, Some("9780140328721".to_string()));
        assert_eq!(ids.edition_id, Some("OL7353617M".to_string()));
        assert_eq!(ids.work_id, Some("OL45804W".to_string()));
    }

    #[test]
    fn normalize_exact_isbn_search_accepts_isbn13() {
        assert_eq!(
            normalize_exact_isbn_search("978-0-14-032872-1"),
            Some("9780140328721".to_string())
        );
    }

    #[test]
    fn normalize_exact_isbn_search_accepts_isbn10_with_x() {
        assert_eq!(
            normalize_exact_isbn_search("0-684-84328-5"),
            Some("0684843285".to_string())
        );
        assert_eq!(
            normalize_exact_isbn_search("0-8044-2957-x"),
            Some("080442957X".to_string())
        );
    }

    #[test]
    fn normalize_exact_isbn_search_rejects_non_exact_values() {
        assert_eq!(
            normalize_exact_isbn_search("The Hobbit 9780140328721"),
            None
        );
        assert_eq!(normalize_exact_isbn_search("isbn 9780140328721"), None);
        assert_eq!(normalize_exact_isbn_search(""), None);
    }

    #[test]
    fn deduplicate_images_by_url() {
        let images = vec![
            ExternalImage {
                url: rs_plugin_common_interfaces::RsRequest {
                    url: "https://covers.openlibrary.org/b/id/1-L.jpg".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
            ExternalImage {
                url: rs_plugin_common_interfaces::RsRequest {
                    url: "https://covers.openlibrary.org/b/id/1-L.jpg".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
            ExternalImage {
                url: rs_plugin_common_interfaces::RsRequest {
                    url: "https://covers.openlibrary.org/b/id/2-L.jpg".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
        ];

        let deduped = deduplicate_images(images);
        assert_eq!(deduped.len(), 2);
        assert_eq!(
            deduped[0].url.url,
            "https://covers.openlibrary.org/b/id/1-L.jpg"
        );
        assert_eq!(
            deduped[1].url.url,
            "https://covers.openlibrary.org/b/id/2-L.jpg"
        );
    }

    #[test]
    fn book_search_query_combines_relation_names_and_optional_roles() {
        let book = RsLookupBook {
            name: Some("The Left Hand of Darkness".to_string()),
            people: Some(vec![RsLookupPersonFilter {
                name: Some("Ursula K. Le Guin".to_string()),
                role: None,
                ..Default::default()
            }]),
            series: Some(vec![RsLookupSerieFilter {
                name: Some("Hainish Cycle".to_string()),
                ..Default::default()
            }]),
            tags: Some(vec![RsLookupTagFilter {
                name: Some("Science Fiction".to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        };

        assert_eq!(
            build_book_search_query(&book).as_deref(),
            Some(
                "title:\"The Left Hand of Darkness\" AND author:\"Ursula K. Le Guin\" AND series_name:\"Hainish Cycle\" AND subject:\"Science Fiction\""
            )
        );
    }

    #[test]
    fn book_search_query_uses_openlibrary_relation_ids() {
        let mut person_ids = RsIds::default();
        person_ids.set("openlib-person", "j-r-r-tolkien-ol26320a");
        let mut series_ids = RsIds::default();
        series_ids.set("openlib-series", "ol330052l");
        let mut tag_ids = RsIds::default();
        tag_ids.set("openlib-tag", "science-fiction");
        let book = RsLookupBook {
            people: Some(vec![RsLookupPersonFilter {
                ids: Some(person_ids),
                role: Some(PersonType::Author),
                ..Default::default()
            }]),
            series: Some(vec![RsLookupSerieFilter {
                ids: Some(series_ids),
                ..Default::default()
            }]),
            tags: Some(vec![RsLookupTagFilter {
                ids: Some(tag_ids),
                ..Default::default()
            }]),
            ..Default::default()
        };

        assert_eq!(
            build_book_search_query(&book).as_deref(),
            Some(
                "author_key:\"OL26320A\" AND series_key:\"OL330052L\" AND subject_key:\"science_fiction\""
            )
        );
    }

    #[test]
    fn book_search_query_rejects_non_author_roles() {
        let book = RsLookupBook {
            people: Some(vec![RsLookupPersonFilter {
                name: Some("Someone".to_string()),
                role: Some(PersonType::Custom("Editor".to_string())),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert_eq!(build_book_search_query(&book), None);
    }

    #[test]
    fn direct_id_results_are_checked_against_relation_filters() {
        let mut person_ids = RsIds::default();
        person_ids.set("openlib-person", "tolkien-OL26320A");
        let mut tag_ids = RsIds::default();
        tag_ids.set("openlib-tag", "fantasy");
        let query = RsLookupBook {
            people: Some(vec![RsLookupPersonFilter {
                ids: Some(person_ids),
                ..Default::default()
            }]),
            tags: Some(vec![RsLookupTagFilter {
                ids: Some(tag_ids),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let record = OpenLibraryBookRecord {
            authors: vec!["J.R.R. Tolkien".to_string()],
            author_keys: vec!["OL26320A".to_string()],
            subjects: vec!["Fantasy".to_string()],
            ..Default::default()
        };

        assert!(book_matches_filters(&query, &record));
    }
}
