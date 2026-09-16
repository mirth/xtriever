"""The snapshot manifest's exclusion rules — `crates/xtriever-cli/src/wiki/rules.rs` in
Python (research D6): applied in order, the first match wins and is the rule counted.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Rule:
    kind: str  # "title_suffix" | "lead_contains"
    value: str
    within_chars: int | None = None


def parse_rules(exclusions: list[dict]) -> list[Rule]:
    rules = []
    for e in exclusions:
        if e["rule"] == "title_suffix":
            rules.append(Rule("title_suffix", e["value"]))
        elif e["rule"] == "lead_contains":
            rules.append(Rule("lead_contains", e["value"], int(e["within_chars"])))
        else:
            raise ValueError(f"unknown exclusion rule {e['rule']!r}")
    return rules


def excluded_by(rules: list[Rule], title: str, text: str) -> int | None:
    """The index of the first rule the article matches, or None.

    `lead_contains` counts if the phrase *starts* inside the first `within_chars` characters
    (characters, not bytes — Python string offsets are already characters)."""
    for i, rule in enumerate(rules):
        if rule.kind == "title_suffix":
            if title.endswith(rule.value):
                return i
        else:
            at = text.find(rule.value)
            if at != -1 and at < rule.within_chars:
                return i
    return None


def rule_name(rule: Rule) -> str:
    """The name the Rust build counts under (`build.rs`)."""
    if rule.kind == "title_suffix":
        return f"title_suffix:{rule.value}"
    return f"lead_contains:{rule.value}:{rule.within_chars}"
