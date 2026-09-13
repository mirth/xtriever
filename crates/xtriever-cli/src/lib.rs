//! `xtriever` — the command-line tools' library half, so the integration tests can reach the
//! modules the binary is built from. Feature 008 adds the `wiki` family (specs/008-wiki-corpus).
//!
//! A binary crate in spirit: `anyhow` errors throughout (Principle VII).

pub mod wiki;
