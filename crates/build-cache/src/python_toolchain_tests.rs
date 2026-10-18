use super::*;

#[test]
fn python_metadata_selects_only_the_default_exact_host_runtime() {
    let module = "python.defaults(\n python_version = \"3.11\",\n)";
    let expected = "a".repeat(64);
    let other = "b".repeat(64);
    let versions = format!(
        r#"TOOL_VERSIONS = {{
    "3.11.14": {{
        "sha256": {{
            "x86_64-unknown-linux-gnu": "{expected}",
            "aarch64-unknown-linux-gnu": "{other}",
        }},
    }},
    "3.12.1": {{
        "sha256": {{
            "x86_64-unknown-linux-gnu": "{other}",
        }},
    }},
}}
MINOR_MAPPING = {{
    "3.11": "3.11.14",
    "3.12": "3.12.1",
}}
"#
    );
    assert_eq!(
        python_default_hash(module, &versions, "x86_64-unknown-linux-gnu").unwrap(),
        expected
    );
    assert!(python_default_hash(module, &versions, "unknown-platform").is_err());
    assert!(
        python_default_hash(
            "python.defaults(python_version = dynamic())",
            &versions,
            "x86_64-unknown-linux-gnu"
        )
        .is_err()
    );
}
