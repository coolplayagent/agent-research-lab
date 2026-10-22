use super::*;

#[test]
fn codex_login_failure_survives_service_hops_without_exposing_private_details() {
    for details in [
        "codex_login::auth::manager: status=401 error_code: Some(\"refresh_token_reused\")",
        "HTTP 401: {\"code\":\"token_expired\"}",
    ] {
        let failure: anyhow::Error = rpc::PrivateDiagnostic(details.into()).into();
        let sandbox = rpc::Reply::from_result(Err(failure));
        let executor = rpc::Reply::from_result(sandbox.decode::<serde_json::Value>());
        let error = executor.decode::<serde_json::Value>().unwrap_err();
        let code = execution_failure_code(CodingAgent::Codex, &error);
        assert_eq!(code, Code::AuthenticationRequired);
        assert_eq!(
            serde_json::to_string(&code).unwrap(),
            "\"authentication_required\""
        );
        assert!(!error.to_string().contains(details));
        assert_eq!(rpc::private_details(&error).unwrap(), details);
        assert_eq!(
            execution_failure_code(CodingAgent::Custom, &error),
            Code::VerificationFailed
        );
    }
}

#[test]
fn unknown_execution_errors_stay_failed_without_guessing_authentication() {
    for details in [
        "HTTP 401: unknown provider failure",
        "codex_login::auth::manager: network unavailable",
        "task could not write the challenge file",
    ] {
        let error: anyhow::Error = rpc::PrivateDiagnostic(details.into()).into();
        assert_eq!(
            execution_failure_code(CodingAgent::Codex, &error),
            Code::VerificationFailed
        );
    }
    let public_message = anyhow::anyhow!("401 token_expired");
    assert_eq!(
        execution_failure_code(CodingAgent::Codex, &public_message),
        Code::VerificationFailed
    );
}
