use iced::keyboard::{Key, key};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text::{LineHeight, Span};
use iced::widget::text_editor::{Binding, KeyPress, Motion, Status};
use iced::widget::{
    Column, Row, button, canvas, column, container, lazy, mouse_area, pane_grid, rich_text, row, rule, scrollable, sensor, space,
    span, stack, table, text, text_editor, text_input, tooltip,
};
use std::path::Path;

use iced::{Alignment, Element, Font, Length, Padding, Pixels, Theme};
use schematic::docs::{self, Block, Inline};

use crate::app::{
    EDITOR_PAD, Find, LINE_HEIGHT, LineKind, Message, NamingKind, PAGE, Page, Palette, PaneKind, Side, Source, Studio, Tab,
    console_id, console_text_id, editor_id, editor_text_id, find_id, naming_id, palette_id,
};
use crate::chart::Chart;
use crate::commands::{self, Menu};
use crate::icons::{Glyph, Ink, icon};
use crate::worker::Grid;
use crate::{fonts, forms, highlight, theme};

type TextStyle = fn(&Theme) -> text::Style;

pub struct TabItem {
    pub label: String,
    pub detail: Option<String>,
    pub active: bool,
    pub select: Option<Message>,
    pub close: Option<Message>,
}

fn single(name: &str, detail: Option<String>) -> Vec<TabItem> {
    vec![TabItem { label: name.to_string(), detail, active: true, select: None, close: None }]
}

pub fn view(studio: &Studio) -> Element<'_, Message> {
    let panes = pane_grid(&studio.panes, |_, kind, _| {
        let (tabs, controls, body): (Vec<TabItem>, Option<Element<'_, Message>>, Element<'_, Message>) = match kind {
            PaneKind::Editor => (editor_tabs(studio), Some(editor_controls(studio)), workspace(studio)),
            PaneKind::Console => {
                (single("Console", Some(studio.namespace.clone())), Some(small("Clear", Some(Message::ClearConsole))), console(studio))
            }
            PaneKind::Environment => match studio.side {
                Side::Environment => (side_tabs(studio), None, environment(studio)),
                Side::Files => (side_tabs(studio), Some(file_controls(studio)), files(studio)),
                Side::Project => (side_tabs(studio), Some(project_controls(studio)), project(studio)),
                Side::Outline => (side_tabs(studio), None, outline(studio)),
            },
            PaneKind::Viewer => {
                let grid = studio.grid.as_ref();
                (single("Data", grid.map(|g| g.name.clone())), grid.map(pager), viewer(studio))
            }
            PaneKind::Plots => {
                let plot = studio.plot();
                let detail = plot.map(|p| p.title.clone().unwrap_or_else(|| p.describe()));
                (single("Plots", detail), Some(plot_controls(studio)), plots(studio))
            }
        };
        let maximized = studio.panes.maximized().is_some();
        let fold = tip(
            small(if maximized { "⤡" } else { "⤢" }, Some(if maximized { Message::RestorePanes } else { Message::Maximize(*kind) })),
            if maximized { "Restore the layout" } else { "Maximize this pane" },
        );
        let hide = tip(small("–", Some(Message::TogglePane(*kind))), "Hide this pane (View menu shows it again)");
        let pane_buttons = row![fold, hide].spacing(0);
        let controls: Element<'_, Message> = match controls {
            Some(controls) => row![controls, pane_buttons].spacing(6).align_y(Alignment::Center).into(),
            None => pane_buttons.into(),
        };
        let bar = pane_grid::TitleBar::new(strip(tabs, Some(controls))).style(theme::strip);
        pane_grid::Content::new(body).title_bar(bar).style(theme::pane)
    })
    .spacing(1)
    .style(theme::panes)
    .on_resize(6, Message::PaneResized)
    .on_drag(Message::PaneDragged);
    let workspace = container(panes).width(Length::Fill).height(Length::Fill).style(theme::gaps);
    let base: Element<'_, Message> = column![menu_bar(studio), line(), toolbar(studio), line(), workspace, line(), status_bar(studio)].into();
    match (studio.menu, &studio.palette) {
        (Some(menu), _) => stack![base, dropdown(studio, menu)].into(),
        (None, Some(palette)) => stack![base, palette_layer(studio, palette)].into(),
        (None, None) => base,
    }
}

const MENU_HEIGHT: f32 = 26.0;

fn menu_width(menu: Menu) -> f32 {
    18.0 + 7.4 * menu.label().chars().count() as f32
}

fn menu_bar(studio: &Studio) -> Element<'_, Message> {
    let mut bar = Row::new().height(MENU_HEIGHT).align_y(Alignment::Center).padding(Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 6.0 });
    for menu in Menu::ALL {
        let open = studio.menu == Some(menu);
        let head = button(text(menu.label()).size(12.5).font(fonts::UI_MEDIUM))
            .padding([2, 0])
            .width(menu_width(menu))
            .style(theme::choice(open))
            .on_press(Message::MenuToggle(menu));
        let head: Element<'_, Message> = if studio.menu.is_some() { mouse_area(head).on_enter(Message::MenuHover(menu)).into() } else { head.into() };
        bar = bar.push(container(head).center_x(Length::Shrink));
    }
    container(bar).width(Length::Fill).style(theme::bar).into()
}

fn dropdown(studio: &Studio, menu: Menu) -> Element<'_, Message> {
    let x = 6.0 + Menu::ALL.iter().take_while(|m| **m != menu).map(|m| menu_width(*m)).sum::<f32>();
    let mut list = Column::new().width(280);
    let mut any = false;
    for (index, command) in commands::commands(studio).into_iter().enumerate() {
        if command.menu != menu {
            continue;
        }
        if command.gap && any {
            list = list.push(container(line()).padding([4, 0]));
        }
        any = true;
        let enabled = command.message.is_some();
        let label: TextStyle = if enabled { theme::strong } else { theme::faint };
        let mark = match command.checked {
            Some(true) => "✓",
            _ => "",
        };
        let content = row![
            text(mark).size(12).style(theme::accent).width(14),
            text(command.name).size(12.5).style(label),
            space::horizontal(),
            text(command.keys).size(11.5).style(theme::faint),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        list = list.push(button(content).padding([4, 10]).width(Length::Fill).style(theme::row(false)).on_press_maybe(enabled.then_some(Message::MenuPick(index))));
    }
    let panel = container(list.padding(4)).style(theme::popup);
    let placed = container(panel).padding(Padding { top: MENU_HEIGHT, right: 0.0, bottom: 0.0, left: x }).width(Length::Fill).height(Length::Fill);
    let backdrop = mouse_area(container(space::horizontal()).width(Length::Fill).height(Length::Fill)).on_press(Message::MenuClose);
    stack![backdrop, placed].into()
}

