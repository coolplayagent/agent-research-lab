use super::*;
#[test]
fn failure_output_is_bounded_and_never_returned_in_the_public_identifier() {
    let state = tempfile::tempdir().unwrap();
    storage::private_dir(state.path()).unwrap();
    let output = tempfile::NamedTempFile::new().unwrap();
    fs::write(output.path(), vec![b'x'; 80 * 1024]).unwrap();
    for index in 0..35 {
        let id = capture(state.path(), output.path(), &format!("fixture-{index}"), 1).unwrap();
        assert_eq!(id.len(), 40);
        let value: serde_json::Value =
            storage::read(&state.path().join(format!("{id}.diagnostic.json"))).unwrap();
        assert_eq!(value["stderr"].as_str().unwrap().len(), 64 * 1024);
    }
    assert_eq!(fs::read_dir(state.path()).unwrap().count(), 32);
}
