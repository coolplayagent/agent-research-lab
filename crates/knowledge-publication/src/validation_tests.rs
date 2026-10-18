use super::*;
use crate::{report::render_report, test_support::report};

#[test]
fn citations_and_publication_boundary_are_validated() {
    let root = tempfile::tempdir().unwrap();
    let mut report = report();
    validate_report(&report, root.path()).unwrap();
    assert!(render_report(&report).contains("[^reference]"));
    report.findings[0].source_ids = vec!["missing".into()];
    assert!(validate_report(&report, root.path()).is_err());
    report.findings[0].source_ids = vec!["reference".into()];
    report.publication_reviewed = false;
    assert!(validate_report(&report, root.path()).is_err());
    report.publication_reviewed = true;
    report.summary = "private_holdout: secret".into();
    assert!(validate_report(&report, root.path()).is_err());
}

#[test]
fn source_archive_checksum_and_confinement_are_enforced() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("sources")).unwrap();
    fs::write(root.path().join("sources/paper.md"), "Public paper").unwrap();
    let mut report = report();
    report.sources[0].resource = "sources/paper.md".into();
    report.sources[0].sha256 = Some(file_sha256(&root.path().join("sources/paper.md")).unwrap());
    validate_report(&report, root.path()).unwrap();
    report.sources[0].resource = "sources/../private.md".into();
    assert!(validate_report(&report, root.path()).is_err());
}

#[test]
fn downloaded_sources_require_an_archive_with_matching_reviewed_bytes() {
    let root = tempfile::tempdir().unwrap();
    let mut report = report();
    report.sources[0].access_status = SourceAccessStatus::DownloadedVerified;
    assert!(
        validate_report(&report, root.path())
            .unwrap_err()
            .to_string()
            .contains("local archive")
    );
    fs::create_dir(root.path().join("sources")).unwrap();
    let path = root.path().join("sources/paper.md");
    fs::write(&path, "Reviewed public archive").unwrap();
    report.sources[0].resource = "sources/paper.md".into();
    report.sources[0].sha256 = Some(file_sha256(&path).unwrap());
    validate_report(&report, root.path()).unwrap();
    fs::write(&path, "Unreviewed replacement").unwrap();
    assert!(
        validate_report(&report, root.path())
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch")
    );
}
