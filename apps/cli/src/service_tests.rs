use super::*;

#[test]
fn local_operator_is_an_explicit_serve_option() {
    for args in [
        vec!["lab", "serve", "--local-operator"],
        vec!["lab", "--config", "local.toml", "serve", "--local-operator"],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Serve {
                local_operator: true,
                ..
            }
        ));
    }
    assert!(matches!(
        Cli::try_parse_from(["lab", "serve"]).unwrap().command,
        Commands::Serve {
            local_operator: false,
            ..
        }
    ));
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
