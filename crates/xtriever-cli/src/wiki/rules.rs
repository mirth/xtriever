//! Exclusion rules from the manifest, applied in order — first match wins and is the rule
//! counted (spec FR-003; research D3).

use serde::{Deserialize, Serialize};

/// One exclusion rule, as data.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum Rule {
    /// Exclude when the title ends with the value.
    TitleSuffix {
        /// The suffix.
        value: String,
    },
    /// Exclude when the value occurs within the first `within_chars` characters of the body.
    LeadContains {
        /// The phrase.
        value: String,
        /// The window, in characters (not bytes).
        within_chars: usize,
    },
}

/// The index of the first rule `title`/`text` match, if any.
#[must_use]
pub fn excluded_by(rules: &[Rule], title: &str, text: &str) -> Option<usize> {
    rules.iter().position(|rule| match rule {
        Rule::TitleSuffix { value } => title.ends_with(value.as_str()),
        Rule::LeadContains {
            value,
            within_chars,
        } => {
            // The phrase counts if it *starts* inside the window.
            let window_end = text
                .char_indices()
                .nth(*within_chars)
                .map_or(text.len(), |(i, _)| i);
            text.find(value.as_str()).is_some_and(|at| at < window_end)
        }
    })
}
