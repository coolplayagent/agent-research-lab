use super::*;

#[test]
fn runtime_catalogs_fit_without_losing_individual_or_aggregate_limits() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("catalog.json");
    let catalog = fs::File::create(&file).unwrap();
    catalog.set_len(41_304_532).unwrap();
    let mut bytes = 256 * 1024 * 1024;
    let mut files = 10000;
    budget(root.path(), &mut bytes, &mut files, 0, &|| Ok(())).unwrap();
    assert_eq!(bytes, 256 * 1024 * 1024 - 41_304_532);
    assert!(budget(root.path(), &mut 16_000_000, &mut 10000, 0, &|| Ok(())).is_err());
    catalog.set_len(crate::MAX_RUNTIME_FILE + 1).unwrap();
    assert!(
        budget(
            root.path(),
            &mut (256 * 1024 * 1024),
            &mut 10000,
            0,
            &|| Ok(())
        )
        .is_err()
    );
}
