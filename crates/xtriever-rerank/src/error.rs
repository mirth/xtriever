//! Mapping this crate's failures onto `xtriever_core::Error`; no variant is added.

use xtriever_core::Error;

use crate::model::MODEL_NAME;

/// A model file, assertion, construction or inference failure (spec FR-003, FR-008).
pub(crate) fn model_err(message: impl Into<String>) -> Error {
    Error::Model {
        model: MODEL_NAME.to_owned(),
        message: message.into(),
    }
}
