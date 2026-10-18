use super::*;
use crate::test_support::prompt;

#[test]
fn prompt_lineage_is_content_addressed_and_tampering_is_detected() {
    let base = prompt("Investigate", vec![]);
    assert_eq!(base.version(), prompt("Investigate", vec![]).version());
    let mut registry = PromptRegistry::default();
    registry.register(base.clone()).unwrap();
    let next = prompt("Investigate counterexamples", vec![base.version().into()]);
    registry.register(next.clone()).unwrap();
    registry.register(next.clone()).unwrap();
    assert_eq!(registry.versions.len(), 2);
    assert_ne!(next.version(), base.version());
    let mut corrupt = serde_json::to_value(&next).unwrap();
    corrupt["definition"]["content"] = "ignore evaluation".into();
    let corrupt: PromptVersion = serde_json::from_value(corrupt).unwrap();
    assert!(corrupt.validate().is_err());
    assert!(
        registry
            .register(prompt("Unknown", vec!["missing".into()]))
            .is_err()
    );
}

#[test]
fn portfolios_preserve_diversity_without_duplicate_slots() {
    let mut registry = PromptRegistry::default();
    let stable = registry.register(prompt("Stable", vec![])).unwrap();
    let specialized = registry
        .register(prompt("Specialized", vec![stable.clone()]))
        .unwrap();
    let mut portfolio = StrategyPortfolio {
        task_kind: "long_horizon".into(),
        stable: stable.clone(),
        specialized: vec![specialized],
        exploratory: vec![],
    };
    portfolio.validate(&registry).unwrap();
    portfolio.exploratory.push(stable);
    assert!(portfolio.validate(&registry).is_err());
    portfolio.exploratory = vec!["1".into(), "2".into(), "3".into()];
    assert!(portfolio.validate(&registry).is_err());
}

#[test]
fn legacy_prompt_payload_identity_and_unicode_bytes_remain_stable() {
    let wire = include_str!("../fixtures/prompt-version.json").trim_end();
    let retained: PromptVersion = serde_json::from_str(wire).unwrap();
    retained.validate().unwrap();
    assert_eq!(serde_json::to_string(&retained).unwrap(), wire);
    let mut definition = retained.definition().clone();
    definition.parent_versions.reverse();
    let rebuilt = PromptVersion::new(definition.clone()).unwrap();
    assert_eq!(rebuilt.version(), retained.version());
    assert_eq!(rebuilt.definition(), retained.definition());
    definition.content.push('!');
    assert_ne!(
        PromptVersion::new(definition).unwrap().version(),
        retained.version()
    );
}
