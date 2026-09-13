//! Article → passages → `SourceDocument`s (research D5, D6; contracts/artefact.md "What a hit
//! means"). The chunker is priced with the embedder's own window: content word-pieces per unit,
//! budget `256 − token_count(title)` so `[CLS] title body [SEP]` fits 256 positions.

use std::collections::BTreeMap;

use anyhow::{Context, bail};
use serde::Deserialize;
use xtriever_analysis::chunk::{Passage, chunk};
use xtriever_core::{AnalyzerId, ChunkInfo, FieldDef, FieldKind, FieldName, Schema, Value};
use xtriever_dense::MiniLmEmbedder;
use xtriever_pipeline::{HybridConfig, SourceDocument};

/// The embedder's window in positions (`[CLS]` … `[SEP]`), the pinned model's `max_tokens`.
pub const WINDOW: usize = 256;

/// One article as read from the snapshot's JSONL.
#[derive(Clone, Debug, Deserialize)]
pub struct Article {
    /// The MediaWiki page id, decimal.
    pub id: String,
    /// The canonical article URL from the snapshot.
    pub url: String,
    /// The title.
    pub title: String,
    /// The body, plain text.
    pub text: String,
}

/// The index schema: `title` (boost 2.0) and `text` (boost 1.0), both `standard_en`, both
/// indexed; `text` is the dense field. Mirrors the BEIR configurations' field shape.
#[must_use]
pub fn wiki_config() -> HybridConfig {
    let field = |name: &str, boost: f32| FieldDef {
        name: FieldName::from(name),
        kind: FieldKind::Text(AnalyzerId("standard_en".to_owned())),
        indexed: true,
        stored: false,
        boost,
    };
    HybridConfig::new(
        Schema {
            fields: vec![field("title", 2.0), field("text", 1.0)],
        },
        vec![FieldName::from("text")],
    )
}

/// The `text` field / stored passage: the title line, a blank line, the passage body.
#[must_use]
pub fn passage_text(title: &str, body: &str) -> String {
    format!("{title}\n\n{body}")
}

/// The article's passages with their provenance, as the pipeline ingests them.
///
/// # Errors
///
/// A title that alone fills the window; a tokenizer failure.
pub fn documents_for(
    article: &Article,
    embedder: &MiniLmEmbedder,
) -> anyhow::Result<Vec<SourceDocument>> {
    let title_positions = embedder
        .token_count(&article.title)
        .with_context(|| format!("counting title of {}", article.id))?;
    if title_positions >= WINDOW {
        bail!(
            "article {} ({:?}): the title alone needs {title_positions} positions, the window is {WINDOW}",
            article.id,
            article.title
        );
    }
    let budget = WINDOW - title_positions;
    // The cost closure cannot return an error; the first one is parked here and the unit is
    // priced at zero so chunking finishes, then the build fails on it below.
    let failure: std::cell::RefCell<Option<xtriever_core::Error>> = std::cell::RefCell::new(None);
    let cost = |unit: &str| -> usize {
        match embedder.token_count(unit) {
            Ok(n) => n.saturating_sub(2),
            Err(e) => {
                failure.borrow_mut().get_or_insert(e);
                0
            }
        }
    };
    let passages = chunk(&article.text, budget, &cost);
    if let Some(e) = failure.into_inner() {
        return Err(anyhow::Error::from(e).context(format!("pricing article {}", article.id)));
    }
    Ok(passages
        .iter()
        .enumerate()
        .map(|(ordinal, p)| source_document(article, ordinal, p))
        .collect())
}

fn source_document(article: &Article, ordinal: usize, p: &Passage) -> SourceDocument {
    let mut fields = BTreeMap::new();
    fields.insert(FieldName::from("title"), Value::Text(article.title.clone()));
    fields.insert(
        FieldName::from("text"),
        Value::Text(passage_text(&article.title, &p.text)),
    );
    SourceDocument {
        external_id: format!("{}#{ordinal}", article.id),
        fields,
        chunk: Some(ChunkInfo {
            parent: article.id.clone(),
            ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
            byte_range: Some(p.byte_range),
        }),
    }
}
