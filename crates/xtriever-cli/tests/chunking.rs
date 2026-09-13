//! `wiki::chunking` — an article becomes `SourceDocument`s whose `text` field starts with the
//! title line, whose ids are `page#ordinal`, and whose passages equal the set-B goldens.
//! Model-backed (the cost is the embedder's tokenizer).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use xtriever_cli::wiki::chunking::{Article, documents_for, wiki_config};
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
