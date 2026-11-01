use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use schematic::Interpreter;
use schematic::check::check_file;
use schematic::error::Error;
use schematic::project::{Project, Reference, Source};
use schematic::reader::read_all;

const USAGE: &str = "usage: schematic [file | -e expression | --version]
       schematic new <name>
       schematic add <name>[@version] [--path dir | --git url [--tag t | --commit c] [--subdir d]]
       schematic remove <name>
       schematic restore [--locked]
       schematic run [file]
       schematic check [file...]";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => repl(),
        ["--version"] | ["-V"] => {
            println!("schematic {}", env!("CARGO_PKG_VERSION"));
            0
        }
        ["-e", expr] => run_source(expr, true),
        ["new", name] => report(new_project(name)),
        ["add", rest @ ..] if !rest.is_empty() => report(add(rest)),
        ["remove", name] => report(remove(name)),
        ["restore"] => report(restore(false)),
        ["restore", "--locked"] => report(restore(true)),
        ["run"] => report(run_project()),
        ["run", path] => run_file(path),
        ["check", files @ ..] => match check(files) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        [path] if !path.starts_with('-') => run_file(path),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    std::process::exit(code);
}

fn report(result: Result<(), Error>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn project_at(start: &Path) -> Result<Project, Error> {
    match Project::find(start) {
        Some(root) => Project::open(&root),
        None => Err(Error::new("project-error", "no project.scm here or in a folder above; run schematic new <name> first")),
    }
}

fn load_path_for(start: &Path) -> Vec<PathBuf> {
    let mut dirs = Project::find(start).and_then(|root| Project::open(&root).ok()).map(|p| p.load_path()).unwrap_or_default();
    dirs.push(PathBuf::from("."));
    dirs
}

fn run_file(path: &str) -> i32 {
    if let Err(e) = std::fs::read_to_string(path) {
        eprintln!("error: cannot read {path}: {e}");
        return 2;
    }
    let mut interp = Interpreter::new();
    interp.set_load_path(load_path_for(Path::new(path)));
    match interp.load_file(Path::new(path)) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn run_source(source: &str, print_result: bool) -> i32 {
    let mut interp = Interpreter::new();
    interp.set_load_path(load_path_for(Path::new(".")));
    match interp.eval_str(source) {
        Ok(value) => {
            if print_result {
                println!("{value}");
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn new_project(name: &str) -> Result<(), Error> {
    let project = Project::create(Path::new(name), name)?;
    let main = project.main_file();
    let shown = main.strip_prefix(&project.root).unwrap_or(&main).to_string_lossy().replace('\\', "/");
    println!("Created {name}/ with project.scm and {shown}");
    Ok(())
}

fn add(args: &[&str]) -> Result<(), Error> {
    let bad = |message: &str| Error::new("project-error", format!("{message}\n{USAGE}"));
    let (spec, flags) = args.split_first().ok_or_else(|| bad("add needs a package name"))?;
    let (name, version) = match spec.split_once('@') {
        Some((name, version)) => (name, Some(version.to_string())),
        None => (*spec, None),
    };
    let mut path = None;
    let mut git = None;
    let mut tag = None;
    let mut commit = None;
    let mut subdir = None;
    let mut flags = flags.iter();
    while let Some(flag) = flags.next() {
        let value = flags.next().ok_or_else(|| bad(&format!("{flag} needs a value")))?.to_string();
        match *flag {
            "--path" => path = Some(value),
            "--git" => git = Some(value),
            "--tag" => tag = Some(value),
            "--commit" => commit = Some(value),
            "--subdir" => subdir = Some(value),
            other => return Err(bad(&format!("unknown option {other}"))),
        }
    }
    let source = match (path, git) {
        (Some(path), None) => Source::Path(path),
        (None, Some(url)) => {
            let reference = match (tag, commit) {
                (Some(tag), None) => Reference::Tag(tag),
                (None, Some(commit)) => Reference::Commit(commit),
                _ => return Err(bad("--git needs exactly one of --tag or --commit")),
            };
            Source::Git { url, reference, subdir }
        }
        (None, None) => Source::Registry { version },
        (Some(_), Some(_)) => return Err(bad("use --path or --git, not both")),
    };
    let mut project = project_at(Path::new("."))?;
    let registry = project.registry();
    let locked = project.add(name, source, &registry)?;
    let version = locked.version.map(|v| format!(" {v}")).unwrap_or_default();
    let commit = match &locked.origin {
        schematic::project::Origin::Git { commit, .. } => format!(" at {}", &commit[..commit.len().min(10)]),
        schematic::project::Origin::Path(path) => format!(" from {path}"),
    };
    println!("Added {name}{version}{commit}");
    Ok(())
}

fn remove(name: &str) -> Result<(), Error> {
    let mut project = project_at(Path::new("."))?;
    project.remove(name)?;
    println!("Removed {name}");
    Ok(())
}

fn restore(locked: bool) -> Result<(), Error> {
    let mut project = project_at(Path::new("."))?;
    let registry = project.registry();
    let statuses = project.restore(locked, &registry)?;
    if statuses.is_empty() {
        println!("{} has no dependencies", project.manifest.name);
    }
    for (name, status) in statuses {
        println!("{name:<24} {}", status.label());
    }
    Ok(())
}

fn scripts_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scripts_under(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "scm") {
            out.push(path);
        }
    }
}

fn check(files: &[&str]) -> Result<i32, Error> {
    let targets: Vec<(PathBuf, String)> = if files.is_empty() {
        let project = project_at(Path::new("."))?;
        let mut found = Vec::new();
        scripts_under(&project.root.join(&project.manifest.source), &mut found);
        found.sort();
        found
            .into_iter()
            .map(|path| {
                let shown = path.strip_prefix(&project.root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                (path, shown)
            })
            .collect()
    } else {
        files.iter().map(|file| (PathBuf::from(file), file.to_string())).collect()
    };
    let mut count = 0;
    for (path, shown) in &targets {
        for diagnostic in check_file(path, &load_path_for(path)) {
            println!("{shown}:{diagnostic}");
            count += 1;
        }
    }
    match count {
        0 => println!("No problems"),
        1 => println!("1 problem"),
        n => println!("{n} problems"),
    }
    Ok(i32::from(count > 0))
}

fn run_project() -> Result<(), Error> {
    let project = project_at(Path::new("."))?;
    let mut interp = Interpreter::new();
    let mut dirs = project.load_path();
    dirs.push(PathBuf::from("."));
    interp.set_load_path(dirs);
    interp.load_file(&project.main_file())?;
    Ok(())
}

fn repl() -> i32 {
    let mut interp = Interpreter::new();
    interp.set_load_path(load_path_for(Path::new(".")));
    let stdin = io::stdin();
    let mut buffer = String::new();
    loop {
        let prompt = if buffer.is_empty() { "> " } else { "  " };
        print!("{prompt}");
        let _ = io::stdout().flush();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => buffer.push_str(&line),
        }
        let forms = match read_all(&buffer) {
            Err(e) if e.incomplete => continue,
            Err(e) => {
                eprintln!("error: {e}");
                buffer.clear();
                continue;
            }
            Ok(forms) => forms,
        };
        buffer.clear();
        for form in &forms {
            match interp.eval(form) {
                Ok(value) => println!("{value}"),
                Err(e) => {
                    eprintln!("error: {e}");
                    break;
                }
            }
        }
    }
    println!();
    0
}
