use super::*;
use crate::{preparation::prepare_from, report::deferred, test_support::fixture};
use std::os::unix::fs::MetadataExt;

#[test]
fn resuming_verifies_existing_files_and_keeps_them_in_the_total_budget() {
    let (_temp, worktree, logs, cargo, cache, _) = fixture();
    let mut seed = Seeder {
        started: Instant::now(),
        report: SeedReport::default(),
    };
    let archive = cargo.join("registry/cache/index.crates.io-fixture/example-1.0.0.crate");
    let destination =
        logs.join("home/.cargo/registry/cache/index.crates.io-fixture/example-1.0.0.crate");
    let checksum = storage::digest(b"public archive");
    seed.copy(&archive, &destination, Some(&checksum), false)
        .unwrap();
    let inode = fs::metadata(&destination).unwrap().ino();
    seed.started = Instant::now() - Duration::from_secs(56);
    assert!(deferred(
        &seed
            .copy(&archive, &destination, Some(&checksum), false)
            .unwrap_err()
    ));
    let report = prepare_from(&worktree, &logs, Some(&cargo), Some(&cache)).unwrap();
    assert_eq!(report.reused_files, 1);
    assert_eq!(report.retained_files, 3);
    assert_eq!(fs::metadata(&destination).unwrap().ino(), inode);
    seed.started = Instant::now();
    seed.report.retained_bytes = MAX_BYTES;
    assert!(
        seed.copy(&archive, &destination, Some(&checksum), false)
            .unwrap_err()
            .to_string()
            .contains("budget")
    );
    seed.report.retained_bytes = 0;
    seed.report.retained_files = MAX_FILES;
    assert!(
        seed.copy(&archive, &destination, Some(&checksum), false)
            .unwrap_err()
            .to_string()
            .contains("budget")
    );
    fs::write(&destination, "damaged private cache").unwrap();
    let report = prepare_from(&worktree, &logs, Some(&cargo), Some(&cache)).unwrap();
    assert!(report.copied_files > 0);
    assert_eq!(fs::read(&destination).unwrap(), b"public archive");
}

#[test]
fn failed_checksum_preserves_previous_copy_and_cleans_temporary_files() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("private/archive");
    checked_directory(destination.parent().unwrap(), true).unwrap();
    fs::write(&source, b"wrong new bytes").unwrap();
    fs::write(&destination, b"previous private bytes").unwrap();
    let mut seed = Seeder::new(Instant::now(), SeedReport::default());
    assert!(
        seed.copy(
            &source,
            &destination,
            Some(&storage::digest(b"reviewed bytes")),
            false
        )
        .unwrap_err()
        .to_string()
        .contains("checksum mismatch")
    );
    assert_eq!(fs::read(&destination).unwrap(), b"previous private bytes");
    assert_eq!(fs::read(&source).unwrap(), b"wrong new bytes");
    assert_eq!(seed.report.copied_files, 0);
    assert_eq!(
        fs::read_dir(destination.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn private_copy_modes_are_selected_without_inheriting_host_permissions() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("host-binary");
    fs::write(&source, b"public binary").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o755)).unwrap();
    let mut seed = Seeder::new(Instant::now(), SeedReport::default());
    for (name, executable, mode) in [("archive", false, 0o600), ("binary", true, 0o700)] {
        let destination = temp.path().join("private").join(name);
        seed.copy(
            &source,
            &destination,
            Some(&storage::digest(b"public binary")),
            executable,
        )
        .unwrap();
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            mode
        );
        assert_ne!(
            fs::metadata(&source).unwrap().ino(),
            fs::metadata(&destination).unwrap().ino()
        );
    }
    assert_eq!(
        fs::metadata(&source).unwrap().permissions().mode() & 0o777,
        0o755
    );
}