fn palette_layer<'a>(studio: &'a Studio, palette: &'a Palette) -> Element<'a, Message> {
    let found = commands::matching(studio, &palette.query);
    let input = text_input("Type a command, or : and a line number", &palette.query)
        .id(palette_id())
        .on_input(Message::PaletteQuery)
        .on_submit(Message::PaletteRun(None))
        .size(13.5)
        .padding([7, 10])
        .style(theme::field)
        .width(Length::Fill);
    let mut list = Column::new().width(Length::Fill);
    for (index, command) in found.iter().enumerate().take(12) {
        let label: TextStyle = if command.message.is_some() { theme::strong } else { theme::faint };
        let content = row![
            text(command.name).size(13).style(label),
            space::horizontal(),
            text(command.keys).size(11.5).style(theme::faint),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let press = command.message.is_some().then_some(Message::PaletteRun(Some(index)));
        list = list.push(button(content).padding([5, 10]).width(Length::Fill).style(theme::row(index == palette.selected)).on_press_maybe(press));
    }
    if found.is_empty() {
        list = list.push(container(text("Nothing matches").size(12.5).style(theme::muted)).padding([6, 10]));
    }
    let panel = container(column![input, list].spacing(6).padding(8)).width(560).style(theme::popup);
    let placed = container(panel).padding(Padding { top: 72.0, right: 0.0, bottom: 0.0, left: 0.0 }).width(Length::Fill).height(Length::Fill).center_x(Length::Fill);
    let backdrop = mouse_area(container(space::horizontal()).width(Length::Fill).height(Length::Fill)).on_press(Message::PaletteClose);
    stack![backdrop, placed].into()
}

fn line<'a>() -> Element<'a, Message> {
    rule::horizontal(1).style(theme::separator).into()
}

fn strip<'a>(tabs: Vec<TabItem>, controls: Option<Element<'a, Message>>) -> Element<'a, Message> {
    let mut bar = Row::new().height(30).align_y(Alignment::Center);
    for tab in tabs {
        let mut label = row![text(tab.label).size(12.5).font(fonts::UI_MEDIUM)].spacing(8).align_y(Alignment::Center);
        if let Some(detail) = tab.detail {
            label = label.push(text(detail).size(12).style(theme::muted));
        }
        let closable = tab.close.is_some();
        let pick = button(container(label).height(Length::Fill).align_y(Alignment::Center))
            .padding(Padding { top: 0.0, right: if closable { 4.0 } else { 12.0 }, bottom: 0.0, left: 12.0 })
            .height(Length::Fill)
            .style(theme::tab_button(tab.active))
            .on_press_maybe(tab.select);
        let mut inner = Row::new().align_y(Alignment::Center).height(Length::Fill).push(pick);
        if let Some(close) = tab.close {
            let cross = button(text("×").size(14).font(fonts::UI_MEDIUM)).padding([0, 6]).style(theme::ghost).on_press(close);
            inner = inner.push(container(cross).padding(Padding { top: 0.0, right: 4.0, bottom: 0.0, left: 0.0 }));
        }
        bar = bar.push(container(inner).height(Length::Fill).style(theme::tab_shell(tab.active)));
    }
    bar = bar.push(space::horizontal());
    if let Some(controls) = controls {
        bar = bar.push(container(controls).padding([0, 6]));
    }
    column![bar, line()].into()
}

fn small<'a>(label: &'a str, message: Option<Message>) -> Element<'a, Message> {
    button(text(label).font(fonts::UI_MEDIUM).size(12)).padding([3, 8]).style(theme::ghost).on_press_maybe(message).into()
}

fn tip<'a>(content: impl Into<Element<'a, Message>>, hint: &'a str) -> Element<'a, Message> {
    let bubble = container(text(hint).size(12)).padding([4, 8]).style(theme::tip);
    tooltip(content, bubble, tooltip::Position::Bottom).gap(4).into()
}

fn tool<'a>(glyph: Glyph, ink: Ink, label: &'a str, hint: &'a str, message: Option<Message>) -> Element<'a, Message> {
    let ink = if message.is_none() { Ink::Muted } else { ink };
    let content = row![icon(glyph, ink), text(label).size(12.5).font(fonts::UI_MEDIUM)].spacing(6).align_y(Alignment::Center);
    tip(button(content).padding([4, 8]).style(theme::tool).on_press_maybe(message), hint)
}

fn divider<'a>() -> Element<'a, Message> {
    container(rule::vertical(1).style(theme::separator)).height(18).padding([0, 6]).into()
}

fn toolbar(studio: &Studio) -> Element<'_, Message> {
    let ready = studio.worker.is_some();
    let stop_ink = if studio.running { Ink::Danger } else { Ink::Muted };
    let theme_glyph = if theme::is_dark(&studio.theme) { Glyph::Sun } else { Glyph::Moon };
    let toggle = tip(button(icon(theme_glyph, Ink::Muted)).padding(6).style(theme::tool).on_press(Message::ToggleTheme), "Switch theme");
    let items = row![
        tool(Glyph::Play, Ink::Accent, "Run", "Run the form at the cursor (Ctrl+Enter)", ready.then_some(Message::RunForm)),
        tool(Glyph::PlayAll, Ink::Text, "Run file", "Run the whole file (Ctrl+Shift+Enter)", ready.then_some(Message::RunFile)),
        tool(Glyph::Stop, stop_ink, "Stop", "Interrupt the running code", studio.running.then_some(Message::Stop)),
        divider(),
        small("New", Some(Message::NewFile)),
        tool(Glyph::Open, Ink::Text, "Open", "Open a script (Ctrl+O)", Some(Message::Open)),
        tool(Glyph::Save, Ink::Text, "Save", "Save (Ctrl+S)", Some(Message::Save)),
        small("Save as", Some(Message::SaveAs)),
        space::horizontal(),
        toggle,
    ]
    .spacing(2)
    .align_y(Alignment::Center);
    container(items).padding([4, 8]).width(Length::Fill).style(theme::bar).into()
}

