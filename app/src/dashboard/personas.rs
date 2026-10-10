//! Display identities only. Never alter task, prompt, policy or experiment state.
use super::*;

const FIRST: [&str; 32] = [
    "云", "星", "月", "雨", "松", "竹", "青", "白", "银", "清", "墨", "秋", "朝", "远", "听", "逐",
    "南", "北", "长", "晓", "晚", "晴", "初", "若", "沐", "知", "怀", "望", "舒", "静", "明", "灵",
];
const LAST: [&str; 32] = [
    "舟", "河", "岚", "川", "溪", "林", "禾", "澜", "羽", "泉", "竹", "辰", "野", "帆", "山", "风",
    "海", "月", "雪", "岑", "棠", "榆", "苓", "芷", "珂", "瑾", "言", "音", "光", "露", "笙", "桐",
];
const MAX_PERSONAS: usize = 50_000;

fn available_name(id: &str, used: &BTreeSet<String>) -> String {
    let start = usize::from_str_radix(&storage::digest(id.as_bytes())[..4], 16).unwrap()
        % (FIRST.len() * LAST.len());
    for offset in 0..=used.len() {
        let index = start + offset;
        let cycle = index / (FIRST.len() * LAST.len());
        let base = format!(
            "{}{}",
            FIRST[index / LAST.len() % FIRST.len()],
            LAST[index % LAST.len()]
        );
        let name = if cycle == 0 {
            base
        } else {
            format!("{base}{}", cycle + 1)
        };
        if !used.contains(&name) {
            return name;
        }
    }
    unreachable!("N reserved names cannot fill N+1 distinct candidates")
}

pub(super) fn assign(state: &Path, jobs: &mut [Value]) -> Result<()> {
    let path = state.join("dashboard/personas.json");
    let _lock = storage::lock(&state.join("dashboard/personas.lock"))?;
    let mut names: BTreeMap<String, String> = if path.try_exists()? {
        storage::read(&path)?
    } else {
        BTreeMap::new()
    };
    ensure!(names.len() <= MAX_PERSONAS, "persona directory is full");
    let mut used: BTreeSet<String> = names.values().cloned().collect();
    ensure!(
        used.len() == names.len(),
        "duplicate persona names in directory"
    );
    ensure!(
        used.iter()
            .all(|n| !n.is_empty() && n.chars().count() <= 32),
        "invalid persona name"
    );
    let ids: BTreeSet<_> = jobs.iter().filter_map(|j| j["id"].as_str()).collect();
    let mut changed = false;
    for id in ids {
        if names.contains_key(id) {
            continue;
        }
        ensure!(names.len() < MAX_PERSONAS, "persona directory is full");
        let name = available_name(id, &used);
        used.insert(name.clone());
        names.insert(id.to_owned(), name);
        changed = true;
    }
    if changed {
        storage::write(&path, &names)?;
    }
    for job in jobs {
        job["profile"]["display_name"] =
            json!(names.get(job["id"].as_str().context("missing task ID")?));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_unique_persist_across_retries_and_are_never_recycled() {
        let root = std::env::temp_dir().join(format!(
            "lab-names-{}-{}",
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
        let mut jobs: Vec<_> = (0..1100)
            .map(|i| json!({"id":format!("task-{i}"),"run_id":"attempt-1","profile":{}}))
            .collect();
        assign(&root, &mut jobs).unwrap();
        let names: BTreeSet<_> = jobs
            .iter()
            .map(|j| j["profile"]["display_name"].as_str().unwrap())
            .collect();
        assert_eq!(names.len(), jobs.len());
        let original = jobs[0]["profile"]["display_name"].clone();
        let mut retry = vec![
            json!({"id":"task-0","run_id":"attempt-2","profile":{}}),
            json!({"id":"new-task","profile":{}}),
        ];
        assign(&root, &mut retry).unwrap();
        assert_eq!(retry[0]["profile"]["display_name"], original);
        assert!(!names.contains(retry[1]["profile"]["display_name"].as_str().unwrap()));
        let path = root.join("dashboard/personas.json");
        fs::write(&path, "{\"one\":\"云舟\",\"two\":\"云舟\"}").unwrap();
        assert!(assign(&root, &mut retry).is_err());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\"one\":\"云舟\",\"two\":\"云舟\"}"
        );
    }
}
