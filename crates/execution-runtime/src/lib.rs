//! Optional research runtime; durable jobs and worker orchestration have explicit owners.
pub mod admission;
pub mod batch;
mod candidate;
mod completion;
mod followups;
mod git;
mod host_budget;
mod job_store;
mod jobs;
mod launch;
mod memory;
pub mod people;
mod post_success;
mod prompts;
mod receipts;
mod recovery;
mod registration;
mod scheduler;
mod scope;
mod seeding;
mod status;
mod team_channel;
mod watch;
mod worker_identity;

pub use contracts::{FollowupAdmission, MemoryWriteback};
pub use host_budget::pause;
pub use jobs::{enqueue, enqueue_with_inputs, retry};
pub use post_success::validate_state;
pub use prompts::{rendered_experiment_prompt, rendered_job_prompt};
pub use receipts::completed_result;
pub use scheduler::{
    ObservedRun, run, run_scoped, run_scoped_with_admission, run_scoped_with_observations,
    run_seeded,
};
pub use seeding::seed;
pub use status::{doctor, status, status_for_jobs, status_summary};
pub use task::{Job, Launch, Task};

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
