use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Conflict,
}

impl Change {
    fn from_letter(letter: char) -> Option<Change> {
        Some(match letter {
            'A' => Change::Added,
            'M' => Change::Modified,
            'D' => Change::Deleted,
            'R' => Change::Renamed,
            'C' => Change::Copied,
            'T' => Change::TypeChanged,
            'U' => Change::Conflict,
            _ => return None,
        })
    }

    pub fn letter(self) -> &'static str {
        match self {
            Change::Added => "A",
            Change::Modified => "M",
            Change::Deleted => "D",
            Change::Renamed => "R",
            Change::Copied => "C",
            Change::TypeChanged => "T",
            Change::Untracked => "?",
            Change::Conflict => "!",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Change::Added => "added",
            Change::Modified => "modified",
            Change::Deleted => "deleted",
            Change::Renamed => "renamed",
            Change::Copied => "copied",
            Change::TypeChanged => "type changed",
            Change::Untracked => "untracked",
            Change::Conflict => "conflict",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub staged: Option<Change>,
    pub unstaged: Option<Change>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Status {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub entries: Vec<Entry>,
}

impl Status {
    pub fn staged(&self) -> impl Iterator<Item = (&Entry, Change)> {
        self.entries.iter().filter_map(|e| e.staged.map(|c| (e, c)))
    }

    pub fn unstaged(&self) -> impl Iterator<Item = (&Entry, Change)> {
        self.entries.iter().filter_map(|e| e.unstaged.map(|c| (e, c)))
    }

    pub fn changes(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    pub when: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub root: PathBuf,
    pub status: Result<Status, String>,
    pub log: Vec<Commit>,
}

fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git").arg("-C").arg(dir).args(args).env("GIT_TERMINAL_PROMPT", "0").output().map_err(|e| format!("cannot run git: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim_end().to_string();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if stderr.is_empty() { stdout } else { stderr })
}

pub fn repo_root(dir: &Path) -> Option<PathBuf> {
    let shown = run(dir, &["rev-parse", "--show-toplevel"]).ok()?;
    (!shown.is_empty()).then(|| PathBuf::from(shown))
}

pub fn relative(root: &Path, path: &Path) -> Option<String> {
    let rest = path.strip_prefix(root).ok()?;
    let shown = rest.to_string_lossy().replace('\\', "/");
    (!shown.is_empty()).then_some(shown)
}

fn tail(line: &str, fields: usize) -> &str {
    line.splitn(fields, ' ').last().unwrap_or("")
}

fn changes(xy: &str) -> (Option<Change>, Option<Change>) {
    let mut letters = xy.chars();
    let staged = letters.next().and_then(Change::from_letter);
    let unstaged = letters.next().and_then(Change::from_letter);
    (staged, unstaged)
}

pub fn parse_status(text: &str) -> Status {
    let mut status = Status::default();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            status.branch = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            status.upstream = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            let mut numbers = rest.split_whitespace().filter_map(|n| n.trim_start_matches(['+', '-']).parse::<usize>().ok());
            status.ahead = numbers.next().unwrap_or(0);
            status.behind = numbers.next().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("1 ") {
            let (staged, unstaged) = changes(rest.split(' ').next().unwrap_or(""));
            status.entries.push(Entry { path: tail(rest, 8).to_string(), staged, unstaged });
        } else if let Some(rest) = line.strip_prefix("2 ") {
            let (staged, unstaged) = changes(rest.split(' ').next().unwrap_or(""));
            let path = tail(rest, 9).split('\t').next().unwrap_or("").to_string();
            status.entries.push(Entry { path, staged, unstaged });
        } else if let Some(rest) = line.strip_prefix("u ") {
            status.entries.push(Entry { path: tail(rest, 10).to_string(), staged: None, unstaged: Some(Change::Conflict) });
        } else if let Some(rest) = line.strip_prefix("? ") {
            status.entries.push(Entry { path: rest.to_string(), staged: None, unstaged: Some(Change::Untracked) });
        }
    }
    status
}

pub fn parse_log(text: &str) -> Vec<Commit> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let hash = parts.next()?.to_string();
            let subject = parts.next()?.to_string();
            let when = parts.next().unwrap_or("").to_string();
            Some(Commit { hash, subject, when })
        })
        .collect()
}

pub fn status(root: &Path) -> Result<Status, String> {
    run(root, &["status", "--porcelain=v2", "--branch", "--untracked-files=all"]).map(|text| parse_status(&text))
}

pub fn log(root: &Path, count: usize) -> Vec<Commit> {
    let count = count.to_string();
    run(root, &["log", "--format=%h%x09%s%x09%ar", "-n", &count]).map(|text| parse_log(&text)).unwrap_or_default()
}

