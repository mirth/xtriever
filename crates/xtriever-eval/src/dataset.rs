//! Pinned BEIR datasets: the manifest, hash verification, and hash-verified loading
//! (spec FR-006–FR-011; research D1, D2).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// One pinned file: size and SHA-256, plus its line count for the record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    /// Exact size in bytes.
    pub bytes: u64,
    /// Lower-case hex SHA-256 of the file.
    pub sha256: String,
    /// Number of lines (`corpus`/`queries`: records; `qrels`: 1 header + pairs). Informational.
    #[serde(default)]
    pub lines: u64,
}

/// Counts asserted after loading (FR-009).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Counts {
    /// Records in `corpus.jsonl`.
    pub documents: u64,
    /// Records in `queries.jsonl`.
    pub queries: u64,
    /// Distinct query ids in `qrels/test.tsv`.
    pub judged_queries: u64,
    /// Data rows in `qrels/test.tsv`.
    pub judgement_pairs: u64,
}

/// One dataset's pins.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatasetEntry {
    /// Where the archive comes from; identifies provenance, not content.
    pub url: String,
    /// The archive's pins (checked by `scripts/fetch-beir.sh`).
    pub archive: FileEntry,
    /// The files the harness reads, keyed by path relative to the extracted directory.
    pub files: BTreeMap<String, FileEntry>,
    /// Expected counts.
    pub counts: Counts,
}

/// `reference/datasets/beir-manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Free-text provenance note.
    #[serde(default)]
    pub note: String,
    /// Exactly `scifact`, `nfcorpus`, `fiqa`.
    pub datasets: BTreeMap<String, DatasetEntry>,
}

/// The three dataset names the harness knows.
pub const DATASETS: &[&str] = &["scifact", "nfcorpus", "fiqa"];

impl Manifest {
    /// Read and validate the manifest.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let m: Manifest = serde_json::from_str(&text)?;
        for (name, entry) in &m.datasets {
            for rel in REQUIRED_FILES {
                if !entry.files.contains_key(*rel) {
                    return Err(Error::Manifest(format!("{name}: missing pin for {rel}")));
                }
            }
        }
        Ok(m)
    }

    /// The entry for `name`, or `Error::Manifest` if the manifest has no such dataset.
    pub fn dataset(&self, name: &str) -> Result<&DatasetEntry> {
        self.datasets.get(name).ok_or_else(|| {
            Error::Manifest(format!(
                "unknown dataset `{name}`; manifest has: {}",
                self.datasets.keys().cloned().collect::<Vec<_>>().join(", ")
            ))
        })
    }

    /// Verify `<cache_dir>/<name>/<rel>` against its pinned size and hash, returning the path.
    /// Both size and hash are compared and both appear in the error (FR-007).
    pub fn verify_file(&self, cache_dir: &Path, name: &str, rel: &str) -> Result<PathBuf> {
        let entry = self.dataset(name)?;
        let pin = entry
            .files
            .get(rel)
            .ok_or_else(|| Error::Manifest(format!("{name}: no pin for {rel}")))?;
        let path = cache_dir.join(name).join(rel);
        let (bytes, sha) = hash_file(&path)?;
        if bytes != pin.bytes || sha != pin.sha256 {
            return Err(Error::HashMismatch {
                path,
                expected: format!("{}/{}", pin.bytes, pin.sha256),
                actual: format!("{bytes}/{sha}"),
            });
        }
        Ok(path)
    }
}

/// The files the harness reads; `train`/`dev` qrels are never touched (FR-011).
const REQUIRED_FILES: &[&str] = &["corpus.jsonl", "queries.jsonl", "qrels/test.tsv"];

/// Streaming SHA-256 in 1 MiB chunks; returns `(bytes, hex)`.
fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    let hex = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((total, hex))
}

#[derive(Deserialize)]
struct CorpusLine {
    _id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct QueryLine {
    _id: String,
    text: String,
}

fn parse_err(path: &Path, line: usize, msg: impl Into<String>) -> Error {
    Error::Parse {
        path: path.to_path_buf(),
        line,
        msg: msg.into(),
    }
}

fn load_corpus(path: &Path) -> Result<Corpus> {
    let mut corpus = Corpus::default();
    for (i, line) in BufReader::new(std::fs::File::open(path)?)
        .lines()
        .enumerate()
    {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let rec: CorpusLine =
            serde_json::from_str(&line).map_err(|e| parse_err(path, i + 1, e.to_string()))?;
        corpus.ids.push(rec._id);
        corpus.titles.push(rec.title);
        corpus.texts.push(rec.text);
    }
    Ok(corpus)
}

fn load_queries(path: &Path) -> Result<QuerySet> {
    let mut set = QuerySet::default();
    for (i, line) in BufReader::new(std::fs::File::open(path)?)
        .lines()
        .enumerate()
    {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let rec: QueryLine =
            serde_json::from_str(&line).map_err(|e| parse_err(path, i + 1, e.to_string()))?;
        set.queries.push((rec._id, rec.text));
    }
    Ok(set)
}

/// `query-id<TAB>corpus-id<TAB>score`, one header line, CRLF line endings (research D2).
fn load_qrels(path: &Path) -> Result<Qrels> {
    let mut qrels = Qrels::default();
    for (i, line) in BufReader::new(std::fs::File::open(path)?)
        .lines()
        .enumerate()
    {
        let line = line?;
        let line = line.trim_end_matches('\r');
        if i == 0 || line.is_empty() {
            continue; // the header
        }
        let mut cols = line.split('\t');
        let (Some(q), Some(d), Some(s), None) =
            (cols.next(), cols.next(), cols.next(), cols.next())
        else {
            return Err(parse_err(
                path,
                i + 1,
                "expected exactly three tab-separated columns",
            ));
        };
        let grade: u32 = s
            .parse()
            .map_err(|_| parse_err(path, i + 1, format!("grade `{s}` is not an integer")))?;
        qrels
            .grades
            .entry(q.to_owned())
            .or_default()
            .insert(d.to_owned(), grade);
    }
    Ok(qrels)
}

/// Documents in file order; the index into these vectors is the internal `DocId`.
#[derive(Debug, Default, Clone)]
pub struct Corpus {
    /// BEIR `_id`, in file order.
    pub ids: Vec<String>,
    /// Titles (empty string when absent or empty).
    pub titles: Vec<String>,
    /// Bodies.
    pub texts: Vec<String>,
}

/// Queries `(id, text)` in file order.
#[derive(Debug, Default, Clone)]
pub struct QuerySet {
    /// All queries in the file; only judged ones are run.
    pub queries: Vec<(String, String)>,
}

/// Relevance judgements: query id → document id → grade.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Qrels {
    /// Grades as read; 0 is present in some BEIR sets and means non-relevant.
    pub grades: BTreeMap<String, BTreeMap<String, u32>>,
}

