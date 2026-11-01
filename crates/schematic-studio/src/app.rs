use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use iced::widget::scrollable::{AbsoluteOffset, RelativeOffset, Viewport};
use iced::widget::text_editor::{Action, Motion};
use iced::widget::{Id, operation, pane_grid, text_editor};
use iced::{Size, Subscription, Task, Theme, window};
use schematic::plot::Plot;
use schematic::reader::read_all;

use crate::highlight::Marks;
use crate::worker::{self, Binding, Event, Grid, Request};
use crate::{fonts, forms, theme, view};

pub const PAGE: usize = 100;
pub const LINE_HEIGHT: f32 = 21.0;
pub const EDITOR_PAD: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    Editor,
    Console,
    Environment,
    Viewer,
    Plots,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Input,
    Output,
    Result,
    Error,
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Data,
    Values,
    Functions,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Data => "Data",
            Group::Values => "Values",
            Group::Functions => "Functions",
        }
    }

    fn of(kind: &str) -> Group {
        match kind {
            "table" | "array" => Group::Data,
            "function" => Group::Functions,
            _ => Group::Values,
        }
    }
}

pub struct Line {
    pub kind: LineKind,
    pub text: String,
}

pub struct Studio {
    pub panes: pane_grid::State<PaneKind>,
    pub editor: text_editor::Content,
    pub marks: Marks,
    pub editor_offset: f32,
    pub editor_height: f32,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub console: Vec<Line>,
    pub input: text_editor::Content,
    pub input_marks: Marks,
    pub pending: String,
    pub history: Vec<String>,
    pub recalled: Option<usize>,
    pub draft: String,
    pub bindings: Vec<Binding>,
    pub grid: Option<Grid>,
    pub plots: Vec<Plot>,
    pub plot_at: usize,
    pub worker: Option<Sender<Request>>,
    pub interrupt: Option<Arc<AtomicBool>>,
    pub running: bool,
    pub theme: Theme,
    pub status: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Worker(Event),
    Edit(Action),
    EditorScrolled(Viewport),
    EditorResized(Size),
    Input(Action),
    Open,
    Opened(Option<PathBuf>),
    Save,
    SaveAs,
    SaveTo(Option<PathBuf>),
    RunForm,
    RunFile,
    Stop,
    Submit,
    HistoryBack,
    HistoryForward,
    ClearConsole,
    Inspect(String),
    Page(i64),
    SavePlot,
    SavePlotTo(Option<PathBuf>),
    PlotBack,
    PlotForward,
    PaneResized(pane_grid::ResizeEvent),
    PaneDragged(pane_grid::DragEvent),
    ToggleTheme,
}

pub fn run() -> iced::Result {
    let settings = window::Settings {
        size: Size::new(1280.0, 820.0),
        min_size: Some(Size::new(880.0, 560.0)),
        ..window::Settings::default()
    };
    let mut app = iced::application(Studio::boot, Studio::update, view::view)
        .title(Studio::title)
        .subscription(Studio::subscription)
        .theme(Studio::theme)
        .default_font(fonts::UI)
        .window(settings);
    for file in fonts::FILES {
        app = app.font(file);
    }
    app.run()
}

pub fn console_id() -> Id {
    Id::new("console")
}

pub fn editor_id() -> Id {
    Id::new("editor")
}

pub fn scroll_target(top: f32, bottom: f32, offset: f32, height: f32) -> Option<f32> {
    if height <= 0.0 {
        None
    } else if top < offset {
        Some((top - EDITOR_PAD).max(0.0))
    } else if bottom > offset + height {
        Some(bottom + EDITOR_PAD - height)
    } else {
        None
    }
}

fn marks_of(content: &text_editor::Content) -> Marks {
    let text = content.text();
    let cursor = content.cursor().position;
    let offset = forms::offset_of(&text, cursor.line, cursor.column);
    let pair = forms::matching_bracket(&text, offset)
        .map(|(open, close)| [forms::line_offset(&text, open), forms::line_offset(&text, close)]);
    Marks { pair }
}

fn scripts() -> rfd::AsyncFileDialog {
    rfd::AsyncFileDialog::new().add_filter("Schematic scripts", &["scm"]).add_filter("All files", &["*"])
}

async fn pick_file() -> Option<PathBuf> {
    let handle = scripts().set_title("Open a script").pick_file().await?;
    Some(handle.path().to_path_buf())
}

