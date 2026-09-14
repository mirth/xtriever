//! Xtriever `analysis`: text analysis that needs no model — pure Rust, `std`-only
//! (constitution Principle III).
//!
//! Feature 008 adds [`chunk`]: a body of text into passages under a caller-supplied cost
//! function, specified byte for byte against a Python reference
//! (`specs/008-wiki-corpus/contracts/chunker.md`). The cost function is how a tokenizer's units
//! reach this crate without the tokenizer itself.

pub mod chunk;
