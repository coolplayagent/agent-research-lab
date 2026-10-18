use super::read_strict;
use crate::test_support::{example, setup};
use evolution::EvaluationContext;

#[test]
fn strict_decoder_rejects_nested_authority_and_unknown_state_fields() {
    let temp = setup();
    let path = temp.path().join("host/context.json");
    let mut context = example("context");
    context["candidate"]["approved"] = true.into();
    storage::write(&path, &context).unwrap();
    let error = read_strict::<EvaluationContext>(&path).unwrap_err();
    assert!(error.to_string().contains("$.candidate.approved"));
    let gate = crate::test_support::prepare_gate(temp.path(), "evaluate");
    let mut request: serde_json::Value = storage::read(&gate).unwrap();
    request["observations"][0]["variant"] = "unknown".into();
    storage::write(&gate, &request).unwrap();
    assert!(read_strict::<crate::PromotionRequest>(&gate).is_err());
}
