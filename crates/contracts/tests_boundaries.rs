//! Bounded source checks complement type checks and architectural review.
use std::path::PathBuf;
fn has_han(text: &str) -> bool {
    text.chars().any(|c| matches!(c as u32,
        0x2E80..=0x2EFF | 0x2F00..=0x2FDF | 0x3005..=0x3007 | 0x3021..=0x3029 |
        0x3038..=0x303B | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF |
        0x20000..=0x323AF))
}
#[test]
fn authored_source_respects_presentation_and_state_boundaries() {
    let root = PathBuf::from(std::env::var("TEST_SRCDIR").unwrap())
        .join(std::env::var("TEST_WORKSPACE").unwrap());
    let inventory = include_str!("../../source_inventory.txt");
    let mut count = 0;
    let mut violations = Vec::new();
    for path in inventory.lines() {
        let text = std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
        count += 1;
        let locale = path.contains("/locales/");
        if !locale && has_han(&text) {
            violations.push(format!("{path}: presentation text must be in locale resources or test fixtures"));
        }
        if path.ends_with(".rs") && !path.starts_with("crates/contracts/") {
            for (index, line) in text.lines().enumerate() {
                if let Some(name) = line.split("enum ").nth(1).and_then(|s| s.split_whitespace().next())
                    && (name.ends_with("State") || name.ends_with("Status") || name == "Presence") {
                        violations.push(format!("{path}:{}: state domain belongs in contracts", index + 1));
                    }
            }
        }
        if path.ends_with(".ts") && !locale && !path.ends_with("/contracts.ts") {
            for (index, line) in text.lines().enumerate() {
                for member in [".state", ".status", ".presence"] {
                    for suffix in line.split(member).skip(1) {
                        let suffix = suffix.trim_start();
                        if ["=== \"", "!== \"", "== \"", "!= \"", "=== '", "!== '"].iter().any(|op| suffix.starts_with(op)) {
                            violations.push(format!("{path}:{}: compare against shared state constants", index + 1));
                        }
                    }
                }
            }
        }
    }
    assert!(count > 100, "source inventory is incomplete: {count}");
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}
#[test]
fn boundary_checker_detects_han_and_allows_unicode_data() {
    assert!(has_han("\u{4e2d}"));
    assert!(has_han("\u{20000}"));
    assert!(!has_han("English, cafe, \u{00e9}, emoji \u{1f30e}"));
}
