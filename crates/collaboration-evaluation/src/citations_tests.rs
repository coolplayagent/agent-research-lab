use super::*;
use crate::tests::fixture;

#[test]
fn source_checks_require_exact_commit_digest_lines_and_quote() {
    let (_temp, _c, plan, citation) = fixture();
    check_citation(&plan, &citation).unwrap();
    for altered in [
        Citation {
            path: "../private/answers".into(),
            ..citation.clone()
        },
        Citation {
            commit: "b".repeat(40),
            ..citation.clone()
        },
        Citation {
            sha256: "c".repeat(64),
            ..citation.clone()
        },
        Citation {
            end_line: 999,
            ..citation.clone()
        },
        Citation {
            quote: "invented support".into(),
            ..citation
        },
    ] {
        assert!(check_citation(&plan, &altered).is_err());
    }
}
