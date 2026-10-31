mod view;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use iced::widget::scrollable::RelativeOffset;
use iced::widget::{Id, operation, pane_grid, text_editor};
use iced::{Subscription, Task, Theme};
use schematic::reader::read_all;
use schematic_studio::forms;
use schematic_studio::worker::{self, Binding, Event, Grid, Request};

pub const PAGE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    Editor,
    Console,
    Environment,
    Viewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Input,
    Output,
    Result,
    Error,
    Note,
}

pub struct Line {
    pub kind: LineKind,
    pub text: String,
}

pub struct Studio {
    pub panes: pane_grid::State<PaneKind>,
    pub editor: text_editor::Content,
    pub path: String,
    pub dirty: bool,
    pub console: Vec<Line>,
    pub input: String,
    pub pending: String,
    pub bindings: Vec<Binding>,
    pub grid: Option<Grid>,
    pub worker: Option<Sender<Request>>,
    pub interrupt: Option<Arc<AtomicBool>>,
    pub running: bool,
    pub theme: Theme,
    pub status: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Worker(Event),
    Edit(text_editor::Action),
    PathChanged(String),
    Open,
    Save,
    RunForm,
    RunFile,
    Stop,
    InputChanged(String),
    Submit,
    ClearConsole,
    Inspect(String),
    Page(i64),
    PaneResized(pane_grid::ResizeEvent),
    PaneDragged(pane_grid::DragEvent),
    ThemeChanged(Theme),
}

fn main() -> iced::Result {
    iced::application(Studio::boot, Studio::update, view::view)
        .title(Studio::title)
        .subscription(Studio::subscription)
        .theme(Studio::theme)
        .window_size((1280.0, 820.0))
        .run()
}

pub fn console_id() -> Id {
    Id::new("console")
}

impl Studio {
    fn new() -> Studio {
        use pane_grid::{Axis, Configuration};
        let split = |axis, ratio, a, b| Configuration::Split { axis, ratio, a: Box::new(a), b: Box::new(b) };
        let layout = split(
            Axis::Vertical,
            0.6,
            split(Axis::Horizontal, 0.62, Configuration::Pane(PaneKind::Editor), Configuration::Pane(PaneKind::Console)),
            split(Axis::Horizontal, 0.4, Configuration::Pane(PaneKind::Environment), Configuration::Pane(PaneKind::Viewer)),
        );
        let mut studio = Studio {
            panes: pane_grid::State::with_configuration(layout),
            editor: text_editor::Content::new(),
            path: String::new(),
            dirty: false,
            console: Vec::new(),
            input: String::new(),
            pending: String::new(),
            bindings: Vec::new(),
            grid: None,
            worker: None,
            interrupt: None,
            running: false,
            theme: Theme::Light,
            status: "Starting the interpreter".to_string(),
        };
        studio.note("Welcome to Schematic Studio. Ctrl+Enter runs the form at the cursor, Ctrl+Shift+Enter runs the whole file.");
        studio
    }

    fn boot() -> Studio {
        let mut studio = Studio::new();
        if let Some(path) = std::env::args().nth(1) {
            studio.path = path;
            studio.open();
        }
        studio
    }

