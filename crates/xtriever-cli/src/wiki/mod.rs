//! `xtriever wiki …` — the Simple English Wikipedia corpus build (specs/008-wiki-corpus,
//! contracts/cli.md). One module per concern: the manifest and its verification, the
//! exclusion rules, the URL derivation, the embedding cache, document shaping, the build,
//! the verify pass, the host goldens, and the records.
//!
//! Re-ranking (Feature 015, ADR-0012): the engine's default orders the re-ranked head by
//! `0.5·minmax(fused) + 0.5·minmax(cross-encoder)` at depth 20; an index built before 015 —
//! the shipped Wikipedia index — adopts it on upgrade without a rebuild, and
//! `SearchOptions::rerank_mode = Some(Replace)` selects the previous order. `wiki expected`
//! follows the index's recorded mode, so the host goldens must be regenerated after a default
//! change (Feature 017 did, adding depth 10 to the measured depths).

use std::path::PathBuf;

use clap::{Args, Subcommand};

pub mod build;
pub mod cache;
pub mod chunking;
pub mod expected;
pub mod manifest;
pub mod record;
pub mod rules;
pub mod url;
pub mod verify;

/// The `wiki` subcommands.
#[derive(Subcommand)]
pub enum WikiCommand {
    /// Build the hybrid index from the pinned snapshot (resumable at shard granularity).
    Build(BuildArgs),
    /// Re-verify a built index: every passage inside the embedder's window, every URL derivable.
    Verify(VerifyArgs),
    /// Write the host-side goldens for the measurement queries (device parity).
    Expected(ExpectedArgs),
}

/// `wiki build` arguments.
#[derive(Args)]
pub struct BuildArgs {
    /// The snapshot manifest.
    #[arg(long, default_value = "reference/datasets/wiki-manifest.json")]
    pub manifest: PathBuf,
    /// Where scripts/fetch-wiki.sh put the snapshot.
    #[arg(long, default_value = "reference/datasets/wiki")]
    pub snapshot_dir: PathBuf,
    /// The pinned embedder directory.
    #[arg(long, default_value = "reference/models/all-MiniLM-L6-v2-q8")]
    pub embedder_dir: PathBuf,
    /// Output directory (created; written as `<out>.partial` until complete).
    #[arg(long)]
    pub out: PathBuf,
    /// The embedding cache root (local, never committed).
    #[arg(long, default_value = "target/xt-wiki-cache")]
    pub cache_dir: PathBuf,
    /// How the embedder's weights are loaded.
    #[arg(long, default_value = "mmap", value_parser = ["buffered", "mmap"])]
    pub load_path: String,
    /// Read only the first N articles — development builds; the identity is marked partial.
    #[arg(long)]
    pub limit: Option<usize>,
    /// Build a sparse index (Feature 027) with the pinned sparse document encoder in this
    /// directory. Off by default; pair it with the re-ranker (ADR-0017).
    #[arg(long)]
    pub sparse_encoder: Option<PathBuf>,
    /// The sparse option's scale (needs `--sparse-encoder`; default: the engine's, 10).
    #[arg(long)]
    pub sparse_scale: Option<u32>,
    /// The `_sparse` field's boost (needs `--sparse-encoder`; default: the engine's, 1.0).
    #[arg(long)]
    pub sparse_boost: Option<f32>,
}

/// `wiki verify` arguments.
#[derive(Args)]
pub struct VerifyArgs {
    /// The index directory.
    #[arg(long)]
    pub index: PathBuf,
    /// The pinned embedder directory.
    #[arg(long, default_value = "reference/models/all-MiniLM-L6-v2-q8")]
    pub embedder_dir: PathBuf,
    /// Where the snapshot lives (for the URL check).
    #[arg(long, default_value = "reference/datasets/wiki")]
    pub snapshot_dir: PathBuf,
}

/// `wiki expected` arguments.
#[derive(Args)]
pub struct ExpectedArgs {
    /// The index directory.
    #[arg(long)]
    pub index: PathBuf,
    /// The pinned embedder directory.
    #[arg(long, default_value = "reference/models/all-MiniLM-L6-v2-q8")]
    pub embedder_dir: PathBuf,
    /// The pinned re-ranker directory.
    #[arg(long, default_value = "reference/models/ms-marco-MiniLM-L-6-v2-q8")]
    pub reranker_dir: PathBuf,
    /// The measurement queries.
    #[arg(long, default_value = "reference/fixtures/008/queries.json")]
    pub queries: PathBuf,
    /// Where to write the goldens.
    #[arg(long)]
    pub out: PathBuf,
}

/// Dispatch.
pub fn run(cmd: WikiCommand) -> anyhow::Result<()> {
    match cmd {
        WikiCommand::Build(args) => build::run(&args),
        WikiCommand::Verify(args) => verify::run(&args),
        WikiCommand::Expected(args) => expected::run(&args),
    }
}
