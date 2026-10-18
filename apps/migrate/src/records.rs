use anyhow::{Context, Result, ensure};
use contracts::{FollowupAdmission, MemoryWriteback, PersonKind};
use runtime::people::{Directory, new_person};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use task::{Job, PostSuccess};
fn archive(root: &Path, path: &Path) -> Result<()> {
    let relative = path.strip_prefix(root)?;
    let archive = root.join("migration-originals").join(relative);
    ensure!(!archive.try_exists()?, "migration archive already exists");
    fs::create_dir_all(archive.parent().unwrap())?;
    fs::copy(path, archive)?;
    Ok(())
}
fn jobs(root: &Path, files: &BTreeMap<String, String>) -> Result<BTreeMap<PathBuf, Vec<Job>>> {
    let mut result: BTreeMap<PathBuf, Vec<Job>> = BTreeMap::new();
    for name in files.keys() {
        let path = Path::new(name);
        if path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|n| n == "jobs")
            && path.extension().is_some_and(|n| n == "json")
        {
            let job: Job = storage::read(&root.join(path))?;
            config::safe_id(&job.task.id)?;
            config::safe_id(&job.run_id)?;
            ensure!(
                path.file_stem().and_then(|s| s.to_str()) == Some(&job.task.id),
                "job filename identity mismatch"
            );
            result
                .entry(path.parent().unwrap().parent().unwrap().to_owned())
                .or_default()
                .push(job);
        }
    }
    Ok(result)
}
pub fn convert(
    root: &Path,
    files: &BTreeMap<String, String>,
    changed: &mut Vec<String>,
) -> Result<()> {
    ensure!(
        !root.join("migration-originals").try_exists()?,
        "source already contains migration originals"
    );
    let mut groups = jobs(root, files)?;
    for name in files.keys() {
        let p = Path::new(name);
        if p.ends_with("dashboard/personas.json") || p.ends_with("people/directory.json") {
            groups
                .entry(p.parent().unwrap().parent().unwrap().to_owned())
                .or_default();
        }
    }
    ensure!(
        !groups.is_empty()
            || files.keys().any(|name| Path::new(name)
                .file_name()
                .is_some_and(|n| n == "messages.sqlite")),
        "no recognized OutcomeHub state"
    );
    for (relative, jobs) in groups {
        let state = root.join(&relative);
        let directory_path = state.join("people/directory.json");
        let mut directory = if directory_path.try_exists()? {
            storage::read::<Directory>(&directory_path)?
        } else {
            Directory {
                schema_version: 1,
                revision: 1,
                people: BTreeMap::new(),
                tasks: BTreeMap::new(),
                defaults: BTreeMap::new(),
            }
        };
        runtime::people::validate(&directory)?;
        let old = state.join("dashboard/personas.json");
        if old.try_exists()? {
            let names: BTreeMap<String, String> = storage::read(&old)?;
            ensure!(names.len() <= 50_000, "persona capacity exceeded");
            for (task, name) in names {
                config::safe_id(&task)?;
                if let Some(id) = directory.tasks.get(&task) {
                    ensure!(
                        directory.people[id].name == name,
                        "conflicting historical persona name"
                    );
                    continue;
                }
                let role = jobs
                    .iter()
                    .find(|j| j.task.id == task)
                    .map_or("research", |j| j.task.role.as_str());
                let id = new_person(
                    &mut directory,
                    &format!("legacy:{task}"),
                    &name,
                    role,
                    PersonKind::Temporary,
                    "",
                    "Historical task participant",
                )?;
                ensure!(
                    directory.people[&id].name == name,
                    "persona name normalization requires manual source reconciliation"
                );
                directory.tasks.insert(task, id);
            }
            archive(root, &old)?;
            fs::remove_file(&old)?;
            changed.push(old.strip_prefix(root)?.to_string_lossy().into_owned());
        }
        for job in &jobs {
            if let Some(person) = &job.persona {
                ensure!(
                    directory.people.contains_key(&person.id),
                    "frozen person missing from source directory"
                );
                if let Some(id) = directory.tasks.get(&job.task.id) {
                    ensure!(id == &person.id, "frozen person identity changed");
                } else {
                    directory
                        .tasks
                        .insert(job.task.id.clone(), person.id.clone());
                }
            } else if !directory.tasks.contains_key(&job.task.id) {
                let role = if job.task.id.starts_with("synthesis-") {
                    "synthesis"
                } else {
                    &job.task.role
                };
                let id = new_person(
                    &mut directory,
                    &format!("legacy:{}", job.task.id),
                    "",
                    role,
                    PersonKind::Temporary,
                    "",
                    "Historical task participant",
                )?;
                directory.tasks.insert(job.task.id.clone(), id);
            }
        }
        runtime::people::validate(&directory)?;
        if directory_path.try_exists()? {
            archive(root, &directory_path)?;
        }
        storage::write(&directory_path, &directory)?;
        changed.push(
            directory_path
                .strip_prefix(root)?
                .to_string_lossy()
                .into_owned(),
        );
        let mut attempts: BTreeMap<String, Job> =
            jobs.iter().map(|j| (j.run_id.clone(), j.clone())).collect();
        for name in files.keys() {
            let path = Path::new(name);
            let Some(local) = path.strip_prefix(&relative).ok() else {
                continue;
            };
            let run = if local.parent() == Some(Path::new("workflow/host-followups")) {
                local.file_stem().and_then(|v| v.to_str())
            } else if local.file_name().is_some_and(|v| v == "receipt.json")
                && local.starts_with("runs")
            {
                local
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|v| v.to_str())
            } else {
                None
            };
            if let Some(run) = run {
                let (task, number) = run
                    .rsplit_once("-attempt-")
                    .context("invalid historical attempt identity")?;
                let number: u32 = number.parse()?;
                let mut job = jobs
                    .iter()
                    .find(|j| j.task.id == task)
                    .context("historical completion has no frozen task")?
                    .clone();
                ensure!(
                    number > 0
                        && number <= job.attempt
                        && run == format!("{task}-attempt-{number}"),
                    "unknown historical attempt"
                );
                job.run_id = run.into();
                job.attempt = number;
                attempts.insert(run.into(), job);
            }
        }
        for job in attempts.values() {
            let journal = state
                .join("workflow/host-followups")
                .join(format!("{}.json", job.run_id));
            let receipt_path = state.join("runs").join(&job.run_id).join("receipt.json");
            let prior = journal
                .try_exists()?
                .then(|| storage::read::<Value>(&journal))
                .transpose()?;
            let receipt = receipt_path
                .try_exists()?
                .then(|| storage::read::<Value>(&receipt_path))
                .transpose()?;
            if prior.is_none()
                && receipt
                    .as_ref()
                    .is_none_or(|r| r["post_success_protocol"] == 1)
            {
                continue;
            }
            let record = completion(job, prior.as_ref(), receipt.as_ref())?;
            if journal.try_exists()? {
                archive(root, &journal)?;
            }
            storage::write(&journal, &record)?;
            changed.push(journal.strip_prefix(root)?.to_string_lossy().into_owned());
        }
    }
    // All remaining journals, including previous attempts, must have a known current structure.
    let converted_files = crate::snapshot::inventory(root)?;
    validate(root, &converted_files)
}
fn completion(job: &Job, prior: Option<&Value>, receipt: Option<&Value>) -> Result<PostSuccess> {
    if let Some(prior) = prior {
        ensure!(
            prior["task_id"] == job.task.id && prior["run_id"] == job.run_id,
            "completion journal identity mismatch"
        );
        if let Ok(mut record) = serde_json::from_value::<PostSuccess>(prior.clone()) {
            ensure!(
                record.schema_version == 1,
                "unsupported completion journal schema"
            );
            if record.memory == MemoryWriteback::Running {
                record.memory = MemoryWriteback::Unknown;
            }
            if record.persona_memory == MemoryWriteback::Running {
                record.persona_memory = MemoryWriteback::Unknown;
            }
            return Ok(record);
        }
        let marker = json!({"task_id":job.task.id,"run_id":job.run_id});
        ensure!(
            prior == &marker || prior["schema_version"] == 1,
            "unrecognized completion journal"
        );
        // Reject unknown state values; no state string may imply success by default.
        if let Some(state) = prior.get("memory") {
            serde_json::from_value::<MemoryWriteback>(state.clone())?;
        }
        if let Some(state) = prior.get("persona_memory") {
            serde_json::from_value::<MemoryWriteback>(state.clone())?;
        }
        if let Some(state) = prior.get("followups") {
            serde_json::from_value::<FollowupAdmission>(state.clone())?;
        }
    }
    let digest = if let Some(receipt) = receipt {
        storage::digest(&serde_json::to_vec(receipt)?)
    } else {
        prior
            .and_then(|v| v["receipt_sha256"].as_str())
            .context("missing completion receipt; reconcile source before migration")?
            .to_owned()
    };
    ensure!(
        digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid completion receipt digest"
    );
    let mut value = prior.cloned().unwrap_or_else(|| json!({}));
    let fields = value
        .as_object_mut()
        .context("completion journal must be an object")?;
    for (key, default) in [
        ("schema_version", json!(1)),
        ("task_id", json!(job.task.id)),
        ("run_id", json!(job.run_id)),
        ("receipt_sha256", json!(digest)),
        ("followup_queued", json!([])),
        ("followup_rejections", json!([])),
        ("persona_memory_error", Value::Null),
        ("memory_error", Value::Null),
        ("followup_error", Value::Null),
        ("memory", json!(MemoryWriteback::Unknown)),
        ("persona_memory", json!(MemoryWriteback::Unknown)),
        ("followups", json!(FollowupAdmission::Blocked)),
    ] {
        fields.entry(key).or_insert(default);
    }
    let mut record: PostSuccess = serde_json::from_value(value)?;
    ensure!(
        record.receipt_sha256 == digest,
        "historical receipt and journal disagree"
    );
    record.memory = MemoryWriteback::Unknown;
    record.persona_memory = MemoryWriteback::Unknown;
    record.persona_memory_error =
        Some("Offline migration: prior writeback outcome is unknown".into());
    record.memory_error =
        Some("Offline migration: reconcile original evidence before any writeback".into());
    record.followups = FollowupAdmission::Blocked;
    record.followup_error = Some("Offline migration: automatic replay disabled".into());
    Ok(record)
}
pub fn validate(root: &Path, files: &BTreeMap<String, String>) -> Result<()> {
    for name in files
        .keys()
        .filter(|n| !n.starts_with("migration-originals/"))
    {
        let p = Path::new(name);
        ensure!(
            !p.ends_with("dashboard/personas.json"),
            "unconverted persona directory"
        );
        if p.ends_with("people/directory.json") {
            runtime::people::validate(&storage::read(&root.join(p))?)?;
        }
        if p.parent()
            .and_then(Path::file_name)
            .is_some_and(|n| n == "host-followups")
            && p.extension().is_some_and(|n| n == "json")
        {
            let record: PostSuccess =
                storage::read(&root.join(p)).context("unconverted completion journal")?;
            ensure!(
                record.schema_version == 1
                    && p.file_stem().and_then(|v| v.to_str()) == Some(&record.run_id),
                "completion journal identity mismatch"
            );
        }
    }
    Ok(())
}