fn status_bar(studio: &Studio) -> Element<'_, Message> {
    let (line, column) = studio.cursor();
    let dot_style: TextStyle = if studio.running { theme::accent } else { theme::faint };
    let status_style: TextStyle = if studio.status.starts_with("Error") { theme::alarm } else { theme::muted };
    let path = studio.doc().path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    let mut bar = row![text("•").size(13).style(dot_style), text(&studio.status).size(11.5).style(status_style), space::horizontal()]
        .spacing(12)
        .align_y(Alignment::Center);
    let count = studio.doc().problems.len();
    if let Some(problem) = studio.problem_here() {
        bar = bar.push(text(problem.message.clone()).size(11.5).style(theme::alarm));
    } else if count > 0 {
        let label = if count == 1 { "1 problem, F8 to jump".to_string() } else { format!("{count} problems, F8 to jump") };
        bar = bar.push(text(label).size(11.5).style(theme::warn));
    }
    bar = bar.push(text(path).size(11.5).style(theme::faint)).push(text(format!("Ln {line}, Col {column}")).size(11.5).style(theme::muted));
    container(bar).padding([3, 10]).width(Length::Fill).style(theme::bar).into()
}

fn editor_tabs(studio: &Studio) -> Vec<TabItem> {
    let mut tabs: Vec<TabItem> = studio
        .docs
        .iter()
        .enumerate()
        .map(|(i, doc)| TabItem {
            label: doc.label(),
            detail: None,
            active: studio.tab == Tab::Doc(i),
            select: Some(Message::SelectTab(Tab::Doc(i))),
            close: Some(Message::CloseTab(Tab::Doc(i))),
        })
        .collect();
    if studio.help.is_some() {
        tabs.push(TabItem {
            label: "Help".to_string(),
            detail: None,
            active: studio.tab == Tab::Help,
            select: Some(Message::SelectTab(Tab::Help)),
            close: Some(Message::CloseTab(Tab::Help)),
        });
    }
    tabs
}

fn editor_controls(studio: &Studio) -> Element<'_, Message> {
    let help = (studio.tab != Tab::Help).then_some(Message::OpenHelp);
    row![small("+", Some(Message::NewFile)), small("Help", help)].spacing(2).into()
}

fn environment_label(studio: &Studio) -> Option<String> {
    match studio.bindings.len() {
        0 => None,
        1 => Some("1 binding".to_string()),
        n => Some(format!("{n} bindings")),
    }
}

fn folder_name(path: &Path) -> String {
    path.file_name().and_then(|n| n.to_str()).map_or_else(|| path.display().to_string(), str::to_string)
}

fn side_tabs(studio: &Studio) -> Vec<TabItem> {
    let tab = |label: &str, side: Side, detail: Option<String>| TabItem {
        label: label.to_string(),
        detail: if studio.side == side { detail } else { None },
        active: studio.side == side,
        select: Some(Message::ShowSide(side)),
        close: None,
    };
    vec![
        tab("Environment", Side::Environment, environment_label(studio)),
        tab("Files", Side::Files, studio.root.as_deref().map(folder_name)),
        tab("Project", Side::Project, studio.project.as_ref().map(|p| p.name.clone())),
        tab("Outline", Side::Outline, None),
    ]
}

fn outline(studio: &Studio) -> Element<'_, Message> {
    let items = forms::outline(&studio.doc().content.text());
    if items.is_empty() {
        return hint("Top-level definitions of the current file are listed here; click one to jump to it.");
    }
    let current = studio.doc().content.cursor().position.line;
    let at = items.iter().rposition(|item| item.line <= current);
    let mut list = Column::new().width(Length::Fill).padding(Padding { top: 4.0, right: 0.0, bottom: 8.0, left: 0.0 });
    for (i, item) in items.into_iter().enumerate() {
        let kind: TextStyle = match item.kind {
            forms::OutlineKind::Function | forms::OutlineKind::Macro => theme::accent,
            forms::OutlineKind::Namespace => theme::warn,
            forms::OutlineKind::Value => theme::muted,
        };
        let content = row![
            text(item.kind.label()).size(11).style(kind).width(44),
            text(item.label).font(fonts::MONO).size(12.5).style(theme::strong),
            space::horizontal(),
            text(format!("{}", item.line + 1)).size(11).style(theme::faint),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        list = list.push(button(content).padding([3, 12]).width(Length::Fill).style(theme::row(at == Some(i))).on_press(Message::JumpTo(item.offset)));
    }
    scrollable(list).height(Length::Fill).style(theme::scroll).into()
}

fn project_controls(studio: &Studio) -> Element<'_, Message> {
    let ready = studio.project.is_some() && studio.busy.is_none();
    let picked = studio.picked_package.clone().filter(|_| ready);
    row![
        small("Add…", ready.then_some(Message::StartNaming(NamingKind::Package))),
        small("Remove", picked.map(Message::RemovePackage)),
        small("Restore", ready.then_some(Message::RestoreProject)),
    ]
    .spacing(2)
    .into()
}

fn naming_row<'a>(label: &'a str, placeholder: &'a str, value: &'a str) -> Element<'a, Message> {
    let field = text_input(placeholder, value)
        .id(naming_id())
        .on_input(Message::Naming)
        .on_submit(Message::ConfirmNaming)
        .size(12.5)
        .padding([4, 8])
        .style(theme::field)
        .width(Length::Fill);
    row![
        text(label).size(12).style(theme::muted),
        field,
        small("OK", Some(Message::ConfirmNaming)),
        small("Cancel", Some(Message::CancelNaming)),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding([6, 12])
    .into()
}