pub fn snapshot(dir: &Path) -> Option<Snapshot> {
    let root = repo_root(dir)?;
    let status = status(&root);
    let log = log(&root, 12);
    Some(Snapshot { root, status, log })
}

pub fn init(dir: &Path) -> Result<String, String> {
    run(dir, &["init", "-q", "-b", "main"]).map(|_| "Initialized a repository".to_string())
}

fn with_paths<'a>(head: &[&'a str], paths: &'a [String]) -> Vec<&'a str> {
    let mut args: Vec<&str> = head.to_vec();
    args.push("--");
    args.extend(paths.iter().map(String::as_str));
    args
}

pub fn stage(root: &Path, paths: &[String]) -> Result<String, String> {
    let args = if paths.is_empty() { vec!["add", "-A"] } else { with_paths(&["add"], paths) };
    run(root, &args).map(|_| if paths.is_empty() { "Staged everything".to_string() } else { format!("Staged {}", paths.join(", ")) })
}

pub fn unstage(root: &Path, paths: &[String]) -> Result<String, String> {
    let targets: Vec<String> = if paths.is_empty() { vec![".".to_string()] } else { paths.to_vec() };
    let restored = run(root, &with_paths(&["restore", "--staged"], &targets));
    let result = match restored {
        Ok(out) => Ok(out),
        Err(_) => run(root, &with_paths(&["rm", "-q", "--cached", "-r"], &targets)),
    };
    result.map(|_| if paths.is_empty() { "Unstaged everything".to_string() } else { format!("Unstaged {}", paths.join(", ")) })
}

pub fn discard(root: &Path, path: &str, change: Change) -> Result<String, String> {
    match change {
        Change::Untracked => {
            let target = root.join(path);
            let removed = if target.is_dir() { std::fs::remove_dir_all(&target) } else { std::fs::remove_file(&target) };
            removed.map_err(|e| format!("cannot remove {path}: {e}"))?;
        }
        _ => {
            run(root, &["restore", "--worktree", "--", path])?;
        }
    }
    Ok(format!("Discarded the changes to {path}"))
}

pub fn commit(root: &Path, message: &str) -> Result<String, String> {
    run(root, &["commit", "-q", "-m", message])?;
    let shown = run(root, &["log", "-1", "--format=%h %s"])?;
    Ok(format!("Committed {shown}"))
}

pub fn diff(root: &Path, path: &str, staged: bool, change: Option<Change>) -> Result<String, String> {
    if change == Some(Change::Untracked) {
        let text = std::fs::read_to_string(root.join(path)).map_err(|e| format!("cannot read {path}: {e}"))?;
        let mut out = format!("+++ {path}\n");
        for line in text.lines() {
            out.push('+');
            out.push_str(line);
            out.push('\n');
        }
        return Ok(out);
    }
    let args = if staged { vec!["diff", "--cached", "--", path] } else { vec!["diff", "--", path] };
    run(root, &args)
}

pub fn pull(root: &Path) -> Result<String, String> {
    run(root, &["pull", "--ff-only"]).map(|out| if out.is_empty() { "Pulled".to_string() } else { out })
}

