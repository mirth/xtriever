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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    use std::path::Path;

    use xtriever_core::{FieldName, Value};
    use xtriever_dense::{LoadPath, MiniLmEmbedder};

    #[derive(serde::Deserialize)]
    struct SetB {
        articles: Vec<ArticleB>,
    }
    #[derive(serde::Deserialize)]
    struct ArticleB {
        id: String,
        title: String,
        body: String,
        passages: Vec<Golden>,
    }
    #[derive(serde::Deserialize)]
    struct Golden {
        text: String,
        byte_range: (u64, u64),
    }

    #[test]
    #[ignore = "needs the model"]
    fn real_articles_shape_into_documents_matching_the_goldens() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let embedder = MiniLmEmbedder::load(
            &repo.join("reference/models/all-MiniLM-L6-v2"),
            LoadPath::Buffered,
        )
        .unwrap();
        let set: SetB = serde_json::from_str(
            &std::fs::read_to_string(repo.join("reference/fixtures/008/chunk_b.json")).unwrap(),
        )
        .unwrap();
        let mut checked = 0;
        for a in set
            .articles
            .iter()
            .filter(|a| !a.id.starts_with("synthetic"))
        {
            let article = Article {
                id: a.id.clone(),
                url: String::new(),
                title: a.title.clone(),
                text: a.body.clone(),
            };
            let docs = documents_for(&article, &embedder).unwrap();
            assert_eq!(docs.len(), a.passages.len(), "{}", a.title);
            for (i, (doc, want)) in docs.iter().zip(&a.passages).enumerate() {
                assert_eq!(doc.external_id, format!("{}#{i}", a.id));
                let text = match &doc.fields[&FieldName::from("text")] {
                    Value::Text(t) => t.clone(),
                    other => panic!("{other:?}"),
                };
                assert_eq!(text, format!("{}\n\n{}", a.title, want.text));
                assert_eq!(
                    doc.fields[&FieldName::from("title")],
                    Value::Text(a.title.clone())
                );
                let chunk = doc.chunk.as_ref().unwrap();
                assert_eq!(
                    (chunk.parent.as_str(), chunk.ordinal, chunk.byte_range),
                    (a.id.as_str(), i as u32, Some(want.byte_range))
                );
                // FR-006 on the real thing: the stored text fits the window whole.
                assert!(embedder.token_count(&text).unwrap() <= 256, "{}#{i}", a.id);
            }
            checked += docs.len();
        }
        assert!(checked > 100);
        let cfg = wiki_config();
        assert_eq!(cfg.dense_fields, vec![FieldName::from("text")]);
        assert_eq!(cfg.schema.fields.len(), 2);
        assert_eq!(cfg.schema.fields[0].boost, 2.0);
    }
}
