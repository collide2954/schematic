use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender};

use iced::futures::channel::mpsc as futures_mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::stream;
use schematic::plot::Plot;
use schematic::reader::read_all;
use schematic::value::Shown;
use schematic::vm::Arity;
use schematic::{Interpreter, Value};

#[derive(Debug, Clone)]
pub enum Request {
    Run { source: String, show_results: bool, load_path: Vec<PathBuf> },
    Inspect { name: String, offset: usize, limit: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub name: String,
    pub kind: String,
    pub summary: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub name: String,
    pub columns: Vec<String>,
    pub numeric: Vec<bool>,
    pub rows: Vec<Vec<String>>,
    pub offset: usize,
    pub total: usize,
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready(Sender<Request>, Arc<AtomicBool>),
    Output(String),
    Result(String),
    Error(String),
    Environment(Vec<Binding>),
    Namespace(String),
    Grid(Grid),
    Plot(Box<Plot>),
    Finished,
}

type Row = Box<dyn Fn(usize) -> Vec<String>>;

impl PartialEq for Event {
    fn eq(&self, other: &Event) -> bool {
        use Event::*;
        match (self, other) {
            (Output(a), Output(b)) | (Result(a), Result(b)) | (Error(a), Error(b)) | (Namespace(a), Namespace(b)) => a == b,
            (Environment(a), Environment(b)) => a == b,
            (Grid(a), Grid(b)) => a == b,
            (Plot(a), Plot(b)) => a == b,
            (Finished, Finished) => true,
            _ => false,
        }
    }
}

struct Forward<F>(F);

impl<F: FnMut(Event)> Write for Forward<F> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        (self.0)(Event::Output(String::from_utf8_lossy(buf).into_owned()));
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn subscription() -> impl Stream<Item = Event> {
    stream::channel(100, async |mut output| {
        let (events, mut inbox) = futures_mpsc::unbounded();
        std::thread::spawn(move || {
            let (requests, queue) = std::sync::mpsc::channel();
            let interp = Interpreter::new();
            let _ = events.unbounded_send(Event::Ready(requests, interp.interrupt_handle()));
            run(interp, queue, move |event| {
                let _ = events.unbounded_send(event);
            });
        });
        while let Some(event) = inbox.next().await {
            let _ = output.send(event).await;
        }
    })
}

pub fn run(mut interp: Interpreter, requests: Receiver<Request>, mut emit: impl FnMut(Event) + Clone + 'static) {
    interp.set_output(Box::new(Forward(emit.clone())));
    for request in requests {
        match request {
            Request::Run { source, show_results, load_path } => {
                let mut dirs = load_path;
                dirs.push(PathBuf::from("."));
                interp.set_load_path(dirs);
                execute(&mut interp, &source, show_results, &mut emit);
                emit(Event::Environment(environment(&interp)));
                emit(Event::Namespace(interp.current_namespace()));
                emit(Event::Finished);
            }
            Request::Inspect { name, offset, limit } => match interp.global(&name) {
                Some(Value::Plot(plot)) => emit(Event::Plot(Box::new((**plot).clone()))),
                Some(value) => emit(Event::Grid(grid(&name, value, offset, limit))),
                None => emit(Event::Error(format!("{name} is not defined"))),
            },
        }
    }
}

fn execute(interp: &mut Interpreter, source: &str, show_results: bool, emit: &mut impl FnMut(Event)) {
    let forms = match read_all(source) {
        Ok(forms) => forms,
        Err(e) => return emit(Event::Error(e.to_string())),
    };
    for form in &forms {
        match interp.eval(form) {
            Ok(Value::Plot(plot)) => emit(Event::Plot(Box::new((*plot).clone()))),
            Ok(Value::Nil) => {}
            Ok(value) if show_results => emit(Event::Result(value.to_string())),
            Ok(_) => {}
            Err(e) => return emit(Event::Error(e.to_string())),
        }
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("{n} {word}") } else { format!("{n} {word}s") }
}

fn summarize(value: &Value) -> String {
    match value {
        Value::Table(t) => format!("{}, {}", plural(t.nrows(), "row"), plural(t.ncols(), "column")),
        Value::Array(a) => {
            let missing = a.missing_count();
            let base = format!("{} {} values", a.len(), a.kind());
            if missing > 0 { format!("{base}, {missing} missing") } else { base }
        }
        Value::Vector(v) => plural(v.len(), "item"),
        Value::List(l) => plural(l.len(), "item"),
        Value::Set(s) => plural(s.len(), "item"),
        Value::Map(m) if m.len() == 1 => "1 entry".to_string(),
        Value::Map(m) => format!("{} entries", m.len()),
        Value::Closure(c) => {
            let arity = if c.proto.rest { Arity::AtLeast(c.proto.params) } else { Arity::Exact(c.proto.params) };
            arity.to_string()
        }
        Value::Native(n) => n.arity.to_string(),
        Value::Plot(p) => p.describe(),
        other => {
            let shown = other.to_string();
            let mut chars = shown.chars();
            let short: String = chars.by_ref().take(60).collect();
            if chars.next().is_some() { format!("{short}...") } else { short }
        }
    }
}

fn columns_of(value: &Value) -> Vec<String> {
    match value {
        Value::Table(t) => t.names().iter().map(|n| n.to_string()).collect(),
        _ => Vec::new(),
    }
}

fn environment(interp: &Interpreter) -> Vec<Binding> {
    interp
        .user_globals()
        .map(|(name, value)| Binding { name, kind: value.type_name().to_string(), summary: summarize(value), columns: columns_of(value) })
        .collect()
}

fn cell(value: &Value) -> String {
    Shown(value).to_string()
}

fn is_number(value: &Value) -> bool {
    matches!(value, Value::Int(_) | Value::Float(_))
}

fn grid(name: &str, value: &Value, offset: usize, limit: usize) -> Grid {
    let (columns, numeric, total, row): (Vec<String>, Vec<bool>, usize, Row) = match value {
        Value::Table(t) => {
            let mut columns = vec!["#".to_string()];
            columns.extend(t.names().iter().map(|n| n.to_string()));
            let mut numeric = vec![true];
            numeric.extend(t.columns().map(|(_, column)| matches!(column.kind(), "int" | "float")));
            let t = t.clone();
            (columns, numeric, t.nrows(), Box::new(move |i| {
                let mut cells = vec![i.to_string()];
                cells.extend(t.columns().map(|(_, column)| cell(&column.get(i).unwrap_or(Value::Nil))));
                cells
            }))
        }
        Value::Map(m) => {
            let pairs: Vec<(Value, Value)> = m.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let numeric = vec![false, pairs.iter().all(|(_, v)| is_number(v))];
            (vec!["key".into(), "value".into()], numeric, pairs.len(), Box::new(move |i| vec![pairs[i].0.to_string(), cell(&pairs[i].1)]))
        }
        Value::Vector(_) | Value::List(_) | Value::Set(_) | Value::Array(_) => {
            let items: Vec<Value> = schematic::builtins::collections::seq_items(value).map(Iterator::collect).unwrap_or_default();
            let numeric = vec![true, items.iter().all(is_number)];
            (vec!["#".into(), "value".into()], numeric, items.len(), Box::new(move |i| vec![i.to_string(), cell(&items[i])]))
        }
        other => {
            let shown = cell(other);
            (vec!["value".into()], vec![is_number(other)], 1, Box::new(move |_| vec![shown.clone()]))
        }
    };
    let end = (offset + limit).min(total);
    let rows = (offset.min(end)..end).map(row).collect();
    Grid { name: name.to_string(), columns, numeric, rows, offset: offset.min(total), total }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::mpsc;

    use super::*;

    fn drive(requests: Vec<Request>) -> Vec<Event> {
        let (tx, rx) = mpsc::channel();
        for request in requests {
            tx.send(request).unwrap();
        }
        drop(tx);
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        run(Interpreter::new(), rx, move |e| sink.borrow_mut().push(e));
        Rc::try_unwrap(events).unwrap().into_inner()
    }

    fn run_request(source: &str) -> Request {
        Request::Run { source: source.to_string(), show_results: true, load_path: Vec::new() }
    }

    fn quiet_request(source: &str) -> Request {
        Request::Run { source: source.to_string(), show_results: false, load_path: Vec::new() }
    }

    fn binding(name: &str, kind: &str, summary: &str) -> Binding {
        Binding { name: name.into(), kind: kind.into(), summary: summary.into(), columns: Vec::new() }
    }

    fn table(name: &str, summary: &str, columns: &[&str]) -> Binding {
        Binding { columns: columns.iter().map(|c| c.to_string()).collect(), ..binding(name, "table", summary) }
    }

    #[test]
    fn running_code_reports_output_results_and_the_environment() {
        let events = drive(vec![run_request("(define t (table {:a [1 2]})) (println \"hi\") (+ 1 2)")]);
        assert_eq!(
            events,
            vec![
                Event::Result("t".into()),
                Event::Output("hi\n".into()),
                Event::Result("3".into()),
                Event::Environment(vec![table("t", "2 rows, 1 column", &["a"])]),
                Event::Namespace("user".into()),
                Event::Finished,
            ]
        );
    }

    #[test]
    fn quiet_runs_report_only_output_and_errors() {
        let events = drive(vec![quiet_request("(define a 1) (print a) (error \"boom\") (define b 2)")]);
        assert_eq!(
            events,
            vec![
                Event::Output("1".into()),
                Event::Error("boom".into()),
                Event::Environment(vec![binding("a", "integer", "1")]),
                Event::Namespace("user".into()),
                Event::Finished,
            ]
        );
        let events = drive(vec![run_request("(+ 1")]);
        assert!(matches!(&events[0], Event::Error(_)), "{events:?}");
        assert_eq!(events[1..], [Event::Environment(vec![]), Event::Namespace("user".into()), Event::Finished]);
    }

    #[test]
    fn runs_report_the_namespace_they_end_in_and_load_from_the_script_folder() {
        let dir = std::env::temp_dir().join(format!("studio-ns-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("helpers")).unwrap();
        std::fs::write(dir.join("helpers").join("text.scm"), "(ns helpers.text)\n(define (loud s) (string-upcase s))\n").unwrap();
        let events = drive(vec![Request::Run {
            source: "(ns report (:require [helpers.text :as t])) (define shout (t/loud \"hi\"))".into(),
            show_results: false,
            load_path: vec![dir.clone()],
        }]);
        assert_eq!(
            events,
            vec![
                Event::Environment(vec![binding("shout", "string", "\"HI\"")]),
                Event::Namespace("report".into()),
                Event::Finished,
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bindings_summarize_values() {
        let source = format!(
            "(define t (table {{:a [1]}})) (define xs #[1 2 nil]) (define v [1 2 3]) (define m {{:a 1 :b 2}}) \
             (define (f x & more) x) (define g inc) (define s \"{}\") (define n 2.5)",
            "x".repeat(70)
        );
        let events = drive(vec![quiet_request(&source)]);
        let Event::Environment(bindings) = &events[0] else { panic!("{events:?}") };
        assert_eq!(
            *bindings,
            vec![
                table("t", "1 row, 1 column", &["a"]),
                binding("xs", "array", "3 int values, 1 missing"),
                binding("v", "vector", "3 items"),
                binding("m", "map", "2 entries"),
                binding("f", "function", "at least 1 argument"),
                binding("g", "function", "1 argument"),
                binding("s", "string", &format!("\"{}...", "x".repeat(59))),
                binding("n", "float", "2.5"),
            ]
        );
    }

    #[test]
    fn plots_are_events_of_their_own() {
        let events = drive(vec![
            quiet_request("(define p (bar-chart [\"a\"] [1])) p"),
            Request::Inspect { name: "p".into(), offset: 0, limit: 10 },
        ]);
        assert!(matches!(&events[0], Event::Plot(plot) if plot.kind == schematic::plot::Kind::Bar), "{events:?}");
        assert_eq!(events[1], Event::Environment(vec![binding("p", "plot", "bar chart with 1 bar")]));
        assert_eq!(events[2], Event::Namespace("user".into()));
        assert_eq!(events[3], Event::Finished);
        assert!(matches!(&events[4], Event::Plot(_)), "{events:?}");
        assert_eq!(events.len(), 5);
    }

    #[test]
    fn inspecting_values_pages_them_into_grids() {
        let setup = "(define t (table {:city [\"Oslo\" \"Rome\" \"Bern\"] :n [1 2 nil]})) (define v [10 20]) (define m {:a 1}) (define s \"text\")";
        let inspect = |name: &str, offset, limit| Request::Inspect { name: name.into(), offset, limit };
        let events = drive(vec![
            quiet_request(setup),
            inspect("t", 1, 1),
            inspect("v", 0, 10),
            inspect("m", 0, 10),
            inspect("s", 0, 10),
            inspect("t", 99, 10),
            inspect("nope", 0, 10),
        ]);
        let grid = |name: &str, columns: &[&str], numeric: &[bool], rows: &[&[&str]], offset, total| {
            Event::Grid(Grid {
                name: name.into(),
                columns: columns.iter().map(|c| c.to_string()).collect(),
                numeric: numeric.to_vec(),
                rows: rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect(),
                offset,
                total,
            })
        };
        assert_eq!(
            events[3..],
            [
                grid("t", &["#", "city", "n"], &[true, false, true], &[&["1", "Rome", "2"]], 1, 3),
                grid("v", &["#", "value"], &[true, true], &[&["0", "10"], &["1", "20"]], 0, 2),
                grid("m", &["key", "value"], &[false, true], &[&[":a", "1"]], 0, 1),
                grid("s", &["value"], &[false], &[&["text"]], 0, 1),
                grid("t", &["#", "city", "n"], &[true, false, true], &[], 3, 3),
                Event::Error("nope is not defined".into()),
            ]
        );
    }
}
