//! Xtriever's Swift/iOS FFI surface.
//!
//! Everything here sits behind the non-default `spike` feature (Constitution §III). With default
//! features this crate is an empty placeholder, exactly as it was before Feature 001.
//!
//! # Layout
//!
//! - [`ffi`] is the uniffi boundary: wire types and thin delegating shims. Compiler-generated
//!   `unsafe` lives here and nowhere else.
//! - `spike` holds every line of hand-written logic and re-declares `#![deny(unsafe_code)]`.
//!
//! This split is what keeps Principle VII meaningful: the lint relaxation below covers generated
//! code only. See [`ADR-0003`].
//!
//! [`ADR-0003`]: ../../../docs/adr/0003-uniffi-scaffolding-requires-unsafe-allow.md
//
// `unsafe_code` is denied workspace-wide. uniffi's `setup_scaffolding!`, `#[uniffi::export]` and
// the `Record`/`Enum`/`Error` derives emit `#[unsafe(no_mangle)] pub unsafe extern "C" fn` and
// `unsafe impl` — 38 sites in uniffi_macros-0.32.1 (setup_scaffolding.rs:41-100,
// export/scaffolding.rs:242+, record.rs:120, enum_.rs:254, error.rs:92,119,144). They self-allow
// `missing_docs` and `clippy::missing_safety_doc`, but not `unsafe_code`, and no feature suppresses
// it. This allow therefore covers GENERATED code only; `spike/mod.rs` re-denies it for our own.
#![allow(unsafe_code)]

#[cfg(feature = "spike")]
uniffi::setup_scaffolding!();

#[cfg(feature = "spike")]
pub mod ffi;

// Gating the module here rather than at each export is deliberate: uniffi generates scaffolding for
// `#[uniffi::export]` items even when a `#[cfg]` inside the block is false, so a `#[cfg]` next to an
// export is a compile error waiting to happen (research risk R3). With the gate at the module
// declaration, that situation cannot arise.
#[cfg(feature = "spike")]
mod spike;
