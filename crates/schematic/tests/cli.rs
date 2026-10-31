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
