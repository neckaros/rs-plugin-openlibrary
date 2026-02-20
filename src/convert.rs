use rs_plugin_common_interfaces::{
    domain::{
        book::Book,
        external_images::{ExternalImage, ImageType},
        other_ids::OtherIds,
        person::Person,
        rs_ids::RsIds,
        tag::Tag,
        Relations,
    },
    lookup::{RsLookupMetadataResult, RsLookupMetadataResultWrapper},
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

fn slugify(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    let mut previous_was_dash = false;

    for c in value.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            previous_was_dash = false;
        } else if !previous_was_dash {
            slug.push('-');
            previous_was_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.starts_with('-') {
        slug.remove(0);
    }

    if slug.is_empty() {
        "unknown".to_string()
    } else {
        slug
    }
}

fn relation_key(value: &str) -> String {
    let trimmed = value.trim().trim_matches('/');
    if trimmed.is_empty() {
        return "unknown".to_string();
    }

    let candidate = trimmed.rsplit('/').next().unwrap_or(trimmed).trim();
    if candidate
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        candidate.to_ascii_lowercase()
    } else {
        slugify(candidate)
    }
}

fn build_images(record: &OpenLibraryBookRecord) -> Vec<ExternalImage> {
    let mut cover_urls: Vec<String> = Vec::new();
    for cover_id in record
        .cover_ids
        .iter()
        .copied()
        .chain(record.cover_id.into_iter())
    {
        let url = build_cover_url_from_id(cover_id);
        if !cover_urls.contains(&url) {
            cover_urls.push(url);
        }
    }

    if !cover_urls.is_empty() {
        return cover_urls
            .into_iter()
            .map(|url| ExternalImage {
                kind: Some(ImageType::Poster),
                url: RsRequest {
                    url,
                    ..Default::default()
                },
                ..Default::default()
            })
            .collect();
    }

    let image_url = record
        .edition_id
        .as_ref()
        .map(|edition_id| build_cover_url_from_olid(edition_id))
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

fn build_people_details(record: &OpenLibraryBookRecord) -> Option<Vec<Person>> {
    let mut people: Vec<Person> = Vec::new();
    let mut seen_ids: Vec<String> = Vec::new();

    for (index, name) in record.authors.iter().enumerate() {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }

        let person_key = record
            .author_keys
            .get(index)
            .map(|key| key.trim())
            .filter(|key| !key.is_empty())
            .map(relation_key);

        let base_key = slugify(name);
        let relation_key = person_key
            .as_ref()
            .map(|key| format!("{base_key}-{key}"))
            .unwrap_or(base_key);
        let other_id = format!("openlib-person:{relation_key}");

        if seen_ids.contains(&other_id) {
            continue;
        }
        seen_ids.push(other_id.clone());

        let mut params = serde_json::Map::new();
        if let Some(author_key) = person_key {
            params.insert("openlibraryAuthorId".to_string(), json!(author_key));
        }

        people.push(Person {
            id: other_id.clone(),
            name: name.to_string(),
            kind: Some("author".to_string()),
            params: if params.is_empty() {
                None
            } else {
                Some(serde_json::Value::Object(params))
            },
            generated: true,
            otherids: Some(OtherIds(vec![other_id])),
            ..Default::default()
        });
    }

    if people.is_empty() {
        None
    } else {
        Some(people)
    }
}

fn build_tags_details(record: &OpenLibraryBookRecord) -> Option<Vec<Tag>> {
    let mut tags: Vec<Tag> = Vec::new();
    let mut seen_ids: Vec<String> = Vec::new();

    for value in &record.subjects {
        let name = value.trim();
        if name.is_empty() {
            continue;
        }

        let key = relation_key(name);
        let other_id = format!("openlib-tag:{key}");

        if seen_ids.contains(&other_id) {
            continue;
        }
        seen_ids.push(other_id.clone());

        tags.push(Tag {
            id: other_id.clone(),
            name: name.to_string(),
            parent: None,
            kind: Some("subject".to_string()),
            alt: None,
            thumb: None,
            params: Some(json!({ "openlibraryTagKey": key })),
            modified: 0,
            added: 0,
            generated: true,
            path: "/".to_string(),
            otherids: Some(OtherIds(vec![other_id])),
        });
    }

    if tags.is_empty() {
        None
    } else {
        Some(tags)
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

pub fn openlibrary_book_to_result(record: OpenLibraryBookRecord) -> RsLookupMetadataResultWrapper {
    let images = build_images(&record);
    let ext_images = if images.is_empty() {
        None
    } else {
        Some(images)
    };
    let people_details = build_people_details(&record);
    let tags_details = build_tags_details(&record);

    let relations = if ext_images.is_some() || people_details.is_some() || tags_details.is_some() {
        Some(Relations {
            people_details,
            tags_details,
            ext_images,
            ..Default::default()
        })
    } else {
        None
    };
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

    RsLookupMetadataResultWrapper {
        metadata: RsLookupMetadataResult::Book(book),
        relations,
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
            cover_ids: vec![12345],
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
    fn uses_all_cover_ids_for_images() {
        let record = OpenLibraryBookRecord {
            title: "The Hobbit".to_string(),
            cover_ids: vec![12345, 67890],
            ..Default::default()
        };

        let images = openlibrary_book_to_images(&record);
        assert_eq!(images.len(), 2);
        assert_eq!(
            images[0].url.url,
            "https://covers.openlibrary.org/b/id/12345-L.jpg"
        );
        assert_eq!(
            images[1].url.url,
            "https://covers.openlibrary.org/b/id/67890-L.jpg"
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
            assert_eq!(book.id, "isbn13:9780140328721".to_string());
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

    #[test]
    fn includes_images_people_and_tags_in_relations_details_only() {
        let record = OpenLibraryBookRecord {
            title: "The Hobbit".to_string(),
            cover_ids: vec![12345],
            authors: vec!["J.R.R. Tolkien".to_string()],
            author_keys: vec!["OL26320A".to_string()],
            subjects: vec!["Fantasy".to_string()],
            ..Default::default()
        };

        let result = openlibrary_book_to_result(record);
        let relations = result.relations.expect("Expected relations");

        let images = relations.ext_images.expect("Expected ext_images");
        assert_eq!(images.len(), 1);
        assert_eq!(
            images[0].url.url,
            "https://covers.openlibrary.org/b/id/12345-L.jpg"
        );

        let people = relations.people_details.expect("Expected people_details");
        assert_eq!(people.len(), 1);
        assert_eq!(people[0].id, "openlib-person:j-r-r-tolkien-ol26320a");
        assert_eq!(people[0].name, "J.R.R. Tolkien");
        assert_eq!(
            people[0].otherids,
            Some(OtherIds(vec![
                "openlib-person:j-r-r-tolkien-ol26320a".to_string()
            ]))
        );

        let tags = relations.tags_details.expect("Expected tags_details");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].id, "openlib-tag:fantasy");
        assert_eq!(tags[0].name, "Fantasy");
        assert_eq!(
            tags[0].otherids,
            Some(OtherIds(vec!["openlib-tag:fantasy".to_string()]))
        );

        assert!(relations.people.is_none());
        assert!(relations.tags.is_none());
    }
}
