//! Shared by the examples: the repository root, a strict `--name value` parser (the Feature
//! 027 spike examples; `beir` keeps its own, which takes positional subcommands) and a
//! directory's size. Not an example itself — Cargo builds only `examples/*.rs` and
//! `examples/*/main.rs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The repository root, from this crate's manifest directory.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Bytes under `dir`, recursively. A directory that cannot be read is an error, never 0: a
/// size line that silently said 0 would make a size criterion meaningless.
pub fn dir_bytes(dir: &Path) -> std::io::Result<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        total += if entry.file_type()?.is_dir() {
            dir_bytes(&path)?
        } else {
            entry.metadata()?.len()
        };
    }
    Ok(total)
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
