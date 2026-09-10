//! Swift bindings generator.
//!
//! Built only with `--features cli`; never part of a device build. UniFFI recommends a dedicated
//! binary in multi-crate workspaces rather than an artifact dependency:
//! <https://mozilla.github.io/uniffi-rs/latest/tutorial/foreign_language_bindings.html>
//!
//! ```sh
//! cargo run -p xtriever-ffi --features cli --bin uniffi-bindgen -- \
//!     target/aarch64-apple-ios/release/libxtriever_ffi.a build/swift --swift-sources
//! ```

fn main() {
    uniffi::uniffi_bindgen_swift()
}
