//! Shared by the Feature 027 spike examples (`rerank_runs`, `splade_field`): the repository
//! root and a strict `--name value` parser. Not an example itself — Cargo builds only
//! `examples/*.rs` and `examples/*/main.rs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The repository root, from this crate's manifest directory.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The command line as `--name value` pairs. Refused, naming the argument: a flag not in
/// `allowed`, a flag given twice, a flag with no value (it ends the line, or the next argument
/// is another `--flag`, which would otherwise be taken as the value), and a stray argument.
pub fn flags(allowed: &[&str]) -> anyhow::Result<BTreeMap<String, String>> {
    parse(std::env::args().skip(1), allowed)
}

fn parse(
    args: impl IntoIterator<Item = String>,
    allowed: &[&str],
) -> anyhow::Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let Some(name) = arg.strip_prefix("--") else {
            anyhow::bail!("unexpected argument {arg:?}; every argument is --name value");
        };
        anyhow::ensure!(
            allowed.contains(&name),
            "unknown flag --{name}; this example takes --{}",
            allowed.join(", --")
        );
        let value = match args.next() {
            Some(value) if !value.starts_with("--") => value,
            Some(next) => anyhow::bail!("--{name} needs a value, got the flag {next}"),
            None => anyhow::bail!("--{name} needs a value"),
        };
        anyhow::ensure!(
            out.insert(name.to_owned(), value).is_none(),
            "--{name} given twice"
        );
    }
    Ok(out)
}
