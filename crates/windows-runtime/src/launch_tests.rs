use super::*;
#[test]
fn quoting_preserves_spaces_quotes_and_trailing_slashes() {
    assert_eq!(quote("a b"), "\"a b\"");
    assert_eq!(quote("x\\"), "\"x\\\\\"");
    assert_eq!(quote("x\"y"), "\"x\\\"y\"");
}