impl Qrels {
    /// The judged documents of `query` (all grades, including 0), if judged.
    pub fn judged(&self, query: &str) -> Option<&BTreeMap<String, u32>> {
        self.grades.get(query)
    }

    /// Number of judgement rows.
    pub fn pairs(&self) -> usize {
        self.grades.values().map(BTreeMap::len).sum()
    }
}

/// Judged ids missing from the corpus or the query set (reported, never fatal — FR-010).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Dangling {
    /// Judged query ids absent from `queries.jsonl`.
    pub queries: Vec<String>,
    /// Judged document ids absent from `corpus.jsonl`.
    pub documents: Vec<String>,
}

/// A loaded, verified dataset.
#[derive(Debug, Clone)]
pub struct Dataset {
    /// `scifact` | `nfcorpus` | `fiqa`.
    pub name: String,
    /// Documents.
    pub corpus: Corpus,
    /// Queries.
    pub queries: QuerySet,
    /// Test-split judgements.
    pub qrels: Qrels,
    /// Referential-integrity report.
    pub dangling: Dangling,
    /// The manifest hashes of the files that were loaded, for the report.
    pub hashes: BTreeMap<String, String>,
    /// The manifest counts, re-asserted at load.
    pub counts: Counts,
}

impl Dataset {
    /// Load `name` from `cache_dir`, verifying every file's hash before parsing it (FR-006) and
    /// asserting the manifest counts afterwards (FR-009).
    pub fn load(manifest: &Manifest, name: &str, cache_dir: &Path) -> Result<Self> {
        let entry = manifest.dataset(name)?;
        // Verify every file before parsing any of them (FR-006): a bad file never reaches a parser.
        let mut hashes = BTreeMap::new();
        let mut paths = BTreeMap::new();
        for rel in REQUIRED_FILES {
            let path = manifest.verify_file(cache_dir, name, rel)?;
            let pin = &entry.files[*rel];
            hashes.insert((*rel).to_owned(), pin.sha256.clone());
            paths.insert(*rel, path);
        }
        let corpus = load_corpus(&paths["corpus.jsonl"])?;
        let queries = load_queries(&paths["queries.jsonl"])?;
        let qrels = load_qrels(&paths["qrels/test.tsv"])?;

        // Referential integrity: reported, never fatal (FR-010).
        let doc_ids: BTreeSet<&str> = corpus.ids.iter().map(String::as_str).collect();
        let query_ids: BTreeSet<&str> = queries.queries.iter().map(|(id, _)| id.as_str()).collect();
        let mut dangling = Dangling::default();
        for (q, docs) in &qrels.grades {
            if !query_ids.contains(q.as_str()) {
                dangling.queries.push(q.clone());
            }
            for d in docs.keys() {
                if !doc_ids.contains(d.as_str()) && !dangling.documents.contains(d) {
                    dangling.documents.push(d.clone());
                }
            }
        }

        // The manifest's counts are assertions, not documentation (FR-009).
        let counts = entry.counts;
        let observed = Counts {
            documents: corpus.ids.len() as u64,
            queries: queries.queries.len() as u64,
            judged_queries: qrels.grades.len() as u64,
            judgement_pairs: qrels.pairs() as u64,
        };
        for (what, want, got) in [
            ("documents", counts.documents, observed.documents),
            ("queries", counts.queries, observed.queries),
            (
                "judged_queries",
                counts.judged_queries,
                observed.judged_queries,
            ),
            (
                "judgement_pairs",
                counts.judgement_pairs,
                observed.judgement_pairs,
            ),
        ] {
            if want != got {
                return Err(Error::Manifest(format!(
                    "{name}: {what} is {got}, manifest says {want}"
                )));
            }
        }
        Ok(Self {
            name: name.to_owned(),
            corpus,
            queries,
            qrels,
            dangling,
            hashes,
            counts,
        })
    }
}