fn project(studio: &Studio) -> Element<'_, Message> {
    let Some(info) = &studio.project else {
        let mut column = Column::new().push(hint(
            "No project.scm in this folder or above it. A project names its packages in project.scm, pins them in project.lock, and keeps their sources in packages/.",
        ));
        if studio.root.is_some() {
            column = column.push(container(small("Create a project here", Some(Message::CreateProject))).padding([0, 8]));
        }
        return column.into();
    };
    let header = row![
        text(&info.name).size(12.5).font(fonts::UI_MEDIUM),
        text(&info.version).size(12).style(theme::muted),
        space::horizontal(),
        small("Open main", Some(Message::PickEntry(info.main.clone()))),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding(Padding { top: 4.0, right: 6.0, bottom: 4.0, left: 12.0 });
    let mut list = Column::new().push(header).push(line()).width(Length::Fill);
    if let Some(busy) = &studio.busy {
        list = list.push(container(text(format!("{busy}…")).size(12).style(theme::accent)).padding([6, 12]));
    }
    if let Some(naming) = studio.naming.as_ref().filter(|n| n.kind == NamingKind::Package) {
        list = list.push(naming_row("Add", "name, name@version, or name ../path", &naming.text)).push(line());
    }
    if info.dependencies.is_empty() {
        return list.push(hint("No packages yet. Add one by name from the registry, or as name ../path for a local folder.")).into();
    }
    let heading = |label: &'static str| text(label).size(11.5).font(fonts::UI_MEDIUM).style(theme::muted);
    let columns = row![
        heading("Package").width(Length::FillPortion(3)),
        heading("Version").width(72),
        heading("Status").width(Length::FillPortion(2)),
    ]
    .spacing(10)
    .padding(Padding { top: 6.0, right: 12.0, bottom: 5.0, left: 12.0 });
    list = list.push(columns).push(line());
    for dependency in &info.dependencies {
        let style: TextStyle = match dependency.status {
            schematic::project::Status::Vendored | schematic::project::Status::Local => theme::fine,
            schematic::project::Status::Changed | schematic::project::Status::Unlocked => theme::warn,
            schematic::project::Status::Missing => theme::alarm,
        };
        let content = row![
            text(&dependency.name).font(fonts::MONO).size(12.5).style(theme::strong).width(Length::FillPortion(3)),
            text(dependency.version.clone().unwrap_or_default()).size(12).style(theme::muted).width(72),
            text(dependency.status.label()).size(12).style(style).width(Length::FillPortion(2)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let picked = studio.picked_package.as_deref() == Some(dependency.name.as_str());
        list = list.push(
            button(content).padding([3, 12]).width(Length::Fill).style(theme::row(picked)).on_press(Message::PickPackage(dependency.name.clone())),
        );
    }
    scrollable(list.padding(Padding { top: 0.0, right: 0.0, bottom: 8.0, left: 0.0 })).height(Length::Fill).style(theme::scroll).into()
}

fn file_controls(studio: &Studio) -> Element<'_, Message> {
    let picked = studio.selected.clone();
    let has_root = studio.root.is_some();
    row![
        small("+ File", has_root.then_some(Message::StartNaming(NamingKind::File))),
        small("+ Folder", has_root.then_some(Message::StartNaming(NamingKind::Folder))),
        small("Rename", picked.clone().map(|path| Message::StartNaming(NamingKind::Rename(path)))),
        small("Delete", picked.map(|_| Message::DeleteEntry)),
    ]
    .spacing(2)
    .into()
}

fn files(studio: &Studio) -> Element<'_, Message> {
    let Some(root) = &studio.root else {
        let open = container(small("Open a folder…", Some(Message::OpenFolder))).padding([0, 8]);
        return column![hint("Browse, open, and manage the files of a folder here."), open].into();
    };
    let header = row![
        text(folder_name(root)).size(12.5).font(fonts::UI_MEDIUM),
        space::horizontal(),
        small("Refresh", Some(Message::RefreshFiles)),
        small("Change…", Some(Message::OpenFolder)),
    ]
    .spacing(2)
    .align_y(Alignment::Center)
    .padding(Padding { top: 4.0, right: 6.0, bottom: 4.0, left: 12.0 });
    let mut list = Column::new().push(header).push(line()).width(Length::Fill);
    if let Some(naming) = studio.naming.as_ref().filter(|n| n.kind != NamingKind::Package) {
        let label = match naming.kind {
            NamingKind::Folder => "New folder",
            NamingKind::Rename(_) => "Rename to",
            _ => "New file",
        };
        list = list.push(naming_row(label, "name", &naming.text)).push(line());
    }
    for entry in &studio.entries {
        let chevron = match (entry.dir, entry.expanded) {
            (true, true) => "▾",
            (true, false) => "▸",
            (false, _) => "",
        };
        let style: TextStyle = if entry.dir || entry.name.ends_with(".scm") { theme::strong } else { theme::muted };
        let content = row![text(chevron).size(11).style(theme::faint).width(12), text(&entry.name).size(12.5).style(style)]
            .spacing(4)
            .align_y(Alignment::Center);
        let selected = studio.selected.as_deref() == Some(entry.path.as_path());
        let indent = 12.0 + 14.0 * entry.depth as f32;
        list = list.push(
            button(content)
                .padding(Padding { top: 3.0, right: 12.0, bottom: 3.0, left: indent })
                .width(Length::Fill)
                .style(theme::row(selected))
                .on_press(Message::PickEntry(entry.path.clone())),
        );
    }
    scrollable(list.padding(Padding { top: 0.0, right: 0.0, bottom: 8.0, left: 0.0 })).height(Length::Fill).style(theme::scroll).into()
}

fn completion_binding(press: &KeyPress) -> Option<Message> {
    let plain = !press.modifiers.command() && !press.modifiers.shift();
    match &press.key {
        Key::Named(key::Named::ArrowUp) if plain => Some(Message::CompleteMove(-1)),
        Key::Named(key::Named::ArrowDown) if plain => Some(Message::CompleteMove(1)),
        Key::Named(key::Named::Tab | key::Named::Enter) if plain => Some(Message::CompleteAccept),
        Key::Named(key::Named::Escape) => Some(Message::CompleteClose),
        _ => None,
    }
}

fn closer_of(open: char) -> char {
    match open {
        '(' => ')',
        '[' => ']',
        _ => '}',
    }
}

