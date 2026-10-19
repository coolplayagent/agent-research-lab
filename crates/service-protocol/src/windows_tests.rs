use super::*;

#[test]
fn external_service_endpoints_fail_closed() {
    assert!(endpoint(Path::new("C:\\private\\service.sock")).is_err());
    assert!(Listener::bind(Path::new("C:\\private\\service.sock")).is_err());
}
