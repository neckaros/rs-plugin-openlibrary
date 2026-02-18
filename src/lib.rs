use extism_pdk::{http, log, plugin_fn, FnResult, HttpRequest, Json, LogLevel, WithReturnCode};
use std::collections::HashSet;

use rs_plugin_common_interfaces::{
    domain::external_images::ExternalImage,
    lookup::{RsLookupMetadataResultWithImages, RsLookupQuery, RsLookupWrapper},
    PluginInformation, PluginType,
};

mod convert;
mod openlibrary;

use convert::{openlibrary_book_to_images, openlibrary_book_to_result};
use openlibrary::{
    book_record_from_edition_response, book_record_from_search_doc, book_record_from_work_response,
    build_edition_url, build_isbn_url, build_search_url, build_work_editions_url, build_work_url,
    first_record_from_work_editions, merge_work_with_edition, normalize_isbn13,
    normalize_openlibrary_id, OpenLibraryBookRecord, OpenLibraryEditionResponse,
    OpenLibrarySearchResponse, OpenLibraryWorkEditionsResponse, OpenLibraryWorkResponse,
};
use serde::de::DeserializeOwned;

#[plugin_fn]
pub fn infos() -> FnResult<Json<PluginInformation>> {
    Ok(Json(PluginInformation {
        name: "openlibrary_metadata".into(),
        capabilities: vec![PluginType::LookupMetadata],
        version: 3,
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
                    .and_then(|ids| ids.isbn13.as_ref())
                    .and_then(|value| normalize_isbn13(value)),
                edition_id: ids
                    .and_then(|ids| ids.openlibrary_edition_id.as_ref())
                    .and_then(|value| normalize_openlibrary_id(value, "books")),
                work_id: ids
                    .and_then(|ids| ids.openlibrary_work_id.as_ref())
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

fn fetch_by_isbn(isbn13: &str) -> FnResult<Vec<OpenLibraryBookRecord>> {
    let edition: OpenLibraryEditionResponse = execute_get(build_isbn_url(isbn13))?;
    Ok(vec![book_record_from_edition_response(&edition)])
}

fn fetch_by_edition(edition_id: &str) -> FnResult<Vec<OpenLibraryBookRecord>> {
    let edition: OpenLibraryEditionResponse = execute_get(build_edition_url(edition_id))?;
    Ok(vec![book_record_from_edition_response(&edition)])
}

fn fetch_by_work(work_id: &str) -> FnResult<Vec<OpenLibraryBookRecord>> {
    let work: OpenLibraryWorkResponse = execute_get(build_work_url(work_id))?;
    let editions: OpenLibraryWorkEditionsResponse = execute_get(build_work_editions_url(work_id))?;
    let merged = merge_work_with_edition(
        book_record_from_work_response(&work),
        first_record_from_work_editions(&editions),
    );
    Ok(vec![merged])
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
    if body.chars().all(|c| c.is_ascii_digit()) && (last.is_ascii_digit() || last == 'X' || last == 'x') {
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

fn lookup_book_records(lookup: &RsLookupWrapper) -> FnResult<Vec<OpenLibraryBookRecord>> {
    let Some(mut ids) = extract_book_ids(&lookup.query) else {
        return Ok(vec![]);
    };

    if ids.isbn13.is_none() {
        if let RsLookupQuery::Book(book) = &lookup.query {
            if let Some(name) = book.name.as_deref() {
                ids.isbn13 = normalize_exact_isbn_search(name);
            }
        }
    }

    let records = if let Some(isbn13) = ids.isbn13 {
        fetch_by_isbn(&isbn13)?
    } else if let Some(edition_id) = ids.edition_id {
        fetch_by_edition(&edition_id)?
    } else if let Some(work_id) = ids.work_id {
        fetch_by_work(&work_id)?
    } else {
        let search = match &lookup.query {
            RsLookupQuery::Book(book) => book.name.as_deref(),
            _ => None,
        };

        match search {
            Some(name) if !name.trim().is_empty() => fetch_by_search(name)?,
            _ => {
                return Err(WithReturnCode::new(
                    extism_pdk::Error::msg("Not supported"),
                    404,
                ));
            }
        }
    };

    Ok(deduplicate_records(records))
}

fn lookup_book_records_for_images(lookup: &RsLookupWrapper) -> FnResult<Vec<OpenLibraryBookRecord>> {
    let Some(mut ids) = extract_book_ids(&lookup.query) else {
        return Ok(vec![]);
    };

    if ids.isbn13.is_none() {
        if let RsLookupQuery::Book(book) = &lookup.query {
            if let Some(name) = book.name.as_deref() {
                ids.isbn13 = normalize_exact_isbn_search(name);
            }
        }
    }

    if ids.isbn13.is_some() || ids.edition_id.is_some() || ids.work_id.is_some() {
        let mut records = Vec::new();

        if let Some(isbn13) = ids.isbn13.as_deref() {
            records.extend(fetch_by_isbn(isbn13)?);
        }
        if let Some(edition_id) = ids.edition_id.as_deref() {
            records.extend(fetch_by_edition(edition_id)?);
        }
        if let Some(work_id) = ids.work_id.as_deref() {
            records.extend(fetch_by_work(work_id)?);
        }

        return Ok(records);
    }

    lookup_book_records(lookup)
}

#[plugin_fn]
pub fn lookup_metadata(
    Json(lookup): Json<RsLookupWrapper>,
) -> FnResult<Json<Vec<RsLookupMetadataResultWithImages>>> {
    let all_books = lookup_book_records(&lookup)?;

    let results: Vec<RsLookupMetadataResultWithImages> =
        all_books.into_iter().map(openlibrary_book_to_result).collect();

    Ok(Json(results))
}

#[plugin_fn]
pub fn lookup_metadata_images(
    Json(lookup): Json<RsLookupWrapper>,
) -> FnResult<Json<Vec<ExternalImage>>> {
    let all_books = lookup_book_records_for_images(&lookup)?;

    let images: Vec<ExternalImage> = all_books
        .into_iter()
        .flat_map(|book| openlibrary_book_to_images(&book))
        .collect();

    Ok(Json(deduplicate_images(images)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rs_plugin_common_interfaces::{domain::rs_ids::RsIds, lookup::RsLookupBook};

    #[test]
    fn book_query_extracts_ids() {
        let query = RsLookupQuery::Book(RsLookupBook {
            name: None,
            ids: Some(RsIds {
                isbn13: Some("9780140328721".to_string()),
                openlibrary_edition_id: Some("/books/OL7353617M".to_string()),
                openlibrary_work_id: Some("works/OL45804W".to_string()),
                ..Default::default()
            }),
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
        assert_eq!(normalize_exact_isbn_search("The Hobbit 9780140328721"), None);
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
}