fn pairing(press: &KeyPress, text: &str, offset: usize) -> Option<Binding<Message>> {
    let context = forms::context_at(text, offset);
    let open_next = context.after.is_none_or(|c| c.is_whitespace() || matches!(c, ')' | ']' | '}'));
    match &press.key {
        Key::Named(key::Named::Backspace) if !press.modifiers.command() => {
            let pair = matches!((context.before, context.after), (Some('('), Some(')')) | (Some('['), Some(']')) | (Some('{'), Some('}')));
            let quotes = context.before == Some('"') && context.after == Some('"') && context.in_string;
            (pair && !context.in_string && !context.in_comment || quotes).then(|| Binding::Sequence(vec![Binding::Backspace, Binding::Delete]))
        }
        Key::Character(c) if press.modifiers.command() => {
            let _ = c;
            None
        }
        Key::Character(c) => {
            let typed = c.chars().next()?;
            match typed {
                '(' | '[' | '{' if !context.in_string && !context.in_comment && open_next => {
                    Some(Binding::Sequence(vec![Binding::Insert(typed), Binding::Insert(closer_of(typed)), Binding::Move(Motion::Left)]))
                }
                ')' | ']' | '}' if context.after == Some(typed) && !context.in_string => Some(Binding::Move(Motion::Right)),
                '"' if context.in_string && context.after == Some('"') && context.before != Some('\\') => Some(Binding::Move(Motion::Right)),
                '"' if !context.in_string && !context.in_comment && open_next => {
                    Some(Binding::Sequence(vec![Binding::Insert('"'), Binding::Insert('"'), Binding::Move(Motion::Left)]))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn editor_binding(studio: &Studio) -> impl Fn(KeyPress) -> Option<Binding<Message>> + '_ {
    let completing = studio.completion.as_ref().is_some_and(|c| c.source == Source::Editor);
    move |press| {
        if completing
            && let Some(message) = completion_binding(&press) {
                return Some(Binding::Custom(message));
            }
        let command = press.modifiers.command();
        let shift = press.modifiers.shift();
        let global = match &press.key {
            Key::Named(key::Named::Enter) if command => Some(if shift { Message::RunFile } else { Message::RunForm }),
            Key::Named(key::Named::Space) if command => Some(Message::Complete(Source::Editor)),
            Key::Named(key::Named::F1) => Some(Message::LookUp(Source::Editor)),
            Key::Named(key::Named::F8) => Some(Message::NextProblem),
            Key::Character(c) if command && c.eq_ignore_ascii_case("z") => Some(if shift { Message::Redo } else { Message::Undo }),
            Key::Character(c) if command && c.eq_ignore_ascii_case("y") => Some(Message::Redo),
            Key::Character(c) if command && c == "/" => Some(Message::ToggleComment),
            Key::Character(c) if command && shift && c.eq_ignore_ascii_case("f") => Some(Message::Reindent),
            Key::Named(key::Named::ArrowUp) if command && shift => Some(Message::ExpandSelection),
            _ => None,
        };
        if let Some(message) = global {
            return Some(Binding::Custom(message));
        }
        if !matches!(press.status, Status::Focused { .. }) {
            return Binding::from_key_press(press);
        }
        if matches!(press.key, Key::Named(key::Named::Tab)) {
            return Some(Binding::Sequence(vec![Binding::Insert(' '), Binding::Insert(' ')]));
        }
        let plain_enter = matches!(press.key, Key::Named(key::Named::Enter)) && !command && !shift;
        if plain_enter || matches!(press.key, Key::Character(_) | Key::Named(key::Named::Backspace)) {
            let content = &studio.doc().content;
            let text = content.text();
            let position = content.cursor().position;
            let offset = forms::offset_of(&text, position.line, position.column);
            if plain_enter {
                let indent = forms::indent_for(&text, offset);
                let mut steps = vec![Binding::Enter];
                steps.extend(std::iter::repeat_n(Binding::Insert(' '), indent));
                return Some(Binding::Sequence(steps));
            }
            if let Some(binding) = pairing(&press, &text, offset) {
                return Some(binding);
            }
        }
        Binding::from_key_press(press)
    }
}

fn console_binding(studio: &Studio) -> impl Fn(KeyPress) -> Option<Binding<Message>> {
    let cursor = studio.input.cursor().position.line;
    let first_line = cursor == 0;
    let last_line = cursor + 1 >= studio.input.line_count();
    let browsing = studio.recalled.is_some();
    let completing = studio.completion.as_ref().is_some_and(|c| c.source == Source::Console);
    move |press| {
        if !matches!(press.status, Status::Focused { .. }) {
            return None;
        }
        if completing
            && let Some(message) = completion_binding(&press) {
                return Some(Binding::Custom(message));
            }
        let plain = !press.modifiers.shift() && !press.modifiers.command();
        match &press.key {
            Key::Named(key::Named::Enter) if plain => Some(Binding::Custom(Message::Submit)),
            Key::Named(key::Named::Space) if press.modifiers.command() => Some(Binding::Custom(Message::Complete(Source::Console))),
            Key::Named(key::Named::F1) => Some(Binding::Custom(Message::LookUp(Source::Console))),
            Key::Named(key::Named::ArrowUp) if plain && first_line => Some(Binding::Custom(Message::HistoryBack)),
            Key::Named(key::Named::ArrowDown) if plain && last_line && browsing => {
                Some(Binding::Custom(Message::HistoryForward))
            }
            Key::Named(key::Named::Tab) => Some(Binding::Sequence(vec![Binding::Insert(' '), Binding::Insert(' ')])),
            _ => Binding::from_key_press(press),
        }
    }
}

fn workspace(studio: &Studio) -> Element<'_, Message> {
    match (studio.tab, &studio.help) {
        (Tab::Help, Some(_)) => help(studio),
        _ => match &studio.find {
            Some(find) => column![find_bar(studio, find), line(), editor(studio)].into(),
            None => editor(studio),
        },
    }
}

fn find_bar<'a>(studio: &'a Studio, find: &'a Find) -> Element<'a, Message> {
    let count = studio.doc().found.len();
    let where_at = match (find.current, count) {
        (_, 0) if find.query.is_empty() => String::new(),
        (_, 0) => "No matches".to_string(),
        (Some(i), n) => format!("{} of {n}", i + 1),
        (None, n) => format!("{n} matches"),
    };
    let query = text_input("Find", &find.query)
        .id(find_id())
        .on_input(Message::FindQuery)
        .on_submit(Message::FindStep(1))
        .size(12.5)
        .padding([4, 8])
        .style(theme::field)
        .width(260);
    let case = button(text("Aa").size(12).font(fonts::UI_MEDIUM)).padding([3, 7]).style(theme::choice(find.case)).on_press(Message::FindCase);
    let mut first = row![
        query,
        text(where_at).size(12).style(theme::muted).width(90),
        small("↑", (count > 0).then_some(Message::FindStep(-1))),
        small("↓", (count > 0).then_some(Message::FindStep(1))),
        tip(case, "Match case"),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if !find.replacing {
        first = first.push(small("Replace…", Some(Message::FindOpen(true))));
    }
    first = first.push(space::horizontal()).push(small("×", Some(Message::FindClose)));
    let mut bar = Column::new().push(first).spacing(6).padding(Padding { top: 6.0, right: 10.0, bottom: 6.0, left: 12.0 });
    if find.replacing {
        let replacement = text_input("Replace with", &find.replacement)
            .on_input(Message::FindReplacement)
            .on_submit(Message::FindReplace)
            .size(12.5)
            .padding([4, 8])
            .style(theme::field)
            .width(260);
        let ready = count > 0;
        let second = row![
            replacement,
            small("Replace", ready.then_some(Message::FindReplace)),
            small("Replace all", ready.then_some(Message::FindReplaceAll)),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        bar = bar.push(second);
    }
    container(bar).width(Length::Fill).style(theme::pane).into()
}

fn editor(studio: &Studio) -> Element<'_, Message> {
    let doc = studio.doc();
    let lines = doc.content.line_count().max(1);
    let current = doc.content.cursor().position.line;
    let digits = lines.to_string().len().max(2);
    let numbers = Column::with_children((0..lines).map(|i| {
        let flagged = doc.marks.problems.iter().any(|(line, _)| *line == i);
        let style: TextStyle = if flagged { theme::alarm } else if i == current { theme::strong } else { theme::faint };
        text(format!("{:>digits$}", i + 1))
            .font(fonts::MONO)
            .size(13.5)
            .line_height(LineHeight::Absolute(Pixels(LINE_HEIGHT)))
            .style(style)
            .into()
    }))
    .padding(Padding { top: EDITOR_PAD, right: 10.0, bottom: EDITOR_PAD, left: 14.0 });
    let code = text_editor(&doc.content)
        .id(editor_text_id())
        .on_action(Message::Edit)
        .placeholder("Open a script or start typing. Ctrl+Enter runs the form at the cursor.")
        .font(fonts::MONO)
        .size(13.5)
        .line_height(LineHeight::Absolute(Pixels(LINE_HEIGHT)))
        .padding(Padding { top: EDITOR_PAD, right: 12.0, bottom: EDITOR_PAD, left: 6.0 })
        .height(Length::Shrink)
        .wrapping(if studio.wrap { text::Wrapping::Word } else { text::Wrapping::None })
        .style(theme::editor)
        .highlight_with::<highlight::Schematic>(doc.marks.clone(), highlight::format)
        .key_binding(editor_binding(studio));
    let body = row![numbers, code].align_y(Alignment::Start).width(Length::Fill);
    let scroller = scrollable(body)
        .id(editor_id())
        .width(Length::Fill)
        .height(Length::Fill)
        .on_scroll(Message::EditorScrolled)
        .style(theme::scroll);
    let view: Element<'_, Message> = sensor(scroller).on_show(Message::EditorResized).on_resize(Message::EditorResized).into();
    let Some((popup, rows)) = popup(studio, Source::Editor) else { return view };
    let cursor = doc.content.cursor().position;
    let gutter = digits as f32 * 8.1 + 24.0;
    let x = (gutter + 6.0 + cursor.column as f32 * 8.1).min((studio.editor_width - POPUP_WIDTH - 4.0).max(0.0));
    let height = rows as f32 * 24.0 + 8.0;
    let below = EDITOR_PAD + (cursor.line as f32 + 1.0) * LINE_HEIGHT - doc.offset + 2.0;
    let y = if studio.editor_height > 0.0 && below + height > studio.editor_height {
        (below - LINE_HEIGHT - height - 4.0).max(0.0)
    } else {
        below
    };
    let layer = container(popup).padding(Padding { top: y, right: 0.0, bottom: 0.0, left: x }).width(Length::Fill).height(Length::Fill);
    stack![view, layer].into()
}

const POPUP_WIDTH: f32 = 400.0;

fn popup(studio: &Studio, source: Source) -> Option<(Element<'_, Message>, usize)> {
    let completion = studio.completion.as_ref().filter(|c| c.source == source)?;
    let rows = completion.items.iter().enumerate().map(|(i, item)| {
        let content = row![
            text(&item.name).font(fonts::MONO).size(12.5).style(theme::strong),
            text(&item.detail).size(11.5).style(theme::muted).wrapping(text::Wrapping::None),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        button(content)
            .padding([3, 10])
            .width(Length::Fill)
            .style(theme::row(i == completion.selected))
            .on_press(Message::CompletePick(i))
            .into()
    });
    let list = container(Column::with_children(rows).width(Length::Fill)).width(POPUP_WIDTH).padding(4).style(theme::popup);
    Some((list.into(), completion.items.len()))
}

fn code(source: &str, theme: &Theme, size: f32) -> Element<'static, Message> {
    let base = theme::tones(theme).text;
    let spans: Vec<Span<'static, (), Font>> = highlight::segments(source)
        .into_iter()
        .map(|(range, kind)| {
            let format = kind.map(|kind| highlight::format(&kind, theme));
            span(source[range].to_string())
                .color(format.and_then(|f| f.color).unwrap_or(base))
                .font(format.and_then(|f| f.font).unwrap_or(fonts::MONO))
        })
        .collect();
    rich_text(spans).size(size).width(Length::Fill).into()
}

fn prose(markdown: &str, theme: &Theme, size: f32) -> Element<'static, Message> {
    let t = theme::tones(theme);
    let spans: Vec<Span<'static, (), Font>> = docs::inlines(markdown)
        .into_iter()
        .map(|inline| match inline {
            Inline::Text(s) => span(s).color(t.text),
            Inline::Code(s) => span(s).font(fonts::MONO).size(size - 0.5).color(t.text).background(t.raised).padding([0, 3]),
            Inline::Bold(s) => span(s).font(fonts::UI_SEMIBOLD).color(t.text),
            Inline::Italic(s) => span(s).font(fonts::UI_MEDIUM).color(t.muted),
            Inline::Link(s, _) => span(s).color(t.accent).underline(true),
        })
        .collect();
    rich_text(spans).size(size).width(Length::Fill).into()
}

fn table_block(rows: Vec<Vec<String>>, theme: &Theme) -> Element<'static, Message> {
    let mut rows = rows.into_iter();
    let Some(header) = rows.next() else { return space::vertical().height(0).into() };
    let n = header.len();
    let width = move |i: usize| -> Length {
        match (n, i) {
            (2, 0) => Length::FillPortion(2),
            (2, _) => Length::FillPortion(3),
            _ => Length::FillPortion(1),
        }
    };
    let head = Row::with_children(
        header.into_iter().enumerate().map(|(i, cell)| text(cell).size(11.5).font(fonts::UI_SEMIBOLD).style(theme::muted).width(width(i)).into()),
    )
    .spacing(12);
    let mut column = Column::new().width(Length::Fill).push(container(head).padding([4, 0])).push(line());
    for row in rows {
        let cells = Row::with_children(row.iter().enumerate().map(|(i, cell)| container(prose(cell, theme, 12.5)).width(width(i)).into()))
            .spacing(12)
            .align_y(Alignment::Start);
        column = column.push(container(cells).padding([5, 0])).push(line());
    }
    column.into()
}

fn render(block: Block, theme: &Theme) -> Element<'static, Message> {
    match block {
        Block::Heading(level, title) => {
            let (size, top) = match level {
                1 => (22.0, 4.0),
                2 => (17.0, 16.0),
                _ => (14.0, 8.0),
            };
            container(text(title).size(size).font(fonts::UI_SEMIBOLD)).padding(Padding { top, right: 0.0, bottom: 0.0, left: 0.0 }).into()
        }
        Block::Paragraph(text) => prose(&text, theme, 13.0),
        Block::Code(source) => container(code(&source, theme, 12.5)).padding([8, 12]).width(Length::Fill).style(theme::code_block).into(),
        Block::Quote(text) => container(prose(&text, theme, 13.0)).padding([6, 12]).width(Length::Fill).style(theme::quote).into(),
        Block::List(items) => {
            let rows = items.into_iter().map(|item| {
                row![text("•").size(13).style(theme::muted).width(16), prose(&item, theme, 13.0)].spacing(4).align_y(Alignment::Start).into()
            });
            Column::with_children(rows).spacing(4).into()
        }
        Block::Table(rows) => table_block(rows, theme),
    }
}

fn document(page: Page, dark: bool) -> Element<'static, Message> {
    let theme = if dark { theme::dark() } else { theme::light() };
    let markdown = match page {
        Page::Guide => docs::GUIDE,
        Page::Reference => docs::REFERENCE,
    };
    let blocks = docs::blocks(markdown).into_iter().map(|block| render(block, &theme));
    Column::with_children(blocks)
        .spacing(10)
        .padding(Padding { top: 8.0, right: 20.0, bottom: 24.0, left: 20.0 })
        .width(Length::Fill)
        .max_width(760)
        .into()
}

