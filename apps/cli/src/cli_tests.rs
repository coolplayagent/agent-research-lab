use super::*;

#[test]
fn summary_is_opt_in_and_only_available_for_status() {
    let status = Cli::try_parse_from(["lab", "status"]).unwrap();
    assert!(matches!(
        status.command,
        Commands::Status { summary: false }
    ));
    let report = Cli::try_parse_from(["lab", "--config", "local.toml", "report"]).unwrap();
    assert!(matches!(report.command, Commands::Report));
    assert_eq!(report.config, Some("local.toml".into()));
    for args in [
        ["lab", "--config", "local.toml", "status", "--summary"],
        ["lab", "status", "--summary", "--config", "local.toml"],
    ] {
        let summary = Cli::try_parse_from(args).unwrap();
        assert!(matches!(
            summary.command,
            Commands::Status { summary: true }
        ));
        assert_eq!(summary.config, Some("local.toml".into()));
    }
    assert!(Cli::try_parse_from(["lab", "report", "--summary"]).is_err());
    assert!(Cli::try_parse_from(["lab", "run", "--summary"]).is_err());
}

#[test]
fn retired_iteration_activation_commands_are_not_available() {
    for name in [
        "software-activate",
        "software-activation-reconcile",
        "software-activation-worker",
    ] {
        assert!(Cli::try_parse_from(["lab", name, "operation"]).is_err());
    }
}
