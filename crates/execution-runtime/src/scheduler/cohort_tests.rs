use crate::tests::cohort_tests::{add, events, fixture, probe, run_selected};

#[test]
fn sandbox_candidate_waits_for_selected_ordinary_peers_without_blocking_later_peers() {
    let (_temp, mut config) = fixture();
    config.max_agents = 4;
    probe(&mut config, "ready", "exit 0");
    probe(&mut config, "repo-sandbox", "exit 0");
    add(&config, "a-ordinary", "ready", false);
    add(&config, "b-ordinary", "ready", false);
    add(&config, "c-sandbox", "repo-sandbox", false);
    add(&config, "d-ordinary", "ready", false);
    run_selected(
        &config,
        &["a-ordinary", "b-ordinary", "c-sandbox", "d-ordinary"],
        20,
    );
    assert_eq!(
        events(&config),
        [
            "probe:ready",
            "probe:ready",
            "probe:ready",
            "acquire:a-ordinary-attempt-1",
            "acquire:b-ordinary-attempt-1",
            "acquire:d-ordinary-attempt-1",
            "probe:repo-sandbox",
            "acquire:c-sandbox-attempt-1",
        ]
    );
}

#[test]
fn first_sandbox_candidate_allows_ordinary_peers_after_its_launch_failure() {
    let (_temp, mut config) = fixture();
    probe(&mut config, "ready", "exit 0");
    probe(&mut config, "repo-sandbox", "exit 0");
    add(&config, "a-sandbox", "repo-sandbox", false);
    add(&config, "b-ordinary", "ready", false);
    run_selected(&config, &["a-sandbox", "b-ordinary"], 20);
    assert_eq!(
        events(&config),
        [
            "probe:repo-sandbox",
            "probe:ready",
            "acquire:a-sandbox-attempt-1",
            "acquire:b-ordinary-attempt-1",
        ]
    );
}