fn matches<'a>(query: &str, theme: &Theme) -> Element<'a, Message> {
    let found = docs::search(query);
    if found.is_empty() {
        return hint("Nothing in the reference matches.");
    }
    let mut column = Column::new().spacing(16).padding(Padding { top: 8.0, right: 20.0, bottom: 24.0, left: 20.0 }).max_width(760);
    for entry in found.into_iter().take(40) {
        let mut block = Column::new().spacing(4).push(text(entry.name.clone()).font(fonts::MONO_BOLD).size(14).style(theme::accent));
        for found in &entry.uses {
            block = block
                .push(code(&found.signatures.join(", "), theme, 12.5))
                .push(prose(&found.description, theme, 12.5))
                .push(text(found.section.clone()).size(11).style(theme::faint));
        }
        column = column.push(block);
    }
    column.into()
}

fn help(studio: &Studio) -> Element<'_, Message> {
    let Some(help) = &studio.help else {
        return hint("Press F1 on a name to look it up.");
    };
    let dark = theme::is_dark(&studio.theme);
    let page = |label: &'static str, page: Page| {
        button(text(label).size(12).font(fonts::UI_MEDIUM)).padding([3, 8]).style(theme::choice(help.page == page)).on_press(Message::HelpPage(page))
    };
    let search = text_input("Search the reference", &help.query)
        .on_input(Message::HelpQuery)
        .size(12.5)
        .padding([5, 8])
        .style(theme::field)
        .width(Length::Fill);
    let bar = row![page("Guide", Page::Guide), page("Reference", Page::Reference), search].spacing(6).padding([8, 12]).align_y(Alignment::Center);
    let body: Element<'_, Message> = if help.query.trim().is_empty() {
        let page = help.page;
        lazy((page, dark), move |_| document(page, dark)).into()
    } else {
        matches(&help.query, &studio.theme)
    };
    column![bar, line(), scrollable(body).width(Length::Fill).height(Length::Fill).style(theme::scroll)].into()
}

