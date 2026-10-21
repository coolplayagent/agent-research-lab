use super::*;
#[test]
fn explicit_program_requires_absolute_existing_file() {
    assert!(discover(CodingAgent::Codex, Some(Path::new("relative.exe"))).is_err());
    assert!(discover(CodingAgent::Codex, Some(Path::new("/missing/program"))).is_err());
}
#[test]
fn executable_identity_includes_script_content() {
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("node.exe");
    let script = temp.path().join("cli.js");
    fs::write(&executable, b"runtime").unwrap();
    fs::write(&script, b"first").unwrap();
    let args = vec![script.to_str().unwrap().into()];
    let first = executable_digest(&executable, &args).unwrap();
    fs::write(script, b"second").unwrap();
    assert_ne!(first, executable_digest(&executable, &args).unwrap());
}

#[test]
fn codex_companion_is_pinned_and_removal_changes_identity() {
    let root = tempfile::tempdir().unwrap();
    let program = root.path().join("codex");
    let host = root.path().join("codex-code-mode-host");
    fs::write(&program, b"runtime").unwrap();
    let alone = executable_digest(&program, &[]).unwrap();
    fs::write(&host, b"host-one").unwrap();
    let first = executable_digest(&program, &[]).unwrap();
    assert_ne!(alone, first);
    fs::write(&host, b"host-two").unwrap();
    assert_ne!(first, executable_digest(&program, &[]).unwrap());
    fs::remove_file(host).unwrap();
    assert_eq!(alone, executable_digest(&program, &[]).unwrap());
}
