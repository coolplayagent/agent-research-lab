//! Safe identity projections and history cursors, backed by the host directory.
use super::*;

pub(super) fn assign(c: &Config, jobs: &[Job], projected: &mut [Value]) -> Result<Value> {
    let directory = runtime::people::register_history(c, jobs)?;
    for job in projected.iter_mut() {
        if let Some(person) = job["id"]
            .as_str()
            .and_then(|task| directory.tasks.get(task))
            .and_then(|id| directory.people.get(id))
        {
            job["profile"]["person_id"] = json!(person.id);
            job["profile"]["display_name"] = json!(person.name);
            job["profile"]["handle"] = json!(person.id);
            job["profile"]["kind"] = json!(person.kind);
        }
    }
    let relevant: BTreeSet<_> = projected
        .iter()
        .filter_map(|j| j["profile"]["person_id"].as_str())
        .collect();
    let mut people: Vec<_> = directory
        .people
        .values()
        .filter(|p| p.kind != runtime::people::Kind::Temporary || relevant.contains(p.id.as_str()))
        .collect();
    people.sort_by_key(|p| {
        (
            p.kind == runtime::people::Kind::Temporary,
            std::cmp::Reverse(p.created_at),
            &p.id,
        )
    });
    let counts = task_counts(&directory);
    Ok(
        json!({"people":people.into_iter().take(1024).map(|p| person(p, &counts)).collect::<Vec<_>>(),"total":directory.people.len(),"revision":directory.revision,"defaults":directory.defaults,"models":c.models}),
    )
}
fn task_counts(directory: &runtime::people::Directory) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for id in directory.tasks.values() {
        *counts.entry(id.clone()).or_default() += 1;
    }
    counts
}
fn person(p: &runtime::people::Person, counts: &BTreeMap<String, usize>) -> Value {
    json!({"id":p.id,"name":p.name,"kind":p.kind,"role":p.role,"soul":sessions::text_field(&json!(p.soul),4096),"purpose":sessions::text_field(&json!(p.purpose),1024),"revision":p.revision,"created_at":p.created_at,"task_count":counts.get(&p.id).copied().unwrap_or(0)})
}
pub(super) fn list(c: &Config, after: &str) -> Result<Value> {
    if !after.is_empty() {
        config::safe_id(after)?;
    }
    let directory = runtime::people::directory(c)?;
    let counts = task_counts(&directory);
    let rows: Vec<_> = directory
        .people
        .values()
        .filter(|p| p.id.as_str() > after)
        .take(101)
        .collect();
    let next = (rows.len() > 100).then(|| rows[99].id.clone());
    Ok(
        json!({"people":rows.into_iter().take(100).map(|p|person(p,&counts)).collect::<Vec<_>>(),"next_after":next,"total":directory.people.len(),"revision":directory.revision,"defaults":directory.defaults,"models":c.models}),
    )
}
pub(super) fn historical_job(state: &Path, run: &str) -> Result<Job> {
    ensure!(run.len() <= 120, "invalid run identity");
    let (id, attempt) = run
        .rsplit_once("-attempt-")
        .context("invalid run identity")?;
    config::safe_id(id)?;
    let attempt: u32 = attempt.parse()?;
    let mut job = read_job(&state.join("jobs").join(format!("{id}.json")))?;
    ensure!(
        job.task.id == id
            && attempt > 0
            && attempt <= job.attempt
            && attempt <= 3
            && run == format!("{id}-attempt-{attempt}"),
        "unknown historical attempt"
    );
    job.run_id = run.into();
    job.attempt = attempt;
    Ok(job)
}
pub(super) fn history(c: &Config, id: &str, after: &str, shared: &Shared) -> Result<Value> {
    config::safe_id(id)?;
    let directory = runtime::people::directory(c)?;
    let p = directory.people.get(id).context("unknown person")?;
    ensure!(after.len() <= 120, "invalid history cursor");
    let cursor = if after.is_empty() {
        None
    } else {
        let (task, attempt) = after
            .rsplit_once("-attempt-")
            .context("invalid history cursor")?;
        ensure!(
            directory.tasks.get(task).is_some_and(|person| person == id),
            "foreign history cursor"
        );
        Some((task, attempt.parse::<u32>()?))
    };
    // Stable task/attempt cursor: unrelated new sessions do not shift pages.
    let mut runs = Vec::new();
    for task in directory
        .tasks
        .iter()
        .filter(|(_, person)| person.as_str() == id)
        .map(|(task, _)| task)
        .rev()
    {
        for attempt in (1..=3).rev() {
            let run = format!("{task}-attempt-{attempt}");
            if cursor.is_some_and(|cursor| (task.as_str(), attempt) >= cursor) {
                continue;
            }
            if let Ok(job) = historical_job(&c.state_dir, &run) {
                runs.push(job);
            }
            if runs.len() == 25 {
                break;
            }
        }
        if runs.len() == 25 {
            break;
        }
    }
    let next = (runs.len() > 24).then(|| runs[23].run_id.clone());
    let snapshot = shared.read().unwrap();
    let rows: Vec<_> = runs
        .iter()
        .take(24)
        .map(|job| {
            let mut value = snapshot["jobs"]
                .as_array()
                .and_then(|jobs| jobs.iter().find(|j| j["run_id"] == job.run_id))
                .cloned()
                .unwrap_or_else(|| project(job, &json!({"state":"historical"})));
            value["profile"]["person_id"] = json!(id);
            value["profile"]["display_name"] = json!(p.name);
            value["profile"]["handle"] = json!(id);
            value["profile"]["kind"] = json!(p.kind);
            value
        })
        .collect();
    Ok(json!({"person":person(p,&task_counts(&directory)),"sessions":rows,"next_after":next}))
}
pub(super) fn memory(c: &Config, id: &str, query: &str) -> Result<Value> {
    let mut view = runtime::people::inspect_memory(c, id, query)?;
    if let Some(events) = view["events"].as_array_mut() {
        let mut seen = BTreeSet::new();
        events.retain(|e| {
            e["id"]
                .as_str()
                .is_some_and(|id| seen.insert(id.to_owned()))
        });
        for event in events {
            event["summary"] = sessions::text_field(&event["summary"], 2000);
            event["response"] = sessions::text_field(&event["response"], 4096);
            // Metadata can be user/agent-origin; expose only selected evidence references.
            let metadata = &event["metadata"];
            event["metadata"] = json!({"origin":metadata["origin"],"run_id":metadata["run_id"],"source_commit":metadata["source_commit"],"superpod_commit":metadata["superpod_commit"],"prompt_digest":metadata["prompt_digest"],"config_digest":metadata["config_digest"],"receipt_sha256":metadata["receipt_sha256"]});
        }
    }
    Ok(view)
}
