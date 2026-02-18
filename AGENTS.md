# Agent Notes

## Integration Test Prerequisite
- Always run `cargo build --release --target wasm32-unknown-unknown` before running integration tests.
- Integration tests in `tests/lookup_test.rs` load `target/wasm32-unknown-unknown/release/rs_plugin_openlibrary.wasm`, so running them without rebuilding can use a stale WASM artifact.
- do not build and run test in parallel it needs to be sequential to avoid using stale wasm file
