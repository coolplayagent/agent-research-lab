//! Reconcile recorded CLI executions into generic, append-only version metadata.
use super::*;
use collaboration_model::{
    AssessmentInput, CollaborationSubject, CollaborationTopic, EvidencePins, EvolutionInput, Goal,
    ResearchChange,
};
use contracts::{SubjectKind, TopicState};

fn identity(prefix: &str, parts: &[&str]) -> String {
    format!(
        "{prefix}-{}",
        &storage::digest(&serde_json::to_vec(parts).expect("string parts serialize"))[..32]
    )
}

fn pin(value: &str) -> Option<String> {
    ([40, 64].contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_owned())
}

fn evidence_pins(value: &Value) -> Option<EvidencePins> {
    Some(EvidencePins {
        source: Some(pin(value["source_commit"].as_str()?)?),
        prompt: Some(pin(value["prompt_digest"].as_str()?)?),
        policy: Some(pin(value["policy_digest"].as_str()?)?),
        superpod: Some(pin(value["superpod_commit"].as_str()?)?),
    })
}

/// Missing or inconsistent history cannot become an assessed version reference.
fn pins_for_goal(c: &Config, goal: &Goal) -> Option<BTreeMap<String, EvidencePins>> {
    let repository = goal.input.parameters["repository"].as_str()?;
    if goal.input.scenario != "research" || goal.work.is_empty() {
        return None;
    }
    let mut pins = BTreeMap::new();
    for work in &goal.work {
        let expected = super::research_scenario::job_id(goal, work);
        let path = c.state_dir.join("jobs").join(format!("{expected}.json"));
        let bound = if path.exists() {
            let job = read_job(&path).ok()?;
            if job.task.id != expected
                || job.task.persona_id.as_deref() != Some(work.person_id.as_str())
                || job.task.repository != repository
                || job.task.role != "research"
                || job.attempt != 1
                || job.run_id != format!("{expected}-attempt-1")
            {
                return None;
            }
            EvidencePins {
                source: Some(pin(&job.source_commit)?),
                prompt: Some(pin(&job.prompt_digest)?),
                policy: Some(pin(&job.config_digest)?),
                superpod: Some(pin(&job.superpod_commit)?),
            }
        } else {
            let result = work.result.as_ref()?;
            if result.evidence["job_id"].as_str() != Some(expected.as_str()) {
                return None;
            }
            evidence_pins(&result.evidence)?
        };
        if pins
            .values()
            .next()
            .is_some_and(|first: &EvidencePins| first.source != bound.source)
        {
            return None;
        }
        pins.insert(work.id.clone(), bound);
    }
    Some(pins)
}

async fn topic(
    hub: &collaboration_client::Client,
    goal: &Goal,
) -> Result<Option<CollaborationTopic>> {
    let id = identity("research-topic", &[&goal.input.group_id]);
    let mut after = String::new();
    let mut count = 0;
    loop {
        let page = hub.topics(&after, "").await?;
        let rows = page["topics"]
            .as_array()
            .context("invalid research topics page")?;
        for row in rows {
            let existing: CollaborationTopic = serde_json::from_value(row["topic"].clone())?;
            if existing.id == id {
                ensure!(
                    existing.group_id == goal.input.group_id,
                    "research topic identity conflict"
                );
                return Ok((existing.state == TopicState::Active).then_some(existing));
            }
        }
        count += rows.len();
        match page["next_after"].as_str() {
            Some(next) => {
                ensure!(
                    next != after && count < 2048,
                    "research topic pagination exceeds bound"
                );
                after = next.into();
            }
            None => break,
        }
    }
    let group = hub.group(&goal.input.group_id).await?;
    if group.archived {
        return Ok(None);
    }
    let topic = CollaborationTopic {
        id,
        group_id: goal.input.group_id.clone(),
        title: "Research versions".into(),
        objective: "Source-bound version evaluations from recorded research goals".into(),
        state: TopicState::Active,
        revision: 0,
    };
    Ok(Some(serde_json::from_value(
        hub.research_change(ResearchChange::Topic { topic }).await?,
    )?))
}

