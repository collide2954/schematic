use std::path::{Path, PathBuf};
use std::process::Command;

use schematic::Interpreter;
use schematic::project::{Origin, Project, Reference, Registry, Source, Status};

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
}

fn write(path: PathBuf, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn upstream(root: &Path) -> PathBuf {
    let dir = root.join("upstream");
    write(dir.join("pkg").join("project.scm"), "{:name \"text-tools\" :version \"0.1.0\"}\n");
    write(dir.join("pkg").join("src").join("text-tools.scm"), "(ns text-tools)\n(define (slugify s) (string-join \"-\" (string-split (string-downcase s))))\n");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "first"]);
    git(&dir, &["tag", "v0.1.0"]);
    write(dir.join(".gitattributes"), "* text eol=lf\n");
    write(dir.join("pkg").join("src").join("text-tools.scm"), "(ns text-tools)\n(define (slugify s) (string-join \"-\" (string-split (string-downcase s))))\n(define (shout s) (string-upcase s))\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "second"]);
    git(&dir, &["tag", "v0.2.0"]);
    dir
}

fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[test]
fn projects_add_restore_and_remove_packages() {
    let root = std::env::temp_dir().join(format!("schematic-project-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let gitconfig = root.join("gitconfig");
    std::fs::write(&gitconfig, "[core]\n\tautocrlf = true\n").unwrap();
    unsafe { std::env::set_var("GIT_CONFIG_GLOBAL", &gitconfig) };
    let upstream = upstream(&root);
    let registry_dir = root.join("registry");
    write(
        registry_dir.join("packages").join("text-tools.scm"),
        &format!(
            "{{:name \"text-tools\" :git \"{}\" :subdir \"pkg\" :versions {{\"0.1.0\" {{:tag \"v0.1.0\"}} \"0.2.0\" {{:tag \"v0.2.0\"}}}}}}\n",
            slashes(&upstream)
        ),
    );
    let registry = Registry::new(&slashes(&registry_dir));
    assert!(registry.package("nope").unwrap_err().to_string().starts_with("no package nope in the registry"));

    let project_dir = root.join("report");
    let mut project = Project::create(&project_dir, "report").unwrap();
    assert!(project_dir.join("project.scm").is_file() && project_dir.join("src").join("report.scm").is_file());
    assert_eq!(project.main_file(), project_dir.join("src").join("report.scm"));
    assert!(Project::create(&project_dir, "again").unwrap_err().to_string().contains("already a project"));
    assert_eq!(Project::find(&project_dir.join("src").join("report.scm")), Some(project_dir.clone()));
    assert_eq!(Project::find(&root), None);

    let locked = project.add("text-tools", Source::Registry { version: None }, &registry).unwrap();
    assert_eq!(locked.version.as_deref(), Some("0.2.0"));
    assert!(locked.hash.starts_with("sha256:"));
    let vendored = project_dir.join("packages").join("text-tools").join("src").join("text-tools.scm");
    assert!(std::fs::read_to_string(&vendored).unwrap().contains("shout"));
    assert!(!std::fs::read(&vendored).unwrap().contains(&b'\r'));
    assert!(!project_dir.join("packages").join("text-tools").join(".git").exists());
    let lock_text = std::fs::read_to_string(project_dir.join("project.lock")).unwrap();
    assert!(lock_text.contains(":version \"0.2.0\"") && lock_text.contains(":subdir \"pkg\"") && lock_text.contains(":commit \""), "{lock_text}");
    assert!(std::fs::read_to_string(project_dir.join("project.scm")).unwrap().contains("\"text-tools\" \"0.2.0\""));
    assert_eq!(project.status(), vec![("text-tools".to_string(), Status::Vendored)]);

    let mut interp = Interpreter::new();
    interp.set_load_path(project.load_path());
    assert_eq!(interp.eval_str("(require 'text-tools) (text-tools/slugify \"Hello World\")").unwrap().to_string(), "\"hello-world\"");

    std::fs::write(&vendored, "(ns text-tools) (define (slugify s) :broken)\n").unwrap();
    assert_eq!(project.status()[0].1, Status::Changed);
    assert_eq!(project.restore(true, &registry).unwrap(), vec![("text-tools".to_string(), Status::Vendored)]);
    assert!(std::fs::read_to_string(&vendored).unwrap().contains("shout"));
    std::fs::remove_dir_all(project_dir.join("packages")).unwrap();
    assert_eq!(project.status()[0].1, Status::Missing);
    assert_eq!(project.restore(false, &registry).unwrap()[0].1, Status::Vendored);

    let reopened = Project::open(&project_dir).unwrap();
    assert_eq!(reopened.lock, project.lock);
    assert_eq!(reopened.manifest, project.manifest);

    project.add("text-tools", Source::Registry { version: Some("0.1.0".into()) }, &registry).unwrap();
    assert!(!std::fs::read_to_string(&vendored).unwrap().contains("shout"));
    std::fs::remove_dir_all(project_dir.join("packages")).unwrap();
    assert_eq!(project.restore(true, &registry).unwrap()[0].1, Status::Vendored);
    assert!(!std::fs::read(&vendored).unwrap().contains(&b'\r'));
    assert_eq!(project.add("text-tools", Source::Registry { version: Some("9.9.9".into()) }, &registry).unwrap_err().to_string(), "text-tools has no version 9.9.9; the registry lists 0.1.0, 0.2.0");

    write(root.join("local-lib").join("src").join("local-lib.scm"), "(ns local-lib)\n(define answer 42)\n");
    let local = project.add("local-lib", Source::Path("../local-lib".into()), &registry).unwrap();
    assert!(local.hash.starts_with("sha256:") && local.version.is_none());
    assert_eq!(project.status(), vec![("local-lib".to_string(), Status::Local), ("text-tools".to_string(), Status::Vendored)]);
    assert!(project.load_path().contains(&project_dir.join("../local-lib").join("src")));
    interp.set_load_path(project.load_path());
    assert_eq!(interp.eval_str("(require 'local-lib) local-lib/answer").unwrap().to_string(), "42");

    let fancy = project.add("fancy", Source::Git { url: slashes(&upstream), reference: Reference::Tag("v0.1.0".into()), subdir: Some("pkg".into()) }, &registry).unwrap();
    assert!(fancy.version.is_none() && project_dir.join("packages").join("fancy").join("src").is_dir());
    project.remove("fancy").unwrap();
    assert!(!project_dir.join("packages").join("fancy").exists());
    assert!(project.lock.get("fancy").is_none() && project.manifest.dependencies.iter().all(|d| d.name != "fancy"));
    assert!(project.remove("fancy").unwrap_err().to_string().contains("not a dependency"));

    project.lock.packages.retain(|p| p.name != "text-tools");
    std::fs::remove_dir_all(project_dir.join("packages")).unwrap();
    assert!(project.restore(true, &registry).unwrap_err().to_string().contains("not pinned"));
    assert_eq!(project.restore(false, &registry).unwrap()[1].1, Status::Vendored);
    std::fs::remove_dir_all(&root).unwrap();
}

fn upstream_with_dependencies(root: &Path) -> PathBuf {
    let dir = root.join("upstream");
    write(dir.join(".gitattributes"), "* text eol=lf\n");
    write(dir.join("words").join("project.scm"), "{:name \"words\" :version \"0.1.0\" :license \"MIT\"}\n");
    write(dir.join("words").join("src").join("words.scm"), "(ns words)\n(define (shout s) (string-upcase s))\n");
    write(dir.join("pkg").join("project.scm"), "{:name \"text-tools\" :version \"0.1.0\" :license \"Apache-2.0\" :dependencies {\"words\" \"0.1.0\"}}\n");
    write(dir.join("pkg").join("src").join("text-tools.scm"), "(ns text-tools (:require [words :as w]))\n(define (loud-slug s) (w/shout (string-join \"-\" (string-split s))))\n");
    write(dir.join("bare").join("src").join("bare.scm"), "(ns bare)\n(define x 1)\n");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "first"]);
    git(&dir, &["tag", "v0.1.0"]);
    write(dir.join("words").join("project.scm"), "{:name \"words\" :version \"0.2.0\" :license \"MIT\"}\n");
    write(dir.join("words").join("src").join("words.scm"), "(ns words)\n(define (shout s) (string-upcase s))\n(define (quiet s) (string-downcase s))\n");
    write(dir.join("pkg").join("project.scm"), "{:name \"text-tools\" :version \"0.2.0\" :license \"Apache-2.0\" :dependencies {\"words\" \"0.2.0\"}}\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "second"]);
    git(&dir, &["tag", "v0.2.0"]);
    dir
}

#[test]
fn packages_bring_their_own_dependencies_and_declare_licenses() {
    let root = std::env::temp_dir().join(format!("schematic-deps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let upstream = slashes(&upstream_with_dependencies(&root));
    let registry_dir = root.join("registry");
    let entry = |subdir: &str, extra: &str, versions: &str| format!("{{:git \"{upstream}\" :subdir \"{subdir}\"{extra} :versions {{{versions}}}}}\n");
    let both = "\"0.1.0\" {:tag \"v0.1.0\"} \"0.2.0\" {:tag \"v0.2.0\"}";
    write(registry_dir.join("packages").join("text-tools.scm"), &entry("pkg", "", both));
    write(registry_dir.join("packages").join("words.scm"), &entry("words", " :license \"MIT\"", both));
    write(registry_dir.join("packages").join("bare.scm"), &entry("bare", "", "\"0.1.0\" {:tag \"v0.1.0\"}"));
    let registry = Registry::new(&slashes(&registry_dir));
    let project_dir = root.join("app");
    let mut project = Project::create(&project_dir, "app").unwrap();

    let locked = project.add("text-tools", Source::Registry { version: None }, &registry).unwrap();
    assert_eq!((locked.version.as_deref(), locked.license.as_deref(), locked.dependencies.as_slice()), (Some("0.2.0"), Some("Apache-2.0"), &["words".to_string()][..]));
    let packages = project.packages();
    let summary: Vec<String> = packages.iter().map(|p| format!("{} {:?} {:?} {} {:?}", p.name, p.version, p.license, p.direct, p.required_by)).collect();
    assert_eq!(summary, ["text-tools Some(\"0.2.0\") Some(\"Apache-2.0\") true []", "words Some(\"0.2.0\") Some(\"MIT\") false [\"text-tools\"]"]);
    assert!(project_dir.join("packages").join("words").join("src").join("words.scm").is_file());
    let lock_text = std::fs::read_to_string(project_dir.join("project.lock")).unwrap();
    assert!(lock_text.contains("\"words\" {:version \"0.2.0\" :license \"MIT\"") && lock_text.contains(":dependencies [\"words\"]"), "{lock_text}");
    let manifest_text = std::fs::read_to_string(project_dir.join("project.scm")).unwrap();
    assert!(manifest_text.contains("\"text-tools\" \"0.2.0\"") && !manifest_text.contains("words"), "{manifest_text}");
    let mut interp = Interpreter::new();
    interp.set_load_path(project.load_path());
    assert_eq!(interp.eval_str("(require 'text-tools) (text-tools/loud-slug \"a b\")").unwrap().to_string(), "\"A-B\"");

    std::fs::remove_dir_all(project_dir.join("packages").join("words")).unwrap();
    assert_eq!(project.status()[1], ("words".to_string(), Status::Missing));
    assert_eq!(project.restore(true, &registry).unwrap()[1].1, Status::Vendored);
    assert_eq!(Project::open(&project_dir).unwrap().packages(), packages);

    project.add("words", Source::Registry { version: Some("0.1.0".into()) }, &registry).unwrap();
    let words = project.packages().into_iter().find(|p| p.name == "words").unwrap();
    assert_eq!((words.version.as_deref(), words.direct, words.required_by), (Some("0.1.0"), true, vec!["text-tools".to_string()]), "the project's own choice wins");
    project.remove("words").unwrap();
    let words = project.packages().into_iter().find(|p| p.name == "words").unwrap();
    assert!(!words.direct && project_dir.join("packages").join("words").is_dir(), "still needed by text-tools");
    assert_eq!(project.restore(false, &registry).unwrap(), vec![("text-tools".to_string(), Status::Vendored), ("words".to_string(), Status::Vendored)]);
    assert_eq!(project.lock.get("words").unwrap().version.as_deref(), Some("0.2.0"), "restore goes back to what text-tools asks for");

    project.remove("text-tools").unwrap();
    assert!(project.packages().is_empty() && project.lock.packages.is_empty());
    assert!(!project_dir.join("packages").join("words").exists());

    project.manifest.allowed_licenses = vec!["MIT".into()];
    project.save().unwrap();
    let error = project.add("text-tools", Source::Registry { version: None }, &registry).unwrap_err().to_string();
    assert_eq!(error, "text-tools is licensed Apache-2.0, and project.scm allows only MIT");
    assert!(project.manifest.dependencies.is_empty() && !project_dir.join("packages").join("text-tools").exists());
    assert!(Project::open(&project_dir).unwrap().manifest.dependencies.is_empty());
    let error = project.add("bare", Source::Registry { version: None }, &registry).unwrap_err().to_string();
    assert_eq!(error, "bare declares no license, and project.scm allows only MIT");
    project.manifest.allowed_licenses = vec!["MIT".into(), "Apache-2.0".into()];
    project.add("text-tools", Source::Registry { version: None }, &registry).unwrap();
    assert_eq!(project.packages().len(), 2);
    let manifest_text = std::fs::read_to_string(project_dir.join("project.scm")).unwrap();
    assert!(manifest_text.contains(":allowed-licenses [\"MIT\" \"Apache-2.0\"]"), "{manifest_text}");

    write(root.join("helpers").join("src").join("helpers.scm"), "(ns helpers)\n(define two 2)\n");
    write(root.join("tools").join("project.scm"), "{:name \"tools\" :license \"MIT\" :dependencies {\"helpers\" {:path \"../helpers\"}}}\n");
    assert_eq!(
        project.add("tools", Source::Path("../tools".into()), &registry).unwrap_err().to_string(),
        "helpers declares no license, and project.scm allows only MIT, Apache-2.0"
    );
    write(root.join("helpers").join("project.scm"), "{:name \"helpers\" :license \"MIT\"}\n");
    write(root.join("tools").join("src").join("tools.scm"), "(ns tools (:require [helpers]))\n(define four (* 2 helpers/two))\n");
    project.add("tools", Source::Path("../tools".into()), &registry).unwrap();
    let names: Vec<String> = project.packages().into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["text-tools", "tools", "words", "helpers"]);
    let helpers = project.lock.get("helpers").unwrap();
    assert_eq!((&helpers.origin, helpers.license.as_deref()), (&Origin::Path("../tools/../helpers".into()), Some("MIT")));
    interp.set_load_path(project.load_path());
    assert_eq!(interp.eval_str("(require 'tools) tools/four").unwrap().to_string(), "4");
    std::fs::remove_dir_all(&root).unwrap();
}
