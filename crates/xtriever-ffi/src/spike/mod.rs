//! All hand-written spike logic.
//!
//! This module re-establishes the workspace guarantee that ADR-0003 relaxes at the crate root: the
//! lint allow there covers uniffi's generated scaffolding in `crate::ffi`, and must not leak into
//! code we wrote.
//!
//! The one exemption is the `VarBuilder::from_mmaped_safetensors` call in [`embed`], which carries
//! an item-scoped `#[allow(unsafe_code)]` and a `// SAFETY:` comment under ADR-0002:
//!
//! ```sh
//! grep -c 'unsafe {' crates/xtriever-ffi/src/spike/*.rs   # expect: exactly 1, in embed.rs
//! ```
//!
//! That block survives only while `tests/load_paths.rs` proves it agrees bit-for-bit with the safe
//! loader (ADR-0002 condition 4). If the two ever diverge, the mmap path is deleted.
#![deny(unsafe_code)]

pub mod embed;
pub mod index;
pub mod query;