async fn save_file(current: Option<PathBuf>) -> Option<PathBuf> {
    let mut dialog = scripts().set_title("Save the script as");
    if let Some(current) = &current {
        if let Some(dir) = current.parent() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(name) = current.file_name().and_then(|n| n.to_str()) {
            dialog = dialog.set_file_name(name);
        }
    }
    let handle = dialog.save_file().await?;
    Some(handle.path().to_path_buf())
}

async fn save_svg() -> Option<PathBuf> {
    let dialog = rfd::AsyncFileDialog::new().add_filter("SVG image", &["svg"]).set_file_name("chart.svg").set_title("Save the plot as");
    let handle = dialog.save_file().await?;
    Some(handle.path().to_path_buf())
}

impl Studio {
    pub fn new() -> Studio {
        use pane_grid::{Axis, Configuration};
        let split = |axis, ratio, a, b| Configuration::Split { axis, ratio, a: Box::new(a), b: Box::new(b) };
        let layout = split(
            Axis::Vertical,
            0.62,
            split(Axis::Horizontal, 0.64, Configuration::Pane(PaneKind::Editor), Configuration::Pane(PaneKind::Console)),
            split(
                Axis::Horizontal,
                0.34,
                Configuration::Pane(PaneKind::Environment),
                split(Axis::Horizontal, 0.5, Configuration::Pane(PaneKind::Viewer), Configuration::Pane(PaneKind::Plots)),
            ),
        );
        let mut studio = Studio {
            panes: pane_grid::State::with_configuration(layout),
            editor: text_editor::Content::new(),
            marks: Marks::default(),
            editor_offset: 0.0,
            editor_height: 0.0,
            path: None,
            dirty: false,
            console: Vec::new(),
            input: text_editor::Content::new(),
            input_marks: Marks::default(),
            pending: String::new(),
            history: Vec::new(),
            recalled: None,
            draft: String::new(),
            bindings: Vec::new(),
            grid: None,
            plots: Vec::new(),
            plot_at: 0,
            worker: None,
            interrupt: None,
            running: false,
            theme: theme::light(),
            status: "Starting the interpreter".to_string(),
        };
        studio.note("Welcome to Schematic Studio. Ctrl+Enter runs the form at the cursor, Ctrl+Shift+Enter runs the whole file.");
        studio
    }

    fn boot() -> Studio {
        let mut studio = Studio::new();
        if let Some(path) = std::env::args().nth(1) {
            studio.load(PathBuf::from(path));
        }
        studio
    }

    pub fn file_name(&self) -> String {
        self.path.as_deref().and_then(Path::file_name).and_then(|n| n.to_str()).unwrap_or("Untitled").to_string()
    }

    pub fn title(&self) -> String {
        let mark = if self.dirty { "*" } else { "" };
        format!("{}{mark} - Schematic Studio", self.file_name())
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run(worker::subscription).map(Message::Worker)
    }

    pub fn groups(&self) -> Vec<(Group, Vec<&Binding>)> {
        [Group::Data, Group::Values, Group::Functions]
            .into_iter()
            .filter_map(|group| {
                let members: Vec<&Binding> = self.bindings.iter().filter(|b| Group::of(&b.kind) == group).collect();
                (!members.is_empty()).then_some((group, members))
            })
            .collect()
    }

    pub fn plot(&self) -> Option<&Plot> {
        self.plots.get(self.plot_at)
    }

