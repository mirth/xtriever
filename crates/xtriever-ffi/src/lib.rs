//! Xtriever's Swift/iOS FFI surface (Feature 007): one object over the hybrid pipeline.
//!
//! # The surface
//!
//! - [`IndexHandle::open`] opens a hybrid index directory **read-only** with the pinned embedder
//!   and, optionally, the pinned re-ranker, through one [`LoadPath`] for both models. Nothing
//!   in the directory is modified or created — the pipeline is opened `read_only`, without the
//!   lexical backend's lock file (Feature 008 D11) — so an index inside an app bundle opens in
//!   place. The pipeline's own refusals (format version, fingerprint, interrupted commit, torn
//!   store) apply unchanged.
//! - [`IndexHandle::info`] reports the index's identity and configuration plus the models' load
//!   times; [`IndexHandle::search`] runs one search under wire [`SearchOptions`] and returns the
//!   pipeline's hits — external ids, passage text, fused and re-rank scores, explanation — and
//!   its stage report, converted field by field. The FFI adds no computation: for the same
//!   directory, models, query and options the hits equal `HybridIndex::search`'s bit for bit
//!   (`tests/parity.rs`), which is also what the committed Swift goldens are minted from.
//! - **The clock lives here**: the pipeline reads none (Principle III) and takes an elapsed-time
//!   source from its caller; `search` starts an `Instant` on entry and passes it whenever the
//!   caller set `max_time_ms`, so the pipeline's check points and the re-ranker's remaining time
//!   behave exactly as Features 005/006 define. `xtriever-ffi` is a leaf crate.
//! - Searches on one handle are serialised by a `Mutex` — the handle is `Sync` by construction,
//!   which uniffi objects must be.
//! - **Errors** cross as [`XtrieverError`], one case per `xtriever_core::Error` variant with the
//!   engine's message; a variant added to the core later arrives as `Backend` with its text
//!   rather than being lost. Every export returns `Result`, which uniffi lowers to a Swift
//!   `throws`; uniffi also catches a panic inside an export, so nothing crosses the boundary
//!   uncaught.
//!
//! **Async lives on the Swift side**: `swift/Xtriever/Sources/Xtriever/XtrieverIndex.swift` runs
//! these synchronous calls on a private serial dispatch queue and resumes the caller through a
//! continuation. uniffi 0.32.1 polls a Rust future on the awaiting task's thread, so a CPU-bound
//! Rust `async fn` would block Swift's cooperative pool — the wrong tool (007 research D2).
//! `scripts/build-ios-package.sh` builds the static libraries, generates the bindings, and
//! assembles the XCFramework the package links.
//!
//! # Layout
//!
//! - [`ffi`] is the uniffi boundary: wire types, the error enum and thin delegating shims.
//!   Compiler-generated `unsafe` lives here and nowhere else.
//! - `index` holds every line of hand-written logic and re-declares `#![deny(unsafe_code)]`.
//!
//! This split is what keeps Principle VII meaningful: the lint relaxation below covers generated
//! code only. See [`ADR-0003`].
//!
//! [`ADR-0003`]: ../../../docs/adr/0003-uniffi-scaffolding-requires-unsafe-allow.md
//
// `unsafe_code` is denied workspace-wide. uniffi's `setup_scaffolding!`, `#[uniffi::export]` and
// the `Record`/`Enum`/`Error`/`Object` derives emit `#[unsafe(no_mangle)] pub unsafe extern "C" fn`
// and `unsafe impl` (uniffi_macros-0.32.1: setup_scaffolding.rs:41-100, export/scaffolding.rs:242+,
// record.rs:120, enum_.rs:254, error.rs:92,119,144). They self-allow `missing_docs` and
// `clippy::missing_safety_doc`, but not `unsafe_code`, and no feature suppresses it. This allow
// therefore covers GENERATED code only; `index.rs` re-denies it for our own.
#![allow(unsafe_code)]

uniffi::setup_scaffolding!();

pub mod ffi;
mod index;

pub use ffi::{
    ChunkInfo, Degradation, DegradeReason, Hit, HitExplain, IndexHandle, IndexInfo, LoadPath,
    RerankReport, SearchOptions, SearchResponse, StageReport, XtrieverError,
};
pub use index::{from_response, to_pipeline_options};
