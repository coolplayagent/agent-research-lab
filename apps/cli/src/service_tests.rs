use super::*;

#[test]
fn serve_has_no_operator_credential_mode() {
    assert!(matches!(
        Cli::try_parse_from(["lab", "serve"]).unwrap().command,
        Commands::Serve { .. }
    ));
    assert!(Cli::try_parse_from(["lab", "serve", "--local-operator"]).is_err());
}
#[test]
fn service_state_selection_rejects_ambiguous_sources() {
    for command in ["serve", "serve-link"] {
        for args in [
            vec![
                "lab",
                "--config",
                "local.toml",
                command,
                "--state-dir",
                "/tmp/other",
            ],
            vec![
                "lab",
                command,
                "--state-dir",
                "/tmp/other",
                "--config",
                "local.toml",
            ],
        ] {
            let cli = Cli::try_parse_from(args).unwrap();
            let error = execute(cli).unwrap_err();
            assert!(error.to_string().contains("do not also pass --state-dir"));
        }
        assert!(Cli::try_parse_from(["lab", command, "--state-dir", "/tmp/other"]).is_ok());
        assert!(Cli::try_parse_from(["lab", "--config", "local.toml", command]).is_ok());
    }
}
