//! Bindings generator for both foreign surfaces.
//!
//! Built only with `--features cli`; never part of a device build. UniFFI recommends a dedicated
//! binary in multi-crate workspaces rather than an artifact dependency:
//! <https://mozilla.github.io/uniffi-rs/latest/tutorial/foreign_language_bindings.html>
//!
//! Swift (Feature 007; `scripts/build-ios-package.sh`) uses the Swift-specific entry point and
//! its flags:
//!
//! ```sh
//! cargo run -p xtriever-ffi --features cli --bin uniffi-bindgen -- \
//!     target/aarch64-apple-ios/release/libxtriever_ffi.a build/swift --swift-sources
//! ```
//!
//! Python (Feature 011; maturin runs this through `cargo run --bin uniffi-bindgen`, research
//! D2) uses the generic `generate` subcommand, dispatched on the first argument:
//!
//! ```sh
//! cargo run -p xtriever-ffi --features cli --bin uniffi-bindgen -- \
//!     generate --library target/release/libxtriever_ffi.dylib --language python --out-dir build/python
//! ```

fn main() {
    if std::env::args().nth(1).as_deref() == Some("generate") {
        uniffi::uniffi_bindgen_main()
    } else {
        uniffi::uniffi_bindgen_swift()
    }
}
