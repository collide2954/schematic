use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::error::Error;
pub use crate::fetch::Reference;
use crate::fetch;
use crate::hash::hash_tree;
use crate::license;
use crate::map::Map;
use crate::reader::read_all;
use crate::value::Value;

pub const MANIFEST: &str = "project.scm";
pub const LOCK: &str = "project.lock";
pub const PACKAGES: &str = "packages";
pub const DEFAULT_REGISTRY: &str = "https://raw.githubusercontent.com/collide2954/schematic-registry/main";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Registry { version: Option<String> },
    Path(String),
    Git { url: String, reference: Reference, subdir: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub license: Option<String>,
    pub allowed_licenses: Vec<String>,
    pub source: String,
    pub main: Option<String>,
    pub registry: Option<String>,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    Git { url: String, subdir: Option<String>, commit: String },
    Path(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locked {
    pub name: String,
    pub version: Option<String>,
    pub license: Option<String>,
    pub origin: Origin,
    pub hash: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lock {
    pub registry: Option<String>,
    pub packages: Vec<Locked>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Vendored,
    Missing,
    Changed,
    Unlocked,
    Local,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Vendored => "vendored",
            Status::Missing => "missing",
            Status::Changed => "changed",
            Status::Unlocked => "not locked",
            Status::Local => "local path",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageEntry {
    pub name: String,
    pub license: Option<String>,
    pub git: String,
    pub subdir: Option<String>,
    pub versions: BTreeMap<String, Reference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    pub name: String,
    pub version: Option<String>,
    pub license: Option<String>,
    pub status: Status,
    pub direct: bool,
    pub required_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry {
    pub base: String,
}

#[derive(Debug)]
pub struct Project {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub lock: Lock,
}

fn project_error(message: impl Into<String>) -> Error {
    Error::new("project-error", message)
}

fn field<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.get(&Value::keyword(key))
}

fn text_field(map: &Map, key: &str) -> Option<String> {
    match field(map, key) {
        Some(Value::Str(s)) => Some(s.to_string()),
        _ => None,
    }
}

fn text_list(map: &Map, key: &str) -> Vec<String> {
    match field(map, key) {
        Some(Value::Vector(items)) => items.iter().filter_map(|item| if let Value::Str(s) = item { Some(s.to_string()) } else { None }).collect(),
        _ => Vec::new(),
    }
}

fn quoted_list(items: &[String]) -> String {
    format!("[{}]", items.iter().map(|s| quoted(s)).collect::<Vec<_>>().join(" "))
}

fn license_field(map: &Map, what: &str) -> Result<Option<String>, Error> {
    match field(map, "license") {
        None => Ok(None),
        Some(Value::Str(text)) if license::parse(&text.to_string()).is_some() => Ok(Some(text.to_string())),
        Some(other) => Err(project_error(format!("{what} has an unreadable :license {other}; use an SPDX expression such as \"MIT\" or \"MIT OR Apache-2.0\""))),
    }
}

fn one_map(text: &str, what: &str) -> Result<std::rc::Rc<Map>, Error> {
    let forms = read_all(text)?;
    match forms.first() {
        Some(Value::Map(map)) => Ok(map.clone()),
        _ => Err(project_error(format!("{what} must hold one map"))),
    }
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn version_key(version: &str) -> (Vec<u64>, String) {
    let numbers: Vec<u64> = version.split(['.', '-']).map_while(|part| part.parse().ok()).collect();
    (numbers, version.to_string())
}

impl Source {
    fn parse(name: &str, value: &Value) -> Result<Source, Error> {
        match value {
            Value::Str(version) => Ok(Source::Registry { version: Some(version.to_string()) }),
            Value::Keyword(k) if &*k.name() == "latest" => Ok(Source::Registry { version: None }),
            Value::Map(map) => {
                if let Some(path) = text_field(map, "path") {
                    return Ok(Source::Path(path));
                }
                if let Some(url) = text_field(map, "git") {
                    let reference = match (text_field(map, "tag"), text_field(map, "commit")) {
                        (Some(tag), _) => Reference::Tag(tag),
                        (None, Some(commit)) => Reference::Commit(commit),
                        (None, None) => return Err(project_error(format!("dependency {name} needs a :tag or :commit with its :git"))),
                    };
                    return Ok(Source::Git { url, reference, subdir: text_field(map, "subdir") });
                }
                Ok(Source::Registry { version: text_field(map, "version") })
            }
            other => Err(project_error(format!("dependency {name} has an unreadable source {other}"))),
        }
    }

    fn render(&self) -> String {
        match self {
            Source::Registry { version: Some(version) } => quoted(version),
            Source::Registry { version: None } => ":latest".to_string(),
            Source::Path(path) => format!("{{:path {}}}", quoted(path)),
            Source::Git { url, reference, subdir } => {
                let mut out = format!("{{:git {}", quoted(url));
                match reference {
                    Reference::Tag(tag) => out.push_str(&format!(" :tag {}", quoted(tag))),
                    Reference::Commit(commit) => out.push_str(&format!(" :commit {}", quoted(commit))),
                }
                if let Some(subdir) = subdir {
                    out.push_str(&format!(" :subdir {}", quoted(subdir)));
                }
                out.push('}');
                out
            }
        }
    }
}

impl Manifest {
    pub fn new(name: &str) -> Manifest {
        Manifest {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            license: None,
            allowed_licenses: Vec::new(),
            source: "src".to_string(),
            main: None,
            registry: None,
            dependencies: Vec::new(),
        }
    }

    pub fn parse(text: &str) -> Result<Manifest, Error> {
        let map = one_map(text, MANIFEST)?;
        let name = text_field(&map, "name").ok_or_else(|| project_error(format!("{MANIFEST} needs a :name")))?;
        let license = license_field(&map, MANIFEST)?;
        let allowed_licenses = text_list(&map, "allowed-licenses");
        if let Some(bad) = allowed_licenses.iter().find(|l| license::parse(l).is_none()) {
            return Err(project_error(format!("{MANIFEST} has an unreadable entry {} in :allowed-licenses", quoted(bad))));
        }
        let mut dependencies = Vec::new();
        if let Some(Value::Map(deps)) = field(&map, "dependencies") {
            for (key, value) in deps.iter() {
                let Value::Str(dep) = key else {
                    return Err(project_error(format!("dependency names must be strings, got {key}")));
                };
                dependencies.push(Dependency { name: dep.to_string(), source: Source::parse(dep, value)? });
            }
        }
        dependencies.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Manifest {
            name,
            version: text_field(&map, "version").unwrap_or_else(|| "0.1.0".to_string()),
            license,
            allowed_licenses,
            source: text_field(&map, "source").unwrap_or_else(|| "src".to_string()),
            main: text_field(&map, "main"),
            registry: text_field(&map, "registry"),
            dependencies,
        })
    }

    pub fn render(&self) -> String {
        let mut out = format!("{{:name {}\n :version {}", quoted(&self.name), quoted(&self.version));
        if let Some(license) = &self.license {
            out.push_str(&format!("\n :license {}", quoted(license)));
        }
        if self.source != "src" {
            out.push_str(&format!("\n :source {}", quoted(&self.source)));
        }
        if let Some(main) = &self.main {
            out.push_str(&format!("\n :main {}", quoted(main)));
        }
        if let Some(registry) = &self.registry {
            out.push_str(&format!("\n :registry {}", quoted(registry)));
        }
        if !self.allowed_licenses.is_empty() {
            out.push_str(&format!("\n :allowed-licenses {}", quoted_list(&self.allowed_licenses)));
        }
        out.push_str("\n :dependencies {");
        for (i, dep) in self.dependencies.iter().enumerate() {
            if i > 0 {
                out.push_str("\n                ");
            }
            out.push_str(&format!("{} {}", quoted(&dep.name), dep.source.render()));
        }
        out.push_str("}}\n");
        out
    }

    fn upsert(&mut self, dependency: Dependency) {
        self.dependencies.retain(|d| d.name != dependency.name);
        self.dependencies.push(dependency);
        self.dependencies.sort_by(|a, b| a.name.cmp(&b.name));
    }
}

impl Lock {
    pub fn parse(text: &str) -> Result<Lock, Error> {
        let map = one_map(text, LOCK)?;
        let mut packages = Vec::new();
        if let Some(Value::Map(entries)) = field(&map, "packages") {
            for (key, value) in entries.iter() {
                let (Value::Str(name), Value::Map(entry)) = (key, value) else {
                    return Err(project_error(format!("{LOCK} has an unreadable entry for {key}")));
                };
                let hash = text_field(entry, "hash").ok_or_else(|| project_error(format!("{LOCK} entry {name} has no :hash")))?;
                let origin = match (text_field(entry, "path"), text_field(entry, "git"), text_field(entry, "commit")) {
                    (Some(path), _, _) => Origin::Path(path),
                    (None, Some(url), Some(commit)) => Origin::Git { url, subdir: text_field(entry, "subdir"), commit },
                    _ => return Err(project_error(format!("{LOCK} entry {name} needs a :path, or a :git and :commit"))),
                };
                packages.push(Locked {
                    name: name.to_string(),
                    version: text_field(entry, "version"),
                    license: text_field(entry, "license"),
                    origin,
                    hash,
                    dependencies: text_list(entry, "dependencies"),
                });
            }
        }
        packages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Lock { registry: text_field(&map, "registry"), packages })
    }

    pub fn render(&self) -> String {
        let mut out = String::from("{");
        if let Some(registry) = &self.registry {
            out.push_str(&format!(":registry {}\n ", quoted(registry)));
        }
        out.push_str(":packages {");
        for (i, package) in self.packages.iter().enumerate() {
            if i > 0 {
                out.push_str("\n            ");
            }
            out.push_str(&format!("{} {{", quoted(&package.name)));
            let mut fields: Vec<String> = Vec::new();
            if let Some(version) = &package.version {
                fields.push(format!(":version {}", quoted(version)));
            }
            if let Some(license) = &package.license {
                fields.push(format!(":license {}", quoted(license)));
            }
            match &package.origin {
                Origin::Git { url, subdir, commit } => {
                    fields.push(format!(":git {}", quoted(url)));
                    if let Some(subdir) = subdir {
                        fields.push(format!(":subdir {}", quoted(subdir)));
                    }
                    fields.push(format!(":commit {}", quoted(commit)));
                }
                Origin::Path(path) => fields.push(format!(":path {}", quoted(path))),
            }
            if !package.dependencies.is_empty() {
                fields.push(format!(":dependencies {}", quoted_list(&package.dependencies)));
            }
            fields.push(format!(":hash {}", quoted(&package.hash)));
            out.push_str(&fields.join(" "));
            out.push('}');
        }
        out.push_str("}}\n");
        out
    }

    pub fn get(&self, name: &str) -> Option<&Locked> {
        self.packages.iter().find(|p| p.name == name)
    }

    fn upsert(&mut self, locked: Locked) {
        self.packages.retain(|p| p.name != locked.name);
        self.packages.push(locked);
        self.packages.sort_by(|a, b| a.name.cmp(&b.name));
    }
}

impl PackageEntry {
    pub fn parse(name: &str, text: &str) -> Result<PackageEntry, Error> {
        let map = one_map(text, &format!("the registry entry for {name}"))?;
        let git = text_field(&map, "git").ok_or_else(|| project_error(format!("the registry entry for {name} has no :git")))?;
        let mut versions = BTreeMap::new();
        if let Some(Value::Map(listed)) = field(&map, "versions") {
            for (key, value) in listed.iter() {
                let (Value::Str(version), Value::Map(spec)) = (key, value) else {
                    return Err(project_error(format!("the registry entry for {name} has an unreadable version {key}")));
                };
                let reference = match (text_field(spec, "tag"), text_field(spec, "commit")) {
                    (Some(tag), _) => Reference::Tag(tag),
                    (None, Some(commit)) => Reference::Commit(commit),
                    _ => return Err(project_error(format!("version {version} of {name} needs a :tag or :commit"))),
                };
                versions.insert(version.to_string(), reference);
            }
        }
        Ok(PackageEntry {
            name: text_field(&map, "name").unwrap_or_else(|| name.to_string()),
            license: license_field(&map, &format!("the registry entry for {name}"))?,
            git,
            subdir: text_field(&map, "subdir"),
            versions,
        })
    }

    pub fn latest(&self) -> Option<(&String, &Reference)> {
        self.versions.iter().max_by_key(|(version, _)| version_key(version))
    }
}

impl Registry {
    pub fn new(base: &str) -> Registry {
        Registry { base: base.trim_end_matches('/').to_string() }
    }

    pub fn default_base() -> String {
        std::env::var("SCHEMATIC_REGISTRY").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| DEFAULT_REGISTRY.to_string())
    }

    pub fn package(&self, name: &str) -> Result<PackageEntry, Error> {
        let text = if self.base.starts_with("http://") || self.base.starts_with("https://") {
            fetch::http_get(&format!("{}/packages/{name}.scm", self.base))
                .map_err(|e| project_error(format!("no package {name} in the registry at {}: {e}", self.base)))?
        } else {
            let path = Path::new(&self.base).join("packages").join(format!("{name}.scm"));
            std::fs::read_to_string(&path).map_err(|_| project_error(format!("no package {name} in the registry at {}", self.base)))?
        };
        PackageEntry::parse(name, &text)
    }
}

impl Project {
    pub fn find(start: &Path) -> Option<PathBuf> {
        let mut dir = if start.is_file() { start.parent()? } else { start };
        loop {
            if dir.join(MANIFEST).is_file() {
                return Some(dir.to_path_buf());
            }
            dir = dir.parent()?;
        }
    }

    pub fn open(root: &Path) -> Result<Project, Error> {
        let manifest_path = root.join(MANIFEST);
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| project_error(format!("cannot read {}: {e}", manifest_path.display())))?;
        let manifest = Manifest::parse(&text)?;
        let lock_path = root.join(LOCK);
        let lock = if lock_path.is_file() {
            Lock::parse(&std::fs::read_to_string(&lock_path).map_err(|e| project_error(format!("cannot read {}: {e}", lock_path.display())))?)?
        } else {
            Lock::default()
        };
        Ok(Project { root: root.to_path_buf(), manifest, lock })
    }

    pub fn create(dir: &Path, name: &str) -> Result<Project, Error> {
        if dir.join(MANIFEST).exists() {
            return Err(project_error(format!("{} is already a project", dir.display())));
        }
        let manifest = Manifest::new(name);
        let source = dir.join(&manifest.source);
        std::fs::create_dir_all(&source).map_err(|e| project_error(format!("cannot create {}: {e}", source.display())))?;
        let main = source.join(format!("{}.scm", name.replace('.', "/")));
        if let Some(parent) = main.parent() {
            std::fs::create_dir_all(parent).map_err(|e| project_error(format!("cannot create {}: {e}", parent.display())))?;
        }
        std::fs::write(&main, format!("(ns {name})\n\n(println \"hello from {name}\")\n"))
            .map_err(|e| project_error(format!("cannot write {}: {e}", main.display())))?;
        let project = Project { root: dir.to_path_buf(), manifest, lock: Lock::default() };
        project.save()?;
        Ok(project)
    }

    pub fn save(&self) -> Result<(), Error> {
        let manifest_path = self.root.join(MANIFEST);
        std::fs::write(&manifest_path, self.manifest.render())
            .map_err(|e| project_error(format!("cannot write {}: {e}", manifest_path.display())))?;
        let lock_path = self.root.join(LOCK);
        if !self.lock.packages.is_empty() || lock_path.exists() {
            std::fs::write(&lock_path, self.lock.render()).map_err(|e| project_error(format!("cannot write {}: {e}", lock_path.display())))?;
        }
        Ok(())
    }

    pub fn registry(&self) -> Registry {
        Registry::new(&self.manifest.registry.clone().unwrap_or_else(Registry::default_base))
    }

    pub fn main_file(&self) -> PathBuf {
        let main = self.manifest.main.clone().unwrap_or_else(|| self.manifest.name.clone());
        self.root.join(&self.manifest.source).join(format!("{}.scm", main.replace('.', "/")))
    }

    pub fn package_dir(&self, name: &str) -> PathBuf {
        self.root.join(PACKAGES).join(name)
    }

    fn dependency_root(&self, dependency: &Dependency) -> PathBuf {
        match &dependency.source {
            Source::Path(path) => self.root.join(path),
            _ => self.package_dir(&dependency.name),
        }
    }

    fn path_of(&self, name: &str) -> Option<String> {
        if let Some(dependency) = self.manifest.dependencies.iter().find(|d| d.name == name) {
            return match &dependency.source {
                Source::Path(path) => Some(path.clone()),
                _ => None,
            };
        }
        match self.lock.get(name).map(|l| &l.origin) {
            Some(Origin::Path(path)) => Some(path.clone()),
            _ => None,
        }
    }

    pub fn root_of(&self, name: &str) -> PathBuf {
        match self.path_of(name) {
            Some(path) => self.root.join(path),
            None => self.package_dir(name),
        }
    }

    fn manifest_at(root: &Path) -> Result<Option<Manifest>, Error> {
        let path = root.join(MANIFEST);
        if !path.is_file() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path).map_err(|e| project_error(format!("cannot read {}: {e}", path.display())))?;
        Manifest::parse(&text).map(Some).map_err(|e| project_error(format!("{}: {e}", path.display())))
    }

    fn source_dir(root: &Path) -> PathBuf {
        let source = Self::manifest_at(root).ok().flatten().map_or_else(|| "src".to_string(), |m| m.source);
        let dir = root.join(source);
        if dir.is_dir() { dir } else { root.to_path_buf() }
    }

    pub fn load_path(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.root.join(&self.manifest.source)];
        for package in self.packages() {
            dirs.push(Self::source_dir(&self.root_of(&package.name)));
        }
        dirs
    }

    fn dependencies_of(&self, name: &str) -> Vec<String> {
        let from_disk = || Self::manifest_at(&self.root_of(name)).ok().flatten().map(|m| m.dependencies.into_iter().map(|d| d.name).collect()).unwrap_or_default();
        if self.path_of(name).is_some() {
            return from_disk();
        }
        match self.lock.get(name) {
            Some(locked) => locked.dependencies.clone(),
            None => from_disk(),
        }
    }

    pub fn packages(&self) -> Vec<PackageInfo> {
        let mut order: Vec<String> = Vec::new();
        let mut required_by: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut queue: VecDeque<(String, Option<String>)> = self.manifest.dependencies.iter().map(|d| (d.name.clone(), None)).collect();
        while let Some((name, by)) = queue.pop_front() {
            if let Some(by) = by {
                required_by.entry(name.clone()).or_default().push(by);
            }
            if order.contains(&name) {
                continue;
            }
            order.push(name.clone());
            for sub in self.dependencies_of(&name) {
                queue.push_back((sub, Some(name.clone())));
            }
        }
        order
            .into_iter()
            .map(|name| {
                let locked = self.lock.get(&name);
                let license = locked.and_then(|l| l.license.clone()).or_else(|| Self::manifest_at(&self.root_of(&name)).ok().flatten().and_then(|m| m.license));
                PackageInfo {
                    version: locked.and_then(|l| l.version.clone()),
                    license,
                    status: self.status_named(&name),
                    direct: self.manifest.dependencies.iter().any(|d| d.name == name),
                    required_by: required_by.remove(&name).unwrap_or_default(),
                    name,
                }
            })
            .collect()
    }

    pub fn status(&self) -> Vec<(String, Status)> {
        self.packages().into_iter().map(|p| (p.name, p.status)).collect()
    }

    fn status_named(&self, name: &str) -> Status {
        if self.path_of(name).is_some() {
            return Status::Local;
        }
        let dir = self.package_dir(name);
        if !dir.is_dir() {
            return Status::Missing;
        }
        match self.lock.get(name) {
            None => Status::Unlocked,
            Some(locked) => match hash_tree(&dir) {
                Ok(hash) if hash == locked.hash => Status::Vendored,
                _ => Status::Changed,
            },
        }
    }

    fn vendor(&self, name: &str, url: &str, reference: &Reference, subdir: Option<&str>) -> Result<(String, String), Error> {
        let (checkout, commit) = fetch::checkout(url, reference)?;
        let source = match subdir {
            Some(subdir) => checkout.join(subdir),
            None => checkout.clone(),
        };
        if !source.is_dir() {
            fetch::remove_tree(&checkout);
            return Err(project_error(format!("{url} has no folder {}", subdir.unwrap_or("."))));
        }
        let target = self.package_dir(name);
        fetch::remove_tree(&target);
        let copied = fetch::copy_tree(&source, &target);
        fetch::remove_tree(&checkout);
        copied.map_err(|e| project_error(format!("cannot copy {name} into {}: {e}", target.display())))?;
        let hash = hash_tree(&target).map_err(|e| project_error(format!("cannot hash {}: {e}", target.display())))?;
        Ok((commit, hash))
    }

    fn resolve(&self, name: &str, source: &Source, registry: &Registry) -> Result<Locked, Error> {
        let locked = |version, license, origin, hash| Locked { name: name.to_string(), version, license, origin, hash, dependencies: Vec::new() };
        match source {
            Source::Path(path) => {
                let dir = self.root.join(path);
                let hash = hash_tree(&dir).map_err(|_| project_error(format!("{name}: no folder at {}", dir.display())))?;
                Ok(locked(None, None, Origin::Path(path.clone()), hash))
            }
            Source::Git { url, reference, subdir } => {
                let (commit, hash) = self.vendor(name, url, reference, subdir.as_deref())?;
                Ok(locked(None, None, Origin::Git { url: url.clone(), subdir: subdir.clone(), commit }, hash))
            }
            Source::Registry { version } => {
                let entry = registry.package(name)?;
                let (version, reference) = match version {
                    Some(wanted) => {
                        let reference = entry
                            .versions
                            .get(wanted)
                            .ok_or_else(|| project_error(format!("{name} has no version {wanted}; the registry lists {}", entry.versions.keys().cloned().collect::<Vec<_>>().join(", "))))?;
                        (wanted.clone(), reference.clone())
                    }
                    None => {
                        let (version, reference) = entry.latest().ok_or_else(|| project_error(format!("{name} has no versions in the registry")))?;
                        (version.clone(), reference.clone())
                    }
                };
                let (commit, hash) = self.vendor(name, &entry.git, &reference, entry.subdir.as_deref())?;
                let origin = Origin::Git { url: entry.git.clone(), subdir: entry.subdir.clone(), commit };
                Ok(locked(Some(version), entry.license.clone(), origin, hash))
            }
        }
    }

    fn settle_one(&mut self, dependency: &Dependency, locked_only: bool, registry: &Registry, fresh: bool) -> Result<Locked, Error> {
        let name = dependency.name.as_str();
        if matches!(dependency.source, Source::Path(_)) {
            return self.resolve(name, &dependency.source, registry);
        }
        let pin = self.lock.get(name).filter(|p| pinned(&dependency.source, p)).cloned();
        match pin {
            Some(pin) if !fresh && self.status_named(name) == Status::Vendored => Ok(pin),
            Some(pin) if !fresh => {
                let Origin::Git { url, subdir, commit } = &pin.origin else { return self.resolve(name, &dependency.source, registry) };
                let (_, hash) = self.vendor(name, url, &Reference::Commit(commit.clone()), subdir.as_deref())?;
                if hash != pin.hash {
                    return Err(project_error(format!("{name} at commit {commit} hashes to {hash}, but {LOCK} expects {}", pin.hash)));
                }
                Ok(pin)
            }
            _ if locked_only => Err(project_error(format!("{name} is not pinned in {LOCK}; run restore without --locked to resolve it"))),
            _ => self.resolve(name, &dependency.source, registry),
        }
    }

    fn check_license(&self, name: &str, locked: &Locked) -> Result<(), Error> {
        let allowed = &self.manifest.allowed_licenses;
        if allowed.is_empty() {
            return Ok(());
        }
        let problem = match &locked.license {
            None => format!("{name} declares no license, and {MANIFEST} allows only {}", allowed.join(", ")),
            Some(license) if license::permits(license, allowed) => return Ok(()),
            Some(license) => format!("{name} is licensed {license}, and {MANIFEST} allows only {}", allowed.join(", ")),
        };
        if !matches!(locked.origin, Origin::Path(_)) {
            fetch::remove_tree(&self.package_dir(name));
        }
        Err(project_error(problem))
    }

    fn settle(&mut self, locked_only: bool, registry: &Registry, fresh: Option<&str>) -> Result<(), Error> {
        let mut queue: VecDeque<(Dependency, Option<String>)> = self.manifest.dependencies.iter().cloned().map(|d| (d, None)).collect();
        let direct: BTreeSet<String> = self.manifest.dependencies.iter().map(|d| d.name.clone()).collect();
        let mut chosen: BTreeMap<String, Source> = BTreeMap::new();
        let mut used_registry = false;
        while let Some((dependency, by)) = queue.pop_front() {
            let name = dependency.name.clone();
            if let Some(existing) = chosen.get(&name) {
                if *existing == dependency.source || direct.contains(&name) {
                    continue;
                }
                match (existing, &dependency.source) {
                    (Source::Registry { version: Some(a) }, Source::Registry { version: Some(b) }) if version_key(b) > version_key(a) => {}
                    (Source::Registry { .. }, Source::Registry { .. }) => continue,
                    (a, b) => {
                        let by = by.map(|by| format!(" by {by}")).unwrap_or_default();
                        return Err(project_error(format!("{name} is wanted from two places: {} and, {by}, {}", a.render(), b.render())));
                    }
                }
            }
            let fresh_here = fresh == Some(name.as_str()) || chosen.contains_key(&name);
            chosen.insert(name.clone(), dependency.source.clone());
            used_registry |= matches!(dependency.source, Source::Registry { .. });
            let mut locked = self.settle_one(&dependency, locked_only, registry, fresh_here)?;
            locked.dependencies.clear();
            if let Some(manifest) = Self::manifest_at(&self.dependency_root(&dependency))? {
                if manifest.license.is_some() {
                    locked.license = manifest.license;
                }
                for mut sub in manifest.dependencies {
                    if let Source::Path(inner) = &sub.source {
                        match &dependency.source {
                            Source::Path(outer) => sub.source = Source::Path(format!("{}/{inner}", outer.trim_end_matches('/'))),
                            _ => return Err(project_error(format!("{name} depends on {} through a local path, which a package cannot do", sub.name))),
                        }
                    }
                    locked.dependencies.push(sub.name.clone());
                    queue.push_back((sub, Some(name.clone())));
                }
            }
            self.check_license(&name, &locked)?;
            self.lock.upsert(locked);
        }
        self.prune(&chosen.into_keys().collect());
        if used_registry {
            self.lock.registry = Some(registry.base.clone());
        }
        self.save()
    }

    fn prune(&mut self, keep: &BTreeSet<String>) {
        self.lock.packages.retain(|p| keep.contains(&p.name));
        if let Ok(entries) = std::fs::read_dir(self.root.join(PACKAGES)) {
            for entry in entries.flatten() {
                if !keep.contains(&*entry.file_name().to_string_lossy()) {
                    fetch::remove_tree(&entry.path());
                }
            }
        }
    }

    pub fn add(&mut self, name: &str, source: Source, registry: &Registry) -> Result<Locked, Error> {
        let (manifest, lock) = (self.manifest.clone(), self.lock.clone());
        self.manifest.upsert(Dependency { name: name.to_string(), source: source.clone() });
        if let Err(e) = self.settle(false, registry, Some(name)) {
            self.manifest = manifest;
            self.lock = lock;
            self.prune(&self.packages().into_iter().map(|p| p.name).collect());
            let _ = self.save();
            return Err(e);
        }
        let locked = self.lock.get(name).cloned().ok_or_else(|| project_error(format!("{name} was not resolved")))?;
        if let (Source::Registry { .. }, Some(version)) = (&source, &locked.version) {
            self.manifest.upsert(Dependency { name: name.to_string(), source: Source::Registry { version: Some(version.clone()) } });
            self.save()?;
        }
        Ok(locked)
    }

    pub fn remove(&mut self, name: &str) -> Result<(), Error> {
        if !self.manifest.dependencies.iter().any(|d| d.name == name) {
            return Err(project_error(format!("{name} is not a dependency of {}", self.manifest.name)));
        }
        self.manifest.dependencies.retain(|d| d.name != name);
        self.prune(&self.packages().into_iter().map(|p| p.name).collect());
        self.save()
    }

    pub fn restore(&mut self, locked: bool, registry: &Registry) -> Result<Vec<(String, Status)>, Error> {
        self.settle(locked, registry, None)?;
        Ok(self.status())
    }
}