    pub fn cursor(&self) -> (usize, usize) {
        let position = self.editor.cursor().position;
        (position.line + 1, position.column + 1)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Worker(event) => self.on_event(event),
            Message::Edit(action) => {
                if action.is_edit() {
                    self.dirty = true;
                }
                self.editor.perform(action);
                self.refresh_marks();
                self.follow_cursor()
            }
            Message::EditorScrolled(viewport) => {
                self.editor_offset = viewport.absolute_offset().y;
                self.editor_height = viewport.bounds().height;
                Task::none()
            }
            Message::EditorResized(size) => {
                self.editor_height = size.height;
                Task::none()
            }
            Message::Input(action) => {
                self.input.perform(action);
                self.input_marks = marks_of(&self.input);
                Task::none()
            }
            Message::Open => Task::perform(pick_file(), Message::Opened),
            Message::Opened(path) => {
                if let Some(path) = path {
                    self.load(path);
                    self.editor_offset = 0.0;
                    return operation::scroll_to(editor_id(), AbsoluteOffset { x: 0.0, y: 0.0 });
                }
                Task::none()
            }
            Message::Save => match self.path.clone() {
                Some(path) => {
                    self.store(path);
                    Task::none()
                }
                None => Task::perform(save_file(None), Message::SaveTo),
            },
            Message::SaveAs => Task::perform(save_file(self.path.clone()), Message::SaveTo),
            Message::SaveTo(path) => {
                if let Some(path) = path {
                    self.store(path);
                }
                Task::none()
            }
            Message::RunForm => self.run_form(),
            Message::RunFile => {
                let source = self.editor.text();
                self.note(&format!("Running {}", self.file_name()));
                self.run(source, false)
            }
            Message::Stop => {
                if let Some(flag) = &self.interrupt {
                    flag.store(true, Ordering::Relaxed);
                    self.status = "Stopping".to_string();
                }
                Task::none()
            }
            Message::Submit => self.submit(),
            Message::HistoryBack => {
                self.recall(-1);
                Task::none()
            }
            Message::HistoryForward => {
                self.recall(1);
                Task::none()
            }
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
            Message::SavePlot => Task::perform(save_svg(), Message::SavePlotTo),
            Message::SavePlotTo(path) => {
                if let (Some(path), Some(plot)) = (path, self.plot()) {
                    self.status = match std::fs::write(&path, plot.svg()) {
                        Ok(()) => format!("Saved {}", path.display()),
                        Err(e) => format!("Cannot save {}: {e}", path.display()),
                    };
                }
                Task::none()
            }
            Message::PlotBack => {
                self.plot_at = self.plot_at.saturating_sub(1);
                Task::none()
            }
            Message::PlotForward => {
                self.plot_at = (self.plot_at + 1).min(self.plots.len().saturating_sub(1));
                Task::none()
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
            Message::ToggleTheme => {
                self.theme = theme::toggled(&self.theme);
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
                self.push(LineKind::Error, text)
            }
            Event::Environment(bindings) => {
                self.bindings = bindings;
                Task::none()
            }
            Event::Grid(grid) => {
                self.grid = Some(grid);
                Task::none()
            }
            Event::Plot(plot) => {
                self.plots.push(plot);
                self.plot_at = self.plots.len() - 1;
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

    fn follow_cursor(&mut self) -> Task<Message> {
        let line = self.editor.cursor().position.line as f32;
        let top = EDITOR_PAD + line * LINE_HEIGHT;
        match scroll_target(top, top + LINE_HEIGHT, self.editor_offset, self.editor_height) {
            Some(y) => {
                self.editor_offset = y;
                operation::scroll_to(editor_id(), AbsoluteOffset { x: 0.0, y })
            }
            None => Task::none(),
        }
    }

    fn refresh_marks(&mut self) {
        self.marks = marks_of(&self.editor);
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
            self.refresh_marks();
        }
        let follow = self.follow_cursor();
        let echo = self.echo(&source);
        Task::batch([follow, echo, self.run(source, true)])
    }

    fn set_input(&mut self, text: &str) {
        self.input = text_editor::Content::with_text(text);
        self.input.perform(Action::Move(Motion::DocumentEnd));
    }

    fn recall(&mut self, step: i64) {
        let next = match (self.recalled, step < 0) {
            (None, true) if !self.history.is_empty() => {
                self.draft = self.input.text();
                Some(self.history.len() - 1)
            }
            (None, _) => return,
            (Some(at), true) => Some(at.saturating_sub(1)),
            (Some(at), false) if at + 1 < self.history.len() => Some(at + 1),
            (Some(_), false) => None,
        };
        let text = match next {
            Some(at) => self.history[at].clone(),
            None => std::mem::take(&mut self.draft),
        };
        self.recalled = next;
        self.set_input(&text);
    }

    fn remember(&mut self, source: &str) {
        if !source.trim().is_empty() && self.history.last().map(String::as_str) != Some(source) {
            self.history.push(source.to_string());
        }
    }

    fn submit(&mut self) -> Task<Message> {
        let line = self.input.text();
        self.input = text_editor::Content::new();
        self.recalled = None;
        if !self.pending.is_empty() {
            self.pending.push('\n');
        }
        self.pending.push_str(&line);
        if self.pending.trim().is_empty() {
            self.pending.clear();
            return Task::none();
        }
        match read_all(&self.pending) {
            Err(e) if e.incomplete => Task::none(),
            Err(e) => {
                let source = std::mem::take(&mut self.pending);
                self.remember(&source);
                let echo = self.echo(&source);
                Task::batch([echo, self.push(LineKind::Error, e.to_string())])
            }
            Ok(_) => {
                let source = std::mem::take(&mut self.pending);
                self.remember(&source);
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

    fn load(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(source) => {
                self.editor = text_editor::Content::with_text(&source.replace("\r\n", "\n"));
                self.marks = Marks::default();
                self.dirty = false;
                self.status = format!("Opened {}", path.display());
                self.path = Some(path);
            }
            Err(e) => self.status = format!("Cannot open {}: {e}", path.display()),
        }
    }

    fn store(&mut self, path: PathBuf) {
        match std::fs::write(&path, self.editor.text()) {
            Ok(()) => {
                self.dirty = false;
                self.status = format!("Saved {}", path.display());
                self.path = Some(path);
            }
            Err(e) => self.status = format!("Cannot save {}: {e}", path.display()),
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
        studio.console.push(Line { kind: LineKind::Error, text: "boom".into() });
        studio.bindings = vec![Binding { name: "t".into(), kind: "table".into(), summary: "2 rows, 1 column".into() }];
        studio.grid = Some(Grid {
            name: "t".into(),
            columns: vec!["#".into(), "a".into()],
            numeric: vec![true, true],
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
        studio.input = text_editor::Content::with_text("(define y");
        let _ = studio.update(Message::Submit);
        assert_eq!(studio.pending, "(define y");
        studio.input = text_editor::Content::with_text("2)");
        let _ = studio.update(Message::Submit);
        assert_eq!(studio.pending, "");
        assert_eq!(studio.console.last().unwrap().text, "(define y\n2)");
        studio.input = text_editor::Content::with_text(")");
        let _ = studio.update(Message::Submit);
        assert_eq!(studio.console.last().unwrap().kind, LineKind::Error);
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

    #[test]
    fn opening_and_saving_go_through_paths_chosen_by_dialogs() {
        let dir = std::env::temp_dir().join(format!("studio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("in.scm");
        std::fs::write(&source, "(define a 1)\r\n").unwrap();
        let mut studio = Studio::new();
        let _ = studio.update(Message::Opened(Some(source.clone())));
        assert_eq!(studio.editor.text(), "(define a 1)\n");
        assert_eq!(studio.path.as_deref(), Some(source.as_path()));
        assert!(!studio.dirty);
        assert_eq!(studio.title(), "in.scm - Schematic Studio");
        let _ = studio.update(Message::Edit(text_editor::Action::Edit(text_editor::Edit::Insert(';'))));
        assert!(studio.dirty);
        let target = dir.join("out.scm");
        let _ = studio.update(Message::SaveTo(Some(target.clone())));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), ";(define a 1)\n");
        assert_eq!(studio.path.as_deref(), Some(target.as_path()));
        assert!(!studio.dirty);
        let _ = studio.update(Message::Opened(None));
        assert_eq!(studio.path.as_deref(), Some(target.as_path()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn console_history_walks_back_through_submitted_forms() {
        let mut studio = Studio::new();
        for form in ["(+ 1 2)", "(+ 3 4)", "(+ 3 4)"] {
            studio.input = text_editor::Content::with_text(form);
            let _ = studio.update(Message::Submit);
        }
        assert_eq!(studio.history, ["(+ 1 2)", "(+ 3 4)"]);
        assert_eq!(studio.input.text(), "");
        studio.input = text_editor::Content::with_text("(dra");
        let _ = studio.update(Message::HistoryBack);
        assert_eq!(studio.input.text(), "(+ 3 4)");
        assert_eq!(studio.input.cursor().position.column, 7);
        let _ = studio.update(Message::HistoryBack);
        assert_eq!(studio.input.text(), "(+ 1 2)");
        let _ = studio.update(Message::HistoryBack);
        assert_eq!(studio.input.text(), "(+ 1 2)");
        let _ = studio.update(Message::HistoryForward);
        assert_eq!(studio.input.text(), "(+ 3 4)");
        let _ = studio.update(Message::HistoryForward);
        assert_eq!(studio.input.text(), "(dra");
        let _ = studio.update(Message::HistoryForward);
        assert_eq!(studio.input.text(), "(dra");
    }

    fn chart(title: &str) -> Plot {
        let source = format!("(bar-chart [\"a\" \"b\"] [1 2] {{:title \"{title}\"}})");
        let value = schematic::Interpreter::new().eval_str(&source).unwrap();
        let schematic::Value::Plot(plot) = value else { panic!("{value}") };
        (*plot).clone()
    }

    #[test]
    fn plots_keep_a_history_you_can_walk() {
        let mut studio = Studio::new();
        let title = |studio: &Studio| studio.plot().and_then(|p| p.title.clone());
        assert_eq!(title(&studio), None);
        let _ = studio.update(Message::Worker(Event::Plot(chart("one"))));
        let _ = studio.update(Message::Worker(Event::Plot(chart("two"))));
        assert_eq!(title(&studio).as_deref(), Some("two"));
        let _ = studio.update(Message::PlotBack);
        assert_eq!(title(&studio).as_deref(), Some("one"));
        let _ = studio.update(Message::PlotBack);
        assert_eq!(title(&studio).as_deref(), Some("one"));
        let _ = studio.update(Message::PlotForward);
        assert_eq!(title(&studio).as_deref(), Some("two"));
        let _ = studio.update(Message::PlotForward);
        assert_eq!((studio.plots.len(), studio.plot_at), (2, 1));
        let _ = view::view(&studio);
    }

    #[test]
    fn plots_arrive_in_their_own_pane() {
        let mut studio = Studio::new();
        assert!(studio.panes.iter().any(|(_, kind)| *kind == PaneKind::Plots));
        let _ = studio.update(Message::Worker(Event::Plot(chart("T"))));
        assert_eq!(studio.plot().map(|p| p.title.clone()), Some(Some("T".to_string())));
        let _ = view::view(&studio);
        let dir = std::env::temp_dir().join(format!("studio-plot-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("chart.svg");
        let _ = studio.update(Message::SavePlotTo(Some(target.clone())));
        assert!(std::fs::read_to_string(&target).unwrap().starts_with("<svg"));
        assert!(studio.status.starts_with("Saved"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_editor_scrolls_to_keep_the_cursor_visible() {
        assert_eq!(scroll_target(10.0, 31.0, 0.0, 0.0), None);
        assert_eq!(scroll_target(10.0, 31.0, 0.0, 300.0), None);
        assert_eq!(scroll_target(430.0, 451.0, 0.0, 300.0), Some(161.0));
        assert_eq!(scroll_target(52.0, 73.0, 200.0, 300.0), Some(42.0));
        assert_eq!(scroll_target(10.0, 31.0, 200.0, 300.0), Some(0.0));
        let mut studio = Studio::new();
        studio.editor = text_editor::Content::with_text(&"x\n".repeat(50));
        let _ = studio.update(Message::EditorResized(Size::new(400.0, 300.0)));
        let _ = studio.update(Message::Edit(text_editor::Action::Move(text_editor::Motion::DocumentEnd)));
        assert!(studio.editor_offset > 700.0, "{}", studio.editor_offset);
    }

    #[test]
    fn bindings_group_into_data_values_and_functions() {
        let mut studio = Studio::new();
        let b = |name: &str, kind: &str| Binding { name: name.into(), kind: kind.into(), summary: String::new() };
        studio.bindings = vec![b("f", "function"), b("t", "table"), b("n", "integer"), b("xs", "array")];
        let groups: Vec<(Group, Vec<&str>)> =
            studio.groups().into_iter().map(|(g, bs)| (g, bs.iter().map(|b| b.name.as_str()).collect())).collect();
        assert_eq!(groups, [(Group::Data, vec!["t", "xs"]), (Group::Values, vec!["n"]), (Group::Functions, vec!["f"])]);
        studio.bindings.clear();
        assert!(studio.groups().is_empty());
    }

    #[test]
    fn the_theme_toggles_and_bracket_marks_follow_the_cursor() {
        let mut studio = Studio::new();
        assert!(!theme::is_dark(&studio.theme));
        let _ = studio.update(Message::ToggleTheme);
        assert!(theme::is_dark(&studio.theme));
        studio.editor = text_editor::Content::with_text("(a [b])");
        let step = |studio: &mut Studio, motion| {
            let _ = studio.update(Message::Edit(text_editor::Action::Move(motion)));
        };
        step(&mut studio, text_editor::Motion::Right);
        assert_eq!(studio.marks.pair, None);
        step(&mut studio, text_editor::Motion::Right);
        step(&mut studio, text_editor::Motion::Right);
        assert_eq!(studio.marks.pair, Some([(0, 3), (0, 5)]));
        step(&mut studio, text_editor::Motion::End);
        assert_eq!(studio.marks.pair, Some([(0, 0), (0, 6)]));
        assert!(!studio.dirty);
    }
}
