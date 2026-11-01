use std::cmp::Ordering;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use iced::futures::channel::mpsc as futures_mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::stream;
use schematic::array::Array;
use schematic::builtins::functional::compare_values;
use schematic::plot::Plot;
use schematic::reader::read_all;
use schematic::value::Shown;
use schematic::vm::Arity;
use schematic::{Interpreter, Value};

#[derive(Debug, Clone)]
pub enum Request {
    Run { tag: u64, source: String, show_results: bool, load_path: Vec<PathBuf>, watches: Vec<String> },
    Inspect { name: String, offset: usize, limit: usize, sort: Option<(String, bool)>, filter: String },
    Evaluate { source: String, label: String },
    Watch { watches: Vec<String> },
    Restart,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub name: String,
    pub kind: String,
    pub summary: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Grid {
    pub name: String,
    pub columns: Vec<String>,
    pub numeric: Vec<bool>,
    pub stats: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub offset: usize,
    pub total: usize,
    pub sort: Option<(String, bool)>,
    pub filter: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Watched {
    pub source: String,
    pub value: Result<String, String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready(Sender<Request>, Arc<AtomicBool>),
    Started(u64),
    Output(String),
    Result(String),
    Error(String),
    Environment(Vec<Binding>),
    Namespace(String),
    Grid(Grid),
    Plot(Box<Plot>),
    Watches(Vec<Watched>),
    Evaluated { label: String, error: Option<String> },
    Restarted(Arc<AtomicBool>),
    Finished(u128),
}

impl PartialEq for Event {
    fn eq(&self, other: &Event) -> bool {
        use Event::*;
        match (self, other) {
            (Output(a), Output(b)) | (Result(a), Result(b)) | (Error(a), Error(b)) | (Namespace(a), Namespace(b)) => a == b,
            (Started(a), Started(b)) => a == b,
            (Environment(a), Environment(b)) => a == b,
            (Grid(a), Grid(b)) => a == b,
            (Plot(a), Plot(b)) => a == b,
            (Watches(a), Watches(b)) => a == b,
            (Evaluated { label: a, error: x }, Evaluated { label: b, error: y }) => a == b && x == y,
            (Restarted(_), Restarted(_)) => true,
            (Finished(_), Finished(_)) => true,
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

pub fn ticks() -> impl Stream<Item = ()> {
    stream::channel(1, async |mut output| {
        let (ticks, mut inbox) = futures_mpsc::unbounded();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                if ticks.unbounded_send(()).is_err() {
                    break;
                }
            }
        });
        while inbox.next().await.is_some() {
            let _ = output.send(()).await;
        }
    })
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
            Request::Run { tag, source, show_results, load_path, watches } => {
                let mut dirs = load_path;
                dirs.push(PathBuf::from("."));
                interp.set_load_path(dirs);
                emit(Event::Started(tag));
                let started = Instant::now();
                execute(&mut interp, &source, show_results, &mut emit);
                let elapsed = started.elapsed().as_millis();
                emit(Event::Environment(environment(&interp)));
                emit(Event::Namespace(interp.current_namespace()));
                if !watches.is_empty() {
                    emit(Event::Watches(watch(&mut interp, &watches)));
                }
                emit(Event::Finished(elapsed));
            }
            Request::Inspect { name, offset, limit, sort, filter } => match interp.global(&name) {
                Some(Value::Plot(plot)) => emit(Event::Plot(Box::new((**plot).clone()))),
                Some(value) => emit(Event::Grid(grid(&name, value, offset, limit, sort.as_ref().map(|(c, d)| (c.as_str(), *d)), &filter))),
                None => emit(Event::Error(format!("{name} is not defined"))),
            },
            Request::Evaluate { source, label } => {
                let error = evaluate(&mut interp, &source, &mut emit);
                emit(Event::Evaluated { label, error });
            }
            Request::Watch { watches } => emit(Event::Watches(watch(&mut interp, &watches))),
            Request::Restart => {
                interp = Interpreter::new();
                interp.set_output(Box::new(Forward(emit.clone())));
                emit(Event::Restarted(interp.interrupt_handle()));
                emit(Event::Environment(Vec::new()));
                emit(Event::Namespace(interp.current_namespace()));
            }
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

fn evaluate(interp: &mut Interpreter, source: &str, emit: &mut impl FnMut(Event)) -> Option<String> {
    match interp.eval_str(source) {
        Ok(Value::Plot(plot)) => {
            emit(Event::Plot(Box::new((*plot).clone())));
            None
        }
        Ok(_) => None,
        Err(e) => Some(e.to_string()),
    }
}

fn watch(interp: &mut Interpreter, watches: &[String]) -> Vec<Watched> {
    watches
        .iter()
        .map(|source| {
            let value = interp.eval_str(source).map(|v| v.to_string()).map_err(|e| e.to_string());
            Watched { source: source.clone(), value }
        })
        .collect()
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

fn trim_float(x: f64) -> String {
    let text = format!("{x:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn column_stats(array: &Array) -> String {
    let missing = array.missing_count();
    let present = array.len() - missing;
    let mut parts = vec![array.kind().to_string(), plural(present, "value")];
    if missing > 0 {
        parts.push(format!("{missing} missing"));
    }
    if matches!(array.kind(), "int" | "float") {
        let numbers: Vec<f64> = array
            .iter()
            .filter_map(|v| match v {
                Value::Int(i) => Some(i as f64),
                Value::Float(x) => Some(x),
                _ => None,
            })
            .collect();
        if !numbers.is_empty() {
            let min = numbers.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = numbers.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let mean = numbers.iter().sum::<f64>() / numbers.len() as f64;
            parts.push(format!("min {}", trim_float(min)));
            parts.push(format!("max {}", trim_float(max)));
            parts.push(format!("mean {}", trim_float(mean)));
        }
    }
    parts.join(" · ")
}

type Cell = Box<dyn Fn(usize, usize) -> Value>;

struct Source {
    columns: Vec<String>,
    numeric: Vec<bool>,
    stats: Vec<String>,
    total: usize,
    cell: Cell,
}

fn source_of(value: &Value) -> Source {
    match value {
        Value::Table(t) => {
            let mut columns = vec!["#".to_string()];
            columns.extend(t.names().iter().map(|n| n.to_string()));
            let mut numeric = vec![true];
            numeric.extend(t.columns().map(|(_, column)| matches!(column.kind(), "int" | "float")));
            let mut stats = vec![String::new()];
            stats.extend(t.columns().map(|(_, column)| column_stats(column)));
            let arrays: Vec<Rc<Array>> = t.columns().map(|(_, column)| column.clone()).collect();
            let cell: Cell = Box::new(move |r, c| if c == 0 { Value::Int(r as i64) } else { arrays[c - 1].get(r).unwrap_or(Value::Nil) });
            Source { columns, numeric, stats, total: t.nrows(), cell }
        }
        Value::Map(m) => {
            let pairs: Vec<(Value, Value)> = m.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let numeric = vec![false, pairs.iter().all(|(_, v)| is_number(v))];
            let total = pairs.len();
            let cell: Cell = Box::new(move |r, c| if c == 0 { pairs[r].0.clone() } else { pairs[r].1.clone() });
            Source { columns: vec!["key".into(), "value".into()], numeric, stats: vec![String::new(); 2], total, cell }
        }
        Value::Vector(_) | Value::List(_) | Value::Set(_) | Value::Array(_) => {
            let items: Vec<Value> = schematic::builtins::collections::seq_items(value).map(Iterator::collect).unwrap_or_default();
            let numeric = vec![true, items.iter().all(is_number)];
            let stats = match value {
                Value::Array(a) => vec![String::new(), column_stats(a)],
                _ => vec![String::new(); 2],
            };
            let total = items.len();
            let cell: Cell = Box::new(move |r, c| if c == 0 { Value::Int(r as i64) } else { items[r].clone() });
            Source { columns: vec!["#".into(), "value".into()], numeric, stats, total, cell }
        }
        other => {
            let shown = other.clone();
            let numeric = vec![is_number(other)];
            let cell: Cell = Box::new(move |_, _| shown.clone());
            Source { columns: vec!["value".into()], numeric, stats: vec![String::new()], total: 1, cell }
        }
    }
}

fn grid(name: &str, value: &Value, offset: usize, limit: usize, sort: Option<(&str, bool)>, filter: &str) -> Grid {
    let source = source_of(value);
    let width = source.columns.len();
    let needle = filter.trim().to_lowercase();
    let mut indices: Vec<usize> = (0..source.total)
        .filter(|&r| needle.is_empty() || (0..width).any(|c| cell(&(source.cell)(r, c)).to_lowercase().contains(&needle)))
        .collect();
    if let Some((column, descending)) = sort
        && let Some(c) = source.columns.iter().position(|n| n == column) {
            indices.sort_by(|&a, &b| {
                let order = compare_values("sort", &(source.cell)(a, c), &(source.cell)(b, c)).unwrap_or(Ordering::Equal);
                if descending { order.reverse() } else { order }
            });
        }
    let total = indices.len();
    let end = (offset + limit).min(total);
    let rows = indices[offset.min(end)..end].iter().map(|&r| (0..width).map(|c| cell(&(source.cell)(r, c))).collect()).collect();
    Grid {
        name: name.to_string(),
        columns: source.columns,
        numeric: source.numeric,
        stats: source.stats,
        rows,
        offset: offset.min(total),
        total,
        sort: sort.map(|(c, d)| (c.to_string(), d)),
        filter: filter.to_string(),
    }
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
        Request::Run { tag: 7, source: source.to_string(), show_results: true, load_path: Vec::new(), watches: Vec::new() }
    }

    fn quiet_request(source: &str) -> Request {
        Request::Run { tag: 7, source: source.to_string(), show_results: false, load_path: Vec::new(), watches: Vec::new() }
    }

    fn inspect(name: &str, offset: usize, limit: usize) -> Request {
        Request::Inspect { name: name.into(), offset, limit, sort: None, filter: String::new() }
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
                Event::Started(7),
                Event::Result("t".into()),
                Event::Output("hi\n".into()),
                Event::Result("3".into()),
                Event::Environment(vec![table("t", "2 rows, 1 column", &["a"])]),
                Event::Namespace("user".into()),
                Event::Finished(0),
            ]
        );
    }

    #[test]
    fn quiet_runs_report_only_output_and_errors() {
        let events = drive(vec![quiet_request("(define a 1) (print a) (error \"boom\") (define b 2)")]);
        assert_eq!(
            events,
            vec![
                Event::Started(7),
                Event::Output("1".into()),
                Event::Error("boom".into()),
                Event::Environment(vec![binding("a", "integer", "1")]),
                Event::Namespace("user".into()),
                Event::Finished(0),
            ]
        );
        let events = drive(vec![run_request("(+ 1")]);
        assert!(matches!(&events[1], Event::Error(_)), "{events:?}");
        assert_eq!(events[2..], [Event::Environment(vec![]), Event::Namespace("user".into()), Event::Finished(0)]);
    }

    #[test]
    fn runs_report_the_namespace_they_end_in_and_load_from_the_script_folder() {
        let dir = std::env::temp_dir().join(format!("studio-ns-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("helpers")).unwrap();
        std::fs::write(dir.join("helpers").join("text.scm"), "(ns helpers.text)\n(define (loud s) (string-upcase s))\n").unwrap();
        let events = drive(vec![Request::Run {
            tag: 1,
            source: "(ns report (:require [helpers.text :as t])) (define shout (t/loud \"hi\"))".into(),
            show_results: false,
            load_path: vec![dir.clone()],
            watches: Vec::new(),
        }]);
        assert_eq!(
            events,
            vec![
                Event::Started(1),
                Event::Environment(vec![binding("shout", "string", "\"HI\"")]),
                Event::Namespace("report".into()),
                Event::Finished(0),
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
        let Event::Environment(bindings) = &events[1] else { panic!("{events:?}") };
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
        let events = drive(vec![quiet_request("(define p (bar-chart [\"a\"] [1])) p"), inspect("p", 0, 10)]);
        assert!(matches!(&events[1], Event::Plot(plot) if plot.kind == schematic::plot::Kind::Bar), "{events:?}");
        assert_eq!(events[2], Event::Environment(vec![binding("p", "plot", "bar chart with 1 bar")]));
        assert_eq!(events[3], Event::Namespace("user".into()));
        assert_eq!(events[4], Event::Finished(0));
        assert!(matches!(&events[5], Event::Plot(_)), "{events:?}");
        assert_eq!(events.len(), 6);
    }

    fn grid_event(name: &str, columns: &[&str], numeric: &[bool], rows: &[&[&str]], offset: usize, total: usize) -> Grid {
        Grid {
            name: name.into(),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            numeric: numeric.to_vec(),
            stats: vec![String::new(); columns.len()],
            rows: rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect(),
            offset,
            total,
            ..Grid::default()
        }
    }

    fn without_stats(event: Event) -> Event {
        match event {
            Event::Grid(mut grid) => {
                grid.stats = vec![String::new(); grid.columns.len()];
                Event::Grid(grid)
            }
            other => other,
        }
    }

    #[test]
    fn inspecting_values_pages_them_into_grids() {
        let setup = "(define t (table {:city [\"Oslo\" \"Rome\" \"Bern\"] :n [1 2 nil]})) (define v [10 20]) (define m {:a 1}) (define s \"text\")";
        let events: Vec<Event> = drive(vec![
            quiet_request(setup),
            inspect("t", 1, 1),
            inspect("v", 0, 10),
            inspect("m", 0, 10),
            inspect("s", 0, 10),
            inspect("t", 99, 10),
            inspect("nope", 0, 10),
        ])
        .into_iter()
        .map(without_stats)
        .collect();
        assert_eq!(
            events[4..],
            [
                Event::Grid(grid_event("t", &["#", "city", "n"], &[true, false, true], &[&["1", "Rome", "2"]], 1, 3)),
                Event::Grid(grid_event("v", &["#", "value"], &[true, true], &[&["0", "10"], &["1", "20"]], 0, 2)),
                Event::Grid(grid_event("m", &["key", "value"], &[false, true], &[&[":a", "1"]], 0, 1)),
                Event::Grid(grid_event("s", &["value"], &[false], &[&["text"]], 0, 1)),
                Event::Grid(grid_event("t", &["#", "city", "n"], &[true, false, true], &[], 3, 3)),
                Event::Error("nope is not defined".into()),
            ]
        );
    }

    #[test]
    fn grids_sort_filter_and_describe_their_columns() {
        let setup = "(define t (table {:city [\"Oslo\" \"Rome\" \"Bern\"] :n [1 2 nil]}))";
        let sorted = |descending: bool| Request::Inspect { name: "t".into(), offset: 0, limit: 10, sort: Some(("n".into(), descending)), filter: String::new() };
        let filtered = Request::Inspect { name: "t".into(), offset: 0, limit: 10, sort: Some(("city".into(), false)), filter: "o".into() };
        let events = drive(vec![quiet_request(setup), sorted(false), sorted(true), filtered]);
        let cities = |event: &Event| match event {
            Event::Grid(grid) => grid.rows.iter().map(|r| r[1].clone()).collect::<Vec<_>>(),
            other => panic!("{other:?}"),
        };
        assert_eq!(cities(&events[4]), ["Bern", "Oslo", "Rome"]);
        assert_eq!(cities(&events[5]), ["Rome", "Oslo", "Bern"]);
        assert_eq!(cities(&events[6]), ["Oslo", "Rome"]);
        let Event::Grid(grid) = &events[6] else { panic!() };
        assert_eq!((grid.total, grid.sort.clone(), grid.filter.as_str()), (2, Some(("city".to_string(), false)), "o"));
        assert_eq!(grid.stats, ["", "string · 3 values", "int · 2 values · 1 missing · min 1 · max 2 · mean 1.5"]);
    }

    #[test]
    fn watches_evaluate_after_runs_and_on_request_and_restarts_forget_everything() {
        let events = drive(vec![
            Request::Run { tag: 3, source: "(define x 2)".into(), show_results: false, load_path: Vec::new(), watches: vec!["(* x 10)".into(), "(oops)".into()] },
            Request::Watch { watches: vec!["x".into()] },
            Request::Evaluate { source: "(histogram [1 2 3])".into(), label: "plotted".into() },
            Request::Evaluate { source: "(nope)".into(), label: "failed".into() },
            Request::Restart,
            Request::Watch { watches: vec!["x".into()] },
        ]);
        let watched = |source: &str, value: Result<&str, &str>| Watched { source: source.into(), value: value.map(str::to_string).map_err(str::to_string) };
        assert_eq!(events[0], Event::Started(3));
        assert_eq!(events[3], Event::Watches(vec![watched("(* x 10)", Ok("20")), watched("(oops)", Err("oops is not defined"))]));
        assert_eq!(events[4], Event::Finished(0));
        assert_eq!(events[5], Event::Watches(vec![watched("x", Ok("2"))]));
        assert!(matches!(&events[6], Event::Plot(_)), "{events:?}");
        assert_eq!(events[7], Event::Evaluated { label: "plotted".into(), error: None });
        assert_eq!(events[8], Event::Evaluated { label: "failed".into(), error: Some("nope is not defined".into()) });
        assert!(matches!(&events[9], Event::Restarted(_)), "{events:?}");
        assert_eq!(events[10..12], [Event::Environment(vec![]), Event::Namespace("user".into())]);
        assert_eq!(events[12], Event::Watches(vec![watched("x", Err("x is not defined"))]));
    }
}
