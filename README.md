cargo build --target wasm32-unknown-unknown --release
cargo test --test lookup_test -- --nocapture
