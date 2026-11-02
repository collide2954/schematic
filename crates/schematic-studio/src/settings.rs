use std::path::{Path, PathBuf};

use iced::widget::pane_grid::{self, Axis, Configuration, Node};
use schematic::Value;
use schematic::reader::read_str;

use crate::app::PaneKind;
use crate::claude::{EFFORTS, MODELS};

pub const THEMES: [&str; 4] = ["light", "dark", "warm", "contrast"];
pub const RECENT: usize = 12;

#[derive(Clone, Debug, PartialEq)]
pub enum Layout {
    Split { horizontal: bool, ratio: f32, a: Box<Layout>, b: Box<Layout> },
    Pane(PaneKind),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub theme: String,
    pub scale: f32,
    pub zoom: f32,
    pub wrap: bool,
    pub restore: bool,
    pub backups: bool,
    pub window: Option<(f32, f32)>,
    pub layout: Option<Layout>,
    pub detached: Vec<PaneKind>,
    pub open: Vec<PathBuf>,
    pub current: usize,
    pub recent: Vec<PathBuf>,
    pub projects: Vec<PathBuf>,
    pub root: Option<PathBuf>,
    pub watches: Vec<String>,
    pub model: String,
    pub effort: String,
    pub ask_before_running: bool,
    pub attach_data: bool,
    pub completions: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            theme: "light".to_string(),
            scale: 1.0,
            zoom: 1.0,
            wrap: false,
            restore: true,
            backups: true,
            window: None,
            layout: None,
            detached: Vec::new(),
            open: Vec::new(),
            current: 0,
            recent: Vec::new(),
            projects: Vec::new(),
            root: None,
            watches: Vec::new(),
            model: MODELS[0].id.to_string(),
            effort: "medium".to_string(),
            ask_before_running: true,
            attach_data: true,
            completions: true,
        }
    }
}

pub fn credentials_path() -> Option<PathBuf> {
    directory().map(|dir| dir.join("credentials.scm"))
}

pub fn load_key() -> Option<String> {
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY")
        && !key.trim().is_empty()
    {
        return Some(key.trim().to_string());
    }
    let text = std::fs::read_to_string(credentials_path()?).ok()?;
    let Ok(Value::Map(map)) = read_str(&text) else { return None };
    match map.get(&keyword("anthropic-key")) {
        Some(Value::Str(key)) if !key.trim().is_empty() => Some(key.trim().to_string()),
        _ => None,
    }
}

pub fn save_key(key: &str) -> std::io::Result<()> {
    let Some(path) = credentials_path() else { return Ok(()) };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if key.trim().is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    std::fs::write(&path, format!("{{:anthropic-key {}}}\n", Value::string(key.trim())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn directory() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    };
    base.map(|dir| dir.join("schematic"))
}

pub fn path() -> Option<PathBuf> {
    directory().map(|dir| dir.join("studio.scm"))
}

pub fn load() -> Settings {
    path().and_then(|p| std::fs::read_to_string(p).ok()).map(|text| parse(&text)).unwrap_or_default()
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    let Some(path) = path() else { return Ok(()) };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, render(settings))
}

fn keyword(name: &str) -> Value {
    Value::keyword(name)
}

fn pane_name(kind: PaneKind) -> &'static str {
    match kind {
        PaneKind::Editor => "editor",
        PaneKind::Console => "console",
        PaneKind::Environment => "environment",
        PaneKind::Viewer => "data",
        PaneKind::Plots => "plots",
        PaneKind::Assistant => "assistant",
    }
}

fn pane_kind(name: &str) -> Option<PaneKind> {
    Some(match name {
        "editor" => PaneKind::Editor,
        "console" => PaneKind::Console,
        "environment" => PaneKind::Environment,
        "data" => PaneKind::Viewer,
        "plots" => PaneKind::Plots,
        "assistant" => PaneKind::Assistant,
        _ => return None,
    })
}

fn layout_value(layout: &Layout) -> Value {
    match layout {
        Layout::Pane(kind) => Value::map([(keyword("pane"), keyword(pane_name(*kind)))]),
        Layout::Split { horizontal, ratio, a, b } => Value::map([
            (keyword("split"), keyword(if *horizontal { "horizontal" } else { "vertical" })),
            (keyword("ratio"), Value::Float(f64::from(*ratio))),
            (keyword("a"), layout_value(a)),
            (keyword("b"), layout_value(b)),
        ]),
    }
}