    fn title(&self) -> String {
        let name = Path::new(&self.path).file_name().and_then(|n| n.to_str()).unwrap_or("Untitled");
        let mark = if self.dirty { "*" } else { "" };
        format!("{name}{mark} - Schematic Studio")
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run(worker::subscription).map(Message::Worker)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Worker(event) => self.on_event(event),
            Message::Edit(action) => {
                if action.is_edit() {
                    self.dirty = true;
                }
                self.editor.perform(action);
                Task::none()
            }
            Message::PathChanged(path) => {
                self.path = path;
                Task::none()
            }
            Message::Open => {
                self.open();
                Task::none()
            }
            Message::Save => {
                self.save();
                Task::none()
            }
            Message::RunForm => self.run_form(),
            Message::RunFile => {
                let source = self.editor.text();
                self.note(&format!("Running {}", self.file_label()));
                self.run(source, false)
            }
            Message::Stop => {
                if let Some(flag) = &self.interrupt {
                    flag.store(true, Ordering::Relaxed);
                    self.status = "Stopping".to_string();
                }
                Task::none()
            }
            Message::InputChanged(input) => {
                self.input = input;
                Task::none()
            }
            Message::Submit => self.submit(),
            Message::ClearConsole => {
                self.console.clear();
                Task::none()
            }
            Message::Inspect(name) => self.inspect(name, 0),
            Message::Page(delta) => {
                let target = self.grid.as_ref().and_then(|grid| {
                    let offset = (grid.offset as i64 + delta * PAGE as i64).max(0) as usize;
                    (offset < grid.total).then(|| (grid.name.clone(), offset))
                });
                match target {
                    Some((name, offset)) => self.inspect(name, offset),
                    None => Task::none(),
                }
            }
            Message::PaneResized(pane_grid::ResizeEvent { split, ratio }) => {
                self.panes.resize(split, ratio);
                Task::none()
            }
            Message::PaneDragged(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.panes.drop(pane, target);
                Task::none()
            }
            Message::PaneDragged(_) => Task::none(),
            Message::ThemeChanged(theme) => {
                self.theme = theme;
                Task::none()
            }
        }
    }

    fn on_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Ready(sender, flag) => {
                self.worker = Some(sender);
                self.interrupt = Some(flag);
                self.status = "Ready".to_string();
                Task::none()
            }
            Event::Output(text) => self.push(LineKind::Output, text),
            Event::Result(text) => self.push(LineKind::Result, text),
            Event::Error(text) => {
                self.status = format!("Error: {text}");
                self.push(LineKind::Error, format!("error: {text}"))
            }
            Event::Environment(bindings) => {
                self.bindings = bindings;
                Task::none()
            }
            Event::Grid(grid) => {
                self.grid = Some(grid);
                Task::none()
            }
            Event::Finished => {
                self.running = false;
                if !self.status.starts_with("Error") {
                    self.status = "Ready".to_string();
                }
                Task::none()
            }
        }
    }

    pub fn file_label(&self) -> String {
        if self.path.is_empty() { "the editor".to_string() } else { self.path.clone() }
    }

    fn note(&mut self, text: &str) {
        self.console.push(Line { kind: LineKind::Note, text: text.to_string() });
    }

    fn push(&mut self, kind: LineKind, text: String) -> Task<Message> {
        match self.console.last_mut() {
            Some(last) if kind == LineKind::Output && last.kind == LineKind::Output => last.text.push_str(&text),
            _ => self.console.push(Line { kind, text }),
        }
        operation::snap_to(console_id(), RelativeOffset::END)
    }

    fn run(&mut self, source: String, show_results: bool) -> Task<Message> {
        let Some(worker) = &self.worker else {
            self.status = "The interpreter is still starting".to_string();
            return Task::none();
        };
        self.running = true;
        self.status = "Running".to_string();
        let _ = worker.send(Request::Run { source, show_results });
        Task::none()
    }

    fn echo(&mut self, source: &str) -> Task<Message> {
        self.push(LineKind::Input, source.to_string())
    }

    fn run_form(&mut self) -> Task<Message> {
        if let Some(selection) = self.editor.selection() {
            let echo = self.echo(&selection);
            return Task::batch([echo, self.run(selection, true)]);
        }
        let text = self.editor.text();
        let cursor = self.editor.cursor();
        let offset = forms::offset_of(&text, cursor.position.line, cursor.position.column);
        let Some(range) = forms::form_at(&text, offset) else {
            return Task::none();
        };
        let source = text[range.clone()].to_string();
        if let Some(next) = forms::next_form(&text, range.end) {
            let (line, column) = forms::position_of(&text, next.start);
            let position = text_editor::Position { line, column };
            self.editor.move_to(text_editor::Cursor { position, selection: None });
        }
        let echo = self.echo(&source);
        Task::batch([echo, self.run(source, true)])
    }

    fn submit(&mut self) -> Task<Message> {
        let line = std::mem::take(&mut self.input);
        if !self.pending.is_empty() {
            self.pending.push('\n');
        }
        self.pending.push_str(&line);
        match read_all(&self.pending) {
            Err(e) if e.incomplete => Task::none(),
            Err(e) => {
                let source = std::mem::take(&mut self.pending);
                let echo = self.echo(&source);
                Task::batch([echo, self.push(LineKind::Error, format!("error: {e}"))])
            }
            Ok(_) => {
                let source = std::mem::take(&mut self.pending);
                let echo = self.echo(&source);
                Task::batch([echo, self.run(source, true)])
            }
        }
    }

    fn inspect(&mut self, name: String, offset: usize) -> Task<Message> {
        if let Some(worker) = &self.worker {
            let _ = worker.send(Request::Inspect { name, offset, limit: PAGE });
        }
        Task::none()
    }

    fn open(&mut self) {
        if self.path.trim().is_empty() {
            self.status = "Enter a file path to open".to_string();
            return;
        }
        match std::fs::read_to_string(&self.path) {
            Ok(source) => {
                self.editor = text_editor::Content::with_text(&source.replace("\r\n", "\n"));
                self.dirty = false;
                self.status = format!("Opened {}", self.path);
            }
            Err(e) => self.status = format!("Cannot open {}: {e}", self.path),
        }
    }

    fn save(&mut self) {
        if self.path.trim().is_empty() {
            self.status = "Enter a file path to save to".to_string();
            return;
        }
        match std::fs::write(&self.path, self.editor.text()) {
            Ok(()) => {
                self.dirty = false;
                self.status = format!("Saved {}", self.path);
            }
            Err(e) => self.status = format!("Cannot save {}: {e}", self.path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn studio_with_content() -> Studio {
        let mut studio = Studio::new();
        studio.editor = text_editor::Content::with_text("(define x 1)\n\n(+ x\n   1)\n");
        studio.console.push(Line { kind: LineKind::Input, text: "(+ 1 2)".into() });
        studio.console.push(Line { kind: LineKind::Result, text: "3".into() });
        studio.console.push(Line { kind: LineKind::Error, text: "error: boom".into() });
        studio.bindings = vec![Binding { name: "t".into(), kind: "table".into(), summary: "2 rows, 1 column".into() }];
        studio.grid = Some(Grid {
            name: "t".into(),
            columns: vec!["#".into(), "a".into()],
            rows: vec![vec!["0".into(), "1".into()]],
            offset: 0,
            total: 2,
        });
        studio
    }

    #[test]
    fn the_window_builds_with_and_without_content() {
        let _ = view::view(&Studio::new());
        let _ = view::view(&studio_with_content());
    }

    #[test]
    fn running_the_form_at_the_cursor_echoes_it_and_moves_on() {
        let mut studio = studio_with_content();
        let _ = studio.update(Message::RunForm);
        assert_eq!(studio.console.last().unwrap().text, "(define x 1)");
        assert_eq!(studio.editor.cursor().position.line, 2);
        assert_eq!(studio.status, "The interpreter is still starting");
        let _ = studio.update(Message::RunForm);
        assert_eq!(studio.console.last().unwrap().text, "(+ x\n   1)");
    }

    #[test]
    fn console_input_waits_for_a_complete_form() {
        let mut studio = Studio::new();
        studio.input = "(define y".into();
        let _ = studio.update(Message::Submit);
        assert_eq!(studio.pending, "(define y");
        studio.input = "2)".into();
        let _ = studio.update(Message::Submit);
        assert_eq!(studio.pending, "");
        assert_eq!(studio.console.last().unwrap().text, "(define y\n2)");
        studio.input = ")".into();
        let _ = studio.update(Message::Submit);
        assert!(studio.console.last().unwrap().text.starts_with("error:"));
    }

    #[test]
    fn output_chunks_join_into_one_console_line() {
        let mut studio = Studio::new();
        let _ = studio.on_event(Event::Output("a".into()));
        let _ = studio.on_event(Event::Output("b\n".into()));
        let _ = studio.on_event(Event::Result("1".into()));
        let texts: Vec<&str> = studio.console.iter().skip(1).map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["ab\n", "1"]);
    }
}
