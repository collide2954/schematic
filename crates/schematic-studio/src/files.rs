use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub dir: bool,
    pub expanded: bool,
}

fn children(dir: &Path) -> Vec<(PathBuf, String, bool)> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut items: Vec<(PathBuf, String, bool)> = read
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            if name.starts_with('.') {
                return None;
            }
            let dir = entry.file_type().ok()?.is_dir();
            Some((entry.path(), name, dir))
        })
        .collect();
    items.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    items
}

fn walk(dir: &Path, depth: usize, expanded: &HashSet<PathBuf>, out: &mut Vec<Entry>) {
    for (path, name, is_dir) in children(dir) {
        let open = is_dir && expanded.contains(&path);
        out.push(Entry { path: path.clone(), name, depth, dir: is_dir, expanded: open });
        if open {
            walk(&path, depth + 1, expanded, out);
        }
    }
}

pub fn listing(root: &Path, expanded: &HashSet<PathBuf>) -> Vec<Entry> {
    let mut out = Vec::new();
    walk(root, 0, expanded, &mut out);
    out
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listings_put_folders_first_and_expand_on_request() {
        let root = std::env::temp_dir().join(format!("studio-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        for name in ["b.scm", "A.scm", ".hidden", "sub/x.scm"] {
            std::fs::write(root.join(name), "").unwrap();
        }
        let names = |entries: &[Entry]| entries.iter().map(|e| (e.name.clone(), e.depth, e.dir)).collect::<Vec<_>>();
        let flat = listing(&root, &HashSet::new());
        assert_eq!(names(&flat), [("sub".to_string(), 0, true), ("A.scm".to_string(), 0, false), ("b.scm".to_string(), 0, false)]);
        assert!(!flat[0].expanded);
        let open = listing(&root, &HashSet::from([root.join("sub")]));
        assert_eq!(names(&open)[..2], [("sub".to_string(), 0, true), ("x.scm".to_string(), 1, false)]);
        assert!(open[0].expanded);
        assert!(listing(&root.join("missing"), &HashSet::new()).is_empty());
        assert!(valid_name("a.scm") && !valid_name("") && !valid_name("a/b") && !valid_name(".."));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
