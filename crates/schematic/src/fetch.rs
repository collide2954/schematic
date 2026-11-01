use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    Tag(String),
    Commit(String),
}

pub fn run(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<String, Error> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    let output = command.output().map_err(|e| Error::new("io-error", format!("cannot run {program}: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(Error::new("io-error", format!("{program} {} failed: {stderr}", args.join(" "))));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn http_get(url: &str) -> Result<String, Error> {
    run("curl", &["-fsSL", url], None).map_err(|e| Error::new("io-error", format!("cannot fetch {url}: {e}")))
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn scratch() -> PathBuf {
    std::env::temp_dir().join(format!("schematic-fetch-{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)))
}

pub fn checkout(url: &str, reference: &Reference) -> Result<(PathBuf, String), Error> {
    let dir = scratch();
    remove_tree(&dir);
    let target = dir.to_string_lossy().to_string();
    match reference {
        Reference::Tag(tag) => {
            run("git", &["clone", "--quiet", "--depth", "1", "--branch", tag, url, &target], None)?;
        }
        Reference::Commit(commit) => {
            run("git", &["clone", "--quiet", url, &target], None)?;
            run("git", &["checkout", "--quiet", commit], Some(&dir))?;
        }
    }
    let commit = run("git", &["rev-parse", "HEAD"], Some(&dir))?;
    Ok((dir, commit))
}

pub fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_name() == ".git" {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn make_writable(path: &Path) {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let child = entry.path();
            if child.is_dir() {
                make_writable(&child);
            } else if let Ok(metadata) = std::fs::metadata(&child) {
                let mut permissions = metadata.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                let _ = std::fs::set_permissions(&child, permissions);
            }
        }
    }
}

pub fn remove_tree(path: &Path) {
    if path.exists() {
        make_writable(path);
        let _ = std::fs::remove_dir_all(path);
    }
}