fn layout_from(value: &Value) -> Option<Layout> {
    let Value::Map(map) = value else { return None };
    if let Some(Value::Keyword(name)) = map.get(&keyword("pane")) {
        return pane_kind(&name.name()).map(Layout::Pane);
    }
    let Some(Value::Keyword(axis)) = map.get(&keyword("split")) else { return None };
    let ratio = match map.get(&keyword("ratio")) {
        Some(Value::Float(x)) => *x as f32,
        Some(Value::Int(i)) => *i as f32,
        _ => return None,
    };
    let a = layout_from(map.get(&keyword("a"))?)?;
    let b = layout_from(map.get(&keyword("b"))?)?;
    Some(Layout::Split { horizontal: &*axis.name() == "horizontal", ratio: ratio.clamp(0.05, 0.95), a: Box::new(a), b: Box::new(b) })
}

pub fn layout_of(state: &pane_grid::State<PaneKind>) -> Option<Layout> {
    fn walk(node: &Node, state: &pane_grid::State<PaneKind>) -> Option<Layout> {
        match node {
            Node::Split { axis, ratio, a, b, .. } => Some(Layout::Split {
                horizontal: *axis == Axis::Horizontal,
                ratio: *ratio,
                a: Box::new(walk(a, state)?),
                b: Box::new(walk(b, state)?),
            }),
            Node::Pane(pane) => state.get(*pane).map(|kind| Layout::Pane(*kind)),
        }
    }
    walk(state.layout(), state)
}

pub fn configuration(layout: &Layout) -> Configuration<PaneKind> {
    match layout {
        Layout::Pane(kind) => Configuration::Pane(*kind),
        Layout::Split { horizontal, ratio, a, b } => Configuration::Split {
            axis: if *horizontal { Axis::Horizontal } else { Axis::Vertical },
            ratio: *ratio,
            a: Box::new(configuration(a)),
            b: Box::new(configuration(b)),
        },
    }
}

fn path_value(path: &Path) -> Value {
    Value::string(&path.to_string_lossy())
}

fn paths(values: impl IntoIterator<Item = PathBuf>) -> Value {
    Value::vector(values.into_iter().map(|p| path_value(&p)))
}

pub fn render(settings: &Settings) -> String {
    let mut pairs = vec![
        (keyword("theme"), keyword(&settings.theme)),
        (keyword("scale"), Value::Float(f64::from(settings.scale))),
        (keyword("zoom"), Value::Float(f64::from(settings.zoom))),
        (keyword("wrap"), Value::Bool(settings.wrap)),
        (keyword("restore"), Value::Bool(settings.restore)),
        (keyword("backups"), Value::Bool(settings.backups)),
        (keyword("open"), paths(settings.open.clone())),
        (keyword("current"), Value::Int(settings.current as i64)),
        (keyword("recent"), paths(settings.recent.clone())),
        (keyword("projects"), paths(settings.projects.clone())),
        (keyword("watches"), Value::vector(settings.watches.iter().map(|w| Value::string(w)))),
        (keyword("model"), Value::string(&settings.model)),
        (keyword("effort"), keyword(&settings.effort)),
        (keyword("ask-before-running"), Value::Bool(settings.ask_before_running)),
        (keyword("attach-data"), Value::Bool(settings.attach_data)),
        (keyword("completions"), Value::Bool(settings.completions)),
    ];
    if let Some((width, height)) = settings.window {
        pairs.push((keyword("window"), Value::vector([Value::Float(f64::from(width)), Value::Float(f64::from(height))])));
    }
    if let Some(layout) = &settings.layout {
        pairs.push((keyword("layout"), layout_value(layout)));
    }
    if !settings.detached.is_empty() {
        pairs.push((keyword("detached"), Value::vector(settings.detached.iter().map(|kind| keyword(pane_name(*kind))))));
    }
    if let Some(root) = &settings.root {
        pairs.push((keyword("root"), path_value(root)));
    }
    let mut out = String::from("{");
    for (i, (key, value)) in pairs.iter().enumerate() {
        out.push_str(if i == 0 { "" } else { "\n " });
        out.push_str(&format!("{key} {value}"));
    }
    out.push_str("}\n");
    out
}

