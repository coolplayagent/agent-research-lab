//! Durable JSON and host identity compatibility with the actual pre-split library.
//! Fixtures are synthetic; their outputs were captured from commit abe6720, not
//! derived from the crate implementation under test. See fixture provenance.
use agent_research_lab::{
    communication::HostMember,
    config::Config,
    multi_agent::{TeamBinding, TeamMembership},
    runtime::{Job, Launch, Task},
    storage,
};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

fn digests() -> BTreeMap<String, String> {
    serde_json::from_str(include_str!("fixtures/serde-abe6720/digests.json")).unwrap()
}

fn snapshot<T: DeserializeOwned + Serialize>(name: &str, input: &str, expected: &[u8]) -> T {
    let actual: T = serde_json::from_str(input).unwrap();
    let bytes = serde_json::to_vec(&actual).unwrap();
    assert_eq!(
        bytes, expected,
        "{name}: field order, defaults, omission or JSON representation changed"
    );
    assert_eq!(storage::digest(&bytes), digests()[name], "{name} digest");
    // Persisted old output must decode and round-trip, not merely new input.
    let retained: T = serde_json::from_slice(expected).unwrap();
    assert_eq!(serde_json::to_vec(&retained).unwrap(), expected);
    actual
}

macro_rules! case {
    ($name:literal, $ty:ty) => {
        snapshot::<$ty>(
            $name,
            include_str!(concat!("fixtures/serde-abe6720/", $name, ".input.json")),
            include_bytes!(concat!(
                "fixtures/serde-abe6720/",
                $name,
                ".serialized.json"
            )),
        )
    };
}

#[test]
fn historical_omissions_and_unknown_job_metadata_match_pre_split_bytes() {
    let config = case!("legacy-config", Config);
    config.validate().unwrap();
    case!("legacy-task", Task);
    case!("legacy-job", Job);
    case!("legacy-launch", Launch);
}

#[test]
fn backend_team_and_full_launch_match_pre_split_bytes() {
    let config = case!("full-config", Config);
    config.validate().unwrap();
    case!("full-task", Task);
    case!("full-job", Job);
    case!("full-launch", Launch);
}

fn identity(prefix: &str, team_input: &str, job_input: &str, expected: &[u8]) {
    let team: TeamBinding = serde_json::from_str(team_input).unwrap();
    let job: Job = serde_json::from_str(job_input).unwrap();
    let member = team.member(&job).unwrap();
    let baseline: HostMember = serde_json::from_slice(expected).unwrap();
    let expected_digests = digests();
    assert_eq!(member, baseline, "{prefix} member binding changed");
    assert_eq!(serde_json::to_vec(&member).unwrap(), expected);
    assert_eq!(
        storage::digest(expected),
        expected_digests[&format!("{prefix}-member")]
    );
    assert_eq!(
        member.cohort_id,
        expected_digests[&format!("{prefix}-cohort-id")]
    );
    assert_eq!(
        member.authority_sha256,
        expected_digests[&format!("{prefix}-authority")]
    );
    assert_eq!(
        job.config_digest,
        expected_digests[&format!("{prefix}-config")]
    );
    assert_eq!(
        storage::digest(&serde_json::to_vec(&job.task).unwrap()),
        expected_digests[&format!("{prefix}-task")]
    );
}

#[test]
fn frozen_cohort_and_member_identities_match_actual_pre_split_library() {
    identity(
        "legacy",
        include_str!("fixtures/serde-abe6720/legacy-team.input.json"),
        include_str!("fixtures/serde-abe6720/legacy-job.input.json"),
        include_bytes!("fixtures/serde-abe6720/legacy-member.serialized.json"),
    );
    identity(
        "full",
        include_str!("fixtures/serde-abe6720/full-team.input.json"),
        include_str!("fixtures/serde-abe6720/full-job.input.json"),
        include_bytes!("fixtures/serde-abe6720/full-member.serialized.json"),
    );
}
