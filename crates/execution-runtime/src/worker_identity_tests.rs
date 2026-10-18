use super::*;

#[test]
fn process_identity_is_stable_for_current_process() {
    let a = process_start(std::process::id()).unwrap();
    assert_eq!(a, process_start(std::process::id()).unwrap());
}
