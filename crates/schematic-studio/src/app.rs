use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;

use iced::widget::scrollable::{AbsoluteOffset, RelativeOffset, Viewport};
use iced::widget::text_editor::{Action, Edit, Motion, Position};
use iced::widget::{Id, operation, pane_grid, text_editor};
use iced::{Size, Subscription, Task, Theme, window};
use schematic::check::{Diagnostic, check_source};
use schematic::plot::Plot;
use schematic::reader::read_all;

use crate::commands::{self, Menu};
use crate::completion::{self, Candidate};
use crate::highlight::Marks;
use crate::chart::PlotView;
use crate::git::{self, Change, Commit, Snapshot as GitSnapshot};
use crate::notebook::{self, CellText, Kind, Output};
use crate::settings::{self, Settings};
use crate::project::{self, ProjectInfo};
use crate::worker::{self, Binding, Event, Grid, Request, Watched};
use crate::{files, fonts, forms, theme, view};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Doc(usize),
    Help,
    Settings,
    Start,
    Diff,
}

#[derive(Debug, Default)]
pub struct Git {
    pub root: Option<PathBuf>,
    pub status: Option<git::Status>,
    pub log: Vec<Commit>,
    pub message: String,
    pub busy: bool,
    pub error: Option<String>,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Guide,
    Reference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Editor,
    Console,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Environment,
    Files,
    Project,
    Outline,
    Watch,
    Git,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamingKind {
    File,
    Folder,
    Rename(PathBuf),
    Package,
    Watch,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Naming {
    pub kind: NamingKind,
    pub parent: PathBuf,
    pub text: String,
}

pub struct Help {
    pub page: Page,
    pub query: String,
}

pub struct Completion {
    pub items: Vec<Candidate>,
    pub selected: usize,
    pub prefix: String,
    pub source: Source,
}

enum Follow {
    Open,
    Refresh,
    Close,
}

fn follow_completion(action: &Action) -> Follow {
    match action {
        Action::Edit(Edit::Insert(c)) if !completion::is_delimiter(*c) => Follow::Open,
        Action::Edit(Edit::Backspace) => Follow::Refresh,
        _ => Follow::Close,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: LineKind,
    pub text: String,
}

pub fn transcript(lines: &[Line]) -> String {
    let mut out = String::from("# Schematic console\n");
    for line in lines {
        let kind = match line.kind {
            LineKind::Input => "scheme",
            LineKind::Output => "output",
            LineKind::Result => "result",
            LineKind::Error => "error",
            LineKind::Note => "note",
        };
        out.push_str(&format!("\n```{kind}\n{}\n```\n", line.text.trim_end_matches('\n')));
    }
    out
}

pub fn parse_transcript(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut open: Option<(LineKind, Vec<&str>)> = None;
    for raw in text.lines() {
        match &mut open {
            Some((kind, body)) => {
                if raw.trim_end() == "```" {
                    lines.push(Line { kind: *kind, text: body.join("\n") });
                    open = None;
                } else {
                    body.push(raw);
                }
            }
            None => {
                let kind = match raw.strip_prefix("```").map(str::trim) {
                    Some("scheme") => Some(LineKind::Input),
                    Some("output") => Some(LineKind::Output),
                    Some("result") => Some(LineKind::Result),
                    Some("error") => Some(LineKind::Error),
                    Some("note") => Some(LineKind::Note),
                    _ => None,
                };
                if let Some(kind) = kind {
                    open = Some((kind, Vec::new()));
                }
            }
        }
    }
    lines
}

pub fn duration(ms: u128) -> String {
    if ms < 1000 { format!("{ms} ms") } else { format!("{:.1} s", ms as f64 / 1000.0) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Typing,
    Space,
    Erase,
    Other,
}

pub fn edit_kind(action: &Action) -> Option<EditKind> {
    Some(match action {
        Action::Edit(Edit::Insert(c)) if c.is_whitespace() => EditKind::Space,
        Action::Edit(Edit::Insert(_)) => EditKind::Typing,
        Action::Edit(Edit::Backspace | Edit::Delete) => EditKind::Erase,
        Action::Edit(_) => EditKind::Other,
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub text: String,
    pub position: Position,
    pub selection: Option<Position>,
}

const HISTORY: usize = 200;

pub struct Document {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub content: text_editor::Content,
    pub marks: Marks,
    pub dirty: bool,
    pub offset: f32,
    pub problems: Vec<Diagnostic>,
    pub checked: u64,
    pub history: Vec<Snapshot>,
    pub future: Vec<Snapshot>,
    pub last_edit: Option<EditKind>,
    pub found: Vec<(usize, usize)>,
    pub modified: Option<std::time::SystemTime>,
    pub stale: bool,
    pub notebook: Option<Notebook>,
}

pub struct Cell {
    pub id: u64,
    pub kind: Kind,
    pub content: text_editor::Content,
    pub marks: Marks,
    pub editing: bool,
    pub outputs: Vec<Output>,
    pub running: bool,
    pub elapsed: Option<u128>,
}

impl Cell {
    pub fn new(text: CellText, outputs: Vec<Output>) -> Cell {
        let content = text_editor::Content::with_text(&text.source);
        let marks = if text.kind == Kind::Code { marks_of(&content) } else { Marks::default() };
        Cell { id: DOCUMENT_IDS.fetch_add(1, Ordering::Relaxed), kind: text.kind, content, marks, editing: false, outputs, running: false, elapsed: None }
    }

    pub fn text(&self) -> CellText {
        CellText { kind: self.kind, source: self.content.text().trim_end_matches('\n').to_string() }
    }
}

pub struct Notebook {
    pub cells: Vec<Cell>,
    pub focused: usize,
}

impl Notebook {
    pub fn from_text(text: &str, mut outputs: Vec<Vec<Output>>) -> Notebook {
        let mut texts = notebook::parse(text);
        if texts.is_empty() {
            texts = notebook::fresh();
        }
        outputs.resize(texts.len(), Vec::new());
        let cells = texts.into_iter().zip(outputs).map(|(text, outputs)| Cell::new(text, outputs)).collect();
        Notebook { cells, focused: 0 }
    }

    pub fn texts(&self) -> Vec<CellText> {
        self.cells.iter().map(Cell::text).collect()
    }

    pub fn outputs(&self) -> Vec<Vec<Output>> {
        self.cells.iter().map(|c| c.outputs.clone()).collect()
    }

    pub fn source(&self) -> String {
        notebook::render(&self.texts())
    }
}

static DOCUMENT_IDS: AtomicU64 = AtomicU64::new(1);

fn modified_at(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub fn csv_name(path: &Path) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut name = String::new();
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            name.push(c);
        } else if !name.is_empty() && !name.ends_with('-') {
            name.push('-');
        }
    }
    let name = name.trim_end_matches('-');
    if name.is_empty() {
        "data".to_string()
    } else if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("data-{name}")
    } else {
        name.to_string()
    }
}

impl Document {
    pub fn new(path: Option<PathBuf>, content: text_editor::Content) -> Document {
        let id = DOCUMENT_IDS.fetch_add(1, Ordering::Relaxed);
        let modified = path.as_deref().and_then(modified_at);
        Document {
            id,
            path,
            content,
            marks: Marks::default(),
            dirty: false,
            offset: 0.0,
            problems: Vec::new(),
            checked: 0,
            history: Vec::new(),
            future: Vec::new(),
            last_edit: None,
            found: Vec::new(),
            modified,
            stale: false,
            notebook: None,
        }
    }

    pub fn with_notebook(path: Option<PathBuf>, source: &str, outputs: Vec<Vec<Output>>) -> Document {
        let mut doc = Document::new(path, text_editor::Content::with_text(source));
        doc.notebook = Some(Notebook::from_text(source, outputs));
        doc
    }

    fn sync_notebook(&mut self) {
        if let Some(nb) = &self.notebook {
            self.content = text_editor::Content::with_text(&nb.source());
            self.dirty = true;
        }
    }

    fn snapshot(&self) -> Snapshot {
        let cursor = self.content.cursor();
        Snapshot { text: self.content.text(), position: cursor.position, selection: cursor.selection }
    }

    fn remember(&mut self) {
        self.history.push(self.snapshot());
        if self.history.len() > HISTORY {
            self.history.remove(0);
        }
        self.future.clear();
    }

    pub fn record(&mut self, kind: EditKind) {
        let fresh = match (kind, self.last_edit) {
            (EditKind::Other, _) => true,
            (EditKind::Space, Some(EditKind::Other)) => false,
            (kind, last) => Some(kind) != last,
        };
        if fresh {
            self.remember();
        }
        self.last_edit = Some(if kind == EditKind::Space && self.last_edit == Some(EditKind::Other) { EditKind::Other } else { kind });
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.content = text_editor::Content::with_text(&snapshot.text);
        self.content.move_to(text_editor::Cursor { position: snapshot.position, selection: snapshot.selection });
        self.dirty = true;
        self.last_edit = None;
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.history.pop() else { return false };
        self.future.push(self.snapshot());
        self.restore(previous);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.future.pop() else { return false };
        self.history.push(self.snapshot());
        self.restore(next);
        true
    }

    pub fn replace_text(&mut self, text: &str, position: Position, selection: Option<Position>) {
        self.remember();
        self.restore(Snapshot { text: text.to_string(), position, selection });
    }

    pub fn untitled() -> Document {
        Document::new(None, text_editor::Content::new())
    }

    pub fn problem_at(&self, offset: usize) -> Option<&Diagnostic> {
        self.problems.iter().find(|d| d.span.start <= offset && offset <= d.span.end.max(d.span.start + 1))
    }

    pub fn name(&self) -> String {
        self.path.as_deref().and_then(Path::file_name).and_then(|n| n.to_str()).unwrap_or("Untitled").to_string()
    }

    pub fn label(&self) -> String {
        format!("{}{}", self.name(), if self.dirty { " •" } else { "" })
    }

    fn blank(&self) -> bool {
        self.path.is_none() && !self.dirty && self.content.text().trim().is_empty()
    }
}

pub struct Studio {
    pub panes: pane_grid::State<PaneKind>,
    pub docs: Vec<Document>,
    pub current: usize,
    pub tab: Tab,
    pub help: Option<Help>,
    pub completion: Option<Completion>,
    pub find: Option<Find>,
    pub wrap: bool,
    pub zoom: f32,
    pub scale: f32,
    pub restore: bool,
    pub backups: bool,
    pub settings_open: bool,
    pub start_open: bool,
    pub persistent: bool,
    pub saved: Option<Settings>,
    pub window_size: Option<(f32, f32)>,
    pub recent: Vec<PathBuf>,
    pub projects: Vec<PathBuf>,
    pub palette: Option<Palette>,
    pub menu: Option<Menu>,
    pub editor_width: f32,
    pub editor_height: f32,
    pub side: Side,
    pub root: Option<PathBuf>,
    pub expanded: HashSet<PathBuf>,
    pub entries: Vec<files::Entry>,
    pub selected: Option<PathBuf>,
    pub naming: Option<Naming>,
    pub project: Option<ProjectInfo>,
    pub picked_package: Option<String>,
    pub busy: Option<String>,
    pub console: Vec<Line>,
    pub input: text_editor::Content,
    pub input_marks: Marks,
    pub pending: String,
    pub history: Vec<String>,
    pub recalled: Option<usize>,
    pub draft: String,
    pub bindings: Vec<Binding>,
    pub namespace: String,
    pub grid: Option<Grid>,
    pub grid_sort: Option<(String, bool)>,
    pub grid_filter: String,
    pub selected_rows: HashSet<usize>,
    pub watches: Vec<String>,
    pub watched: Vec<Watched>,
    pub console_find: Option<String>,
    pub last_run: Option<u128>,
    pub plots: Vec<Plot>,
    pub plot_at: usize,
    pub plot_view: PlotView,
    pub tags: HashMap<u64, (u64, u64)>,
    pub next_tag: u64,
    pub active_run: Option<u64>,
    pub runs: usize,
    pub git: Git,
    pub diff: Option<Diff>,
    pub ticks: u64,
    pub main_window: Option<window::Id>,
    pub detached: Vec<(window::Id, PaneKind)>,
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
    NewFile,
    Open,
    Opened(Option<PathBuf>),
    Save,
    SaveAs,
    SaveTo(Option<PathBuf>),
    SelectTab(Tab),
    CloseTab(Tab),
    CloseDecided(usize, Decision),
    SaveAndClose(usize, Option<PathBuf>),
    OpenHelp,
    LookUp(Source),
    HelpQuery(String),
    HelpPage(Page),
    Complete(Source),
    CompleteMove(i32),
    CompleteAccept,
    CompletePick(usize),
    CompleteClose,
    ShowSide(Side),
    OpenFolder,
    OpenedFolder(Option<PathBuf>),
    PickEntry(PathBuf),
    StartNaming(NamingKind),
    Naming(String),
    ConfirmNaming,
    CancelNaming,
    DeleteEntry,
    DeleteDecided(PathBuf, bool),
    RefreshFiles,
    CreateProject,
    PickPackage(String),
    RemovePackage(String),
    RestoreProject,
    ProjectDone(Result<(ProjectInfo, String), String>),
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
    Checked(u64, u64, Vec<Diagnostic>),
    NextProblem,
    Undo,
    Redo,
    ToggleComment,
    Reindent,
    Shortcut(Shortcut),
    FindOpen(bool),
    FindQuery(String),
    FindReplacement(String),
    FindCase,
    FindStep(i32),
    FindReplace,
    FindReplaceAll,
    FindClose,
    ExpandSelection,
    JumpTo(usize),
    JumpToLine(usize),
    ToggleWrap,
    PaletteOpen(String),
    PaletteQuery(String),
    PaletteMove(i32),
    PaletteRun(Option<usize>),
    PaletteClose,
    MenuToggle(Menu),
    MenuHover(Menu),
    MenuClose,
    MenuPick(usize),
    TogglePane(PaneKind),
    Maximize(PaneKind),
    RestorePanes,
    ShowHelp(Page),
    OpenWebsite,
    About,
    Quit,
    QuitDecided(bool),
    SetTheme(String),
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ScaleBy(i32),
    OpenSettings,
    ToggleRestore,
    ToggleBackups,
    WindowResized(window::Id, f32, f32),
    Dropped(PathBuf),
    OpenPath(PathBuf),
    SortBy(String),
    GridFilter(String),
    SelectRow(usize),
    CopyRows,
    ExportCsv,
    ExportCsvTo(Option<PathBuf>),
    PlotColumn(String),
    ConsoleFind(bool),
    ConsoleQuery(String),
    CopyText(String),
    SaveTranscript,
    SaveTranscriptTo(Option<PathBuf>),
    LoadTranscript,
    LoadTranscriptFrom(Option<PathBuf>),
    RemoveWatch(usize),
    Restart,
    RestartDecided(bool),
    NewProject,
    NewProjectIn(Option<PathBuf>),
    OpenStart,
    OpenProject(PathBuf),
    Tick,
    Reload,
    PlotView(PlotView),
    PlotReset,
    PlotSelect(usize),
    CopyPlot,
    NewNotebook,
    CellEdit(usize, Action),
    RunCell(usize),
    RunCellAndAdvance(usize),
    RunAllCells,
    AddCell(usize, Kind),
    DeleteCell(usize),
    MoveCell(usize, i32),
    ToggleCellKind(usize),
    EditText(usize),
    DoneText(usize),
    ClearOutputs,
    ExportNotebook,
    ExportNotebookTo(Option<PathBuf>),
    ImportCsv,
    ImportCsvFrom(Option<PathBuf>),
    DescribeBinding(String),
    WatchBinding(String),
    GitRefresh,
    GitLoaded(Option<GitSnapshot>),
    GitMessage(String),
    GitCommit,
    GitStage(String),
    GitUnstage(String),
    GitStageAll,
    GitUnstageAll,
    GitDiscard(String, Change),
    GitDiscardDecided(String, Change, bool),
    GitDiff(String, bool, Option<Change>),
    GitDiffLoaded(String, Result<String, String>),
    GitDone(Result<String, String>),
    GitPull,
    GitPush,
    GitInit,
    DetachPane(PaneKind),
    AttachPane(window::Id),
    AttachAll,
    CloseRequested(window::Id),
}

pub fn console_find_id() -> Id {
    Id::new("console-find")
}

pub fn palette_id() -> Id {
    Id::new("palette")
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Palette {
    pub query: String,
    pub selected: usize,
}

pub const WEBSITE: &str = "https://collide2954.github.io/schematic/";

pub fn find_id() -> Id {
    Id::new("find")
}

pub fn run() -> iced::Result {
    let mut app = iced::daemon(Studio::boot, Studio::update, view::window)
        .title(Studio::title)
        .subscription(Studio::subscription)
        .theme(Studio::theme)
        .scale_factor(|studio: &Studio, _: window::Id| studio.scale)
        .default_font(fonts::UI);
    for file in fonts::FILES {
        app = app.font(file);
    }
    app.run()
}

fn main_window_settings(size: Option<(f32, f32)>) -> window::Settings {
    let (width, height) = size.unwrap_or((1280.0, 820.0));
    window::Settings {
        size: Size::new(width, height),
        min_size: Some(Size::new(880.0, 560.0)),
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

fn pane_window_settings(kind: PaneKind) -> window::Settings {
    let (width, height) = match kind {
        PaneKind::Editor => (900.0, 640.0),
        PaneKind::Console => (720.0, 420.0),
        _ => (640.0, 480.0),
    };
    window::Settings {
        size: Size::new(width, height),
        min_size: Some(Size::new(320.0, 240.0)),
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

impl PaneKind {
    pub fn label(self) -> &'static str {
        match self {
            PaneKind::Editor => "Editor",
            PaneKind::Console => "Console",
            PaneKind::Environment => "Environment",
            PaneKind::Viewer => "Data",
            PaneKind::Plots => "Plots",
        }
    }
}

pub fn console_id() -> Id {
    Id::new("console")
}

pub fn editor_id() -> Id {
    Id::new("editor")
}

pub fn naming_id() -> Id {
    Id::new("naming")
}

pub fn editor_text_id() -> Id {
    Id::new("editor-text")
}

pub fn console_text_id() -> Id {
    Id::new("console-text")
}

fn file_name_of(path: &Path) -> String {
    path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string()
}

fn info_name(info: &ProjectInfo) -> String {
    format!("{} in {}", info.name, info.root.display())
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
    Marks { pair, problems: Vec::new(), found: Vec::new() }
}

pub fn line_marks(text: &str, spans: impl Iterator<Item = (usize, usize)>) -> Vec<(usize, Range<usize>)> {
    spans
        .map(|(span_start, span_end)| {
            let start_offset = span_start.min(text.len());
            let (line, column) = forms::line_offset(text, start_offset);
            let line_start = start_offset - column;
            let line_len = text[line_start..].find('\n').unwrap_or(text.len() - line_start);
            let width = span_end.saturating_sub(span_start).max(1);
            let start = column.min(line_len.saturating_sub(1));
            let end = (column + width).min(line_len).max(start + 1);
            (line, start..end)
        })
        .collect()
}

pub fn problem_marks(text: &str, problems: &[Diagnostic]) -> Vec<(usize, Range<usize>)> {
    line_marks(text, problems.iter().map(|d| (d.span.start, d.span.end)))
}

fn marks_for(doc: &Document) -> Marks {
    let text = doc.content.text();
    let mut marks = marks_of(&doc.content);
    marks.problems = problem_marks(&text, &doc.problems);
    marks.found = line_marks(&text, doc.found.iter().copied());
    marks
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Find {
    pub query: String,
    pub replacement: String,
    pub case: bool,
    pub replacing: bool,
    pub current: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    Escape,
    Up,
    Down,
    FindNext,
    FindPrevious,
    Find,
    Replace,
    Save,
    SaveAs,
    Open,
    New,
    NewNotebook,
    Close,
    Palette,
    GoToLine,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Preferences,
    Pane(u8),
}

pub fn shortcut_of(key: &iced::keyboard::Key, modifiers: iced::keyboard::Modifiers) -> Option<Shortcut> {
    use iced::keyboard::Key;
    use iced::keyboard::key::Named;
    let command = modifiers.command();
    let shift = modifiers.shift();
    let letter = |wanted: &str| matches!(key, Key::Character(c) if c.eq_ignore_ascii_case(wanted));
    Some(match key {
        _ if command && shift && letter("n") => Shortcut::NewNotebook,
        Key::Named(Named::Escape) => Shortcut::Escape,
        Key::Named(Named::ArrowUp) if !command && !shift => Shortcut::Up,
        Key::Named(Named::ArrowDown) if !command && !shift => Shortcut::Down,
        Key::Named(Named::F3) => {
            if shift {
                Shortcut::FindPrevious
            } else {
                Shortcut::FindNext
            }
        }
        _ if !command => return None,
        _ if letter("f") && !shift => Shortcut::Find,
        _ if letter("h") => Shortcut::Replace,
        _ if letter("s") => {
            if shift {
                Shortcut::SaveAs
            } else {
                Shortcut::Save
            }
        }
        _ if letter("o") && !shift => Shortcut::Open,
        _ if letter("n") => Shortcut::New,
        _ if letter("w") => Shortcut::Close,
        _ if letter("p") && shift => Shortcut::Palette,
        _ if letter("g") => Shortcut::GoToLine,
        _ if letter("=") || letter("+") => Shortcut::ZoomIn,
        _ if letter("-") => Shortcut::ZoomOut,
        _ if letter("0") => Shortcut::ZoomReset,
        _ if letter(",") => Shortcut::Preferences,
        Key::Character(c) => Shortcut::Pane(c.parse::<u8>().ok().filter(|n| (1..=8).contains(n))?),
        _ => return None,
    })
}

fn events(event: iced::Event, status: iced::event::Status, window: window::Id) -> Option<Message> {
    match event {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            let shortcut = shortcut_of(&key, modifiers)?;
            let captured = status == iced::event::Status::Captured;
            if captured && matches!(shortcut, Shortcut::Find | Shortcut::Replace) {
                return None;
            }
            Some(Message::Shortcut(shortcut))
        }
        iced::Event::Window(window::Event::Resized(size)) => Some(Message::WindowResized(window, size.width, size.height)),
        iced::Event::Window(window::Event::FileDropped(path)) => Some(Message::Dropped(path)),
        iced::Event::Window(window::Event::CloseRequested) => Some(Message::CloseRequested(window)),
        _ => None,
    }
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

async fn pick_folder() -> Option<PathBuf> {
    let handle = rfd::AsyncFileDialog::new().set_title("Open a folder").pick_folder().await?;
    Some(handle.path().to_path_buf())
}

async fn confirm_discard(path: String) -> bool {
    let result = rfd::AsyncMessageDialog::new()
        .set_title("Discard")
        .set_description(format!("Discard the changes to {path}? This cannot be undone."))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        .await;
    matches!(result, rfd::MessageDialogResult::Yes)
}

async fn pick_csv() -> Option<PathBuf> {
    let handle = rfd::AsyncFileDialog::new().add_filter("CSV", &["csv"]).set_title("Import a CSV file").pick_file().await?;
    Some(handle.path().to_path_buf())
}

async fn export_notebook(current: Option<PathBuf>) -> Option<PathBuf> {
    let stem = current.as_deref().and_then(Path::file_stem).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "notebook".to_string());
    let mut dialog = rfd::AsyncFileDialog::new()
        .add_filter("HTML page", &["html"])
        .add_filter("Markdown", &["md"])
        .set_file_name(format!("{stem}.html"))
        .set_title("Export the notebook as");
    if let Some(dir) = current.as_deref().and_then(Path::parent) {
        dialog = dialog.set_directory(dir);
    }
    let handle = dialog.save_file().await?;
    Some(handle.path().to_path_buf())
}

async fn confirm_delete(name: String) -> bool {
    let result = rfd::AsyncMessageDialog::new()
        .set_title("Delete")
        .set_description(format!("Delete {name} for good?"))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        .await;
    matches!(result, rfd::MessageDialogResult::Yes)
}

async fn save_csv(name: String) -> Option<PathBuf> {
    let dialog = rfd::AsyncFileDialog::new().add_filter("CSV", &["csv"]).set_file_name(format!("{name}.csv")).set_title("Export the table as CSV");
    let handle = dialog.save_file().await?;
    Some(handle.path().to_path_buf())
}

async fn save_markdown() -> Option<PathBuf> {
    let dialog = rfd::AsyncFileDialog::new().add_filter("Markdown", &["md"]).set_file_name("console.md").set_title("Save the console transcript");
    let handle = dialog.save_file().await?;
    Some(handle.path().to_path_buf())
}

async fn pick_markdown() -> Option<PathBuf> {
    let dialog = rfd::AsyncFileDialog::new().add_filter("Markdown", &["md"]).set_title("Load a console transcript");
    let handle = dialog.pick_file().await?;
    Some(handle.path().to_path_buf())
}

async fn confirm_restart() -> bool {
    let result = rfd::AsyncMessageDialog::new()
        .set_title("Restart")
        .set_description("Restart the interpreter and forget every definition?")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        .await;
    matches!(result, rfd::MessageDialogResult::Yes)
}

async fn confirm_quit(unsaved: usize) -> bool {
    let what = if unsaved == 1 { "1 file".to_string() } else { format!("{unsaved} files") };
    let result = rfd::AsyncMessageDialog::new()
        .set_title("Quit")
        .set_description(format!("Quit without saving {what}?"))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        .await;
    matches!(result, rfd::MessageDialogResult::Yes)
}

fn open_url(url: &str) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };
    command.spawn().map(|_| ())
}

async fn confirm_close(name: String) -> Decision {
    let result = rfd::AsyncMessageDialog::new()
        .set_title("Unsaved changes")
        .set_description(format!("Save the changes to {name}?"))
        .set_buttons(rfd::MessageButtons::YesNoCancel)
        .show()
        .await;
    match result {
        rfd::MessageDialogResult::Yes => Decision::Save,
        rfd::MessageDialogResult::No => Decision::Discard,
        _ => Decision::Cancel,
    }
}

pub fn default_layout() -> pane_grid::Configuration<PaneKind> {
    use pane_grid::{Axis, Configuration};
    let split = |axis, ratio, a, b| Configuration::Split { axis, ratio, a: Box::new(a), b: Box::new(b) };
    split(
        Axis::Vertical,
        0.62,
        split(Axis::Horizontal, 0.64, Configuration::Pane(PaneKind::Editor), Configuration::Pane(PaneKind::Console)),
        split(
            Axis::Horizontal,
            0.34,
            Configuration::Pane(PaneKind::Environment),
            split(Axis::Horizontal, 0.5, Configuration::Pane(PaneKind::Viewer), Configuration::Pane(PaneKind::Plots)),
        ),
    )
}

impl Studio {
    pub fn new() -> Studio {
        Studio::with_settings(Settings::default())
    }

    pub fn with_settings(settings: Settings) -> Studio {
        let layout = settings.layout.as_ref().map(settings::configuration).unwrap_or_else(default_layout);
        Studio {
            panes: pane_grid::State::with_configuration(layout),
            docs: vec![Document::untitled()],
            current: 0,
            tab: Tab::Doc(0),
            help: None,
            completion: None,
            find: None,
            wrap: settings.wrap,
            zoom: settings.zoom,
            scale: settings.scale,
            restore: settings.restore,
            backups: settings.backups,
            settings_open: false,
            start_open: false,
            persistent: false,
            saved: None,
            window_size: settings.window,
            recent: settings.recent.clone(),
            projects: settings.projects.clone(),
            palette: None,
            menu: None,
            editor_width: 0.0,
            editor_height: 0.0,
            side: Side::Environment,
            root: None,
            expanded: HashSet::new(),
            entries: Vec::new(),
            selected: None,
            naming: None,
            project: None,
            picked_package: None,
            busy: None,
            console: Vec::new(),
            input: text_editor::Content::new(),
            input_marks: Marks::default(),
            pending: String::new(),
            history: Vec::new(),
            recalled: None,
            draft: String::new(),
            bindings: Vec::new(),
            namespace: "user".to_string(),
            grid: None,
            grid_sort: None,
            grid_filter: String::new(),
            selected_rows: HashSet::new(),
            watches: settings.watches.clone(),
            watched: Vec::new(),
            console_find: None,
            last_run: None,
            plots: Vec::new(),
            plot_at: 0,
            plot_view: PlotView::default(),
            tags: HashMap::new(),
            next_tag: 0,
            active_run: None,
            runs: 0,
            git: Git::default(),
            diff: None,
            ticks: 0,
            main_window: None,
            detached: Vec::new(),
            worker: None,
            interrupt: None,
            running: false,
            theme: theme::named(&settings.theme),
            status: "Starting the interpreter".to_string(),
        }
    }

    fn boot() -> (Studio, Task<Message>) {
        let settings = settings::load();
        let mut studio = Studio::with_settings(settings.clone());
        let mut tasks = Vec::new();
        match std::env::args().nth(1) {
            Some(path) => tasks.push(studio.load(PathBuf::from(path))),
            None if settings.restore => {
                for path in settings.open.iter().filter(|p| p.is_file()) {
                    tasks.push(studio.load(path.clone()));
                }
                if settings.current < studio.docs.len() {
                    studio.current = settings.current;
                    studio.tab = Tab::Doc(studio.current);
                }
            }
            None => {}
        }
        if studio.root.is_none()
            && let Some(dir) = settings.root.filter(|r| r.is_dir()).or_else(|| std::env::current_dir().ok()) {
                studio.set_root(dir);
            }
        if tasks.is_empty() {
            studio.start_open = true;
            studio.tab = Tab::Start;
        }
        let (main, open) = window::open(main_window_settings(settings.window));
        studio.main_window = Some(main);
        tasks.push(open.discard());
        for kind in settings.detached {
            tasks.push(studio.detach(kind));
        }
        studio.persistent = true;
        studio.saved = Some(studio.snapshot_settings());
        (studio, Task::batch(tasks))
    }

    fn detach(&mut self, kind: PaneKind) -> Task<Message> {
        if let Some((id, _)) = self.detached.iter().find(|(_, k)| *k == kind) {
            return window::gain_focus(*id);
        }
        if let Some(pane) = self.pane_of(kind) {
            if self.panes.iter().count() == 1 {
                self.status = "The last pane stays in the window".to_string();
                return Task::none();
            }
            if self.panes.maximized() == Some(pane) {
                self.panes.restore();
            }
            self.panes.close(pane);
        }
        let (id, open) = window::open(pane_window_settings(kind));
        self.detached.push((id, kind));
        open.discard()
    }

    fn attach(&mut self, id: window::Id) -> Task<Message> {
        let Some(at) = self.detached.iter().position(|(w, _)| *w == id) else { return Task::none() };
        let (_, kind) = self.detached.remove(at);
        if self.pane_of(kind).is_none() {
            self.toggle_pane(kind);
        }
        window::close(id)
    }

    pub fn line_height(&self) -> f32 {
        LINE_HEIGHT * self.zoom
    }

    pub fn code_size(&self) -> f32 {
        theme::CODE * self.zoom
    }

    pub fn console_size(&self) -> f32 {
        theme::CONSOLE * self.zoom
    }

    fn snapshot_settings(&self) -> Settings {
        let open: Vec<PathBuf> = self.docs.iter().filter_map(|d| d.path.clone()).collect();
        let current = self.docs[..self.current.min(self.docs.len())].iter().filter(|d| d.path.is_some()).count().min(open.len().saturating_sub(1));
        Settings {
            theme: theme::name_of(&self.theme).to_string(),
            scale: self.scale,
            zoom: self.zoom,
            wrap: self.wrap,
            restore: self.restore,
            backups: self.backups,
            window: self.window_size,
            layout: settings::layout_of(&self.panes),
            detached: self.detached.iter().map(|(_, kind)| *kind).collect(),
            open,
            current,
            recent: self.recent.clone(),
            projects: self.projects.clone(),
            root: self.root.clone(),
            watches: self.watches.clone(),
        }
    }

    fn persist(&mut self) {
        if !self.persistent {
            return;
        }
        let current = self.snapshot_settings();
        if self.saved.as_ref() == Some(&current) {
            return;
        }
        if let Err(e) = settings::save(&current) {
            self.status = format!("Cannot save the settings: {e}");
        }
        self.saved = Some(current);
    }

    pub fn set_root(&mut self, root: PathBuf) {
        self.root = Some(root);
        self.expanded.clear();
        self.selected = None;
        self.naming = None;
        self.refresh_files();
    }

    fn refresh_files(&mut self) {
        self.entries = match &self.root {
            Some(root) => files::listing(root, &self.expanded),
            None => Vec::new(),
        };
        self.git.stale = true;
        self.refresh_project();
    }

    fn refresh_project(&mut self) {
        let start = self.root.clone().or_else(|| self.doc().path.as_deref().and_then(Path::parent).map(Path::to_path_buf));
        self.project = start.as_deref().and_then(project::describe);
        if let Some(project) = &self.project {
            settings::remember(&mut self.projects, &project.root);
        }
        if let Some(project) = &self.project
            && self.picked_package.as_ref().is_some_and(|name| !project.dependencies.iter().any(|d| &d.name == name)) {
                self.picked_package = None;
            }
    }

    fn finish_project(&mut self, result: Result<(ProjectInfo, String), String>) {
        self.busy = None;
        match result {
            Ok((info, message)) => {
                self.project = Some(info);
                self.status = message;
            }
            Err(message) => {
                self.status = format!("Error: {message}");
                self.refresh_project();
            }
        }
        self.refresh_files();
    }

    fn naming_parent(&self) -> Option<PathBuf> {
        match &self.selected {
            Some(path) if path.is_dir() => Some(path.clone()),
            Some(path) => path.parent().map(Path::to_path_buf),
            None => self.root.clone(),
        }
    }

    fn retarget(&mut self, old: &Path, new: &Path) {
        for doc in &mut self.docs {
            let Some(rest) = doc.path.as_ref().and_then(|p| p.strip_prefix(old).ok()) else { continue };
            doc.path = Some(if rest.as_os_str().is_empty() { new.to_path_buf() } else { new.join(rest) });
        }
    }

    fn confirm_naming(&mut self) -> Task<Message> {
        let Some(naming) = self.naming.take() else { return Task::none() };
        let name = naming.text.trim().to_string();
        if naming.kind == NamingKind::Watch {
            if name.is_empty() {
                return Task::none();
            }
            if !self.watches.contains(&name) {
                self.watches.push(name);
                self.send_watches();
            }
            self.side = Side::Watch;
            return Task::none();
        }
        if naming.kind == NamingKind::Project {
            if !files::valid_name(&name) {
                self.status = "Project names cannot be empty or contain slashes".to_string();
                return Task::none();
            }
            let dir = naming.parent.join(&name);
            if dir.exists() {
                self.status = format!("{} already exists", dir.display());
                return Task::none();
            }
            return match project::create(&dir) {
                Ok(info) => {
                    let main = info.main.clone();
                    let message = format!("Created {}", info_name(&info));
                    self.set_root(dir);
                    self.project = Some(info);
                    self.side = Side::Project;
                    let check = self.load(main);
                    self.status = message;
                    Task::batch([check, self.scroll_to_doc()])
                }
                Err(e) => {
                    self.status = format!("Cannot create the project: {e}");
                    Task::none()
                }
            };
        }
        if naming.kind == NamingKind::Package {
            if name.is_empty() {
                self.status = "Name a package, as name, name@version, or name ../path".to_string();
                return Task::none();
            }
            self.busy = Some(format!("Adding {name}"));
            let root = naming.parent;
            return Task::perform(async move { project::add(&root, &name) }, Message::ProjectDone);
        }
        if !files::valid_name(&name) {
            self.status = "Names cannot be empty or contain slashes".to_string();
            return Task::none();
        }
        let target = naming.parent.join(&name);
        let verb = match &naming.kind {
            NamingKind::Rename(_) => "rename",
            _ => "create",
        };
        let result = match &naming.kind {
            _ if target.exists() && naming.kind != NamingKind::Rename(target.clone()) => Err(format!("{name} already exists")),
            NamingKind::File | NamingKind::Package | NamingKind::Watch | NamingKind::Project => std::fs::write(&target, "").map_err(|e| e.to_string()),
            NamingKind::Folder => std::fs::create_dir(&target).map_err(|e| e.to_string()),
            NamingKind::Rename(old) => match std::fs::rename(old, &target) {
                Ok(()) => {
                    self.retarget(old, &target);
                    Ok(())
                }
                Err(e) => Err(e.to_string()),
            },
        };
        match result {
            Ok(()) => {
                self.status = format!("{}d {}", verb[..1].to_uppercase() + &verb[1..], target.display());
                self.refresh_files();
                self.selected = Some(target.clone());
                if naming.kind == NamingKind::File {
                    let check = self.load(target);
                    return Task::batch([check, self.scroll_to_doc()]);
                }
                Task::none()
            }
            Err(e) => {
                self.status = format!("Cannot {verb} {name}: {e}");
                Task::none()
            }
        }
    }

    pub fn doc(&self) -> &Document {
        &self.docs[self.current]
    }

    pub fn doc_mut(&mut self) -> &mut Document {
        &mut self.docs[self.current]
    }

    pub fn file_name(&self) -> String {
        self.doc().name()
    }

    pub fn title(&self, id: window::Id) -> String {
        match self.detached.iter().find(|(w, _)| *w == id) {
            Some((_, kind)) => format!("{} - Schematic Studio", kind.label()),
            None => format!("{} - Schematic Studio", self.doc().label().replace(" •", "*")),
        }
    }

    fn theme(&self, _id: window::Id) -> Theme {
        self.theme.clone()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            Subscription::run(worker::subscription).map(Message::Worker),
            Subscription::run(worker::ticks).map(|()| Message::Tick),
            iced::event::listen_with(events),
        ])
    }

    fn check_disk(&mut self) -> Task<Message> {
        let mut reloaded = Vec::new();
        for i in 0..self.docs.len() {
            let doc = &self.docs[i];
            let Some(now) = doc.path.as_deref().and_then(modified_at) else { continue };
            if doc.modified == Some(now) {
                continue;
            }
            if doc.dirty {
                self.status = format!("{} changed on disk", doc.name());
                let doc = &mut self.docs[i];
                doc.stale = true;
                doc.modified = Some(now);
            } else if self.reload(i) {
                reloaded.push(i);
            }
        }
        if reloaded.contains(&self.current) {
            return Task::batch([self.refresh_find(true), self.schedule_check()]);
        }
        Task::none()
    }

    fn reload(&mut self, i: usize) -> bool {
        let Some(path) = self.docs.get(i).and_then(|d| d.path.clone()) else { return false };
        let Ok(source) = std::fs::read_to_string(&path) else { return false };
        let source = source.replace("\r\n", "\n");
        let outputs = notebook::is_notebook(&source).then(|| std::fs::read_to_string(notebook::sidecar(&path)).map(|text| notebook::parse_outputs(&text)).unwrap_or_default());
        let doc = &mut self.docs[i];
        let cursor = doc.content.cursor();
        doc.content = text_editor::Content::with_text(&source);
        doc.content.move_to(cursor);
        doc.notebook = outputs.map(|outputs| Notebook::from_text(&source, outputs));
        doc.dirty = false;
        doc.stale = false;
        doc.modified = modified_at(&path);
        doc.history.clear();
        doc.future.clear();
        doc.last_edit = None;
        doc.marks = marks_for(doc);
        self.status = format!("Reloaded {}", self.docs[i].name());
        true
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
        let position = self.doc().content.cursor().position;
        (position.line + 1, position.column + 1)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let mut task = self.handle(message);
        if self.git.stale {
            self.git.stale = false;
            task = Task::batch([task, self.git_refresh()]);
        }
        self.persist();
        task
    }

    fn git_dir(&self) -> Option<PathBuf> {
        self.root.clone().or_else(|| self.doc().path.as_deref().and_then(Path::parent).map(Path::to_path_buf))
    }

    fn git_refresh(&mut self) -> Task<Message> {
        let Some(dir) = self.git_dir() else {
            self.git.root = None;
            self.git.status = None;
            return Task::none();
        };
        Task::perform(async move { git::snapshot(&dir) }, Message::GitLoaded)
    }

    fn git_op(&mut self, op: impl FnOnce(PathBuf) -> Result<String, String> + Send + 'static) -> Task<Message> {
        let Some(root) = self.git.root.clone() else { return Task::none() };
        if self.git.busy {
            return Task::none();
        }
        self.git.busy = true;
        Task::perform(async move { op(root) }, Message::GitDone)
    }

    pub fn badge(&self, path: &Path) -> Option<(Change, bool)> {
        let root = self.git.root.as_deref()?;
        let status = self.git.status.as_ref()?;
        let relative = git::relative(root, path)?;
        if path.is_dir() {
            let prefix = format!("{relative}/");
            return status.entries.iter().any(|e| e.path.starts_with(&prefix)).then_some((Change::Modified, false));
        }
        let entry = status.entries.iter().find(|e| e.path == relative)?;
        match (entry.staged, entry.unstaged) {
            (_, Some(change)) => Some((change, false)),
            (Some(change), None) => Some((change, true)),
            (None, None) => None,
        }
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Worker(event) => self.on_event(event),
            Message::Edit(action) => {
                let follow = follow_completion(&action);
                let edited = action.is_edit();
                let doc = self.doc_mut();
                match edit_kind(&action) {
                    Some(kind) => doc.record(kind),
                    None => doc.last_edit = None,
                }
                if edited {
                    doc.dirty = true;
                }
                doc.content.perform(action);
                doc.marks = marks_for(doc);
                self.track_completion(Source::Editor, follow);
                let check = if edited { self.schedule_check() } else { Task::none() };
                Task::batch([self.follow_cursor(), check])
            }
            Message::Checked(id, generation, problems) => {
                if let Some(doc) = self.docs.iter_mut().find(|d| d.id == id)
                    && doc.checked == generation {
                        doc.problems = problems;
                        doc.marks = marks_for(doc);
                    }
                Task::none()
            }
            Message::NextProblem => self.next_problem(),
            Message::Undo => self.step_history(true),
            Message::Redo => self.step_history(false),
            Message::Shortcut(shortcut) => self.shortcut(shortcut),
            Message::FindOpen(replacing) => {
                let selection = self.doc().content.selection().filter(|s| !s.contains('\n') && !s.is_empty());
                let find = self.find.get_or_insert_with(Find::default);
                find.replacing |= replacing;
                if let Some(selection) = selection {
                    find.query = selection;
                }
                let refreshed = self.refresh_find(true);
                Task::batch([refreshed, operation::focus(find_id())])
            }
            Message::FindQuery(query) => {
                if let Some(find) = &mut self.find {
                    find.query = query;
                }
                self.refresh_find(true)
            }
            Message::FindReplacement(replacement) => {
                if let Some(find) = &mut self.find {
                    find.replacement = replacement;
                }
                Task::none()
            }
            Message::FindCase => {
                if let Some(find) = &mut self.find {
                    find.case = !find.case;
                }
                self.refresh_find(true)
            }
            Message::FindStep(step) => {
                let count = self.doc().found.len();
                let Some(find) = &mut self.find else { return Task::none() };
                if count == 0 {
                    return Task::none();
                }
                let current = find.current.unwrap_or(0) as i32;
                find.current = Some((current + step).rem_euclid(count as i32) as usize);
                self.select_match()
            }
            Message::FindReplace => self.replace_match(false),
            Message::FindReplaceAll => self.replace_match(true),
            Message::FindClose => {
                self.find = None;
                for doc in &mut self.docs {
                    doc.found.clear();
                }
                let doc = self.doc_mut();
                doc.marks = marks_for(doc);
                operation::focus(editor_text_id())
            }
            Message::ExpandSelection => {
                let doc = self.doc_mut();
                let text = doc.content.text();
                let cursor = doc.content.cursor();
                let a = forms::offset_of(&text, cursor.position.line, cursor.position.column);
                let b = cursor.selection.map_or(a, |other| forms::offset_of(&text, other.line, other.column));
                let Some(range) = forms::enclosing(&text, a.min(b), a.max(b)) else { return Task::none() };
                let (from_line, from_column) = forms::position_of(&text, range.start);
                let (to_line, to_column) = forms::position_of(&text, range.end);
                let position = Position { line: to_line, column: to_column };
                doc.content.move_to(text_editor::Cursor { position, selection: Some(Position { line: from_line, column: from_column }) });
                doc.last_edit = None;
                doc.marks = marks_for(doc);
                Task::none()
            }
            Message::JumpTo(offset) => {
                self.tab = Tab::Doc(self.current);
                let doc = self.doc_mut();
                let text = doc.content.text();
                let (line, column) = forms::position_of(&text, offset);
                doc.content.move_to(text_editor::Cursor { position: Position { line, column }, selection: None });
                doc.last_edit = None;
                doc.marks = marks_for(doc);
                Task::batch([self.follow_cursor(), operation::focus(editor_text_id())])
            }
            Message::ToggleWrap => {
                self.wrap = !self.wrap;
                Task::none()
            }
            Message::JumpToLine(line) => {
                self.palette = None;
                let text = self.doc().content.text();
                let offset = forms::offset_of(&text, line.saturating_sub(1), 0);
                self.update(Message::JumpTo(offset))
            }
            Message::PaletteOpen(query) => {
                self.menu = None;
                self.palette = Some(Palette { query, selected: 0 });
                operation::focus(palette_id())
            }
            Message::PaletteQuery(query) => {
                if let Some(palette) = &mut self.palette {
                    palette.query = query;
                    palette.selected = 0;
                }
                Task::none()
            }
            Message::PaletteMove(step) => {
                let count = self.palette.as_ref().map_or(0, |p| commands::matching(self, &p.query).len());
                if let Some(palette) = &mut self.palette
                    && count > 0 {
                        palette.selected = (palette.selected as i32 + step).rem_euclid(count as i32) as usize;
                    }
                Task::none()
            }
            Message::PaletteRun(index) => {
                let Some(palette) = self.palette.take() else { return Task::none() };
                let found = commands::matching(self, &palette.query);
                match found.get(index.unwrap_or(palette.selected)).and_then(|c| c.message.clone()) {
                    Some(message) => Task::batch([operation::focus(editor_text_id()), self.update(message)]),
                    None => {
                        self.palette = Some(palette);
                        Task::none()
                    }
                }
            }
            Message::PaletteClose => {
                self.palette = None;
                operation::focus(editor_text_id())
            }
            Message::MenuToggle(menu) => {
                self.menu = if self.menu == Some(menu) { None } else { Some(menu) };
                Task::none()
            }
            Message::MenuHover(menu) => {
                if self.menu.is_some() {
                    self.menu = Some(menu);
                }
                Task::none()
            }
            Message::MenuClose => {
                self.menu = None;
                Task::none()
            }
            Message::MenuPick(index) => {
                self.menu = None;
                match commands::commands(self).get(index).and_then(|c| c.message.clone()) {
                    Some(message) => self.update(message),
                    None => Task::none(),
                }
            }
            Message::TogglePane(kind) => {
                self.toggle_pane(kind);
                Task::none()
            }
            Message::Maximize(kind) => {
                if let Some(pane) = self.pane_of(kind) {
                    self.panes.maximize(pane);
                }
                Task::none()
            }
            Message::RestorePanes => {
                self.panes.restore();
                Task::none()
            }
            Message::ShowHelp(page) => {
                let help = self.help.get_or_insert(Help { page, query: String::new() });
                help.page = page;
                help.query.clear();
                self.tab = Tab::Help;
                Task::none()
            }
            Message::OpenWebsite => {
                self.status = match open_url(WEBSITE) {
                    Ok(()) => format!("Opened {WEBSITE}"),
                    Err(e) => format!("Cannot open the website: {e}"),
                };
                Task::none()
            }
            Message::About => {
                self.note(&format!("Schematic Studio {} · {WEBSITE}", env!("CARGO_PKG_VERSION")));
                operation::snap_to(console_id(), RelativeOffset::END)
            }
            Message::Quit => {
                let unsaved = self.docs.iter().filter(|d| d.dirty).count();
                if unsaved == 0 {
                    return iced::exit();
                }
                Task::perform(confirm_quit(unsaved), Message::QuitDecided)
            }
            Message::QuitDecided(yes) => {
                if yes {
                    return iced::exit();
                }
                Task::none()
            }
            Message::SortBy(column) => {
                self.grid_sort = match &self.grid_sort {
                    Some((current, false)) if *current == column => Some((column, true)),
                    Some((current, true)) if *current == column => None,
                    _ => Some((column, false)),
                };
                self.reinspect()
            }
            Message::GridFilter(text) => {
                self.grid_filter = text;
                self.reinspect()
            }
            Message::SelectRow(row) => {
                if !self.selected_rows.remove(&row) {
                    self.selected_rows.insert(row);
                }
                Task::none()
            }
            Message::CopyRows => {
                let Some(grid) = &self.grid else { return Task::none() };
                let mut rows: Vec<usize> = if self.selected_rows.is_empty() {
                    (0..grid.rows.len()).collect()
                } else {
                    self.selected_rows.iter().copied().filter(|r| *r < grid.rows.len()).collect()
                };
                rows.sort_unstable();
                let mut text = grid.columns.join("\t");
                for row in &rows {
                    text.push('\n');
                    text.push_str(&grid.rows[*row].join("\t"));
                }
                self.status = format!("Copied {}", if rows.len() == 1 { "1 row".to_string() } else { format!("{} rows", rows.len()) });
                iced::clipboard::write(text)
            }
            Message::ExportCsv => match self.grid.as_ref().map(|g| g.name.clone()) {
                Some(name) if self.table_shown() => Task::perform(save_csv(name), Message::ExportCsvTo),
                _ => Task::none(),
            },
            Message::ExportCsvTo(path) => {
                let (Some(path), Some(grid)) = (path, &self.grid) else { return Task::none() };
                let literal = schematic::Value::string(&path.to_string_lossy()).to_string();
                let source = format!("(write-csv {} {literal})", grid.name);
                self.evaluate(source, format!("Saved {}", path.display()))
            }
            Message::PlotColumn(column) => {
                let Some(grid) = &self.grid else { return Task::none() };
                let source = if self.table_shown() {
                    format!("(histogram (:{column} {}) {{:title {:?}}})", grid.name, format!("{column} of {}", grid.name))
                } else {
                    format!("(histogram {} {{:title {:?}}})", grid.name, grid.name)
                };
                self.evaluate(source, String::new())
            }
            Message::ConsoleFind(open) => {
                if open {
                    self.console_find.get_or_insert_with(String::new);
                    return operation::focus(console_find_id());
                }
                self.console_find = None;
                Task::none()
            }
            Message::ConsoleQuery(query) => {
                self.console_find = Some(query);
                Task::none()
            }
            Message::CopyText(text) => {
                self.status = "Copied".to_string();
                iced::clipboard::write(text)
            }
            Message::SaveTranscript => Task::perform(save_markdown(), Message::SaveTranscriptTo),
            Message::SaveTranscriptTo(path) => {
                if let Some(path) = path {
                    self.status = match std::fs::write(&path, transcript(&self.console)) {
                        Ok(()) => format!("Saved {}", path.display()),
                        Err(e) => format!("Cannot save {}: {e}", path.display()),
                    };
                }
                Task::none()
            }
            Message::LoadTranscript => Task::perform(pick_markdown(), Message::LoadTranscriptFrom),
            Message::LoadTranscriptFrom(path) => {
                let Some(path) = path else { return Task::none() };
                match std::fs::read_to_string(&path) {
                    Ok(text) => {
                        let lines = parse_transcript(&text);
                        for line in lines.iter().filter(|l| l.kind == LineKind::Input) {
                            self.remember(&line.text);
                        }
                        self.console.extend(lines);
                        self.status = format!("Loaded {}", path.display());
                    }
                    Err(e) => self.status = format!("Cannot load {}: {e}", path.display()),
                }
                operation::snap_to(console_id(), RelativeOffset::END)
            }
            Message::RemoveWatch(index) => {
                if index < self.watches.len() {
                    let removed = self.watches.remove(index);
                    self.watched.retain(|w| w.source != removed);
                }
                Task::none()
            }
            Message::Restart => match self.worker {
                Some(_) => Task::perform(confirm_restart(), Message::RestartDecided),
                None => Task::none(),
            },
            Message::RestartDecided(yes) => {
                if yes && let Some(worker) = &self.worker {
                    let _ = worker.send(Request::Restart);
                    self.grid = None;
                    self.bindings.clear();
                    self.watched.clear();
                    self.selected_rows.clear();
                    self.status = "Restarting".to_string();
                }
                Task::none()
            }
            Message::SetTheme(name) => {
                self.theme = theme::named(&name);
                Task::none()
            }
            Message::ZoomIn => self.zoom_by(1),
            Message::ZoomOut => self.zoom_by(-1),
            Message::ZoomReset => {
                self.zoom = 1.0;
                Task::none()
            }
            Message::ScaleBy(step) => {
                self.scale = (((self.scale * 10.0).round() + step as f32) / 10.0).clamp(0.8, 1.6);
                Task::none()
            }
            Message::OpenSettings => {
                self.settings_open = true;
                self.tab = Tab::Settings;
                self.menu = None;
                Task::none()
            }
            Message::ToggleRestore => {
                self.restore = !self.restore;
                Task::none()
            }
            Message::ToggleBackups => {
                self.backups = !self.backups;
                Task::none()
            }
            Message::WindowResized(id, width, height) => {
                if Some(id) == self.main_window {
                    self.window_size = Some((width, height));
                }
                Task::none()
            }
            Message::DetachPane(kind) => self.detach(kind),
            Message::AttachPane(id) => self.attach(id),
            Message::AttachAll => {
                let ids: Vec<window::Id> = self.detached.iter().map(|(id, _)| *id).collect();
                let tasks: Vec<Task<Message>> = ids.into_iter().map(|id| self.attach(id)).collect();
                Task::batch(tasks)
            }
            Message::CloseRequested(id) => {
                if self.main_window.is_none_or(|main| main == id) {
                    return self.update(Message::Quit);
                }
                self.attach(id)
            }
            Message::Dropped(path) => {
                if path.is_dir() {
                    self.set_root(path);
                    self.side = Side::Files;
                    return Task::none();
                }
                let check = self.load(path);
                Task::batch([check, self.scroll_to_doc()])
            }
            Message::OpenPath(path) => {
                let check = self.load(path);
                Task::batch([check, self.scroll_to_doc()])
            }
            Message::OpenStart => {
                self.start_open = true;
                self.tab = Tab::Start;
                self.menu = None;
                Task::none()
            }
            Message::OpenProject(path) => {
                if path.is_dir() {
                    self.set_root(path);
                    self.side = Side::Project;
                } else {
                    self.status = format!("{} is gone", path.display());
                }
                Task::none()
            }
            Message::Tick => {
                self.ticks += 1;
                if self.side == Side::Git && self.ticks.is_multiple_of(3) && !self.git.busy {
                    self.git.stale = true;
                }
                self.check_disk()
            }
            Message::Reload => {
                if self.reload(self.current) {
                    return Task::batch([self.refresh_find(true), self.schedule_check()]);
                }
                Task::none()
            }
            Message::PlotView(view) => {
                self.plot_view = view;
                Task::none()
            }
            Message::PlotReset => {
                self.plot_view = PlotView::default();
                Task::none()
            }
            Message::PlotSelect(i) => {
                if i < self.plots.len() {
                    self.plot_at = i;
                    self.plot_view = PlotView::default();
                }
                Task::none()
            }
            Message::CopyPlot => match self.plot().map(Plot::svg) {
                Some(svg) => {
                    self.status = "Copied the chart as SVG".to_string();
                    iced::clipboard::write(svg)
                }
                None => Task::none(),
            },
            Message::NewNotebook => {
                let cells = notebook::fresh();
                self.docs.push(Document::with_notebook(None, &notebook::render(&cells), Vec::new()));
                self.select(self.docs.len() - 1)
            }
            Message::CellEdit(i, action) => {
                let edit = action.is_edit();
                let doc = self.doc_mut();
                let Some(nb) = &mut doc.notebook else { return Task::none() };
                let Some(cell) = nb.cells.get_mut(i) else { return Task::none() };
                nb.focused = i;
                cell.content.perform(action);
                if cell.kind == Kind::Code {
                    cell.marks = marks_of(&cell.content);
                }
                if edit {
                    doc.sync_notebook();
                    return self.schedule_check();
                }
                Task::none()
            }
            Message::RunCell(i) => self.run_cell(i),
            Message::RunCellAndAdvance(i) => {
                let task = self.run_cell(i);
                let doc = self.doc_mut();
                let mut pushed = false;
                if let Some(nb) = &mut doc.notebook {
                    if i + 1 >= nb.cells.len() {
                        nb.cells.push(Cell::new(CellText { kind: Kind::Code, source: String::new() }, Vec::new()));
                        pushed = true;
                    }
                    nb.focused = (i + 1).min(nb.cells.len() - 1);
                }
                if pushed {
                    doc.sync_notebook();
                }
                task
            }
            Message::RunAllCells => {
                let indexes: Vec<usize> = match &self.doc().notebook {
                    Some(nb) => nb.cells.iter().enumerate().filter(|(_, c)| c.kind == Kind::Code).map(|(i, _)| i).collect(),
                    None => Vec::new(),
                };
                let tasks: Vec<Task<Message>> = indexes.into_iter().map(|i| self.run_cell(i)).collect();
                Task::batch(tasks)
            }
            Message::AddCell(at, kind) => {
                let doc = self.doc_mut();
                let Some(nb) = &mut doc.notebook else { return Task::none() };
                let at = at.min(nb.cells.len());
                let mut cell = Cell::new(CellText { kind, source: String::new() }, Vec::new());
                cell.editing = kind == Kind::Text;
                nb.cells.insert(at, cell);
                nb.focused = at;
                doc.sync_notebook();
                Task::none()
            }
            Message::DeleteCell(i) => {
                let doc = self.doc_mut();
                let Some(nb) = &mut doc.notebook else { return Task::none() };
                if nb.cells.len() > 1 && i < nb.cells.len() {
                    nb.cells.remove(i);
                    nb.focused = nb.focused.min(nb.cells.len() - 1);
                    doc.sync_notebook();
                }
                Task::none()
            }
            Message::MoveCell(i, delta) => {
                let doc = self.doc_mut();
                let Some(nb) = &mut doc.notebook else { return Task::none() };
                let target = i as i64 + i64::from(delta);
                if i < nb.cells.len() && target >= 0 && (target as usize) < nb.cells.len() {
                    nb.cells.swap(i, target as usize);
                    nb.focused = target as usize;
                    doc.sync_notebook();
                }
                Task::none()
            }
            Message::ToggleCellKind(i) => {
                let doc = self.doc_mut();
                let Some(nb) = &mut doc.notebook else { return Task::none() };
                if let Some(cell) = nb.cells.get_mut(i) {
                    cell.kind = if cell.kind == Kind::Code { Kind::Text } else { Kind::Code };
                    cell.editing = false;
                    cell.outputs.clear();
                    cell.marks = if cell.kind == Kind::Code { marks_of(&cell.content) } else { Marks::default() };
                    doc.sync_notebook();
                }
                Task::none()
            }
            Message::EditText(i) => {
                if let Some(nb) = &mut self.doc_mut().notebook
                    && let Some(cell) = nb.cells.get_mut(i) {
                        cell.editing = true;
                        nb.focused = i;
                    }
                Task::none()
            }
            Message::DoneText(i) => {
                if let Some(cell) = self.doc_mut().notebook.as_mut().and_then(|nb| nb.cells.get_mut(i)) {
                    cell.editing = false;
                }
                Task::none()
            }
            Message::ClearOutputs => {
                let doc = self.doc_mut();
                if let Some(nb) = &mut doc.notebook {
                    for cell in &mut nb.cells {
                        cell.outputs.clear();
                        cell.elapsed = None;
                    }
                    doc.dirty = true;
                }
                Task::none()
            }
            Message::ExportNotebook => Task::perform(export_notebook(self.doc().path.clone()), Message::ExportNotebookTo),
            Message::ExportNotebookTo(path) => {
                let (Some(path), Some(nb)) = (path, &self.doc().notebook) else { return Task::none() };
                let (cells, outputs) = (nb.texts(), nb.outputs());
                let title = self.doc().path.as_deref().and_then(Path::file_stem).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Notebook".to_string());
                let markdown = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
                let text = if markdown { notebook::to_markdown(&cells, &outputs) } else { notebook::to_html(&title, &cells, &outputs) };
                self.status = match std::fs::write(&path, text) {
                    Ok(()) => format!("Exported {}", path.display()),
                    Err(e) => format!("Cannot export {}: {e}", path.display()),
                };
                Task::none()
            }
            Message::ImportCsv => Task::perform(pick_csv(), Message::ImportCsvFrom),
            Message::ImportCsvFrom(path) => {
                let Some(path) = path else { return Task::none() };
                let name = csv_name(&path);
                let shown = path.to_string_lossy().replace('\\', "/").replace('"', "\\\"");
                let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.note(&format!("Importing {file} as {name}"));
                let run = self.run(format!("(define {name} (read-csv \"{shown}\"))"), false);
                let inspect = self.inspect(name, 0);
                self.side = Side::Environment;
                Task::batch([run, inspect])
            }
            Message::DescribeBinding(name) => {
                let source = format!("(describe {name})");
                let echo = self.echo(&source);
                Task::batch([echo, self.run(source, true)])
            }
            Message::WatchBinding(name) => {
                if !self.watches.contains(&name) {
                    self.watches.push(name);
                    self.send_watches();
                }
                self.side = Side::Watch;
                Task::none()
            }
            Message::GitRefresh => {
                self.git.stale = false;
                self.git_refresh()
            }
            Message::GitLoaded(snapshot) => {
                match snapshot {
                    Some(GitSnapshot { root, status, log }) => {
                        self.git.root = Some(root);
                        match status {
                            Ok(status) => {
                                self.git.status = Some(status);
                                self.git.error = None;
                            }
                            Err(e) => self.git.error = Some(e),
                        }
                        self.git.log = log;
                    }
                    None => {
                        self.git.root = None;
                        self.git.status = None;
                        self.git.log.clear();
                    }
                }
                Task::none()
            }
            Message::GitMessage(text) => {
                self.git.message = text;
                Task::none()
            }
            Message::GitCommit => {
                let message = self.git.message.trim().to_string();
                if message.is_empty() {
                    return Task::none();
                }
                self.git_op(move |root| git::commit(&root, &message))
            }
            Message::GitStage(path) => self.git_op(move |root| git::stage(&root, &[path])),
            Message::GitUnstage(path) => self.git_op(move |root| git::unstage(&root, &[path])),
            Message::GitStageAll => self.git_op(|root| git::stage(&root, &[])),
            Message::GitUnstageAll => self.git_op(|root| git::unstage(&root, &[])),
            Message::GitDiscard(path, change) => Task::perform(confirm_discard(path.clone()), move |yes| Message::GitDiscardDecided(path.clone(), change, yes)),
            Message::GitDiscardDecided(path, change, true) => self.git_op(move |root| git::discard(&root, &path, change)),
            Message::GitDiscardDecided(_, _, false) => Task::none(),
            Message::GitDiff(path, staged, change) => {
                let Some(root) = self.git.root.clone() else { return Task::none() };
                let title = if staged { format!("{path} (staged)") } else { path.clone() };
                Task::perform(async move { git::diff(&root, &path, staged, change) }, move |result| Message::GitDiffLoaded(title.clone(), result))
            }
            Message::GitDiffLoaded(title, result) => {
                match result {
                    Ok(text) => {
                        let text = if text.trim().is_empty() { "No differences.".to_string() } else { text };
                        self.diff = Some(Diff { title, text });
                        self.tab = Tab::Diff;
                    }
                    Err(e) => self.status = format!("Error: {e}"),
                }
                Task::none()
            }
            Message::GitDone(result) => {
                self.git.busy = false;
                match result {
                    Ok(message) => {
                        if message.starts_with("Committed") {
                            self.git.message.clear();
                        }
                        self.status = message;
                    }
                    Err(e) => self.status = format!("Error: {e}"),
                }
                self.git.stale = true;
                Task::none()
            }
            Message::GitPull => self.git_op(|root| git::pull(&root)),
            Message::GitPush => {
                let upstream = self.git.status.as_ref().is_some_and(|s| s.upstream.is_some());
                self.git_op(move |root| git::push(&root, upstream))
            }
            Message::GitInit => {
                let Some(dir) = self.git_dir() else { return Task::none() };
                if self.git.busy {
                    return Task::none();
                }
                self.git.busy = true;
                Task::perform(async move { git::init(&dir) }, Message::GitDone)
            }
            Message::NewProject => Task::perform(pick_folder(), Message::NewProjectIn),
            Message::NewProjectIn(parent) => {
                let Some(parent) = parent else { return Task::none() };
                self.naming = Some(Naming { kind: NamingKind::Project, parent, text: String::new() });
                self.side = Side::Project;
                operation::focus(naming_id())
            }
            Message::ToggleComment => {
                let (text, cursor) = (self.doc().content.text(), self.doc().content.cursor());
                let other = cursor.selection.unwrap_or(cursor.position);
                let (first, last) = (cursor.position.line.min(other.line), cursor.position.line.max(other.line));
                self.rewrite(forms::toggle_comment(&text, first, last), cursor.position)
            }
            Message::Reindent => {
                let (text, cursor) = (self.doc().content.text(), self.doc().content.cursor());
                let lines = cursor.selection.map(|other| {
                    let (first, last) = (cursor.position.line.min(other.line), cursor.position.line.max(other.line));
                    first..last + 1
                });
                self.rewrite(forms::reindent(&text, lines), cursor.position)
            }
            Message::EditorScrolled(viewport) => {
                self.doc_mut().offset = viewport.absolute_offset().y;
                self.editor_width = viewport.bounds().width;
                self.editor_height = viewport.bounds().height;
                Task::none()
            }
            Message::EditorResized(size) => {
                self.editor_width = size.width;
                self.editor_height = size.height;
                Task::none()
            }
            Message::Input(action) => {
                let follow = follow_completion(&action);
                self.input.perform(action);
                self.input_marks = marks_of(&self.input);
                self.track_completion(Source::Console, follow);
                Task::none()
            }
            Message::Complete(source) => {
                self.refresh_completion(source, true);
                Task::none()
            }
            Message::CompleteMove(step) => {
                if let Some(completion) = &mut self.completion {
                    let count = completion.items.len().max(1) as i32;
                    completion.selected = (completion.selected as i32 + step).rem_euclid(count) as usize;
                }
                Task::none()
            }
            Message::CompleteAccept => match self.completion.as_ref().map(|c| c.selected) {
                Some(index) => self.accept_completion(index),
                None => Task::none(),
            },
            Message::CompletePick(index) => self.accept_completion(index),
            Message::CompleteClose => {
                self.completion = None;
                Task::none()
            }
            Message::NewFile => {
                self.docs.push(Document::untitled());
                self.select(self.docs.len() - 1)
            }
            Message::Open => Task::perform(pick_file(), Message::Opened),
            Message::Opened(path) => match path {
                Some(path) => {
                    let check = self.load(path);
                    Task::batch([check, self.scroll_to_doc()])
                }
                None => Task::none(),
            },
            Message::Save => match self.doc().path.clone() {
                Some(path) => {
                    self.store(self.current, path);
                    Task::none()
                }
                None => Task::perform(save_file(None), Message::SaveTo),
            },
            Message::SaveAs => Task::perform(save_file(self.doc().path.clone()), Message::SaveTo),
            Message::SaveTo(path) => {
                if let Some(path) = path {
                    self.store(self.current, path);
                }
                Task::none()
            }
            Message::SelectTab(Tab::Doc(i)) => self.select(i),
            Message::SelectTab(Tab::Help) => {
                if self.help.is_some() {
                    self.tab = Tab::Help;
                }
                Task::none()
            }
            Message::SelectTab(Tab::Settings) => {
                if self.settings_open {
                    self.tab = Tab::Settings;
                }
                Task::none()
            }
            Message::SelectTab(Tab::Start) => {
                if self.start_open {
                    self.tab = Tab::Start;
                }
                Task::none()
            }
            Message::CloseTab(Tab::Start) => {
                self.start_open = false;
                self.tab = Tab::Doc(self.current);
                Task::none()
            }
            Message::SelectTab(Tab::Diff) => {
                if self.diff.is_some() {
                    self.tab = Tab::Diff;
                }
                Task::none()
            }
            Message::CloseTab(Tab::Diff) => {
                self.diff = None;
                self.tab = Tab::Doc(self.current);
                Task::none()
            }
            Message::CloseTab(Tab::Help) => {
                self.help = None;
                self.tab = Tab::Doc(self.current);
                Task::none()
            }
            Message::CloseTab(Tab::Settings) => {
                self.settings_open = false;
                self.tab = Tab::Doc(self.current);
                Task::none()
            }
            Message::CloseTab(Tab::Doc(i)) => match self.docs.get(i) {
                Some(doc) if doc.dirty => Task::perform(confirm_close(doc.name()), move |decision| Message::CloseDecided(i, decision)),
                Some(_) => self.close(i),
                None => Task::none(),
            },
            Message::CloseDecided(i, Decision::Discard) => self.close(i),
            Message::CloseDecided(i, Decision::Save) => match self.docs.get(i).and_then(|d| d.path.clone()) {
                Some(path) => {
                    self.store(i, path);
                    self.close(i)
                }
                None => Task::perform(save_file(None), move |path| Message::SaveAndClose(i, path)),
            },
            Message::CloseDecided(_, Decision::Cancel) => Task::none(),
            Message::SaveAndClose(i, Some(path)) => {
                self.store(i, path);
                self.close(i)
            }
            Message::SaveAndClose(_, None) => Task::none(),
            Message::OpenHelp => {
                self.help.get_or_insert(Help { page: Page::Reference, query: String::new() });
                self.tab = Tab::Help;
                Task::none()
            }
            Message::LookUp(source) => {
                let (text, position) = self.text_and_cursor(source);
                let offset = forms::offset_of(&text, position.line, position.column);
                let query = forms::symbol_at(&text, offset).unwrap_or_default().to_string();
                self.help = Some(Help { page: Page::Reference, query });
                self.tab = Tab::Help;
                self.completion = None;
                Task::none()
            }
            Message::HelpQuery(query) => {
                if let Some(help) = &mut self.help {
                    help.query = query;
                }
                Task::none()
            }
            Message::HelpPage(page) => {
                if let Some(help) = &mut self.help {
                    help.page = page;
                }
                Task::none()
            }
            Message::ShowSide(side) => {
                self.side = side;
                if side == Side::Git {
                    self.git.stale = true;
                }
                Task::none()
            }
            Message::OpenFolder => Task::perform(pick_folder(), Message::OpenedFolder),
            Message::OpenedFolder(root) => {
                if let Some(root) = root {
                    self.set_root(root);
                    self.side = Side::Files;
                }
                Task::none()
            }
            Message::PickEntry(path) => {
                self.selected = Some(path.clone());
                if path.is_dir() {
                    if !self.expanded.remove(&path) {
                        self.expanded.insert(path);
                    }
                    self.refresh_files();
                    return Task::none();
                }
                let check = self.load(path);
                Task::batch([check, self.scroll_to_doc()])
            }
            Message::StartNaming(kind) => {
                let (parent, text) = match &kind {
                    NamingKind::Rename(path) => (path.parent().map(Path::to_path_buf), file_name_of(path)),
                    NamingKind::Package => (self.project.as_ref().map(|p| p.root.clone()), String::new()),
                    NamingKind::Watch => {
                        self.side = Side::Watch;
                        (Some(PathBuf::new()), String::new())
                    }
                    NamingKind::Project => return self.update(Message::NewProject),
                    _ => (self.naming_parent(), String::new()),
                };
                if let Some(parent) = parent {
                    self.naming = Some(Naming { kind, parent, text });
                    return operation::focus(naming_id());
                }
                Task::none()
            }
            Message::Naming(text) => {
                if let Some(naming) = &mut self.naming {
                    naming.text = text;
                }
                Task::none()
            }
            Message::ConfirmNaming => self.confirm_naming(),
            Message::CancelNaming => {
                self.naming = None;
                Task::none()
            }
            Message::DeleteEntry => match self.selected.clone() {
                Some(path) => Task::perform(confirm_delete(file_name_of(&path)), move |yes| Message::DeleteDecided(path.clone(), yes)),
                None => Task::none(),
            },
            Message::DeleteDecided(path, true) => {
                let result = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
                self.status = match result {
                    Ok(()) => format!("Deleted {}", path.display()),
                    Err(e) => format!("Cannot delete {}: {e}", path.display()),
                };
                self.selected = None;
                self.refresh_files();
                Task::none()
            }
            Message::DeleteDecided(_, false) => Task::none(),
            Message::RefreshFiles => {
                self.refresh_files();
                Task::none()
            }
            Message::CreateProject => {
                if let Some(root) = self.root.clone() {
                    let created = project::create(&root).map(|info| {
                        let message = format!("Created {}", info_name(&info));
                        (info, message)
                    });
                    self.finish_project(created);
                }
                Task::none()
            }
            Message::PickPackage(name) => {
                self.picked_package = Some(name);
                Task::none()
            }
            Message::RemovePackage(name) => {
                if let Some(root) = self.project.as_ref().map(|p| p.root.clone()) {
                    self.finish_project(project::remove(&root, &name));
                }
                Task::none()
            }
            Message::RestoreProject => match self.project.as_ref().map(|p| p.root.clone()) {
                Some(root) if self.busy.is_none() => {
                    self.busy = Some("Restoring packages".to_string());
                    Task::perform(async move { project::restore(&root) }, Message::ProjectDone)
                }
                _ => Task::none(),
            },
            Message::ProjectDone(result) => {
                self.finish_project(result);
                Task::none()
            }
            Message::RunForm => self.run_form(),
            Message::RunFile if self.doc().notebook.is_some() => self.update(Message::RunAllCells),
            Message::RunFile => {
                let source = self.doc().content.text();
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
                self.plot_view = PlotView::default();
                Task::none()
            }
            Message::PlotForward => {
                self.plot_at = (self.plot_at + 1).min(self.plots.len().saturating_sub(1));
                self.plot_view = PlotView::default();
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
            Event::Started(tag) => {
                self.active_run = Some(tag);
                Task::none()
            }
            Event::Output(text) => match self.active_cell() {
                Some(cell) => {
                    match cell.outputs.last_mut() {
                        Some(Output::Printed(last)) => last.push_str(&text),
                        _ => cell.outputs.push(Output::Printed(text)),
                    }
                    Task::none()
                }
                None => self.push(LineKind::Output, text),
            },
            Event::Result(text) => match self.active_cell() {
                Some(cell) => {
                    cell.outputs.push(Output::Result(text));
                    Task::none()
                }
                None => self.push(LineKind::Result, text),
            },
            Event::Error(text) => {
                self.status = format!("Error: {text}");
                match self.active_cell() {
                    Some(cell) => {
                        cell.outputs.push(Output::Error(text));
                        Task::none()
                    }
                    None => self.push(LineKind::Error, text),
                }
            }
            Event::Environment(bindings) => {
                self.bindings = bindings;
                Task::none()
            }
            Event::Namespace(name) => {
                self.namespace = name;
                Task::none()
            }
            Event::Grid(grid) => {
                let same = self
                    .grid
                    .as_ref()
                    .is_some_and(|g| g.name == grid.name && g.offset == grid.offset && g.sort == grid.sort && g.filter == grid.filter);
                if !same {
                    self.selected_rows.clear();
                }
                self.grid = Some(grid);
                Task::none()
            }
            Event::Plot(plot) => {
                if let Some(cell) = self.active_cell() {
                    cell.outputs.push(Output::Chart(plot.clone()));
                }
                self.plots.push(*plot);
                self.plot_at = self.plots.len() - 1;
                self.plot_view = PlotView::default();
                Task::none()
            }
            Event::Watches(watched) => {
                self.watched = watched;
                Task::none()
            }
            Event::Evaluated { label, error } => {
                match error {
                    Some(e) => self.status = format!("Error: {e}"),
                    None if !label.is_empty() => self.status = label,
                    None => {}
                }
                Task::none()
            }
            Event::Restarted(flag) => {
                self.interrupt = Some(flag);
                self.namespace = "user".to_string();
                self.tags.clear();
                self.active_run = None;
                self.runs = 0;
                self.running = false;
                for cell in self.docs.iter_mut().filter_map(|d| d.notebook.as_mut()).flat_map(|nb| nb.cells.iter_mut()) {
                    cell.running = false;
                }
                self.status = "Ready".to_string();
                self.note("Interpreter restarted.");
                operation::snap_to(console_id(), RelativeOffset::END)
            }
            Event::Finished(elapsed) => {
                self.runs = self.runs.saturating_sub(1);
                self.running = self.runs > 0;
                if let Some(tag) = self.active_run.take()
                    && let Some((doc_id, cell_id)) = self.tags.remove(&tag)
                    && let Some(cell) = self.cell_mut(doc_id, cell_id) {
                        cell.running = false;
                        cell.elapsed = Some(elapsed);
                    }
                self.last_run = Some(elapsed);
                if !self.status.starts_with("Error") {
                    self.status = if self.running { "Running".to_string() } else { format!("Ready · {}", duration(elapsed)) };
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
        self.run_tagged(source, show_results, None)
    }

    fn run_tagged(&mut self, source: String, show_results: bool, cell: Option<(u64, u64)>) -> Task<Message> {
        let Some(worker) = self.worker.clone() else {
            self.status = "The interpreter is still starting".to_string();
            return Task::none();
        };
        self.next_tag += 1;
        let tag = self.next_tag;
        if let Some(cell) = cell {
            self.tags.insert(tag, cell);
        }
        self.runs += 1;
        self.running = true;
        self.status = "Running".to_string();
        let load_path = self.load_path();
        let watches = self.watches.clone();
        let _ = worker.send(Request::Run { tag, source, show_results, load_path, watches });
        Task::none()
    }

    fn cell_mut(&mut self, doc_id: u64, cell_id: u64) -> Option<&mut Cell> {
        self.docs.iter_mut().find(|d| d.id == doc_id)?.notebook.as_mut()?.cells.iter_mut().find(|c| c.id == cell_id)
    }

    fn active_cell(&mut self) -> Option<&mut Cell> {
        let (doc_id, cell_id) = *self.tags.get(&self.active_run?)?;
        self.cell_mut(doc_id, cell_id)
    }

    fn run_cell(&mut self, i: usize) -> Task<Message> {
        if self.worker.is_none() {
            self.status = "The interpreter is still starting".to_string();
            return Task::none();
        }
        let doc_id = self.doc().id;
        let Some(nb) = &mut self.doc_mut().notebook else { return Task::none() };
        let Some(cell) = nb.cells.get_mut(i) else { return Task::none() };
        if cell.kind != Kind::Code {
            return Task::none();
        }
        cell.outputs.clear();
        cell.running = true;
        cell.elapsed = None;
        let (cell_id, source) = (cell.id, cell.content.text());
        nb.focused = i;
        self.run_tagged(source, true, Some((doc_id, cell_id)))
    }

    fn evaluate(&mut self, source: String, label: String) -> Task<Message> {
        match &self.worker {
            Some(worker) => {
                let _ = worker.send(Request::Evaluate { source, label });
            }
            None => self.status = "The interpreter is still starting".to_string(),
        }
        Task::none()
    }

    fn reinspect(&mut self) -> Task<Message> {
        match self.grid.as_ref().map(|g| g.name.clone()) {
            Some(name) => self.inspect(name, 0),
            None => Task::none(),
        }
    }

    pub fn table_shown(&self) -> bool {
        self.grid.as_ref().is_some_and(|g| self.bindings.iter().any(|b| b.name == g.name && b.kind == "table"))
    }

    fn send_watches(&self) {
        if let Some(worker) = &self.worker {
            let _ = worker.send(Request::Watch { watches: self.watches.clone() });
        }
    }

    fn echo(&mut self, source: &str) -> Task<Message> {
        self.push(LineKind::Input, source.to_string())
    }

    fn scroll_to_doc(&self) -> Task<Message> {
        operation::scroll_to(editor_id(), AbsoluteOffset { x: 0.0, y: self.doc().offset })
    }

    fn select(&mut self, i: usize) -> Task<Message> {
        if i < self.docs.len() {
            self.current = i;
        }
        self.tab = Tab::Doc(self.current);
        self.completion = None;
        self.scroll_to_doc()
    }

    fn text_and_cursor(&self, source: Source) -> (String, Position) {
        match source {
            Source::Editor => (self.doc().content.text(), self.doc().content.cursor().position),
            Source::Console => (self.input.text(), self.input.cursor().position),
        }
    }

    fn track_completion(&mut self, source: Source, follow: Follow) {
        match follow {
            Follow::Close => self.completion = None,
            Follow::Open => self.refresh_completion(source, false),
            Follow::Refresh if self.completion.is_some() => self.refresh_completion(source, false),
            Follow::Refresh => {}
        }
    }

    fn refresh_completion(&mut self, source: Source, forced: bool) {
        let (text, position) = self.text_and_cursor(source);
        let offset = forms::offset_of(&text, position.line, position.column);
        let prefix = completion::word_before(&text, offset).to_string();
        let open = self.completion.as_ref().is_some_and(|c| c.source == source);
        let enough = prefix.chars().count() >= 2 || (open && !prefix.is_empty());
        if !forced && !enough {
            self.completion = None;
            return;
        }
        let items = completion::candidates(&prefix, &self.bindings, &completion::locals(&text));
        self.completion = (!items.is_empty()).then_some(Completion { items, selected: 0, prefix, source });
    }

    fn accept_completion(&mut self, index: usize) -> Task<Message> {
        let Some(completion) = self.completion.take() else { return Task::none() };
        let Some(item) = completion.items.get(index) else { return Task::none() };
        let content = match completion.source {
            Source::Editor => &mut self.docs[self.current].content,
            Source::Console => &mut self.input,
        };
        for _ in 0..completion.prefix.chars().count() {
            content.perform(Action::Edit(Edit::Backspace));
        }
        content.perform(Action::Edit(Edit::Paste(Arc::new(item.name.clone()))));
        let marks = marks_of(content);
        match completion.source {
            Source::Editor => {
                let doc = self.doc_mut();
                doc.dirty = true;
                doc.marks = marks_for(doc);
                Task::batch([operation::focus(editor_text_id()), self.schedule_check()])
            }
            Source::Console => {
                self.input_marks = marks;
                operation::focus(console_text_id())
            }
        }
    }

    fn close(&mut self, i: usize) -> Task<Message> {
        if i >= self.docs.len() {
            return Task::none();
        }
        self.docs.remove(i);
        if self.docs.is_empty() {
            self.docs.push(Document::untitled());
        }
        if self.current > i || self.current >= self.docs.len() {
            self.current = self.current.saturating_sub(1);
        }
        if matches!(self.tab, Tab::Doc(_)) {
            self.tab = Tab::Doc(self.current);
        }
        self.scroll_to_doc()
    }

    fn follow_cursor(&mut self) -> Task<Message> {
        let height = self.editor_height;
        let line_height = self.line_height();
        let doc = self.doc_mut();
        let line = doc.content.cursor().position.line as f32;
        let top = EDITOR_PAD + line * line_height;
        match scroll_target(top, top + line_height, doc.offset, height) {
            Some(y) => {
                doc.offset = y;
                operation::scroll_to(editor_id(), AbsoluteOffset { x: 0.0, y })
            }
            None => Task::none(),
        }
    }

    fn run_form(&mut self) -> Task<Message> {
        if let Some(nb) = &self.doc().notebook {
            let focused = nb.focused;
            return self.run_cell(focused);
        }
        if let Some(selection) = self.doc().content.selection() {
            let echo = self.echo(&selection);
            return Task::batch([echo, self.run(selection, true)]);
        }
        let text = self.doc().content.text();
        let cursor = self.doc().content.cursor();
        let offset = forms::offset_of(&text, cursor.position.line, cursor.position.column);
        let Some(range) = forms::form_at(&text, offset) else {
            return Task::none();
        };
        let source = text[range.clone()].to_string();
        if let Some(next) = forms::next_form(&text, range.end) {
            let (line, column) = forms::position_of(&text, next.start);
            let position = text_editor::Position { line, column };
            let doc = self.doc_mut();
            doc.content.move_to(text_editor::Cursor { position, selection: None });
            doc.marks = marks_for(doc);
        }
        let follow = self.follow_cursor();
        let echo = self.echo(&source);
        Task::batch([follow, echo, self.run(source, true)])
    }

    fn zoom_by(&mut self, step: i32) -> Task<Message> {
        self.zoom = (((self.zoom * 10.0).round() + step as f32) / 10.0).clamp(0.7, 1.8);
        self.follow_cursor()
    }

    fn after_rewrite(&mut self) -> Task<Message> {
        let doc = self.doc_mut();
        doc.marks = marks_for(doc);
        self.completion = None;
        let refreshed = self.refresh_find(true);
        Task::batch([self.follow_cursor(), refreshed, self.schedule_check()])
    }

    pub fn pane_of(&self, kind: PaneKind) -> Option<pane_grid::Pane> {
        self.panes.iter().find(|(_, k)| **k == kind).map(|(pane, _)| *pane)
    }

    fn toggle_pane(&mut self, kind: PaneKind) {
        use pane_grid::Axis;
        if let Some(pane) = self.pane_of(kind) {
            if self.panes.maximized() == Some(pane) {
                self.panes.restore();
            }
            self.panes.close(pane);
            return;
        }
        let first = |kinds: &[PaneKind]| kinds.iter().find_map(|k| self.pane_of(*k)).or_else(|| self.panes.iter().next().map(|(p, _)| *p));
        let (anchor, axis, before) = match kind {
            PaneKind::Editor => (first(&[PaneKind::Console, PaneKind::Environment]), Axis::Horizontal, true),
            PaneKind::Console => (first(&[PaneKind::Editor, PaneKind::Environment]), Axis::Horizontal, false),
            PaneKind::Environment => (first(&[PaneKind::Viewer, PaneKind::Plots, PaneKind::Editor]), Axis::Vertical, false),
            PaneKind::Viewer => (first(&[PaneKind::Environment, PaneKind::Plots, PaneKind::Editor]), Axis::Horizontal, false),
            PaneKind::Plots => (first(&[PaneKind::Viewer, PaneKind::Environment, PaneKind::Console]), Axis::Horizontal, false),
        };
        let Some(anchor) = anchor else { return };
        if let Some((pane, _)) = self.panes.split(axis, anchor, kind)
            && before {
                self.panes.swap(pane, anchor);
            }
    }

    fn shortcut(&mut self, shortcut: Shortcut) -> Task<Message> {
        match shortcut {
            Shortcut::Escape => {
                if self.menu.take().is_some() || self.palette.take().is_some() || self.completion.take().is_some() {
                    return Task::none();
                }
                if self.find.is_some() {
                    return self.update(Message::FindClose);
                }
                Task::none()
            }
            Shortcut::Up if self.palette.is_some() => self.update(Message::PaletteMove(-1)),
            Shortcut::Down if self.palette.is_some() => self.update(Message::PaletteMove(1)),
            Shortcut::Up | Shortcut::Down => Task::none(),
            Shortcut::Palette => self.update(Message::PaletteOpen(String::new())),
            Shortcut::GoToLine => self.update(Message::PaletteOpen(":".to_string())),
            Shortcut::ZoomIn => self.update(Message::ZoomIn),
            Shortcut::ZoomOut => self.update(Message::ZoomOut),
            Shortcut::ZoomReset => self.update(Message::ZoomReset),
            Shortcut::Preferences => self.update(Message::OpenSettings),
            Shortcut::FindNext => self.update(Message::FindStep(1)),
            Shortcut::FindPrevious => self.update(Message::FindStep(-1)),
            Shortcut::Find => self.update(Message::FindOpen(false)),
            Shortcut::Replace => self.update(Message::FindOpen(true)),
            Shortcut::Save => self.update(Message::Save),
            Shortcut::SaveAs => self.update(Message::SaveAs),
            Shortcut::Open => self.update(Message::Open),
            Shortcut::New => self.update(Message::NewFile),
            Shortcut::NewNotebook => self.update(Message::NewNotebook),
            Shortcut::Close => self.update(Message::CloseTab(Tab::Doc(self.current))),
            Shortcut::Pane(1) => operation::focus(editor_text_id()),
            Shortcut::Pane(2) => operation::focus(console_text_id()),
            Shortcut::Pane(n) => {
                self.side = match n {
                    3 => Side::Environment,
                    4 => Side::Files,
                    5 => Side::Project,
                    6 => Side::Outline,
                    7 => Side::Watch,
                    _ => Side::Git,
                };
                if self.side == Side::Git {
                    self.git.stale = true;
                }
                Task::none()
            }
        }
    }

    fn refresh_find(&mut self, from_cursor: bool) -> Task<Message> {
        let Some(find) = &self.find else { return Task::none() };
        let (query, case, previous) = (find.query.clone(), find.case, find.current);
        let doc = self.doc_mut();
        let text = doc.content.text();
        let cursor = doc.content.cursor();
        let anchor = match cursor.selection {
            Some(other) if (other.line, other.column) < (cursor.position.line, cursor.position.column) => other,
            _ => cursor.position,
        };
        let offset = forms::offset_of(&text, anchor.line, anchor.column);
        doc.found = forms::find_all(&text, &query, case);
        let count = doc.found.len();
        let current = if count == 0 {
            None
        } else if from_cursor {
            Some(doc.found.iter().position(|(start, _)| *start >= offset).unwrap_or(0))
        } else {
            Some(previous.unwrap_or(0).min(count - 1))
        };
        doc.marks = marks_for(doc);
        if let Some(find) = &mut self.find {
            find.current = current;
        }
        self.select_match()
    }

    fn select_match(&mut self) -> Task<Message> {
        let Some(index) = self.find.as_ref().and_then(|f| f.current) else { return Task::none() };
        let doc = self.doc_mut();
        let Some(&(start, end)) = doc.found.get(index) else { return Task::none() };
        let text = doc.content.text();
        let (from_line, from_column) = forms::position_of(&text, start);
        let (to_line, to_column) = forms::position_of(&text, end);
        let position = Position { line: to_line, column: to_column };
        let selection = Some(Position { line: from_line, column: from_column });
        doc.content.move_to(text_editor::Cursor { position, selection });
        doc.last_edit = None;
        doc.marks = marks_for(doc);
        self.follow_cursor()
    }

    fn replace_match(&mut self, all: bool) -> Task<Message> {
        let Some(find) = self.find.clone() else { return Task::none() };
        let doc = self.doc();
        let text = doc.content.text();
        let targets: Vec<(usize, usize)> = if all { doc.found.clone() } else { find.current.and_then(|i| doc.found.get(i)).copied().into_iter().collect() };
        if targets.is_empty() {
            return Task::none();
        }
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for &(start, end) in &targets {
            out.push_str(&text[last..start]);
            out.push_str(&find.replacement);
            last = end;
        }
        out.push_str(&text[last..]);
        let resume = targets[0].0 + find.replacement.len();
        let (line, column) = forms::position_of(&out, resume);
        if all {
            self.status = format!("Replaced {} of {}", targets.len(), if targets.len() == 1 { "1 match" } else { "the matches" });
        }
        self.doc_mut().replace_text(&out, Position { line, column }, None);
        self.after_rewrite()
    }

    fn rewrite(&mut self, text: String, position: Position) -> Task<Message> {
        if text == self.doc().content.text() {
            return Task::none();
        }
        let line = position.line.min(text.split('\n').count().saturating_sub(1));
        let column = position.column.min(text.split('\n').nth(line).map_or(0, |l| l.chars().count()));
        self.doc_mut().replace_text(&text, Position { line, column }, None);
        self.after_rewrite()
    }

    fn step_history(&mut self, back: bool) -> Task<Message> {
        let doc = self.doc_mut();
        let moved = if back { doc.undo() } else { doc.redo() };
        if !moved {
            return Task::none();
        }
        self.after_rewrite()
    }

    fn load_path(&self) -> Vec<PathBuf> {
        let mut load_path: Vec<PathBuf> = self.doc().path.as_deref().and_then(Path::parent).map(Path::to_path_buf).into_iter().collect();
        load_path.extend(self.root.clone());
        load_path.extend(self.project.iter().flat_map(|p| p.load_path.clone()));
        load_path
    }

    fn schedule_check(&mut self) -> Task<Message> {
        let load_path = self.load_path();
        let doc = self.doc_mut();
        doc.checked += 1;
        let (id, generation, text) = (doc.id, doc.checked, doc.content.text());
        let dir = doc.path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        Task::perform(async move { check_source(&text, dir.as_deref(), &load_path) }, move |problems| Message::Checked(id, generation, problems))
    }

    pub fn problem_here(&self) -> Option<&Diagnostic> {
        let doc = self.doc();
        let text = doc.content.text();
        let cursor = doc.content.cursor().position;
        doc.problem_at(forms::offset_of(&text, cursor.line, cursor.column))
    }

    fn next_problem(&mut self) -> Task<Message> {
        let doc = self.doc();
        let text = doc.content.text();
        let cursor = doc.content.cursor().position;
        let offset = forms::offset_of(&text, cursor.line, cursor.column);
        let Some(target) = doc.problems.iter().find(|d| d.span.start > offset).or(doc.problems.first()).map(|d| d.span.start) else {
            return Task::none();
        };
        let (line, column) = forms::position_of(&text, target);
        let doc = self.doc_mut();
        doc.content.move_to(text_editor::Cursor { position: Position { line, column }, selection: None });
        doc.marks = marks_for(doc);
        self.follow_cursor()
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
        self.input_marks = Marks::default();
        self.completion = None;
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
            let sort = self.grid_sort.clone();
            let filter = self.grid_filter.clone();
            let _ = worker.send(Request::Inspect { name, offset, limit: PAGE, sort, filter });
        }
        Task::none()
    }

    fn load(&mut self, path: PathBuf) -> Task<Message> {
        if let Some(i) = self.docs.iter().position(|d| d.path.as_ref() == Some(&path)) {
            self.current = i;
            self.tab = Tab::Doc(i);
            return Task::none();
        }
        match std::fs::read_to_string(&path) {
            Ok(source) => {
                self.status = format!("Opened {}", path.display());
                settings::remember(&mut self.recent, &path);
                if self.root.is_none()
                    && let Some(parent) = path.parent() {
                        self.set_root(parent.to_path_buf());
                    }
                if self.project.is_none() {
                    self.project = path.parent().and_then(project::describe);
                }
                let source = source.replace("\r\n", "\n");
                let doc = if notebook::is_notebook(&source) {
                    let outputs = std::fs::read_to_string(notebook::sidecar(&path)).map(|text| notebook::parse_outputs(&text)).unwrap_or_default();
                    Document::with_notebook(Some(path), &source, outputs)
                } else {
                    Document::new(Some(path), text_editor::Content::with_text(&source))
                };
                if self.doc().blank() {
                    self.docs[self.current] = doc;
                } else {
                    self.docs.push(doc);
                    self.current = self.docs.len() - 1;
                }
                self.tab = Tab::Doc(self.current);
                return self.schedule_check();
            }
            Err(e) => self.status = format!("Cannot open {}: {e}", path.display()),
        }
        Task::none()
    }

    fn store(&mut self, i: usize, path: PathBuf) {
        let Some(doc) = self.docs.get_mut(i) else { return };
        if self.backups && self.persistent {
            settings::back_up(&path);
        }
        match std::fs::write(&path, doc.content.text()) {
            Ok(()) => {
                doc.dirty = false;
                doc.stale = false;
                doc.modified = modified_at(&path);
                doc.path = Some(path.clone());
                if let Some(nb) = &doc.notebook {
                    let outputs = nb.outputs();
                    let sidecar = notebook::sidecar(&path);
                    if outputs.iter().any(|o| !o.is_empty()) {
                        let _ = std::fs::write(&sidecar, notebook::render_outputs(&outputs));
                    } else if sidecar.exists() {
                        let _ = std::fs::remove_file(&sidecar);
                    }
                }
                self.status = format!("Saved {}", path.display());
                settings::remember(&mut self.recent, &path);
                self.refresh_files();
            }
            Err(e) => self.status = format!("Cannot save {}: {e}", path.display()),
        }
    }
}

impl Default for Studio {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn studio_with_content() -> Studio {
        let mut studio = Studio::new();
        studio.doc_mut().content = text_editor::Content::with_text("(define x 1)\n\n(+ x\n   1)\n");
        studio.console.push(Line { kind: LineKind::Input, text: "(+ 1 2)".into() });
        studio.console.push(Line { kind: LineKind::Result, text: "3".into() });
        studio.console.push(Line { kind: LineKind::Error, text: "boom".into() });
        studio.bindings = vec![Binding { name: "t".into(), kind: "table".into(), summary: "2 rows, 1 column".into(), columns: vec!["a".into()] }];
        studio.grid = Some(Grid {
            name: "t".into(),
            columns: vec!["#".into(), "a".into()],
            numeric: vec![true, true],
            rows: vec![vec!["0".into(), "1".into()]],
            offset: 0,
            total: 2,
            ..Grid::default()
        });
        studio
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("studio-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_window_builds_with_and_without_content() {
        let _ = view::view(&Studio::new());
        let _ = view::view(&studio_with_content());
    }

    #[test]
    fn problems_are_marked_and_shown_at_the_cursor() {
        let mut studio = Studio::new();
        studio.doc_mut().content = text_editor::Content::with_text("(define n 3)\n(string-upcase n)\n");
        let (id, generation) = (studio.doc().id, studio.doc().checked);
        let problems = check_source(&studio.doc().content.text(), None, &[]);
        assert_eq!(problems.len(), 1);
        let _ = studio.update(Message::Checked(id, generation + 7, problems.clone()));
        assert!(studio.doc().problems.is_empty());
        let _ = studio.update(Message::Checked(id, generation, problems));
        assert_eq!(studio.doc().marks.problems, vec![(1, 15..16)]);
        assert!(studio.problem_here().is_none());
        let _ = studio.update(Message::NextProblem);
        let cursor = studio.doc().content.cursor().position;
        assert_eq!((cursor.line, cursor.column), (1, 15));
        assert_eq!(studio.problem_here().map(|d| d.message.as_str()), Some("string-upcase expects a string, got an integer"));
        let _ = view::view(&studio);
        let _ = studio.update(Message::Edit(Action::Edit(Edit::Insert('x'))));
        assert_eq!(studio.doc().checked, generation + 1);
    }

    #[test]
    fn edits_group_into_undo_steps_and_redo_restores_them() {
        let mut studio = Studio::new();
        let edit = |studio: &mut Studio, edit: Edit| {
            let _ = studio.update(Message::Edit(Action::Edit(edit)));
        };
        for c in "ab".chars() {
            edit(&mut studio, Edit::Insert(c));
        }
        edit(&mut studio, Edit::Enter);
        edit(&mut studio, Edit::Insert(' '));
        edit(&mut studio, Edit::Insert('c'));
        assert_eq!(studio.doc().content.text(), "ab\n c");
        assert_eq!(studio.doc().history.len(), 3);
        let _ = studio.update(Message::Undo);
        assert_eq!(studio.doc().content.text(), "ab\n ");
        let _ = studio.update(Message::Undo);
        assert_eq!(studio.doc().content.text(), "ab");
        assert_eq!(studio.doc().content.cursor().position.column, 2);
        let _ = studio.update(Message::Undo);
        assert_eq!(studio.doc().content.text(), "");
        let _ = studio.update(Message::Undo);
        let _ = studio.update(Message::Redo);
        assert_eq!(studio.doc().content.text(), "ab");
        edit(&mut studio, Edit::Insert('!'));
        assert!(studio.doc().future.is_empty());
        let _ = studio.update(Message::Redo);
        assert_eq!(studio.doc().content.text(), "ab!");
    }

    #[test]
    fn comment_toggle_and_reindent_rewrite_the_document_undoably() {
        let mut studio = Studio::new();
        studio.doc_mut().content = text_editor::Content::with_text("(define (f x)\n(+ x 1))\n");
        let _ = studio.update(Message::Edit(Action::Move(Motion::Down)));
        let _ = studio.update(Message::ToggleComment);
        assert_eq!(studio.doc().content.text(), "(define (f x)\n;; (+ x 1))\n");
        assert_eq!(studio.doc().content.cursor().position.line, 1);
        let _ = studio.update(Message::ToggleComment);
        assert_eq!(studio.doc().content.text(), "(define (f x)\n(+ x 1))\n");
        let _ = studio.update(Message::Reindent);
        assert_eq!(studio.doc().content.text(), "(define (f x)\n  (+ x 1))\n");
        assert!(studio.doc().dirty);
        let _ = studio.update(Message::Undo);
        assert_eq!(studio.doc().content.text(), "(define (f x)\n(+ x 1))\n");
    }

    #[test]
    fn find_selects_matches_and_replace_rewrites_them() {
        let mut studio = Studio::new();
        studio.doc_mut().content = text_editor::Content::with_text("(foo 1)\n(Foo 2)\n(bar foo)\n");
        let _ = studio.update(Message::FindOpen(false));
        let _ = studio.update(Message::FindQuery("foo".into()));
        assert_eq!(studio.doc().found, [(1, 4), (9, 12), (21, 24)]);
        assert_eq!(studio.find.as_ref().unwrap().current, Some(0));
        let cursor = studio.doc().content.cursor();
        assert_eq!((cursor.position.line, cursor.position.column), (0, 4));
        assert_eq!(cursor.selection.map(|p| (p.line, p.column)), Some((0, 1)));
        assert_eq!(studio.doc().marks.found, vec![(0, 1..4), (1, 1..4), (2, 5..8)]);
        let _ = studio.update(Message::FindStep(1));
        assert_eq!(studio.doc().content.cursor().position.line, 1);
        let _ = studio.update(Message::FindStep(-1));
        let _ = studio.update(Message::FindStep(-1));
        assert_eq!((studio.find.as_ref().unwrap().current, studio.doc().content.cursor().position.line), (Some(2), 2));
        let _ = studio.update(Message::FindCase);
        assert_eq!(studio.doc().found, [(1, 4), (21, 24)]);
        let _ = studio.update(Message::FindStep(1));
        let _ = studio.update(Message::FindReplacement("qux".into()));
        let _ = studio.update(Message::FindReplace);
        assert_eq!(studio.doc().content.text(), "(qux 1)\n(Foo 2)\n(bar foo)\n");
        assert_eq!(studio.doc().found, [(21, 24)]);
        assert_eq!(studio.doc().content.cursor().position.line, 2);
        let _ = studio.update(Message::FindReplaceAll);
        assert_eq!(studio.doc().content.text(), "(qux 1)\n(Foo 2)\n(bar qux)\n");
        assert!(studio.doc().found.is_empty());
        let _ = studio.update(Message::Shortcut(Shortcut::Escape));
        assert!(studio.find.is_none() && studio.doc().marks.found.is_empty());
        let _ = studio.update(Message::Undo);
        assert_eq!(studio.doc().content.text(), "(qux 1)\n(Foo 2)\n(bar foo)\n");
        let _ = view::view(&studio);
    }

    #[test]
    fn the_outline_jumps_and_the_selection_expands() {
        let mut studio = Studio::new();
        studio.doc_mut().content = text_editor::Content::with_text("(ns app)\n\n(define (f a)\n  (+ a 1))\n");
        let _ = studio.update(Message::ShowSide(Side::Outline));
        let _ = view::view(&studio);
        let _ = studio.update(Message::JumpTo(10));
        assert_eq!(studio.doc().content.cursor().position.line, 2);
        let _ = studio.update(Message::Edit(Action::Move(Motion::Down)));
        let _ = studio.update(Message::Edit(Action::Move(Motion::Right)));
        let _ = studio.update(Message::Edit(Action::Move(Motion::Right)));
        let _ = studio.update(Message::Edit(Action::Move(Motion::Right)));
        let _ = studio.update(Message::ExpandSelection);
        assert_eq!(studio.doc().content.selection().as_deref(), Some("+"));
        let _ = studio.update(Message::ExpandSelection);
        assert_eq!(studio.doc().content.selection().as_deref(), Some("(+ a 1)"));
        let _ = studio.update(Message::ExpandSelection);
        assert_eq!(studio.doc().content.selection().as_deref(), Some("(define (f a)\n  (+ a 1))"));
        let _ = studio.update(Message::ToggleWrap);
        assert!(studio.wrap);
        let _ = view::view(&studio);
    }

    #[test]
    fn the_palette_and_menus_run_commands_and_panes_hide_and_return() {
        let mut studio = studio_with_content();
        let _ = studio.update(Message::Shortcut(Shortcut::Palette));
        assert_eq!(studio.palette.as_ref().map(|p| p.query.as_str()), Some(""));
        let _ = studio.update(Message::PaletteQuery("word wrap".into()));
        let _ = view::view(&studio);
        let _ = studio.update(Message::PaletteRun(None));
        assert!(studio.wrap && studio.palette.is_none());
        let _ = studio.update(Message::Shortcut(Shortcut::GoToLine));
        let _ = studio.update(Message::PaletteQuery(":3".into()));
        let _ = studio.update(Message::PaletteRun(None));
        assert_eq!(studio.doc().content.cursor().position.line, 2);
        let _ = studio.update(Message::MenuToggle(Menu::View));
        assert_eq!(studio.menu, Some(Menu::View));
        let _ = view::view(&studio);
        let _ = studio.update(Message::MenuHover(Menu::Edit));
        assert_eq!(studio.menu, Some(Menu::Edit));
        let _ = studio.update(Message::Shortcut(Shortcut::Escape));
        assert_eq!(studio.menu, None);
        let plots = commands::commands(&studio).iter().position(|c| c.name == "Plots").unwrap();
        let _ = studio.update(Message::MenuPick(plots));
        assert_eq!(studio.pane_of(PaneKind::Plots), None);
        assert_eq!(studio.panes.iter().count(), 4);
        let _ = studio.update(Message::TogglePane(PaneKind::Plots));
        assert!(studio.pane_of(PaneKind::Plots).is_some());
        let _ = studio.update(Message::Maximize(PaneKind::Editor));
        assert_eq!(studio.panes.maximized(), studio.pane_of(PaneKind::Editor));
        let _ = view::view(&studio);
        let _ = studio.update(Message::RestorePanes);
        assert_eq!(studio.panes.maximized(), None);
        for kind in [PaneKind::Editor, PaneKind::Console, PaneKind::Environment, PaneKind::Viewer, PaneKind::Plots] {
            let _ = studio.update(Message::TogglePane(kind));
        }
        assert_eq!(studio.panes.iter().count(), 1, "the last pane stays");
        let _ = studio.update(Message::ShowHelp(Page::Guide));
        assert_eq!((studio.tab, studio.help.as_ref().map(|h| h.page)), (Tab::Help, Some(Page::Guide)));
    }

    #[test]
    fn grids_sort_and_copy_the_console_filters_and_transcripts_round_trip() {
        let mut studio = studio_with_content();
        let _ = studio.update(Message::SortBy("a".into()));
        assert_eq!(studio.grid_sort, Some(("a".to_string(), false)));
        let _ = studio.update(Message::SortBy("a".into()));
        assert_eq!(studio.grid_sort, Some(("a".to_string(), true)));
        let _ = studio.update(Message::SortBy("a".into()));
        assert_eq!(studio.grid_sort, None);
        let _ = studio.update(Message::GridFilter("1".into()));
        let _ = studio.update(Message::SelectRow(0));
        assert!(studio.selected_rows.contains(&0));
        let _ = studio.update(Message::CopyRows);
        assert_eq!(studio.status, "Copied 1 row");
        assert!(studio.table_shown());
        let _ = view::view(&studio);
        let _ = studio.update(Message::ConsoleFind(true));
        let _ = studio.update(Message::ConsoleQuery("boom".into()));
        let _ = view::view(&studio);
        let markdown = transcript(&studio.console);
        assert!(markdown.contains("```scheme\n(+ 1 2)\n```") && markdown.contains("```error\nboom\n```"), "{markdown}");
        let lines = parse_transcript(&markdown);
        assert_eq!(lines, studio.console);
        let _ = studio.update(Message::StartNaming(NamingKind::Watch));
        let _ = studio.update(Message::Naming("(count t)".into()));
        let _ = studio.update(Message::ConfirmNaming);
        assert_eq!((studio.watches.as_slice(), studio.side), (["(count t)".to_string()].as_slice(), Side::Watch));
        let _ = studio.update(Message::Worker(Event::Watches(vec![Watched { source: "(count t)".into(), value: Ok("2".into()) }])));
        let _ = view::view(&studio);
        let _ = studio.update(Message::RemoveWatch(0));
        assert!(studio.watches.is_empty() && studio.watched.is_empty());
        let _ = studio.update(Message::Worker(Event::Finished(1234)));
        assert_eq!(studio.status, "Ready · 1.2 s");
        let _ = studio.update(Message::Worker(Event::Evaluated { label: "Saved x.csv".into(), error: None }));
        assert_eq!(studio.status, "Saved x.csv");
        let _ = studio.update(Message::RestartDecided(true));
        assert!(studio.grid.is_some(), "without a worker nothing is reset");
    }

    #[test]
    fn a_new_project_is_created_from_a_name_in_a_chosen_folder() {
        let dir = scratch("newproject");
        let mut studio = Studio::new();
        let _ = studio.update(Message::NewProjectIn(Some(dir.clone())));
        assert_eq!((studio.naming.as_ref().map(|n| n.kind.clone()), studio.side), (Some(NamingKind::Project), Side::Project));
        let _ = view::view(&studio);
        let _ = studio.update(Message::Naming("bad/name".into()));
        let _ = studio.update(Message::ConfirmNaming);
        assert!(studio.status.starts_with("Project names"));
        let _ = studio.update(Message::NewProjectIn(Some(dir.clone())));
        let _ = studio.update(Message::Naming("report".into()));
        let _ = studio.update(Message::ConfirmNaming);
        let created = dir.join("report");
        assert!(created.join("project.scm").is_file() && created.join("src").join("report.scm").is_file());
        assert_eq!(studio.root.as_deref(), Some(created.as_path()));
        assert_eq!(studio.project.as_ref().map(|p| p.name.as_str()), Some("report"));
        assert_eq!(studio.doc().path.as_deref(), Some(created.join("src").join("report.scm").as_path()));
        assert!(studio.status.starts_with("Created report"), "{}", studio.status);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn shortcuts_are_recognised_from_keys() {
        use iced::keyboard::key::Named;
        use iced::keyboard::{Key, Modifiers};
        let character = |c: &str| Key::Character(c.into());
        assert_eq!(shortcut_of(&Key::Named(Named::Escape), Modifiers::empty()), Some(Shortcut::Escape));
        assert_eq!(shortcut_of(&Key::Named(Named::F3), Modifiers::SHIFT), Some(Shortcut::FindPrevious));
        assert_eq!(shortcut_of(&character("f"), Modifiers::COMMAND), Some(Shortcut::Find));
        assert_eq!(shortcut_of(&character("F"), Modifiers::COMMAND | Modifiers::SHIFT), None);
        assert_eq!(shortcut_of(&character("S"), Modifiers::COMMAND | Modifiers::SHIFT), Some(Shortcut::SaveAs));
        assert_eq!(shortcut_of(&character("P"), Modifiers::COMMAND | Modifiers::SHIFT), Some(Shortcut::Palette));
        assert_eq!(shortcut_of(&character("3"), Modifiers::COMMAND), Some(Shortcut::Pane(3)));
        assert_eq!(shortcut_of(&character("9"), Modifiers::COMMAND), None);
        assert_eq!(shortcut_of(&character("f"), Modifiers::empty()), None);
    }

    #[test]
    fn problem_marks_clip_to_their_line() {
        let text = "ab\ncd";
        let d = |start, end| Diagnostic { line: 1, column: 1, span: schematic::reader::Span { start, end }, message: String::new() };
        assert_eq!(problem_marks(text, &[d(3, 5), d(1, 1), d(5, 5)]), vec![(1, 0..2), (0, 1..2), (1, 1..2)]);
    }

    #[test]
    fn running_the_form_at_the_cursor_echoes_it_and_moves_on() {
        let mut studio = studio_with_content();
        let _ = studio.update(Message::RunForm);
        assert_eq!(studio.console.last().unwrap().text, "(define x 1)");
        assert_eq!(studio.doc().content.cursor().position.line, 2);
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
        let texts: Vec<&str> = studio.console.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["ab\n", "1"]);
    }

    #[test]
    fn opening_and_saving_go_through_paths_chosen_by_dialogs() {
        let dir = scratch("files");
        let source = dir.join("in.scm");
        std::fs::write(&source, "(define a 1)\r\n").unwrap();
        let mut studio = Studio::new();
        let _ = studio.update(Message::Opened(Some(source.clone())));
        assert_eq!(studio.doc().content.text(), "(define a 1)\n");
        assert_eq!(studio.doc().path.as_deref(), Some(source.as_path()));
        assert!(!studio.doc().dirty);
        assert_eq!(studio.title(window::Id::unique()), "in.scm - Schematic Studio");
        let _ = studio.update(Message::Edit(text_editor::Action::Edit(text_editor::Edit::Insert(';'))));
        assert!(studio.doc().dirty);
        assert_eq!(studio.title(window::Id::unique()), "in.scm* - Schematic Studio");
        let target = dir.join("out.scm");
        let _ = studio.update(Message::SaveTo(Some(target.clone())));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), ";(define a 1)\n");
        assert_eq!(studio.doc().path.as_deref(), Some(target.as_path()));
        assert!(!studio.doc().dirty);
        let _ = studio.update(Message::Opened(None));
        assert_eq!(studio.doc().path.as_deref(), Some(target.as_path()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn documents_open_in_tabs_and_close_again() {
        let dir = scratch("tabs");
        let (a, b) = (dir.join("a.scm"), dir.join("b.scm"));
        std::fs::write(&a, "(define a 1)").unwrap();
        std::fs::write(&b, "(define b 2)").unwrap();
        let mut studio = Studio::new();
        let _ = studio.update(Message::Opened(Some(a.clone())));
        assert_eq!(studio.docs.len(), 1, "a blank untitled document is replaced");
        let _ = studio.update(Message::Opened(Some(b.clone())));
        assert_eq!((studio.docs.len(), studio.current, studio.tab), (2, 1, Tab::Doc(1)));
        let _ = studio.update(Message::Opened(Some(a.clone())));
        assert_eq!((studio.docs.len(), studio.current), (2, 0));
        let _ = studio.update(Message::NewFile);
        assert_eq!((studio.docs.len(), studio.current, studio.file_name()), (3, 2, "Untitled".to_string()));
        let _ = studio.update(Message::SelectTab(Tab::Doc(1)));
        assert_eq!(studio.file_name(), "b.scm");
        let _ = studio.update(Message::CloseTab(Tab::Doc(0)));
        assert_eq!((studio.docs.len(), studio.current, studio.file_name()), (2, 0, "b.scm".to_string()));
        let _ = studio.update(Message::Edit(text_editor::Action::Edit(text_editor::Edit::Insert('x'))));
        let _ = studio.update(Message::CloseDecided(0, Decision::Discard));
        assert_eq!((studio.docs.len(), studio.file_name()), (1, "Untitled".to_string()));
        let _ = studio.update(Message::CloseTab(Tab::Doc(0)));
        assert_eq!((studio.docs.len(), studio.file_name()), (1, "Untitled".to_string()));
        let _ = studio.update(Message::CloseTab(Tab::Doc(7)));
        let _ = view::view(&studio);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_files_pane_browses_and_manages_a_folder() {
        let dir = scratch("tree");
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("a.scm"), "(define a 1)").unwrap();
        std::fs::write(sub.join("b.scm"), "(define b 2)").unwrap();
        let mut studio = Studio::new();
        assert!(studio.root.is_none() && studio.entries.is_empty());
        let _ = studio.update(Message::OpenedFolder(Some(dir.clone())));
        assert_eq!(studio.side, Side::Files);
        let names = |studio: &Studio| studio.entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&studio), ["sub", "a.scm"]);
        let _ = studio.update(Message::PickEntry(sub.clone()));
        assert_eq!(names(&studio), ["sub", "b.scm", "a.scm"]);
        let _ = studio.update(Message::PickEntry(sub.join("b.scm")));
        assert_eq!((studio.file_name(), studio.docs.len()), ("b.scm".to_string(), 1));
        let _ = studio.update(Message::StartNaming(NamingKind::File));
        assert_eq!(studio.naming.as_ref().map(|n| n.parent.clone()), Some(sub.clone()));
        let _ = studio.update(Message::Naming("c.scm".into()));
        let _ = studio.update(Message::ConfirmNaming);
        assert!(sub.join("c.scm").is_file());
        assert_eq!((studio.file_name(), studio.docs.len()), ("c.scm".to_string(), 2));
        let _ = studio.update(Message::StartNaming(NamingKind::Rename(sub.join("c.scm"))));
        assert_eq!(studio.naming.as_ref().map(|n| n.text.clone()), Some("c.scm".to_string()));
        let _ = studio.update(Message::Naming("d.scm".into()));
        let _ = studio.update(Message::ConfirmNaming);
        assert!(sub.join("d.scm").is_file() && !sub.join("c.scm").exists());
        assert_eq!(studio.file_name(), "d.scm");
        let _ = studio.update(Message::StartNaming(NamingKind::Folder));
        let _ = studio.update(Message::Naming("deep".into()));
        let _ = studio.update(Message::ConfirmNaming);
        assert!(sub.join("deep").is_dir());
        let _ = studio.update(Message::StartNaming(NamingKind::File));
        let _ = studio.update(Message::Naming(" ".into()));
        let _ = studio.update(Message::ConfirmNaming);
        assert!(studio.status.starts_with("Names cannot"), "{}", studio.status);
        let _ = studio.update(Message::StartNaming(NamingKind::File));
        let _ = studio.update(Message::Naming("a.scm".into()));
        let _ = studio.update(Message::CancelNaming);
        assert!(studio.naming.is_none() && !sub.join("deep").join("a.scm").exists());
        let _ = studio.update(Message::DeleteDecided(sub.join("d.scm"), false));
        assert!(sub.join("d.scm").exists());
        let _ = studio.update(Message::DeleteDecided(sub.join("d.scm"), true));
        assert!(!sub.join("d.scm").exists());
        assert_eq!(names(&studio), ["sub", "deep", "b.scm", "a.scm"]);
        let _ = view::view(&studio);
        let _ = studio.update(Message::ShowSide(Side::Environment));
        let _ = view::view(&studio);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn completion_offers_names_and_inserts_the_pick() {
        let mut studio = Studio::new();
        studio.bindings = vec![Binding { name: "t".into(), kind: "table".into(), summary: "2 rows".into(), columns: vec!["city".into()] }];
        let type_in = |studio: &mut Studio, text: &str| {
            for c in text.chars() {
                let _ = studio.update(Message::Edit(Action::Edit(Edit::Insert(c))));
            }
        };
        let names = |studio: &Studio| -> Vec<String> {
            studio.completion.as_ref().map(|c| c.items.iter().map(|i| i.name.clone()).collect()).unwrap_or_default()
        };
        type_in(&mut studio, "(b");
        assert!(studio.completion.is_none(), "one letter is too little");
        type_in(&mut studio, "a");
        assert!(names(&studio).contains(&"bar-chart".to_string()), "{:?}", names(&studio));
        let _ = view::view(&studio);
        let target = names(&studio).iter().position(|n| n == "bar-chart").unwrap();
        for _ in 0..target {
            let _ = studio.update(Message::CompleteMove(1));
        }
        let _ = studio.update(Message::CompleteAccept);
        assert_eq!(studio.doc().content.text(), "(bar-chart");
        assert!(studio.completion.is_none() && studio.doc().dirty);
        type_in(&mut studio, " t :c");
        assert_eq!(names(&studio), [":city"]);
        let _ = studio.update(Message::CompletePick(0));
        assert_eq!(studio.doc().content.text(), "(bar-chart t :city");
        type_in(&mut studio, " xy");
        assert!(studio.completion.is_none());
        let _ = studio.update(Message::Complete(Source::Editor));
        assert!(studio.completion.is_none(), "nothing starts with xy");
        let _ = studio.update(Message::Edit(Action::Edit(Edit::Backspace)));
        let _ = studio.update(Message::Edit(Action::Edit(Edit::Backspace)));
        let _ = studio.update(Message::Complete(Source::Editor));
        assert_eq!(names(&studio), ["t", ":city", "bar-chart"]);
        let _ = studio.update(Message::CompleteMove(-1));
        assert_eq!(studio.completion.as_ref().map(|c| c.selected), Some(2));
        let _ = studio.update(Message::Edit(Action::Move(Motion::Left)));
        assert!(studio.completion.is_none());
        studio.input = text_editor::Content::with_text("(in");
        let _ = studio.update(Message::Input(Action::Move(Motion::DocumentEnd)));
        let _ = studio.update(Message::Complete(Source::Console));
        assert!(names(&studio).contains(&"inc".to_string()), "{:?}", names(&studio));
        let _ = view::view(&studio);
        let _ = studio.update(Message::CompleteAccept);
        assert!(studio.input.text().starts_with("(in") && studio.input.text().len() > 3);
        let _ = studio.update(Message::Input(Action::Edit(Edit::Insert('c'))));
        let _ = studio.update(Message::CompleteClose);
        assert!(studio.completion.is_none());
    }

    #[test]
    fn the_project_tab_shows_the_project_and_its_packages() {
        let dir = scratch("project-tab");
        std::fs::create_dir_all(dir.join("lib").join("src")).unwrap();
        std::fs::write(dir.join("lib").join("src").join("lib.scm"), "(ns lib)").unwrap();
        let app = dir.join("app");
        std::fs::create_dir_all(&app).unwrap();
        let mut studio = Studio::new();
        let _ = studio.update(Message::OpenedFolder(Some(app.clone())));
        let _ = studio.update(Message::ShowSide(Side::Project));
        assert!(studio.project.is_none());
        let _ = view::view(&studio);
        let _ = studio.update(Message::CreateProject);
        assert_eq!(studio.project.as_ref().map(|p| p.name.as_str()), Some("app"));
        assert!(app.join("project.scm").is_file() && studio.status.starts_with("Created app"));
        assert!(studio.entries.iter().any(|e| e.name == "project.scm"));
        let _ = studio.update(Message::StartNaming(NamingKind::Package));
        assert_eq!(studio.naming.as_ref().map(|n| n.parent.clone()), Some(app.clone()));
        let _ = studio.update(Message::Naming("lib ../lib".into()));
        let _ = view::view(&studio);
        let _ = studio.update(Message::ConfirmNaming);
        assert_eq!(studio.busy.as_deref(), Some("Adding lib ../lib"));
        let _ = view::view(&studio);
        let added = project::add(&app, "lib ../lib").unwrap();
        let _ = studio.update(Message::ProjectDone(Ok(added)));
        assert!(studio.busy.is_none() && studio.status == "Added lib");
        assert_eq!(studio.project.as_ref().map(|p| p.dependencies.len()), Some(1));
        let _ = studio.update(Message::PickPackage("lib".into()));
        let _ = view::view(&studio);
        let _ = studio.update(Message::ProjectDone(Err("boom".into())));
        assert_eq!(studio.status, "Error: boom");
        assert_eq!(studio.project.as_ref().map(|p| p.dependencies.len()), Some(1));
        let _ = studio.update(Message::RemovePackage("lib".into()));
        assert_eq!((studio.project.as_ref().map(|p| p.dependencies.len()), studio.picked_package.clone()), (Some(0), None));
        let _ = studio.update(Message::StartNaming(NamingKind::Package));
        let _ = studio.update(Message::ConfirmNaming);
        assert!(studio.busy.is_none() && studio.status.starts_with("Name a package"));
        let _ = view::view(&studio);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn help_opens_on_the_name_at_the_cursor() {
        let mut studio = Studio::new();
        studio.doc_mut().content = text_editor::Content::with_text("(bar-chart t :a :b)");
        for _ in 0..3 {
            let _ = studio.update(Message::Edit(text_editor::Action::Move(text_editor::Motion::Right)));
        }
        let _ = studio.update(Message::LookUp(Source::Editor));
        assert_eq!(studio.tab, Tab::Help);
        assert_eq!(studio.help.as_ref().map(|h| (h.page, h.query.as_str())), Some((Page::Reference, "bar-chart")));
        let _ = view::view(&studio);
        let _ = studio.update(Message::HelpQuery("hist".into()));
        let _ = studio.update(Message::HelpPage(Page::Guide));
        let _ = view::view(&studio);
        let _ = studio.update(Message::HelpQuery(String::new()));
        let _ = view::view(&studio);
        let _ = studio.update(Message::SelectTab(Tab::Doc(0)));
        assert_eq!(studio.tab, Tab::Doc(0));
        assert!(studio.help.is_some());
        let _ = studio.update(Message::CloseTab(Tab::Help));
        assert!(studio.help.is_none());
        let _ = studio.update(Message::SelectTab(Tab::Help));
        assert_eq!(studio.tab, Tab::Doc(0));
        studio.input = text_editor::Content::with_text("(inc");
        let _ = studio.update(Message::Input(text_editor::Action::Move(text_editor::Motion::DocumentEnd)));
        let _ = studio.update(Message::LookUp(Source::Console));
        assert_eq!(studio.help.as_ref().map(|h| h.query.clone()), Some("inc".to_string()));
        let _ = studio.update(Message::OpenHelp);
        assert_eq!(studio.help.as_ref().map(|h| h.query.clone()), Some("inc".to_string()));
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
        let _ = studio.update(Message::Worker(Event::Plot(Box::new(chart("one")))));
        let _ = studio.update(Message::Worker(Event::Plot(Box::new(chart("two")))));
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
        let _ = studio.update(Message::Worker(Event::Plot(Box::new(chart("T")))));
        assert_eq!(studio.plot().map(|p| p.title.clone()), Some(Some("T".to_string())));
        let _ = view::view(&studio);
        let dir = scratch("plot");
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
        studio.doc_mut().content = text_editor::Content::with_text(&"x\n".repeat(50));
        let _ = studio.update(Message::EditorResized(Size::new(400.0, 300.0)));
        let _ = studio.update(Message::Edit(text_editor::Action::Move(text_editor::Motion::DocumentEnd)));
        assert!(studio.doc().offset > 700.0, "{}", studio.doc().offset);
    }

    #[test]
    fn bindings_group_into_data_values_and_functions() {
        let mut studio = Studio::new();
        let b = |name: &str, kind: &str| Binding { name: name.into(), kind: kind.into(), summary: String::new(), columns: Vec::new() };
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
        studio.doc_mut().content = text_editor::Content::with_text("(a [b])");
        let step = |studio: &mut Studio, motion| {
            let _ = studio.update(Message::Edit(text_editor::Action::Move(motion)));
        };
        step(&mut studio, text_editor::Motion::Right);
        assert_eq!(studio.doc().marks.pair, None);
        step(&mut studio, text_editor::Motion::Right);
        step(&mut studio, text_editor::Motion::Right);
        assert_eq!(studio.doc().marks.pair, Some([(0, 3), (0, 5)]));
        step(&mut studio, text_editor::Motion::End);
        assert_eq!(studio.doc().marks.pair, Some([(0, 0), (0, 6)]));
        assert!(!studio.doc().dirty);
    }

    #[test]
    fn settings_shape_the_studio_and_follow_its_changes() {
        let layout = settings::Layout::Split {
            horizontal: true,
            ratio: 0.4,
            a: Box::new(settings::Layout::Pane(PaneKind::Console)),
            b: Box::new(settings::Layout::Pane(PaneKind::Editor)),
        };
        let saved = Settings { theme: "warm".into(), zoom: 1.2, scale: 1.1, wrap: true, layout: Some(layout.clone()), watches: vec!["x".into()], ..Settings::default() };
        let mut studio = Studio::with_settings(saved);
        assert_eq!(theme::name_of(&studio.theme), "warm");
        assert!(studio.wrap && !studio.persistent);
        assert_eq!(studio.panes.iter().count(), 2);
        let snapshot = studio.snapshot_settings();
        assert_eq!((snapshot.theme.as_str(), snapshot.zoom, snapshot.scale, snapshot.layout), ("warm", 1.2, 1.1, Some(layout)));
        assert_eq!(snapshot.watches, ["x"]);
        assert_eq!(studio.line_height(), LINE_HEIGHT * 1.2);
        let _ = studio.update(Message::ZoomIn);
        let _ = studio.update(Message::ZoomIn);
        assert_eq!(studio.zoom, 1.4);
        for _ in 0..10 {
            let _ = studio.update(Message::ZoomIn);
        }
        assert_eq!(studio.zoom, 1.8);
        let _ = studio.update(Message::ZoomReset);
        assert_eq!(studio.zoom, 1.0);
        let _ = studio.update(Message::ScaleBy(-9));
        assert_eq!(studio.scale, 0.8);
        let _ = studio.update(Message::SetTheme("contrast".into()));
        assert_eq!(theme::name_of(&studio.theme), "contrast");
        let _ = studio.update(Message::OpenSettings);
        assert_eq!(studio.tab, Tab::Settings);
        let _ = studio.update(Message::CloseTab(Tab::Settings));
        assert!(!studio.settings_open && studio.tab == Tab::Doc(0));
        let main = window::Id::unique();
        studio.main_window = Some(main);
        let _ = studio.update(Message::WindowResized(main, 1000.0, 700.0));
        assert_eq!(studio.snapshot_settings().window, Some((1000.0, 700.0)));
        assert!(commands::commands(&studio).iter().any(|c| c.name == "Preferences…"));
    }

    #[test]
    fn the_start_page_lists_recent_work_and_opens_it() {
        let dir = scratch("start");
        let file = dir.join("recent.scm");
        std::fs::write(&file, "(define r 1)\n").unwrap();
        let mut studio = Studio::new();
        studio.recent = vec![file.clone(), dir.join("gone.scm")];
        studio.projects = vec![dir.clone()];
        let _ = studio.update(Message::OpenStart);
        assert!(studio.start_open && studio.tab == Tab::Start);
        let _ = view::view(&studio);
        let _ = studio.update(Message::OpenPath(file.clone()));
        assert_eq!((studio.tab, studio.doc().path.as_deref()), (Tab::Doc(0), Some(file.as_path())));
        assert_eq!(studio.recent[0], file);
        let _ = studio.update(Message::OpenProject(dir.clone()));
        assert_eq!((studio.root.as_deref(), studio.side), (Some(dir.as_path()), Side::Project));
        let _ = studio.update(Message::SelectTab(Tab::Start));
        assert_eq!(studio.tab, Tab::Start);
        let _ = studio.update(Message::CloseTab(Tab::Start));
        assert!(!studio.start_open && studio.tab == Tab::Doc(0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn files_changed_on_disk_are_reloaded_or_flagged() {
        let dir = scratch("reload");
        let file = dir.join("a.scm");
        std::fs::write(&file, "(define a 1)\n").unwrap();
        let mut studio = Studio::new();
        let _ = studio.update(Message::Opened(Some(file.clone())));
        assert!(studio.doc().modified.is_some());
        let _ = studio.update(Message::Tick);
        assert_eq!(studio.doc().content.text(), "(define a 1)\n");
        std::thread::sleep(std::time::Duration::from_millis(30));
        std::fs::write(&file, "(define a 2)\n").unwrap();
        let _ = studio.update(Message::Tick);
        assert_eq!(studio.doc().content.text(), "(define a 2)\n");
        assert!(studio.status.starts_with("Reloaded") && !studio.doc().stale);
        studio.doc_mut().dirty = true;
        std::thread::sleep(std::time::Duration::from_millis(30));
        std::fs::write(&file, "(define a 3)\n").unwrap();
        let _ = studio.update(Message::Tick);
        assert_eq!(studio.doc().content.text(), "(define a 2)\n");
        assert!(studio.doc().stale && studio.status.ends_with("changed on disk"));
        let _ = view::view(&studio);
        let _ = studio.update(Message::Reload);
        assert_eq!(studio.doc().content.text(), "(define a 3)\n");
        assert!(!studio.doc().stale && !studio.doc().dirty);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn plot_views_zoom_and_reset_when_the_chart_changes() {
        let mut studio = Studio::new();
        let _ = studio.update(Message::Worker(Event::Plot(Box::new(chart("one")))));
        let _ = studio.update(Message::Worker(Event::Plot(Box::new(chart("two")))));
        let zoomed = PlotView { zoom: 2.0, pan: (-10.0, -5.0) };
        let _ = studio.update(Message::PlotView(zoomed));
        assert_eq!(studio.plot_view, zoomed);
        let _ = view::view(&studio);
        let _ = studio.update(Message::PlotReset);
        assert_eq!(studio.plot_view, PlotView::default());
        let _ = studio.update(Message::PlotView(zoomed));
        let _ = studio.update(Message::PlotSelect(0));
        assert_eq!((studio.plot_at, studio.plot_view), (0, PlotView::default()));
        let _ = studio.update(Message::PlotView(zoomed));
        let _ = studio.update(Message::PlotForward);
        assert_eq!((studio.plot_at, studio.plot_view), (1, PlotView::default()));
        let _ = studio.update(Message::CopyPlot);
        assert_eq!(studio.status, "Copied the chart as SVG");
    }

    #[test]
    fn notebooks_split_into_cells_and_route_runs_to_them() {
        let dir = scratch("notebook");
        let path = dir.join("book.scm");
        std::fs::write(&path, ";; %% text\n;; # Hello\n\n;; %%\n(+ 1 2)\n\n;; %%\n(println \"hi\")\n").unwrap();
        std::fs::write(notebook::sidecar(&path), "[[] [{:kind :result :text \"3\"}] []]\n").unwrap();
        let mut studio = Studio::new();
        let _ = studio.update(Message::Opened(Some(path.clone())));
        let nb = studio.doc().notebook.as_ref().unwrap();
        assert_eq!(nb.cells.len(), 3);
        assert_eq!((nb.cells[0].kind, nb.cells[1].kind), (Kind::Text, Kind::Code));
        assert_eq!(nb.cells[1].outputs, vec![Output::Result("3".into())]);
        let (doc_id, cell_id) = (studio.doc().id, nb.cells[2].id);
        let _ = view::view(&studio);
        studio.tags.insert(9, (doc_id, cell_id));
        studio.runs = 1;
        let _ = studio.update(Message::Worker(Event::Started(9)));
        let _ = studio.update(Message::Worker(Event::Output("hi".into())));
        let _ = studio.update(Message::Worker(Event::Output("\n".into())));
        let _ = studio.update(Message::Worker(Event::Result("done".into())));
        let _ = studio.update(Message::Worker(Event::Finished(5)));
        let cell = &studio.doc().notebook.as_ref().unwrap().cells[2];
        assert_eq!(cell.outputs, vec![Output::Printed("hi\n".into()), Output::Result("done".into())]);
        assert_eq!((cell.running, cell.elapsed), (false, Some(5)));
        assert!(studio.console.is_empty() && !studio.running && studio.tags.is_empty());
        let _ = studio.update(Message::Worker(Event::Output("plain".into())));
        assert_eq!(studio.console.last().map(|l| l.text.clone()), Some("plain".into()));
        let _ = studio.update(Message::CellEdit(1, text_editor::Action::Move(text_editor::Motion::DocumentEnd)));
        let _ = studio.update(Message::CellEdit(1, text_editor::Action::Edit(text_editor::Edit::Insert('!'))));
        assert!(studio.doc().dirty && studio.doc().content.text().contains("(+ 1 2)!"));
        let _ = studio.update(Message::AddCell(1, Kind::Text));
        let _ = studio.update(Message::MoveCell(1, 1));
        let _ = studio.update(Message::DeleteCell(2));
        let kinds: Vec<Kind> = studio.doc().notebook.as_ref().unwrap().cells.iter().map(|c| c.kind).collect();
        assert_eq!(kinds, [Kind::Text, Kind::Code, Kind::Code]);
        let _ = studio.update(Message::Save);
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.starts_with(";; %% text\n;; # Hello\n\n;; %%\n(+ 1 2)!\n"), "{saved}");
        let out = std::fs::read_to_string(notebook::sidecar(&path)).unwrap();
        assert!(out.contains(":text \"hi\\n\"") && out.contains(":text \"done\""), "{out}");
        let html = dir.join("book.html");
        let _ = studio.update(Message::ExportNotebookTo(Some(html.clone())));
        assert!(std::fs::read_to_string(&html).unwrap().contains("<h1>Hello</h1>"));
        let md = dir.join("book.md");
        let _ = studio.update(Message::ExportNotebookTo(Some(md.clone())));
        assert!(std::fs::read_to_string(&md).unwrap().starts_with("# Hello\n\n```scheme\n(+ 1 2)!\n```"));
        let _ = studio.update(Message::ClearOutputs);
        assert!(studio.doc().notebook.as_ref().unwrap().cells.iter().all(|c| c.outputs.is_empty()) && studio.doc().dirty);
        let _ = studio.update(Message::Save);
        assert!(!notebook::sidecar(&path).exists(), "an empty sidecar is removed");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn new_notebooks_start_with_a_title_and_an_empty_cell() {
        let mut studio = Studio::new();
        let _ = studio.update(Message::NewNotebook);
        assert_eq!(studio.doc().notebook.as_ref().map(|nb| nb.cells.len()), Some(2));
        assert_eq!(studio.doc().content.text().trim_end(), ";; %% text\n;; # Untitled\n\n;; %%");
        let _ = studio.update(Message::RunCellAndAdvance(1));
        assert!(studio.status.contains("still starting"));
        let _ = studio.update(Message::ToggleCellKind(0));
        assert_eq!(studio.doc().notebook.as_ref().unwrap().cells[0].kind, Kind::Code);
        let _ = view::view(&studio);
        assert!(commands::commands(&studio).iter().any(|c| c.name == "Run all cells"));
        assert_eq!(csv_name(Path::new("C:/x/Sales Data 2024.csv")), "sales-data-2024");
        assert_eq!(csv_name(Path::new("2024.csv")), "data-2024");
        assert_eq!(csv_name(Path::new("---.csv")), "data");
    }

    #[test]
    fn the_git_tab_shows_status_and_routes_actions() {
        let mut studio = Studio::new();
        let _ = studio.update(Message::ShowSide(Side::Git));
        let _ = view::view(&studio);
        studio.root = Some(PathBuf::from("C:/repo"));
        let status = git::parse_status(
            "# branch.oid abc\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +1 -0\n1 .M N... 100644 100644 100644 a b src/a.scm\n1 A. N... 000000 100644 100644 0 c new.scm\n? notes.md\n",
        );
        let snapshot = GitSnapshot {
            root: PathBuf::from("C:/repo"),
            status: Ok(status),
            log: vec![Commit { hash: "abc1234".into(), subject: "First".into(), when: "now".into() }],
        };
        let _ = studio.update(Message::GitLoaded(Some(snapshot)));
        assert_eq!(studio.git.status.as_ref().map(git::Status::changes), Some(3));
        let _ = view::view(&studio);
        assert_eq!(studio.badge(Path::new("C:/repo/src/a.scm")), Some((Change::Modified, false)));
        assert_eq!(studio.badge(Path::new("C:/repo/new.scm")), Some((Change::Added, true)));
        assert_eq!(studio.badge(Path::new("C:/repo/other.scm")), None);
        let _ = studio.update(Message::GitCommit);
        assert!(!studio.git.busy, "an empty message commits nothing");
        let _ = studio.update(Message::GitMessage("Add things".into()));
        let _ = studio.update(Message::GitCommit);
        assert!(studio.git.busy);
        let _ = studio.update(Message::GitStageAll);
        let _ = studio.update(Message::GitDone(Ok("Committed abc First".into())));
        assert!(!studio.git.busy && studio.git.message.is_empty() && studio.status == "Committed abc First");
        let _ = studio.update(Message::GitDone(Err("boom".into())));
        assert_eq!(studio.status, "Error: boom");
        let _ = studio.update(Message::GitDiffLoaded("src/a.scm".into(), Ok("+++ a\n-old\n+new\n@@ -1 +1 @@\n".into())));
        assert_eq!(studio.tab, Tab::Diff);
        let _ = view::view(&studio);
        let _ = studio.update(Message::CloseTab(Tab::Diff));
        assert!(studio.diff.is_none() && studio.tab == Tab::Doc(0));
        let all = commands::commands(&studio);
        let listed: Vec<(&str, bool)> = all.iter().filter(|c| c.menu == commands::Menu::Git).map(|c| (c.name.as_str(), c.message.is_some())).collect();
        assert!(listed.contains(&("Stage all", true)) && listed.contains(&("Unstage all", true)) && listed.contains(&("Commit", false)) && listed.contains(&("Pull", true)));
        let _ = studio.update(Message::GitLoaded(None));
        assert!(studio.git.root.is_none() && studio.git.status.is_none());
        let _ = view::view(&studio);
    }

    #[test]
    fn panes_detach_into_windows_and_come_back() {
        let mut studio = Studio::new();
        let main = window::Id::unique();
        studio.main_window = Some(main);
        let _ = studio.update(Message::DetachPane(PaneKind::Plots));
        assert!(studio.pane_of(PaneKind::Plots).is_none());
        assert_eq!(studio.detached.len(), 1);
        let id = studio.detached[0].0;
        assert_eq!(studio.title(id), "Plots - Schematic Studio");
        assert_eq!(studio.title(main), "Untitled - Schematic Studio");
        let _ = view::window(&studio, id);
        let _ = view::window(&studio, main);
        assert_eq!(studio.snapshot_settings().detached, [PaneKind::Plots]);
        let _ = studio.update(Message::DetachPane(PaneKind::Plots));
        assert_eq!(studio.detached.len(), 1, "a detached pane is focused, not opened twice");
        let _ = studio.update(Message::WindowResized(id, 300.0, 200.0));
        assert_eq!(studio.window_size, None, "only the main window's size is kept");
        let _ = studio.update(Message::CloseRequested(id));
        assert!(studio.detached.is_empty() && studio.pane_of(PaneKind::Plots).is_some());
        for kind in [PaneKind::Console, PaneKind::Environment, PaneKind::Viewer, PaneKind::Plots] {
            let _ = studio.update(Message::DetachPane(kind));
        }
        assert_eq!((studio.detached.len(), studio.panes.iter().count()), (4, 1));
        let _ = studio.update(Message::DetachPane(PaneKind::Editor));
        assert_eq!(studio.detached.len(), 4, "the last pane stays");
        let _ = view::view(&studio);
        let _ = studio.update(Message::AttachAll);
        assert!(studio.detached.is_empty() && studio.panes.iter().count() == 5);
        assert!(commands::commands(&studio).iter().any(|c| c.name == "Attach all windows" && c.message.is_none()));
    }
}
