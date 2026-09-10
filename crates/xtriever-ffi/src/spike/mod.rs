//! All hand-written spike logic.
//!
//! This module re-establishes the workspace guarantee that ADR-0003 relaxes at the crate root: the
//! lint allow there covers uniffi's generated scaffolding in `crate::ffi`, and must not leak into
//! code we wrote.
//!
//! The one exemption is the `VarBuilder::from_mmaped_safetensors` call in [`embed`], which carries
//! an item-scoped `#[allow(unsafe_code)]` and a `// SAFETY:` comment under ADR-0002. In PR 1a there
//! is none at all:
//!
//! ```sh
//! grep -rn unsafe crates/xtriever-ffi/src/spike/   # expect: nothing (PR 1a)
//! ```
#![deny(unsafe_code)]

pub mod embed;
pub mod index;
pub mod query;