fn number(value: Option<&Value>) -> Option<f32> {
    match value {
        Some(Value::Float(x)) => Some(*x as f32),
        Some(Value::Int(i)) => Some(*i as f32),
        _ => None,
    }
}

fn path_list(value: Option<&Value>) -> Vec<PathBuf> {
    let Some(Value::Vector(items)) = value else { return Vec::new() };
    items.iter().filter_map(|item| if let Value::Str(s) = item { Some(PathBuf::from(s.as_str())) } else { None }).collect()
}

pub fn parse(text: &str) -> Settings {
    let defaults = Settings::default();
    let Ok(Value::Map(map)) = read_str(text) else { return defaults };
    let get = |name: &str| map.get(&keyword(name));
    let flag = |name: &str, fallback: bool| match get(name) {
        Some(Value::Bool(b)) => *b,
        _ => fallback,
    };
    let theme = match get("theme") {
        Some(Value::Keyword(k)) if THEMES.contains(&&*k.name()) => k.name().to_string(),
        _ => defaults.theme.clone(),
    };
    let window = match get("window") {
        Some(Value::Vector(v)) if v.len() == 2 => {
            let mut sizes = v.iter().filter_map(|x| number(Some(x)));
            sizes.next().zip(sizes.next()).filter(|(w, h)| *w >= 400.0 && *h >= 300.0)
        }
        _ => None,
    };
    let watches = match get("watches") {
        Some(Value::Vector(v)) => v.iter().filter_map(|w| if let Value::Str(s) = w { Some(s.to_string()) } else { None }).collect(),
        _ => Vec::new(),
    };
    Settings {
        theme,
        scale: number(get("scale")).unwrap_or(1.0).clamp(0.8, 1.6),
        zoom: number(get("zoom")).unwrap_or(1.0).clamp(0.7, 1.8),
        wrap: flag("wrap", false),
        restore: flag("restore", true),
        backups: flag("backups", true),
        window,
        layout: get("layout").and_then(layout_from),
        detached: match get("detached") {
            Some(Value::Vector(kinds)) => kinds.iter().filter_map(|k| if let Value::Keyword(k) = k { pane_kind(&k.name()) } else { None }).collect(),
            _ => Vec::new(),
        },
        open: path_list(get("open")),
        current: number(get("current")).unwrap_or(0.0).max(0.0) as usize,
        recent: path_list(get("recent")),
        projects: path_list(get("projects")),
        root: match get("root") {
            Some(Value::Str(s)) => Some(PathBuf::from(s.as_str())),
            _ => None,
        },
        watches,
        model: match get("model") {
            Some(Value::Str(s)) if MODELS.iter().any(|m| m.id == s.as_str()) => s.to_string(),
            _ => defaults.model.clone(),
        },
        effort: match get("effort") {
            Some(Value::Keyword(k)) if EFFORTS.contains(&&*k.name()) => k.name().to_string(),
            _ => defaults.effort.clone(),
        },
        ask_before_running: flag("ask-before-running", true),
        attach_data: flag("attach-data", true),
        completions: flag("completions", true),
    }
}

pub fn remember(list: &mut Vec<PathBuf>, path: &Path) {
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(RECENT);
}

pub const BACKUPS: usize = 10;

pub fn backups() -> Option<PathBuf> {
    directory().map(|dir| dir.join("backups"))
}

pub fn back_up(path: &Path) {
    let Some(dir) = backups() else { return };
    back_up_into(path, &dir);
}

