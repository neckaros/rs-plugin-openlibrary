use rs_plugin_common_interfaces::{
    domain::{
        book::Book,
        external_images::{ExternalImage, ImageType},
        rs_ids::RsIds,
    },
    lookup::{RsLookupMetadataResult, RsLookupMetadataResultWithImages},
    RsRequest,
};
use serde_json::json;

use crate::openlibrary::{
    build_cover_url_from_id, build_cover_url_from_olid, OpenLibraryBookRecord,
};

fn canonical_rs_id(record: &OpenLibraryBookRecord) -> Option<String> {
    let ids = RsIds {
        isbn13: record.isbn13.clone(),
        openlibrary_edition_id: record.edition_id.clone(),
        openlibrary_work_id: record.work_id.clone(),
        ..Default::default()
    };

    ids.as_isbn13()
        .or(ids.as_openlibrary_edition_id())
        .or(ids.as_openlibrary_work_id())
}

fn fallback_local_id(title: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;

    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "openlibrary-title".to_string()
    } else {
        format!("openlibrary-title-{slug}")
    }
}

fn build_images(record: &OpenLibraryBookRecord) -> Vec<ExternalImage> {
    let image_url = record
        .cover_id
        .map(build_cover_url_from_id)
        .or_else(|| {
            record
                .edition_id
                .as_ref()
                .map(|edition_id| build_cover_url_from_olid(edition_id))
        })
        .or_else(|| {
            record
                .work_id
                .as_ref()
                .map(|work_id| build_cover_url_from_olid(work_id))
        });

    match image_url {
        Some(url) => vec![ExternalImage {
            kind: Some(ImageType::Poster),
            url: RsRequest {
                url,
                ..Default::default()
            },
            ..Default::default()
        }],
        None => vec![],
    }
}

fn build_params(record: &OpenLibraryBookRecord) -> serde_json::Value {
    let mut params = serde_json::Map::new();

    if !record.authors.is_empty() {
        params.insert("authors".to_string(), json!(record.authors));
    }
    if !record.subjects.is_empty() {
        params.insert("subjects".to_string(), json!(record.subjects));
    }
    if !record.publishers.is_empty() {
        params.insert("publishers".to_string(), json!(record.publishers));
    }
    if let Some(edition_id) = &record.edition_id {
        params.insert("openlibraryEditionId".to_string(), json!(edition_id));
    }
    if let Some(work_id) = &record.work_id {
        params.insert("openlibraryWorkId".to_string(), json!(work_id));
    }

    serde_json::Value::Object(params)
}

pub fn openlibrary_book_to_result(
    record: OpenLibraryBookRecord,
) -> RsLookupMetadataResultWithImages {
    let images = build_images(&record);
    let params = build_params(&record);

    let book = Book {
        id: canonical_rs_id(&record).unwrap_or_else(|| fallback_local_id(&record.title)),
        name: record.title,
        kind: Some("book".to_string()),
        serie_ref: None,
        volume: None,
        chapter: None,
        year: record.publish_year,
        airdate: None,
        overview: record.description,
        pages: record.pages,
        params: Some(params),
        lang: record.language,
        original: None,
        isbn13: record.isbn13,
        openlibrary_edition_id: record.edition_id,
        openlibrary_work_id: record.work_id,
        google_books_volume_id: None,
        asin: None,
        ..Default::default()
    };

    RsLookupMetadataResultWithImages {
        metadata: RsLookupMetadataResult::Book(book),
        images,
        ..Default::default()
    }
}

pub fn openlibrary_book_to_images(record: &OpenLibraryBookRecord) -> Vec<ExternalImage> {
    build_images(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_cover_id_for_images() {
        let record = OpenLibraryBookRecord {
            title: "The Hobbit".to_string(),
            cover_id: Some(12345),
            edition_id: Some("OL7353617M".to_string()),
            ..Default::default()
        };

        let images = openlibrary_book_to_images(&record);
        assert_eq!(images.len(), 1);
        assert_eq!(
            images[0].url.url,
            "https://covers.openlibrary.org/b/id/12345-L.jpg"
        );
    }

    #[test]
    fn maps_record_to_book_metadata() {
        let record = OpenLibraryBookRecord {
            title: "The Hobbit".to_string(),
            edition_id: Some("OL7353617M".to_string()),
            work_id: Some("OL45804W".to_string()),
            isbn13: Some("9780140328721".to_string()),
            publish_year: Some(1937),
            ..Default::default()
        };

        let result = openlibrary_book_to_result(record);

        if let RsLookupMetadataResult::Book(book) = result.metadata {
            assert_eq!(book.id, "oleid:OL7353617M".to_string());
            assert_eq!(book.name, "The Hobbit");
            assert_eq!(book.kind, Some("book".to_string()));
            assert_eq!(book.year, Some(1937));
            assert_eq!(book.openlibrary_work_id, Some("OL45804W".to_string()));
        } else {
            panic!("Expected Book metadata");
        }
    }

    #[test]
    fn uses_canonical_work_id_when_edition_is_missing() {
        let record = OpenLibraryBookRecord {
            title: "The Hobbit".to_string(),
            work_id: Some("OL45804W".to_string()),
            ..Default::default()
        };

        let result = openlibrary_book_to_result(record);

        if let RsLookupMetadataResult::Book(book) = result.metadata {
            assert_eq!(book.id, "olwid:OL45804W".to_string());
        } else {
            panic!("Expected Book metadata");
        }
    }

    #[test]
    fn uses_canonical_isbn13_id_when_only_isbn_exists() {
        let record = OpenLibraryBookRecord {
            title: "The Hobbit".to_string(),
            isbn13: Some("9780140328721".to_string()),
            ..Default::default()
        };

        let result = openlibrary_book_to_result(record);

        if let RsLookupMetadataResult::Book(book) = result.metadata {
            assert_eq!(book.id, "isbn13:9780140328721".to_string());
        } else {
            panic!("Expected Book metadata");
        }
    }

    #[test]
    fn uses_non_external_fallback_when_no_canonical_id_exists() {
        let record = OpenLibraryBookRecord {
            title: "The Hobbit".to_string(),
            ..Default::default()
        };

        let result = openlibrary_book_to_result(record);

        if let RsLookupMetadataResult::Book(book) = result.metadata {
            assert_eq!(book.id, "openlibrary-title-the-hobbit".to_string());
        } else {
            panic!("Expected Book metadata");
        }
    }
}
