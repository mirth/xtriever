"""The exclusion rules are `crates/xtriever-cli/src/wiki/rules.rs` (research D6): applied in
manifest order, first match wins, `lead_contains` measured in characters."""

from wikidemo.rules import Rule, excluded_by, parse_rules, rule_name

MANIFEST_RULES = [
    {"rule": "title_suffix", "value": " (disambiguation)"},
    {"rule": "lead_contains", "value": "may refer to", "within_chars": 300},
    {"rule": "lead_contains", "value": "may mean", "within_chars": 300},
]


def test_parse_and_names():
    rules = parse_rules(MANIFEST_RULES)
    assert rules == [
        Rule("title_suffix", " (disambiguation)"),
        Rule("lead_contains", "may refer to", 300),
        Rule("lead_contains", "may mean", 300),
    ]
    assert [rule_name(r) for r in rules] == [
        "title_suffix: (disambiguation)",
        "lead_contains:may refer to:300",
        "lead_contains:may mean:300",
    ]


def test_title_suffix():
    rules = parse_rules(MANIFEST_RULES)
    assert excluded_by(rules, "Mercury (disambiguation)", "Mercury may refer to…") == 0  # first match wins
    assert excluded_by(rules, "Mercury", "The planet.") is None


def test_lead_contains_counts_characters_and_the_phrase_start():
    rules = parse_rules(MANIFEST_RULES)
    assert excluded_by(rules, "X", "x" * 10 + "may refer to") == 1
    assert excluded_by(rules, "X", "x" * 299 + "may refer to") == 1  # starts inside the window
    assert excluded_by(rules, "X", "x" * 300 + "may refer to") is None  # starts at the window end
    assert excluded_by(rules, "X", "é" * 299 + "may mean") == 2  # characters, not bytes
    assert excluded_by(rules, "X", "é" * 300 + "may mean") is None
    assert excluded_by(rules, "X", "short") is None
    assert excluded_by(rules, "X", "") is None