fn console(studio: &Studio) -> Element<'_, Message> {
    let lines = studio.console.iter().map(|line| {
        let (marker, marker_style, style, font): (&str, TextStyle, TextStyle, Font) = match line.kind {
            LineKind::Input => ("›", theme::accent, theme::strong, fonts::MONO),
            LineKind::Output => ("", theme::faint, theme::strong, fonts::MONO),
            LineKind::Result => ("=", theme::faint, theme::strong, fonts::MONO),
            LineKind::Error => ("!", theme::alarm, theme::alarm, fonts::MONO),
            LineKind::Note => ("", theme::faint, theme::muted, fonts::MONO_ITALIC),
        };
        let content = line.text.trim_end_matches('\n');
        let body: Element<'_, Message> = match line.kind {
            LineKind::Input | LineKind::Result => code(content, &studio.theme, 13.0),
            _ => text(content).font(font).size(13).style(style).width(Length::Fill).into(),
        };
        row![container(text(marker).font(fonts::MONO_BOLD).size(13).style(marker_style)).width(14), body]
            .spacing(8)
            .into()
    });
    let history = scrollable(column(lines).spacing(3).padding([8, 12]).width(Length::Fill))
        .id(console_id())
        .height(Length::Fill)
        .style(theme::scroll);
    let prompt = if studio.pending.is_empty() { "›" } else { "…" };
    let input = text_editor(&studio.input)
        .id(console_text_id())
        .on_action(Message::Input)
        .placeholder("Type an expression and press Enter")
        .font(fonts::MONO)
        .size(13)
        .padding(Padding { top: 7.0, right: 0.0, bottom: 7.0, left: 0.0 })
        .style(theme::editor)
        .highlight_with::<highlight::Schematic>(studio.input_marks.clone(), highlight::format)
        .key_binding(console_binding(studio));
    let marker = container(text(prompt).font(fonts::MONO_BOLD).size(13).style(theme::accent))
        .width(14)
        .padding(Padding { top: 7.0, right: 0.0, bottom: 7.0, left: 0.0 });
    let entry = row![marker, input].spacing(8).padding([0, 12]);
    let body: Element<'_, Message> = column![history, line(), entry].into();
    let Some((popup, _)) = popup(studio, Source::Console) else { return body };
    let cursor = studio.input.cursor().position;
    let x = 34.0 + cursor.column as f32 * 7.8;
    let bottom = 16.0 + studio.input.line_count().max(1) as f32 * 18.0;
    let layer = container(popup)
        .padding(Padding { top: 0.0, right: 0.0, bottom, left: x })
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::End);
    stack![body, layer].into()
}

