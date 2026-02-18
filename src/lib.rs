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
        version: 2,
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

fn lookup_book_records(lookup: &RsLookupWrapper) -> FnResult<Vec<OpenLibraryBookRecord>> {
    let Some(ids) = extract_book_ids(&lookup.query) else {
        return Ok(vec![]);
    };

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
    let all_books = lookup_book_records(&lookup)?;

    let images: Vec<ExternalImage> = all_books
        .into_iter()
        .flat_map(|book| openlibrary_book_to_images(&book))
        .collect();

    Ok(Json(images))
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
}
