use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_schematic");

fn temp_file(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("schematic-cli-{}-{name}.scm", std::process::id()));
    std::fs::write(&path, contents).unwrap();
    path
}

fn run(args: &[&str], stdin: &str) -> (i32, String, String) {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
    )
}

#[test]
fn runs_a_file() {
    let path = temp_file("runs", "(define (sq x) (* x x))\n(println (sq 7))\n");
    let (code, out, err) = run(&[path.to_str().unwrap()], "");
    assert_eq!((code, out.as_str(), err.as_str()), (0, "49\n", ""));
}

#[test]
fn reports_errors_from_a_file_on_stderr() {
    let path = temp_file("errors", "(println 1)\n(error \"boom\")\n(println 2)\n");
    let (code, out, err) = run(&[path.to_str().unwrap()], "");
    assert_eq!((code, out.as_str()), (1, "1\n"));
    assert!(err.contains("error: boom"), "{err}");
}

fn run_in(dir: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(BIN).args(args).current_dir(dir).stdin(Stdio::null()).output().unwrap();
    (output.status.code().unwrap_or(-1), String::from_utf8_lossy(&output.stdout).into_owned(), String::from_utf8_lossy(&output.stderr).into_owned())
}

#[test]
fn projects_are_created_restored_and_run_from_the_command_line() {
    let root = std::env::temp_dir().join(format!("schematic-cli-project-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let (code, out, _) = run_in(&root, &["new", "demo"]);
    assert_eq!((code, out.as_str()), (0, "Created demo/ with project.scm and src/demo.scm\n"));
    let demo = root.join("demo");
    assert!(demo.join("project.scm").is_file() && demo.join("src").join("demo.scm").is_file());
    let (code, out, _) = run_in(&demo, &["run"]);
    assert_eq!((code, out.as_str()), (0, "hello from demo\n"));
    let (code, out, _) = run_in(&demo, &["restore"]);
    assert_eq!((code, out.as_str()), (0, "demo has no dependencies\n"));
    std::fs::create_dir_all(root.join("lib").join("src")).unwrap();
    std::fs::write(root.join("lib").join("src").join("lib.scm"), "(ns lib)\n(define x 7)\n").unwrap();
    let (code, out, err) = run_in(&demo, &["add", "lib", "--path", "../lib"]);
    assert_eq!((code, out.as_str()), (0, "Added lib from ../lib\n"), "{err}");
    std::fs::write(demo.join("src").join("demo.scm"), "(ns demo (:require [lib]))\n(println lib/x)\n").unwrap();
    assert_eq!(run_in(&demo, &["run"]).1, "7\n");
    assert_eq!(run_in(&demo, &["src/demo.scm"]).1, "7\n");
    let (code, out, _) = run_in(&demo, &["restore", "--locked"]);
    assert_eq!(code, 0);
    assert!(out.contains("lib") && out.contains("local path"), "{out}");
    let (code, out, _) = run_in(&demo, &["packages"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("lib ") && out.contains("no license") && out.contains("local path"), "{out}");
    let (code, out, _) = run_in(&demo, &["remove", "lib"]);
    assert_eq!((code, out.as_str()), (0, "Removed lib\n"));
    let (code, _, err) = run_in(&demo, &["run"]);
    assert_eq!(code, 1);
    assert!(err.contains("cannot find namespace lib"), "{err}");
    let (code, _, err) = run_in(&root, &["add", "x"]);
    assert_eq!(code, 1);
    assert!(err.contains("no project.scm"), "{err}");
    assert_eq!(run_in(&demo, &["add"]).0, 2);
    let (code, _, err) = run_in(&demo, &["add", "x", "--git", "u"]);
    assert_eq!(code, 1);
    assert!(err.contains("exactly one of --tag or --commit"), "{err}");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn check_reports_problems_with_positions() {
    let path = temp_file("check", "(define n 3)\n(string-upcase n)\n(define (sq x) (* x x))\n(sq 1 2)\n");
    let shown = path.to_str().unwrap();
    let (code, out, err) = run(&["check", shown], "");
    assert_eq!(code, 1, "{err}");
    assert_eq!(out, format!("{shown}:2:16: string-upcase expects a string, got an integer\n{shown}:4:1: sq expects 1 argument, got 2\n2 problems\n"));
    let clean = temp_file("check-clean", "(define (sq [x : int]) : int (* x x))\n(sq 7)\n");
    let (code, out, _) = run(&["check", clean.to_str().unwrap()], "");
    assert_eq!((code, out.as_str()), (0, "No problems\n"));
    let (code, out, _) = run(&["check", "definitely-missing-file.scm"], "");
    assert_eq!(code, 1);
    assert!(out.starts_with("definitely-missing-file.scm:1:1: cannot read"), "{out}");
}

#[test]
fn check_covers_a_project_when_no_file_is_given() {
    let root = std::env::temp_dir().join(format!("schematic-cli-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    assert_eq!(run_in(&root, &["new", "demo"]).0, 0);
    let demo = root.join("demo");
    assert_eq!(run_in(&demo, &["check"]), (0, "No problems\n".to_string(), String::new()));
    std::fs::write(demo.join("src").join("extra.scm"), "(ns extra)\n(string-length 1)\n").unwrap();
    let (code, out, _) = run_in(&demo, &["check"]);
    assert_eq!((code, out.as_str()), (1, "src/extra.scm:2:16: string-length expects a string, got an integer\n1 problem\n"));
    let (code, _, err) = run_in(&root, &["check"]);
    assert_eq!(code, 1);
    assert!(err.contains("no project.scm"), "{err}");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn missing_file_is_an_error() {
    let (code, _, err) = run(&["definitely-missing-file.scm"], "");
    assert_eq!(code, 2);
    assert!(err.contains("definitely-missing-file.scm"), "{err}");
}

#[test]
fn evaluates_an_expression_from_the_command_line() {
    let (code, out, _) = run(&["-e", "(* 6 7)"], "");
    assert_eq!((code, out.as_str()), (0, "42\n"));
}

#[test]
fn exit_stops_the_program_with_a_code() {
    let path = temp_file("exit", "(println 1)\n(exit 3)\n(println 2)\n");
    let (code, out, _) = run(&[path.to_str().unwrap()], "");
    assert_eq!((code, out.as_str()), (3, "1\n"));
}

#[test]
fn the_tour_example_runs_clean() {
    let tour = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/tour.scm");
    let (code, out, err) = run(&[tour], "");
    assert_eq!((code, out.as_str(), err.as_str()), (0, "tour complete\n", ""));
}

#[test]
fn prints_the_version() {
    let (code, out, _) = run(&["--version"], "");
    assert_eq!(code, 0);
    assert!(out.starts_with("schematic "), "{out}");
}

#[test]
fn repl_evaluates_lines_and_continues_incomplete_input() {
    let (code, out, err) = run(&[], "(+ 1 2)\n(define x 5)\n(+ x\n   1)\n\"s\"\n");
    assert_eq!(code, 0);
    assert_eq!(err, "");
    assert!(out.contains("3\n"), "{out}");
    assert!(out.contains("x\n"), "{out}");
    assert!(out.contains("6\n"), "{out}");
    assert!(out.contains("\"s\"\n"), "{out}");
}

#[test]
fn repl_reports_errors_and_keeps_going() {
    let (code, out, err) = run(&[], "(undefined-thing)\n(+ 1 1)\n(1 2\n");
    assert_eq!(code, 0);
    assert!(err.contains("undefined-thing is not defined"), "{err}");
    assert!(out.contains("2\n"), "{out}");
}