fn environment(studio: &Studio) -> Element<'_, Message> {
    let groups = studio.groups();
    if groups.is_empty() {
        return hint("Nothing defined yet. Values you define are listed here; click one to open it in the data pane.");
    }
    let selected = studio.grid.as_ref().map(|g| g.name.as_str());
    let heading = |label: &'static str| text(label).size(11.5).font(fonts::UI_MEDIUM).style(theme::muted);
    let header = row![
        heading("Name").width(Length::FillPortion(2)),
        heading("Type").width(72),
        heading("Value").width(Length::FillPortion(3)),
    ]
    .spacing(10)
    .padding(Padding { top: 6.0, right: 12.0, bottom: 5.0, left: 12.0 });
    let mut list = Column::new().push(header).push(line()).width(Length::Fill);
    for (group, members) in groups {
        let label = text(group.label()).size(11.5).font(fonts::UI_SEMIBOLD).style(theme::muted);
        list = list.push(container(label).padding(Padding { top: 9.0, right: 12.0, bottom: 2.0, left: 12.0 }));
        for binding in members {
            let content = row![
                text(&binding.name).font(fonts::MONO).size(12.5).style(theme::strong).width(Length::FillPortion(2)),
                text(&binding.kind).size(12).style(theme::muted).width(72),
                text(&binding.summary).size(12).style(theme::muted).width(Length::FillPortion(3)),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            let chosen = selected == Some(binding.name.as_str());
            list = list.push(
                button(content)
                    .padding([3, 12])
                    .width(Length::Fill)
                    .style(theme::row(chosen))
                    .on_press(Message::Inspect(binding.name.clone())),
            );
        }
    }
    scrollable(list.padding(Padding { top: 0.0, right: 0.0, bottom: 8.0, left: 0.0 })).height(Length::Fill).style(theme::scroll).into()
}

fn pager(grid: &Grid) -> Element<'_, Message> {
    let first = grid.offset + usize::from(!grid.rows.is_empty());
    let last = grid.offset + grid.rows.len();
    let label = text(format!("{first}–{last} of {}", grid.total)).size(12).style(theme::muted);
    let previous = button(text("‹").size(15)).padding([0, 8]).style(theme::ghost);
    let next = button(text("›").size(15)).padding([0, 8]).style(theme::ghost);
    row![
        label,
        previous.on_press_maybe((grid.offset > 0).then_some(Message::Page(-1))),
        next.on_press_maybe((grid.offset + PAGE < grid.total).then_some(Message::Page(1))),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn viewer(studio: &Studio) -> Element<'_, Message> {
    let Some(grid) = &studio.grid else {
        return hint("Click a name in the environment to view it here.");
    };
    let index = grid.columns.first().is_some_and(|c| c == "#");
    let columns = grid.columns.iter().enumerate().map(|(c, name)| {
        let header = text(name).font(fonts::UI_SEMIBOLD).size(11).style(theme::muted);
        let style: TextStyle = if index && c == 0 { theme::faint } else { theme::strong };
        let column = table::column(header, move |r: usize| text(&grid.rows[r][c]).font(fonts::MONO).size(12.5).style(style));
        if grid.numeric.get(c).copied().unwrap_or(false) { column.align_x(Alignment::End) } else { column }
    });
    let body = table(columns, 0..grid.rows.len()).padding_x(10).padding_y(5);
    let both = Direction::Both { vertical: Scrollbar::default(), horizontal: Scrollbar::default() };
    scrollable(body).direction(both).width(Length::Fill).height(Length::Fill).style(theme::scroll).into()
}

fn plot_controls(studio: &Studio) -> Element<'_, Message> {
    let count = studio.plots.len();
    let save = small("Save as SVG", studio.plot().map(|_| Message::SavePlot));
    if count < 2 {
        return save;
    }
    let position = text(format!("{} of {count}", studio.plot_at + 1)).size(12).style(theme::muted);
    let back = button(text("‹").size(15)).padding([0, 8]).style(theme::ghost).on_press_maybe((studio.plot_at > 0).then_some(Message::PlotBack));
    let forward = button(text("›").size(15)).padding([0, 8]).style(theme::ghost);
    let forward = forward.on_press_maybe((studio.plot_at + 1 < count).then_some(Message::PlotForward));
    row![position, back, forward, save].spacing(4).align_y(Alignment::Center).into()
}

fn plots(studio: &Studio) -> Element<'_, Message> {
    match studio.plot() {
        Some(plot) => canvas(Chart { plot }).width(Length::Fill).height(Length::Fill).into(),
        None => hint("Charts appear here. Try (bar-chart t :city :total) or (histogram (:amount t))."),
    }
}

fn hint<'a>(message: &'a str) -> Element<'a, Message> {
    container(text(message).size(12.5).style(theme::faint)).padding(12).into()
}
