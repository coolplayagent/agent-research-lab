use super::*;

#[test]
fn automatic_candidate_excludes_secrets_and_formal_gates() {
    for path in [
        ".env",
        "nested/auth.json",
        ".github/workflows/ci.yml",
        "qualitygate.yaml",
        "qualitygate/rules/custom.yaml",
        "qualitygate/reviews/source.md",
        "private/answers.json",
        "cert.pem",
    ] {
        assert!(forbidden_candidate_path(path), "{path}");
    }
    assert!(!forbidden_candidate_path("src/parser.rs"));
}
