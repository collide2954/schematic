use std::path::{Path, PathBuf};

use schematic::project::{Origin, Project, Source, Status};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyInfo {
    pub name: String,
    pub version: Option<String>,
    pub license: Option<String>,
    pub source: String,
    pub status: Status,
    pub direct: bool,
    pub required_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
    pub root: PathBuf,
    pub name: String,
    pub version: String,
    pub main: PathBuf,
    pub dependencies: Vec<DependencyInfo>,
    pub load_path: Vec<PathBuf>,
}

fn info(project: &Project) -> ProjectInfo {
    let dependencies = project
        .packages()
        .into_iter()
        .map(|package| {
            let declared = project.manifest.dependencies.iter().find(|d| d.name == package.name).map(|d| &d.source);
            let locked = project.lock.get(&package.name);
            let source = match (declared, locked.map(|l| &l.origin)) {
                (Some(Source::Path(path)), _) | (_, Some(Origin::Path(path))) => path.clone(),
                (_, Some(Origin::Git { url, commit, .. })) => format!("{url} @ {}", &commit[..commit.len().min(10)]),
                (Some(Source::Git { url, .. }), _) => url.clone(),
                _ => "registry".to_string(),
            };
            DependencyInfo {
                name: package.name,
                version: package.version,
                license: package.license,
                source,
                status: package.status,
                direct: package.direct,
                required_by: package.required_by,
            }
        })
        .collect();
    ProjectInfo {
        root: project.root.clone(),
        name: project.manifest.name.clone(),
        version: project.manifest.version.clone(),
        main: project.main_file(),
        dependencies,
        load_path: project.load_path(),
    }
}

pub fn describe(start: &Path) -> Option<ProjectInfo> {
    let root = Project::find(start)?;
    Project::open(&root).ok().map(|project| info(&project))
}

pub fn create(dir: &Path) -> Result<ProjectInfo, String> {
    let name = dir.file_name().and_then(|n| n.to_str()).filter(|n| !n.is_empty()).unwrap_or("project").to_lowercase().replace(' ', "-");
    Project::create(dir, &name).map(|project| info(&project)).map_err(|e| e.to_string())
}

pub fn parse_spec(text: &str) -> Result<(String, Source), String> {
    let mut words = text.split_whitespace();
    let Some(first) = words.next() else { return Err("Name a package, as name, name@version, or name ../path".to_string()) };
    let source = match words.next() {
        Some(path) => Source::Path(path.to_string()),
        None => match first.split_once('@') {
            Some((_, version)) => Source::Registry { version: Some(version.to_string()) },
            None => Source::Registry { version: None },
        },
    };
    let name = first.split('@').next().unwrap_or(first).to_string();
    if name.is_empty() || words.next().is_some() {
        return Err("Name a package, as name, name@version, or name ../path".to_string());
    }
    Ok((name, source))
}

pub fn add(root: &Path, spec: &str) -> Result<(ProjectInfo, String), String> {
    let (name, source) = parse_spec(spec)?;
    let mut project = Project::open(root).map_err(|e| e.to_string())?;
    let registry = project.registry();
    let locked = project.add(&name, source, &registry).map_err(|e| e.to_string())?;
    let version = locked.version.map(|v| format!(" {v}")).unwrap_or_default();
    let license = locked.license.map(|l| format!(" ({l})")).unwrap_or_default();
    Ok((info(&project), format!("Added {name}{version}{license}")))
}

pub fn remove(root: &Path, name: &str) -> Result<(ProjectInfo, String), String> {
    let mut project = Project::open(root).map_err(|e| e.to_string())?;
    project.remove(name).map_err(|e| e.to_string())?;
    Ok((info(&project), format!("Removed {name}")))
}

pub fn restore(root: &Path) -> Result<(ProjectInfo, String), String> {
    let mut project = Project::open(root).map_err(|e| e.to_string())?;
    let registry = project.registry();
    let statuses = project.restore(false, &registry).map_err(|e| e.to_string())?;
    let vendored = statuses.iter().filter(|(_, s)| *s == Status::Vendored).count();
    Ok((info(&project), format!("Restored {vendored} of {} packages", statuses.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_name_registry_versions_or_local_paths() {
        assert_eq!(parse_spec("text-tools").unwrap(), ("text-tools".to_string(), Source::Registry { version: None }));
        assert_eq!(parse_spec(" text-tools@0.2.0 ").unwrap(), ("text-tools".to_string(), Source::Registry { version: Some("0.2.0".into()) }));
        assert_eq!(parse_spec("lib ../lib").unwrap(), ("lib".to_string(), Source::Path("../lib".into())));
        assert!(parse_spec("").is_err() && parse_spec("a b c").is_err() && parse_spec("@1").is_err());
    }

    #[test]
    fn projects_are_described_created_and_changed_in_place() {
        let root = std::env::temp_dir().join(format!("studio-project-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("My Report");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(describe(&dir), None);
        let created = create(&dir).unwrap();
        assert_eq!((created.name.as_str(), created.version.as_str()), ("my-report", "0.1.0"));
        assert!(created.main.ends_with("my-report.scm") && created.dependencies.is_empty());
        assert_eq!(describe(&dir.join("src")).map(|p| p.name), Some("my-report".to_string()));
        std::fs::create_dir_all(root.join("lib").join("src")).unwrap();
        std::fs::write(root.join("lib").join("src").join("lib.scm"), "(ns lib)").unwrap();
        let (added, message) = add(&dir, "lib ../lib").unwrap();
        assert_eq!(message, "Added lib");
        assert_eq!(added.dependencies.len(), 1);
        assert_eq!((added.dependencies[0].status, added.dependencies[0].source.as_str()), (Status::Local, "../lib"));
        assert!(added.load_path.iter().any(|p| p.ends_with("src") && p.to_string_lossy().contains("lib")));
        let (restored, message) = restore(&dir).unwrap();
        assert_eq!((message.as_str(), restored.dependencies.len()), ("Restored 0 of 1 packages", 1));
        let (removed, message) = remove(&dir, "lib").unwrap();
        assert_eq!((message.as_str(), removed.dependencies.len()), ("Removed lib", 0));
        assert!(add(&dir, "missing ../nowhere").unwrap_err().contains("no folder"));
        assert!(create(&dir).unwrap_err().contains("already a project"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
