//! US3 scenario 5 / FR-009 — every core error variant is lowered to its mirrored case with the
//! engine's message (research D4). Offline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use xtriever_core::{DocId, Error, FieldName};
use xtriever_ffi::XtrieverError;

fn lower(e: Error) -> XtrieverError {
    XtrieverError::from(e)
}

#[test]
fn every_current_core_variant_maps_to_its_mirror() {
    assert_eq!(
        lower(Error::Schema("bad".into())),
        XtrieverError::Schema {
            message: "bad".into()
        }
    );
    assert_eq!(
        lower(Error::InvalidQuery("q".into())),
        XtrieverError::InvalidQuery {
            message: "q".into()
        }
    );
    assert_eq!(
        lower(Error::UnknownField(FieldName::from("title"))),
        XtrieverError::UnknownField {
            field: "title".into()
        }
    );
    assert_eq!(
        lower(Error::DimensionMismatch {
            expected: 384,
            actual: 8
        }),
        XtrieverError::DimensionMismatch {
            expected: 384,
            actual: 8
        }
    );
    assert_eq!(
        lower(Error::NotFound(DocId(7))),
        XtrieverError::NotFound { id: 7 }
    );
    assert_eq!(
        lower(Error::Model {
            model: "m".into(),
            message: "x".into()
        }),
        XtrieverError::Model {
            model: "m".into(),
            message: "x".into()
        }
    );
    assert_eq!(
        lower(Error::Corrupt("c".into())),
        XtrieverError::Corrupt {
            message: "c".into()
        }
    );
    assert_eq!(
        lower(Error::FingerprintMismatch {
            index: "a".into(),
            current: "b".into()
        }),
        XtrieverError::FingerprintMismatch {
            index: "a".into(),
            current: "b".into()
        }
    );
    assert_eq!(
        lower(Error::BudgetExhausted("rerank stage: 5 ms".into())),
        XtrieverError::BudgetExhausted {
            message: "rerank stage: 5 ms".into()
        }
    );
    let io = lower(Error::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "gone",
    )));
    assert!(
        matches!(&io, XtrieverError::Io { message } if message.contains("gone")),
        "{io:?}"
    );
    let backend = lower(Error::backend(std::fmt::Error));
    assert!(
        matches!(&backend, XtrieverError::Backend { .. }),
        "{backend:?}"
    );
}

#[test]
fn display_carries_the_engine_message() {
    for (e, needle) in [
        (Error::Schema("needle-1".into()), "needle-1"),
        (Error::Corrupt("needle-2".into()), "needle-2"),
        (Error::BudgetExhausted("needle-3".into()), "needle-3"),
        (
            Error::Model {
                model: "needle-4".into(),
                message: "needle-5".into(),
            },
            "needle-5",
        ),
    ] {
        let lowered = lower(e);
        assert!(lowered.to_string().contains(needle), "{lowered}");
    }
}