pub fn push(root: &Path, upstream: bool) -> Result<String, String> {
    let args = if upstream { vec!["push", "-q"] } else { vec!["push", "-q", "-u", "origin", "HEAD"] };
    run(root, &args).map(|_| "Pushed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    }

    #[test]
    fn porcelain_status_and_logs_are_parsed() {
        let text = "# branch.oid abc\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n\
                    1 .M N... 100644 100644 100644 aaaa bbbb src/a.scm\n\
                    1 A. N... 000000 100644 100644 0000 cccc new file.scm\n\
                    1 MM N... 100644 100644 100644 dddd eeee both.scm\n\
                    2 R. N... 100644 100644 100644 ffff gggg R100 new-name.scm\told-name.scm\n\
                    u UU N... 100644 100644 100644 100644 h1 h2 h3 conflict.scm\n\
                    ? notes.md\n";
        let status = parse_status(text);
        assert_eq!((status.branch.as_str(), status.upstream.as_deref(), status.ahead, status.behind), ("main", Some("origin/main"), 2, 1));
        let shown: Vec<(&str, Option<Change>, Option<Change>)> = status.entries.iter().map(|e| (e.path.as_str(), e.staged, e.unstaged)).collect();
        assert_eq!(
            shown,
            [
                ("src/a.scm", None, Some(Change::Modified)),
                ("new file.scm", Some(Change::Added), None),
                ("both.scm", Some(Change::Modified), Some(Change::Modified)),
                ("new-name.scm", Some(Change::Renamed), None),
                ("conflict.scm", None, Some(Change::Conflict)),
                ("notes.md", None, Some(Change::Untracked)),
            ]
        );
        assert_eq!(status.staged().map(|(e, _)| e.path.as_str()).collect::<Vec<_>>(), ["new file.scm", "both.scm", "new-name.scm"]);
        assert_eq!(status.unstaged().map(|(_, c)| c.letter()).collect::<Vec<_>>(), ["M", "M", "!", "?"]);
        let fresh = parse_status("# branch.oid (initial)\n# branch.head main\n");
        assert_eq!((fresh.branch.as_str(), fresh.upstream.as_deref(), fresh.ahead, fresh.changes()), ("main", None, 0, 0));
        assert_eq!(
            parse_log("abc1234\tFirst words\t2 days ago\ndef5678\tSecond\t5 minutes ago\n"),
            [
                Commit { hash: "abc1234".into(), subject: "First words".into(), when: "2 days ago".into() },
                Commit { hash: "def5678".into(), subject: "Second".into(), when: "5 minutes ago".into() },
            ]
        );
        assert_eq!(relative(Path::new("C:/repo"), Path::new("C:/repo/src/a.scm")).as_deref(), Some("src/a.scm"));
        assert_eq!(relative(Path::new("C:/repo"), Path::new("C:/repo")), None);
        assert_eq!(relative(Path::new("C:/repo"), Path::new("D:/x")), None);
    }

    #[test]
    fn repositories_are_staged_committed_diffed_and_restored() {
        let root = std::env::temp_dir().join(format!("studio-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        unsafe {
            std::env::set_var("GIT_AUTHOR_NAME", "Test");
            std::env::set_var("GIT_AUTHOR_EMAIL", "test@example.com");
            std::env::set_var("GIT_COMMITTER_NAME", "Test");
            std::env::set_var("GIT_COMMITTER_EMAIL", "test@example.com");
        }
        assert!(snapshot(&root).is_none() || snapshot(&root).is_some_and(|s| s.root != root), "not a repository yet");
        init(&root).unwrap();
        std::fs::write(root.join("src").join("a.scm"), "(define a 1)\n").unwrap();
        let found = repo_root(&root.join("src")).unwrap();
        assert_eq!(found.canonicalize().unwrap(), root.canonicalize().unwrap());
        let seen = status(&root).unwrap();
        assert_eq!((seen.branch.as_str(), seen.changes()), ("main", 1));
        assert_eq!(seen.entries[0], Entry { path: "src/a.scm".into(), staged: None, unstaged: Some(Change::Untracked) });
        assert!(diff(&root, "src/a.scm", false, Some(Change::Untracked)).unwrap().contains("+(define a 1)"));
        assert_eq!(stage(&root, &["src/a.scm".to_string()]).unwrap(), "Staged src/a.scm");
        assert_eq!(status(&root).unwrap().entries[0].staged, Some(Change::Added));
        unstage(&root, &["src/a.scm".to_string()]).unwrap();
        assert_eq!(status(&root).unwrap().entries[0].unstaged, Some(Change::Untracked));
        stage(&root, &[]).unwrap();
        let committed = commit(&root, "First").unwrap();
        assert!(committed.starts_with("Committed ") && committed.ends_with(" First"), "{committed}");
        let history = log(&root, 5);
        assert_eq!((history.len(), history[0].subject.as_str()), (1, "First"));
        assert!(status(&root).unwrap().entries.is_empty());
        std::fs::write(root.join("src").join("a.scm"), "(define a 2)\n").unwrap();
        let seen = status(&root).unwrap();
        assert_eq!(seen.entries[0].unstaged, Some(Change::Modified));
        let shown = diff(&root, "src/a.scm", false, Some(Change::Modified)).unwrap();
        assert!(shown.contains("-(define a 1)") && shown.contains("+(define a 2)"), "{shown}");
        stage(&root, &[]).unwrap();
        assert!(diff(&root, "src/a.scm", true, Some(Change::Modified)).unwrap().contains("+(define a 2)"));
        unstage(&root, &[]).unwrap();
        assert_eq!(discard(&root, "src/a.scm", Change::Modified).unwrap(), "Discarded the changes to src/a.scm");
        assert_eq!(std::fs::read_to_string(root.join("src").join("a.scm")).unwrap().replace("\r\n", "\n"), "(define a 1)\n");
        std::fs::write(root.join("extra.md"), "x").unwrap();
        discard(&root, "extra.md", Change::Untracked).unwrap();
        assert!(!root.join("extra.md").exists());
        let snap = snapshot(&root.join("src")).unwrap();
        assert_eq!((snap.status.unwrap().changes(), snap.log.len()), (0, 1));
        assert!(commit(&root, "Nothing").is_err());
        assert!(push(&root, false).is_err(), "no remote to push to");
        git(&root, &["status"]);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
