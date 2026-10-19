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