fn pinned(source: &Source, locked: &Locked) -> bool {
    match (source, &locked.origin) {
        (Source::Path(path), Origin::Path(pinned)) => path == pinned,
        (Source::Git { url, subdir, .. }, Origin::Git { url: pinned, subdir: pinned_subdir, .. }) => url == pinned && subdir == pinned_subdir,
        (Source::Registry { version: Some(version) }, Origin::Git { .. }) => locked.version.as_ref() == Some(version),
        (Source::Registry { version: None }, Origin::Git { .. }) => locked.version.is_some(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_round_trip_through_text() {
        let text = "{:name \"report\"\n :version \"0.2.0\"\n :license \"MIT OR Apache-2.0\"\n :main \"report.app\"\n :allowed-licenses [\"MIT\" \"Apache-2.0\"]\n :dependencies {\"text-tools\" \"0.1.0\"\n                \"local\" {:path \"../local\"}\n                \"fancy\" {:git \"https://x/y\" :tag \"v1\" :subdir \"pkg\"}\n                \"edge\" :latest}}\n";
        let manifest = Manifest::parse(text).unwrap();
        assert_eq!((manifest.name.as_str(), manifest.version.as_str(), manifest.source.as_str()), ("report", "0.2.0", "src"));
        assert_eq!(manifest.main.as_deref(), Some("report.app"));
        assert_eq!(manifest.license.as_deref(), Some("MIT OR Apache-2.0"));
        assert_eq!(manifest.allowed_licenses, ["MIT", "Apache-2.0"]);
        assert!(manifest.render().contains("\n :license \"MIT OR Apache-2.0\"\n") && manifest.render().contains("\n :allowed-licenses [\"MIT\" \"Apache-2.0\"]\n"));
        assert!(Manifest::parse("{:name \"x\" :license \"MIT OR\"}").unwrap_err().to_string().contains("unreadable :license"));
        assert!(Manifest::parse("{:name \"x\" :license :mit}").unwrap_err().to_string().contains("unreadable :license"));
        assert!(Manifest::parse("{:name \"x\" :allowed-licenses [\"MIT\" \"(\"]}").unwrap_err().to_string().contains("unreadable entry \"(\""));
        assert_eq!(Manifest::parse("{:name \"x\"}").unwrap().license, None);
        let names: Vec<&str> = manifest.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["edge", "fancy", "local", "text-tools"]);
        assert_eq!(manifest.dependencies[1].source, Source::Git { url: "https://x/y".into(), reference: Reference::Tag("v1".into()), subdir: Some("pkg".into()) });
        assert_eq!(manifest.dependencies[0].source, Source::Registry { version: None });
        assert_eq!(Manifest::parse(&manifest.render()).unwrap(), manifest);
        assert!(manifest.render().contains("\"fancy\" {:git \"https://x/y\" :tag \"v1\" :subdir \"pkg\"}"));
        assert_eq!(Manifest::parse("{:version \"1\"}").unwrap_err().to_string(), "project.scm needs a :name");
        assert_eq!(Manifest::parse("42").unwrap_err().to_string(), "project.scm must hold one map");
        assert!(Manifest::parse("{:name \"x\" :dependencies {\"g\" {:git \"u\"}}}").unwrap_err().to_string().contains("needs a :tag or :commit"));
        assert_eq!(Manifest::new("x").render(), "{:name \"x\"\n :version \"0.1.0\"\n :dependencies {}}\n");
    }

    #[test]
    fn locks_round_trip_and_registry_entries_pick_the_latest_version() {
        let lock = Lock {
            registry: Some("https://example.test/registry".into()),
            packages: vec![
                Locked { name: "local".into(), version: None, license: None, origin: Origin::Path("../local".into()), hash: "sha256:aa".into(), dependencies: Vec::new() },
                Locked {
                    name: "text-tools".into(),
                    version: Some("0.1.0".into()),
                    license: Some("Apache-2.0".into()),
                    origin: Origin::Git { url: "https://x/y".into(), subdir: Some("examples/t".into()), commit: "abc".into() },
                    hash: "sha256:bb".into(),
                    dependencies: vec!["local".into(), "words".into()],
                },
            ],
        };
        let text = lock.render();
        assert!(text.starts_with("{:registry \"https://example.test/registry\"\n :packages {\"local\" {:path"), "{text}");
        assert!(text.contains(":license \"Apache-2.0\" :git") && text.contains(":dependencies [\"local\" \"words\"] :hash"), "{text}");
        assert_eq!(Lock::parse(&text).unwrap(), lock);
        assert_eq!(Lock::default().render(), "{:packages {}}\n");
        let entry = PackageEntry::parse("t", "{:git \"https://x/y\" :license \"MIT\" :versions {\"0.9.0\" {:tag \"v0.9.0\"} \"0.10.0\" {:tag \"v0.10.0\"} \"0.2.1\" {:commit \"abc\"}}}").unwrap();
        assert_eq!(entry.latest(), Some((&"0.10.0".to_string(), &Reference::Tag("v0.10.0".into()))));
        assert_eq!(entry.versions["0.2.1"], Reference::Commit("abc".into()));
        assert_eq!(entry.license.as_deref(), Some("MIT"));
        assert!(PackageEntry::parse("t", "{:versions {}}").unwrap_err().to_string().contains("no :git"));
        assert!(PackageEntry::parse("t", "{:git \"u\" :license \"MIT AND\"}").unwrap_err().to_string().contains("unreadable :license"));
        assert_eq!(Registry::new("https://a/b/").base, "https://a/b");
    }
}