async fn subject(
    hub: &collaboration_client::Client,
    topic: &CollaborationTopic,
    repository: &str,
) -> Result<CollaborationSubject> {
    let id = identity("research-subject", &[&topic.id, repository]);
    let mut after = String::new();
    let mut count = 0;
    loop {
        let page = hub.subjects(&topic.id, &after).await?;
        let rows = page["subjects"]
            .as_array()
            .context("invalid research subjects page")?;
        for row in rows {
            let existing: CollaborationSubject = serde_json::from_value(row["subject"].clone())?;
            if existing.id == id {
                ensure!(
                    existing.topic_id == topic.id,
                    "research subject identity conflict"
                );
                return Ok(existing);
            }
        }
        count += rows.len();
        match page["next_after"].as_str() {
            Some(next) => {
                ensure!(
                    next != after && count < 8192,
                    "research subject pagination exceeds bound"
                );
                after = next.into();
            }
            None => break,
        }
    }
    let subject = CollaborationSubject {
        id,
        topic_id: topic.id.clone(),
        name: repository.to_owned(),
        kind: SubjectKind::Repository,
        reference: format!("repository:{repository}"),
        revision: 0,
    };
    Ok(serde_json::from_value(
        hub.research_change(ResearchChange::Subject { subject })
            .await?,
    )?)
}

/// One storage page per pass, with a host cursor. No service opens the storage database.
pub(super) async fn sync(hub: &collaboration_client::Client, c: &Config) -> Result<usize> {
    let cursor_path = c.state_dir.join("private/research-lineage-cursor.json");
    let after: String = if cursor_path.try_exists()? {
        storage::read(&cursor_path)?
    } else {
        String::new()
    };
    ensure!(after.len() <= 128, "invalid lineage cursor");
    let page = hub.goals("", "research", &after, false).await?;
    let goals: Vec<Goal> = serde_json::from_value(page["goals"].clone())?;
    ensure!(goals.len() <= 256, "lineage page exceeds bound");
    let mut linked = 0;
    for goal in goals {
        let Some(pins_by_work) = pins_for_goal(c, &goal) else {
            continue;
        };
        let Some(topic) = topic(hub, &goal).await? else {
            continue;
        };
        let repository = goal.input.parameters["repository"]
            .as_str()
            .context("missing research repository")?;
        let subject = subject(hub, &topic, repository).await?;
        let source = pins_by_work
            .values()
            .next()
            .and_then(|pins| pins.source.clone())
            .context("missing source pin")?;
        let node_id = identity("research-version", &[&subject.id, &source]);
        hub.research_change(ResearchChange::Node {
            node: EvolutionInput {
                id: node_id.clone(),
                subject_id: subject.id.clone(),
                parents: vec![],
                version: source.clone(),
                reference: format!("repository:{repository}@{source}"),
                goal_id: None,
                pins: EvidencePins {
                    source: Some(source),
                    ..EvidencePins::default()
                },
            },
        })
        .await?;
        let goal_attempts: BTreeMap<_, _> = goal
            .work
            .iter()
            .map(|work| (work.id.clone(), work.attempt))
            .collect();
        let attempt_key = serde_json::to_string(&goal_attempts)?;
        let assessment = AssessmentInput {
            id: identity(
                "research-assessment",
                &[&node_id, &goal.input.id, &attempt_key],
            ),
            node_id,
            goal_id: goal.input.id,
            goal_attempts,
            pins_by_work,
        };
        hub.research_change(ResearchChange::Assessment { assessment })
            .await?;
        linked += 1;
    }
    let next = page["next_after"].as_str().unwrap_or_default();
    ensure!(
        next.is_empty() || next != after,
        "research lineage cursor did not advance"
    );
    storage::write(&cursor_path, &next)?;
    Ok(linked)
}

#[cfg(test)]
#[path = "research_lineage_tests.rs"]
mod tests;
