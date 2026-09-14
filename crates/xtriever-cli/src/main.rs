//! `xtriever` — command-line tools. Feature 008 adds the first family, `wiki`: building,
//! verifying and probing the shipped Simple English Wikipedia index (specs/008-wiki-corpus).
//!
//! A binary: `anyhow` for errors (Principle VII), `clap` for arguments (research D8).

use clap::{Parser, Subcommand};

mod wiki;

/// Xtriever command-line tools.
#[derive(Parser)]
#[command(name = "xtriever", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The Wikipedia corpus: build, verify and probe the shipped index (Feature 008).
    #[command(subcommand)]
    Wiki(wiki::WikiCommand),
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Wiki(cmd) => wiki::run(cmd),
    }
}
