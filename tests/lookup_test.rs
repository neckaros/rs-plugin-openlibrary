use extism::*;
use rs_plugin_common_interfaces::{
    domain::rs_ids::RsIds,
    lookup::{RsLookupBook, RsLookupQuery, RsLookupWrapper},
};

fn build_plugin() -> Plugin {
    let wasm = Wasm::file("target/wasm32-unknown-unknown/release/rs_plugin_openlibrary.wasm");
    let manifest = Manifest::new([wasm]).with_allowed_host("openlibrary.org");
    Plugin::new(&manifest, [], true).expect("Failed to create plugin")
}

fn call_lookup(plugin: &mut Plugin, input: &RsLookupWrapper) -> serde_json::Value {
    let input_str = serde_json::to_string(input).unwrap();
    let output = plugin
        .call::<&str, &[u8]>("lookup_metadata", &input_str)
        .expect("lookup_metadata call failed");
    serde_json::from_slice(output).expect("Failed to parse output JSON")
}

fn call_lookup_images(plugin: &mut Plugin, input: &RsLookupWrapper) -> serde_json::Value {
    let input_str = serde_json::to_string(input).unwrap();
    let output = plugin
        .call::<&str, &[u8]>("lookup_metadata_images", &input_str)
        .expect("lookup_metadata_images call failed");
    serde_json::from_slice(output).expect("Failed to parse output JSON")
}

#[test]
fn test_lookup_the_hobbit_by_name() {
    let mut plugin = build_plugin();

    let input = RsLookupWrapper {
        query: RsLookupQuery::Book(RsLookupBook {
            name: Some("The Hobbit".to_string()),
            ids: None,
        }),
        credential: None,
        params: None,
    };

    let results = call_lookup(&mut plugin, &input);
    let results_array = results.as_array().expect("Expected an array");
    assert!(
        !results_array.is_empty(),
        "Expected at least one result for 'The Hobbit'"
    );
}

#[test]
fn test_lookup_by_isbn13() {
    let mut plugin = build_plugin();

    let input = RsLookupWrapper {
        query: RsLookupQuery::Book(RsLookupBook {
            name: None,
            ids: Some(RsIds {
                isbn13: Some("9780140328721".to_string()),
                ..Default::default()
            }),
        }),
        credential: None,
        params: None,
    };

    let results = call_lookup(&mut plugin, &input);
    let results_array = results.as_array().expect("Expected an array");
    assert_eq!(
        results_array.len(),
        1,
        "Expected exactly one result when fetching by ISBN13"
    );
}

#[test]
fn test_lookup_by_openlibrary_edition_id() {
    let mut plugin = build_plugin();

    let input = RsLookupWrapper {
        query: RsLookupQuery::Book(RsLookupBook {
            name: None,
            ids: Some(RsIds {
                openlibrary_edition_id: Some("OL7353617M".to_string()),
                ..Default::default()
            }),
        }),
        credential: None,
        params: None,
    };

    let results = call_lookup(&mut plugin, &input);
    let results_array = results.as_array().expect("Expected an array");
    assert_eq!(
        results_array.len(),
        1,
        "Expected exactly one result when fetching by edition ID"
    );
}

#[test]
fn test_lookup_by_openlibrary_work_id() {
    let mut plugin = build_plugin();

    let input = RsLookupWrapper {
        query: RsLookupQuery::Book(RsLookupBook {
            name: None,
            ids: Some(RsIds {
                openlibrary_work_id: Some("OL45804W".to_string()),
                ..Default::default()
            }),
        }),
        credential: None,
        params: None,
    };

    let results = call_lookup(&mut plugin, &input);
    let results_array = results.as_array().expect("Expected an array");
    assert_eq!(
        results_array.len(),
        1,
        "Expected exactly one result when fetching by work ID"
    );
}

#[test]
fn test_lookup_empty_name_returns_404() {
    let mut plugin = build_plugin();

    let input = RsLookupWrapper {
        query: RsLookupQuery::Book(RsLookupBook {
            name: Some("".to_string()),
            ids: None,
        }),
        credential: None,
        params: None,
    };

    let input_str = serde_json::to_string(&input).unwrap();
    let error = plugin
        .call::<&str, &[u8]>("lookup_metadata", &input_str)
        .expect_err("Expected 404 error for empty search");
    let message = error.to_string();
    assert!(
        message.contains("Not supported") || message.contains("404"),
        "Expected error message to mention 404/Not supported, got: {message}"
    );
}

#[test]
fn test_lookup_images_by_openlibrary_edition_id() {
    let mut plugin = build_plugin();

    let input = RsLookupWrapper {
        query: RsLookupQuery::Book(RsLookupBook {
            name: None,
            ids: Some(RsIds {
                openlibrary_edition_id: Some("OL7353617M".to_string()),
                ..Default::default()
            }),
        }),
        credential: None,
        params: None,
    };

    let images = call_lookup_images(&mut plugin, &input);
    let images_array = images.as_array().expect("Expected an array");
    assert!(
        !images_array.is_empty(),
        "Expected at least one image when fetching by edition ID"
    );
}

#[test]
fn test_lookup_images_by_openlibrary_work_id() {
    let mut plugin = build_plugin();

    let input = RsLookupWrapper {
        query: RsLookupQuery::Book(RsLookupBook {
            name: None,
            ids: Some(RsIds {
                openlibrary_work_id: Some("OL11967339W".to_string()),
                ..Default::default()
            }),
        }),
        credential: None,
        params: None,
    };

    let images = call_lookup_images(&mut plugin, &input);
    let images_array = images.as_array().expect("Expected an array");
    assert!(
        !images_array.is_empty(),
        "Expected at least one image when fetching by work ID"
    );
    println!("Images found: {:?}", images_array);
}

#[test]
fn test_lookup_images_by_isbn13_id() {
    let mut plugin = build_plugin();

    let input = RsLookupWrapper {
        query: RsLookupQuery::Book(RsLookupBook {
            name: None,
            ids: Some(RsIds {
                isbn13: Some("9780143143390".to_string()),
                ..Default::default()
            }),
        }),
        credential: None,
        params: None,
    };

    let images = call_lookup_images(&mut plugin, &input);
    let images_array = images.as_array().expect("Expected an array");
    assert!(
        !images_array.is_empty(),
        "Expected at least one image when fetching by isbn13 ID"
    );
    println!("Images found: {:?}", images_array);
}
