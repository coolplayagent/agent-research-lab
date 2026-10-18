//! Hash and copy regular package content, including installed permission modes.
use crate::validation::validate_identifier;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

pub fn file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn files(root: &Path, current: &Path, result: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(
            !kind.is_symlink(),
            "symlinks are not accepted in install packages"
        );
        if kind.is_dir() {
            files(root, &entry.path(), result)?;
        } else {
            ensure!(kind.is_file(), "only regular package files are supported");
            result.push(entry.path().strip_prefix(root)?.to_owned());
        }
    }
    Ok(())
}

pub(super) fn tree_sha256(root: &Path) -> Result<String> {
    let mut paths = Vec::new();
    files(root, root, &mut paths)?;
    paths.sort();
    let mut hash = Sha256::new();
    for path in paths {
        hash.update(path.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(file_sha256(&root.join(&path))?.as_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            hash.update(
                fs::metadata(root.join(path))?
                    .permissions()
                    .mode()
                    .to_le_bytes(),
            );
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Content-and-mode digest used by the host's build/snapshot receipt.
pub fn package_sha256(root: &Path) -> Result<String> {
    tree_sha256(root)
}

pub(super) fn copy_package(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    let mut paths = Vec::new();
    files(source, source, &mut paths)?;
    for path in paths {
        if let Some(parent) = target.join(&path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source.join(&path), target.join(&path))?;
    }
    Ok(())
}

pub(super) fn skill_name(path: &Path) -> Result<String> {
    let text =
        fs::read_to_string(path.join("SKILL.md")).context("package must contain SKILL.md")?;
    ensure!(
        text.starts_with("---\n") || text.starts_with("---\r\n"),
        "SKILL.md must have frontmatter"
    );
    let name = text
        .lines()
        .skip(1)
        .take_while(|line| *line != "---")
        .find_map(|line| line.strip_prefix("name:"))
        .context("SKILL.md must name the skill")?;
    let name = name.trim().trim_matches(['\'', '"']).to_owned();
    validate_identifier(&name)?;
    Ok(name)
}