fn back_up_into(path: &Path, dir: &Path) {
    let Ok(previous) = std::fs::read(path) else { return };
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) => (stem, format!(".{extension}")),
        None => (name, String::new()),
    };
    let _ = std::fs::write(dir.join(format!("{stem}.{stamp}{extension}")), previous);
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let prefix = format!("{stem}.");
    let mut copies: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&prefix) && n.ends_with(&extension)))
        .collect();
    copies.sort();
    for old in copies.iter().rev().skip(BACKUPS) {
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_their_file_form() {
        let layout = Layout::Split {
            horizontal: false,
            ratio: 0.6,
            a: Box::new(Layout::Pane(PaneKind::Editor)),
            b: Box::new(Layout::Split { horizontal: true, ratio: 0.4, a: Box::new(Layout::Pane(PaneKind::Viewer)), b: Box::new(Layout::Pane(PaneKind::Plots)) }),
        };
        let settings = Settings {
            theme: "warm".into(),
            scale: 1.25,
            zoom: 0.9,
            wrap: true,
            restore: false,
            backups: true,
            window: Some((1400.0, 900.0)),
            layout: Some(layout),
            detached: vec![PaneKind::Console, PaneKind::Plots],
            open: vec![PathBuf::from("C:\\work\\a.scm"), PathBuf::from("b.scm")],
            current: 1,
            recent: vec![PathBuf::from("b.scm")],
            projects: vec![PathBuf::from("C:\\work")],
            root: Some(PathBuf::from("C:\\work")),
            watches: vec!["(count t)".into()],
            model: "claude-sonnet-5-5".into(),
            effort: "high".into(),
            ask_before_running: false,
            attach_data: false,
            completions: false,
        };
        let text = render(&settings);
        assert!(text.starts_with("{:theme :warm\n :scale 1.25"), "{text}");
        assert!(text.contains(":detached [:console :plots]") && text.contains(":effort :high"), "{text}");
        assert_eq!(parse(&text), settings);
        assert_eq!(parse("garbage"), Settings::default());
        assert_eq!(parse("{:theme :neon :scale 9 :zoom 0.1 :window [10 10] :model \"gpt\" :effort :loud}"), Settings { scale: 1.6, zoom: 0.7, ..Settings::default() });
    }

    #[test]
    fn keys_are_kept_in_their_own_file() {
        let text = format!("{{:anthropic-key {}}}\n", Value::string("sk-test"));
        let Ok(Value::Map(map)) = read_str(&text) else { panic!("{text}") };
        assert_eq!(map.get(&keyword("anthropic-key")), Some(&Value::string("sk-test")));
        assert!(credentials_path().is_none_or(|p| p.ends_with("credentials.scm")));
    }

    #[test]
    fn layouts_convert_to_and_from_pane_grids() {
        let layout = Layout::Split { horizontal: true, ratio: 0.3, a: Box::new(Layout::Pane(PaneKind::Console)), b: Box::new(Layout::Pane(PaneKind::Editor)) };
        let state = pane_grid::State::with_configuration(configuration(&layout));
        assert_eq!(layout_of(&state), Some(layout));
    }

    #[test]
    fn backups_keep_the_previous_versions_and_stay_bounded() {
        let dir = std::env::temp_dir().join(format!("schematic-backups-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("notes.scm");
        let store = dir.join("store");
        back_up_into(&file, &store);
        assert!(!store.exists(), "nothing to back up before the file exists");
        for i in 0..BACKUPS + 3 {
            std::fs::write(&file, format!("(define v {i})")).unwrap();
            back_up_into(&file, &store);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let mut copies: Vec<PathBuf> = std::fs::read_dir(&store).unwrap().map(|e| e.unwrap().path()).collect();
        copies.sort();
        assert_eq!(copies.len(), BACKUPS);
        let names: Vec<String> = copies.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert!(names.iter().all(|n| n.starts_with("notes.") && n.ends_with(".scm")), "{names:?}");
        assert_eq!(std::fs::read_to_string(copies.last().unwrap()).unwrap(), format!("(define v {})", BACKUPS + 2));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn recent_lists_move_entries_to_the_front_and_stay_short() {
        let mut list = Vec::new();
        for i in 0..20 {
            remember(&mut list, Path::new(&format!("{i}.scm")));
        }
        remember(&mut list, Path::new("5.scm"));
        assert_eq!(list.len(), RECENT);
        assert_eq!(list[0], PathBuf::from("5.scm"));
        assert_eq!(list[1], PathBuf::from("19.scm"));
        assert_eq!(list.iter().filter(|p| p.as_path() == Path::new("5.scm")).count(), 1);
    }
}
