//! Role guidelines from a frozen source, never the private rendered task prompt.
use super::*;

const MAX_SOUL: usize = 16 * 1024;

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = process::capture(
        "git",
        &args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
        repo,
        Duration::from_secs(2),
    )?;
    ensure!(output.status.success(), "role source unavailable");
    Ok(String::from_utf8(output.stdout)?)
}

fn soul(state: &Path, workspace: &Path, job: &Value) -> Result<Value> {
    let _deadline = process::deadline_scope(Duration::from_secs(2));
    let role = job["role"].as_str().context("role missing")?;
    let (content, source) = if let Some(version) = job["profile"]["prompt_version"].as_str() {
        let registry = state.join("evolution/prompts.json");
        let metadata = fs::metadata(&registry)?;
        ensure!(
            metadata.is_file() && metadata.len() <= MAX_FILE,
            "registry exceeds bound"
        );
        let prompt = evolution_cli::resolve_registered_prompt(&registry, version, role)?;
        (
            prompt.definition().content.clone(),
            json!({"kind":"registered_prompt","version":version}),
        )
    } else {
        let commit = job["repositories"]["agent-research-lab"]
            .as_str()
            .or_else(|| {
                (job["repository"] == "agent-research-lab")
                    .then(|| job["source_commit"].as_str())
                    .flatten()
            })
            .context("no frozen lab source")?;
        ensure!(
            commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid source commit"
        );
        let path = match role {
            "implement" => "prompts/implement.md",
            "review" => "prompts/critic.md",
            "evaluate" | "evaluator" => "prompts/evaluator.md",
            _ => "prompts/research.md",
        };
        let repo = workspace.join("agent-research-lab");
        let object = format!("{commit}:{path}");
        let size: usize = git(&repo, &["cat-file", "-s", &object])?.trim().parse()?;
        ensure!(size <= MAX_SOUL, "role source exceeds bound");
        (
            git(&repo, &["cat-file", "blob", &object])?,
            json!({"kind":"frozen_source","commit":commit,"path":path}),
        )
    };
    ensure!(content.len() <= MAX_SOUL, "role content exceeds bound");
    Ok(
        json!({"available":true,"content":sessions::text_field(&json!(content),MAX_SOUL),
        "sha256":storage::digest(content.as_bytes()),"source":source}),
    )
}

pub(super) fn read(state: &Path, workspace: &Path, job: &Value) -> Value {
    let soul = soul(state, workspace, job).unwrap_or_else(|_| {
        json!({"available":false,
        "reason":"绑定的角色准则暂不可读取；未使用当前版本替代历史来源。"})
    });
    json!({"run_id":job["run_id"],"soul":soul,
        "notice":"这里展示任务绑定的角色准则，不包含完整任务提示词；独立 Soul 与记忆见数字人档案。"})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soul_reads_frozen_blob_and_never_falls_back_to_mutable_or_missing_custom_prompt() {
        let root = std::env::temp_dir().join(format!(
            "lab-persona-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let repo = root.join("agent-research-lab");
        fs::create_dir_all(repo.join("prompts")).unwrap();
        fs::write(repo.join("prompts/research.md"), "FROZEN_ROLE").unwrap();
        git(&repo, &["init", "-q"]).unwrap();
        git(&repo, &["add", "prompts/research.md"]).unwrap();
        git(
            &repo,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@localhost",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "role",
            ],
        )
        .unwrap();
        let commit = git(&repo, &["rev-parse", "HEAD"]).unwrap();
        fs::write(repo.join("prompts/research.md"), "MUTABLE_PRIVATE_CONTENT").unwrap();
        let mut job = json!({"role":"research","run_id":"a","repositories":{"agent-research-lab":commit.trim()},"profile":{}});
        let view = read(&root, &root, &job);
        assert_eq!(view["soul"]["content"], "FROZEN_ROLE");
        assert_eq!(view["soul"]["sha256"], storage::digest(b"FROZEN_ROLE"));
        assert!(!view.to_string().contains("PRIVATE"));
        job["profile"]["prompt_version"] = json!("missing-custom-version");
        assert_eq!(read(&root, &root, &job)["soul"]["available"], false);
        job["profile"] = json!({});
        job["repositories"] = json!({});
        assert_eq!(read(&root, &root, &job)["soul"]["available"], false);
    }
}
