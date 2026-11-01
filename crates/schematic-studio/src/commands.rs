use crate::app::{Message, Page, PaneKind, Side, Source, Studio, Tab};
use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    File,
    Edit,
    View,
    Run,
    Help,
}

impl Menu {
    pub const ALL: [Menu; 5] = [Menu::File, Menu::Edit, Menu::View, Menu::Run, Menu::Help];

    pub fn label(self) -> &'static str {
        match self {
            Menu::File => "File",
            Menu::Edit => "Edit",
            Menu::View => "View",
            Menu::Run => "Run",
            Menu::Help => "Help",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Command {
    pub menu: Menu,
    pub name: &'static str,
    pub keys: &'static str,
    pub message: Option<Message>,
    pub checked: Option<bool>,
    pub gap: bool,
}

fn item(menu: Menu, name: &'static str, keys: &'static str, message: Option<Message>) -> Command {
    Command { menu, name, keys, message, checked: None, gap: false }
}

fn gap(mut command: Command) -> Command {
    command.gap = true;
    command
}

fn check(mut command: Command, on: bool) -> Command {
    command.checked = Some(on);
    command
}

pub fn commands(studio: &Studio) -> Vec<Command> {
    let doc = studio.doc();
    let ready = studio.worker.is_some();
    let shown = |kind: PaneKind| studio.panes.iter().any(|(_, k)| *k == kind);
    let maximized = studio.panes.maximized().is_some();
    vec![
        item(Menu::File, "New file", "Ctrl+N", Some(Message::NewFile)),
        item(Menu::File, "Open file…", "Ctrl+O", Some(Message::Open)),
        item(Menu::File, "Open folder…", "", Some(Message::OpenFolder)),
        gap(item(Menu::File, "Save", "Ctrl+S", Some(Message::Save))),
        item(Menu::File, "Save as…", "Ctrl+Shift+S", Some(Message::SaveAs)),
        gap(item(Menu::File, "Close tab", "Ctrl+W", Some(Message::CloseTab(Tab::Doc(studio.current))))),
        gap(item(Menu::File, "Quit", "", Some(Message::Quit))),
        item(Menu::Edit, "Undo", "Ctrl+Z", (!doc.history.is_empty()).then_some(Message::Undo)),
        item(Menu::Edit, "Redo", "Ctrl+Y", (!doc.future.is_empty()).then_some(Message::Redo)),
        gap(item(Menu::Edit, "Find…", "Ctrl+F", Some(Message::FindOpen(false)))),
        item(Menu::Edit, "Replace…", "Ctrl+H", Some(Message::FindOpen(true))),
        item(Menu::Edit, "Find next", "F3", studio.find.is_some().then_some(Message::FindStep(1))),
        item(Menu::Edit, "Find previous", "Shift+F3", studio.find.is_some().then_some(Message::FindStep(-1))),
        gap(item(Menu::Edit, "Toggle comment", "Ctrl+/", Some(Message::ToggleComment))),
        item(Menu::Edit, "Reindent", "Ctrl+Shift+F", Some(Message::Reindent)),
        item(Menu::Edit, "Expand selection", "Ctrl+Shift+↑", Some(Message::ExpandSelection)),
        item(Menu::Edit, "Complete", "Ctrl+Space", Some(Message::Complete(Source::Editor))),
        gap(item(Menu::Edit, "Go to line…", "Ctrl+G", Some(Message::PaletteOpen(":".to_string())))),
        check(item(Menu::View, "Editor", "", Some(Message::TogglePane(PaneKind::Editor))), shown(PaneKind::Editor)),
        check(item(Menu::View, "Console", "", Some(Message::TogglePane(PaneKind::Console))), shown(PaneKind::Console)),
        check(item(Menu::View, "Environment", "", Some(Message::TogglePane(PaneKind::Environment))), shown(PaneKind::Environment)),
        check(item(Menu::View, "Data", "", Some(Message::TogglePane(PaneKind::Viewer))), shown(PaneKind::Viewer)),
        check(item(Menu::View, "Plots", "", Some(Message::TogglePane(PaneKind::Plots))), shown(PaneKind::Plots)),
        gap(if maximized {
            item(Menu::View, "Restore layout", "", Some(Message::RestorePanes))
        } else {
            item(Menu::View, "Maximize editor", "", Some(Message::Maximize(PaneKind::Editor)))
        }),
        gap(check(item(Menu::View, "Word wrap", "", Some(Message::ToggleWrap)), studio.wrap)),
        check(item(Menu::View, "Dark theme", "", Some(Message::ToggleTheme)), theme::is_dark(&studio.theme)),
        gap(check(item(Menu::View, "Environment tab", "Ctrl+3", Some(Message::ShowSide(Side::Environment))), studio.side == Side::Environment)),
        check(item(Menu::View, "Files tab", "Ctrl+4", Some(Message::ShowSide(Side::Files))), studio.side == Side::Files),
        check(item(Menu::View, "Project tab", "Ctrl+5", Some(Message::ShowSide(Side::Project))), studio.side == Side::Project),
        check(item(Menu::View, "Outline tab", "Ctrl+6", Some(Message::ShowSide(Side::Outline))), studio.side == Side::Outline),
        item(Menu::Run, "Run form at cursor", "Ctrl+Enter", ready.then_some(Message::RunForm)),
        item(Menu::Run, "Run file", "Ctrl+Shift+Enter", ready.then_some(Message::RunFile)),
        item(Menu::Run, "Stop", "", studio.running.then_some(Message::Stop)),
        gap(item(Menu::Run, "Clear console", "", Some(Message::ClearConsole))),
        item(Menu::Help, "Guide", "", Some(Message::ShowHelp(Page::Guide))),
        item(Menu::Help, "Reference", "", Some(Message::ShowHelp(Page::Reference))),
        item(Menu::Help, "Look up the name at the cursor", "F1", Some(Message::LookUp(Source::Editor))),
        gap(item(Menu::Help, "Command palette", "Ctrl+Shift+P", Some(Message::PaletteOpen(String::new())))),
        gap(item(Menu::Help, "Website", "", Some(Message::OpenWebsite))),
        item(Menu::Help, "About Schematic Studio", "", Some(Message::About)),
    ]
}

fn subsequence(query: &str, name: &str) -> bool {
    let mut chars = name.chars().flat_map(char::to_lowercase);
    query.chars().flat_map(char::to_lowercase).filter(|c| !c.is_whitespace()).all(|wanted| chars.by_ref().any(|c| c == wanted))
}

pub fn matching(studio: &Studio, query: &str) -> Vec<Command> {
    if let Some(rest) = query.strip_prefix(':') {
        let line = rest.trim().parse::<usize>().ok().filter(|n| *n > 0);
        return vec![match line {
            Some(line) => Command { message: Some(Message::JumpToLine(line)), ..item(Menu::Edit, "Go to line", "", None) },
            None => item(Menu::Edit, "Type a line number", "", None),
        }];
    }
    let query = query.trim();
    let mut found: Vec<Command> = commands(studio).into_iter().filter(|c| c.message.is_some() && subsequence(query, c.name)).collect();
    if !query.is_empty() {
        let lowered = query.to_lowercase();
        found.sort_by_key(|c| {
            let name = c.name.to_lowercase();
            (!name.starts_with(&lowered), !name.contains(&lowered))
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsequences_match_loosely_but_prefixes_sort_first() {
        assert!(subsequence("rnf", "Run file"));
        assert!(subsequence("Save As", "Save as…"));
        assert!(!subsequence("x", "Save"));
        let studio = Studio::new();
        let names: Vec<&str> = matching(&studio, "sa").iter().map(|c| c.name).collect();
        assert_eq!(&names[..2], ["Save", "Save as…"]);
        assert!(names.contains(&"Files tab") && !names.contains(&"Expand selection"));
        assert_eq!(matching(&studio, "").len(), commands(&studio).iter().filter(|c| c.message.is_some()).count());
        assert!(matches!(matching(&studio, ":12")[0].message, Some(Message::JumpToLine(12))));
        assert_eq!(matching(&studio, ":x")[0].name, "Type a line number");
        assert!(matching(&studio, ":x")[0].message.is_none());
    }
}
