//! What a GGUF header declares, read with the pinned engine's own reader (Feature 026,
//! research D5): the metadata the pins are asserted against and the tensor table.
//!
//! This file is byte-identical in `xtriever-dense` and `xtriever-rerank` — the downward
//! dependency rule (Principle V) gives the two stage crates no shared home below `core`, and
//! `core` stays engine-free — and `tests/twins.rs` in the dense crate fails if the two copies
//! ever differ, so a fix lands in both or in neither.

use candle_core::quantized::GgmlDType;
use candle_core::quantized::gguf_file::{Content, Value};

use crate::error::model_err;

pub(crate) struct GgufHeader {
    content: Content,
}

impl GgufHeader {
    pub fn read(bytes: &[u8]) -> xtriever_core::Result<Self> {
        let mut cursor = std::io::Cursor::new(bytes);
        let content = Content::read(&mut cursor)
            .map_err(|e| model_err(format!("not a GGUF file this engine can read: {e}")))?;
        Ok(Self { content })
    }

    pub fn string(&self, key: &str) -> xtriever_core::Result<&str> {
        self.content
            .metadata
            .get(key)
            .ok_or_else(|| model_err(format!("GGUF header declares no {key}")))?
            .to_string()
            .map(String::as_str)
            .map_err(|e| model_err(format!("GGUF header {key}: {e}")))
    }

    pub fn number(&self, key: &str) -> xtriever_core::Result<usize> {
        let value = self
            .content
            .metadata
            .get(key)
            .ok_or_else(|| model_err(format!("GGUF header declares no {key}")))?;
        let n: u64 = match value {
            Value::U8(n) => u64::from(*n),
            Value::U16(n) => u64::from(*n),
            Value::U32(n) => u64::from(*n),
            Value::U64(n) => *n,
            Value::I8(n) if *n >= 0 => *n as u64,
            Value::I16(n) if *n >= 0 => *n as u64,
            Value::I32(n) if *n >= 0 => *n as u64,
            Value::I64(n) if *n >= 0 => *n as u64,
            other => {
                return Err(model_err(format!(
                    "GGUF header {key} is {other:?}, not a non-negative integer"
                )));
            }
        };
        usize::try_from(n).map_err(|_| model_err(format!("GGUF header {key} = {n} does not fit")))
    }

    /// `key` must equal `expected`; the message names the field and both values.
    pub fn expect_number(&self, key: &str, expected: usize) -> xtriever_core::Result<()> {
        let actual = self.number(key)?;
        if actual == expected {
            Ok(())
        } else {
            let field = key.rsplit('.').next().unwrap_or(key);
            Err(model_err(format!(
                "GGUF header {field} is {actual}, expected {expected} ({key})"
            )))
        }
    }

    /// The layer-norm epsilon the file declares, if it declares one; an error if it declares
    /// something that is not a float. The loader asserts it against the pinned configuration.
    pub fn layer_norm_epsilon(&self, architecture: &str) -> xtriever_core::Result<Option<f32>> {
        let key = format!("{architecture}.attention.layer_norm_epsilon");
        match self.content.metadata.get(&key) {
            None => Ok(None),
            Some(value) => value
                .to_f32()
                .map(Some)
                .map_err(|e| model_err(format!("GGUF header {key}: {e}"))),
        }
    }

    pub fn quantised_tensors(&self) -> usize {
        self.content
            .tensor_infos
            .values()
            .filter(|t| t.ggml_dtype == GgmlDType::Q8_0)
            .count()
    }

    // Unused in the dense crate — no tensor its pin requires by name — and used by the
    // re-ranker for its classification head; the two files are identical by test.
    #[allow(dead_code)]
    pub fn has_tensor(&self, name: &str) -> bool {
        self.content.tensor_infos.contains_key(name)
    }
}
