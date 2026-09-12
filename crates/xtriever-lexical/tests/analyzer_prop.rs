//! Invariant: analyzer determinism (Principle II). The same text indexed into two fresh indexes
//! must yield identical scores — nothing about tokenization may depend on process state.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use proptest::prelude::*;
use support::TestIndex;
use xtriever_core::{
    AnalyzerId, DocId, Document, FieldDef, FieldKind, LexicalIndex, LexicalQuery, Schema, Value,
};

fn schema(analyzer: &str) -> Schema {
    Schema {
        fields: vec![FieldDef {
            name: "body".into(),
            kind: FieldKind::Text(AnalyzerId(analyzer.into())),
            indexed: true,
            stored: false,
            boost: 1.0,
        }],
    }
}

fn index_text(analyzer: &str, texts: &[String]) -> TestIndex {
    let mut t = TestIndex::create(schema(analyzer));
    let docs: Vec<Document> = texts
        .iter()
        .enumerate()
        .map(|(i, s)| Document {
            id: DocId(i as u32),
            fields: [("body".into(), Value::Text(s.clone()))]
                .into_iter()
                .collect(),
            chunk: None,
        })
        .collect();
    t.index.add(&docs).unwrap();
    t.index.commit().unwrap();
    t
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, .. ProptestConfig::default() })]

    #[test]
    fn same_text_same_scores_in_fresh_indexes(
        texts in prop::collection::vec("[ -~]{0,120}", 1..6),
        analyzer in prop::sample::select(vec!["standard", "standard_en"]),
    ) {
        let a = index_text(analyzer, &texts);
        let b = index_text(analyzer, &texts);
        for t in &texts {
            let q = LexicalQuery::Match(Some("body".into()), t.clone());
            let ha = a.index.search(&q, None, 100).unwrap();
            let hb = b.index.search(&q, None, 100).unwrap();
            prop_assert_eq!(ha.len(), hb.len());
            for (x, y) in ha.iter().zip(&hb) {
                prop_assert_eq!(x.id, y.id);
                prop_assert_eq!(x.score.to_bits(), y.score.to_bits());
            }
            let sa = a.index.stats().unwrap();
            let sb = b.index.stats().unwrap();
            prop_assert_eq!(sa, sb);
        }
    }
}
