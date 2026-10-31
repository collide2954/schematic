use std::io::{self, BufRead, Write};

use schematic::reader::read_all;
use schematic::Interpreter;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => repl(),
        ["--version"] | ["-V"] => {
            println!("schematic {}", env!("CARGO_PKG_VERSION"));
            0
        }
        ["-e", expr] => run_source(expr, true),
        [path] => run_file(path),
        _ => {
            eprintln!("usage: schematic [file | -e expression | --version]");
            2
        }
    };
    std::process::exit(code);
}

fn run_file(path: &str) -> i32 {
    match std::fs::read_to_string(path) {
        Ok(source) => run_source(&source, false),
        Err(e) => {
            eprintln!("error: cannot read {path}: {e}");
            2
        }
    }
}

fn run_source(source: &str, print_result: bool) -> i32 {
    let mut interp = Interpreter::new();
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

fn repl() -> i32 {
    let mut interp = Interpreter::new();
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
