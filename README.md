cargo build --target wasm32-unknown-unknown --release
cargo test --test lookup_test -- --nocapture

Author credits are returned as person objects in `relations.peopleDetails`,
with inline `roles: ["Author"]`. Unknown character names and rank are omitted.
No parallel credit maps or compatibility adapters are used.

This requires the matching server/interface 0.41.0 update. Update the server
and credit-producing plugins together, then refresh existing metadata as needed.

## Relationship search filters

Book lookups accept `people`, `series`, and `tags` filters from interface
0.41.0. Filters may use names or external IDs, and a person filter without a
`role` performs a broad person search. OpenLibrary currently exposes book
authors, so an explicit non-author role returns no result.

OpenLibrary author and series OLIDs are translated to `author_key` and
`series_key` search clauses. Tag IDs using `openlib-tag`,
`openlibrary-subject`, `subject`, or `subject-key` are translated to
OpenLibrary subject keys. Exact ISBN, edition, and work lookups are also
validated against relationship filters rather than bypassing them.
