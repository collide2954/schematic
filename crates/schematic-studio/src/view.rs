use iced::keyboard::{Key, key};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text::{LineHeight, Span};
use iced::widget::text_editor::{Binding, KeyPress, Motion, Status};
use iced::widget::{
    Column, Row, button, canvas, column, container, lazy, mouse_area, pane_grid, rich_text, row, rule, scrollable, sensor, space,
    span, stack, table, text, text_editor, text_input, tooltip,
};
use std::path::{Path, PathBuf};

use iced::{Alignment, Color, Element, Font, Length, Padding, Pixels, Theme, window};
use schematic::docs::{self, Block, Inline};

use crate::app::{
    Attach, Cell, Diff, EDITOR_PAD, Entry, Find, LineKind, Message, NamingKind, Notebook, PAGE, Page, Palette, PaneKind, Side,
    Source, Studio, Tab, assistant_id, composer_id, console_find_id, console_id, console_text_id, duration, editor_id,
    editor_text_id, find_id, naming_id, palette_id,
};
use crate::assistant::Call;
use crate::claude::{EFFORTS, MODELS};
use crate::git::{self, Change};
use crate::notebook::{Kind, Output};
use crate::chart::{Chart, PlotView, Thumb};
use crate::commands::{self, Menu};
use crate::worker::Watched;
use crate::icons::{Glyph, Ink, icon};
use crate::worker::Grid;
use crate::{fonts, forms, highlight, settings, theme};

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

pub fn window(studio: &Studio, id: window::Id) -> Element<'_, Message> {
    match studio.detached.iter().find(|(w, _)| *w == id) {
        Some((_, kind)) => detached_pane(studio, *kind),
        None => view(studio),
    }
}

type PaneParts<'a> = (Vec<TabItem>, Option<Element<'a, Message>>, Element<'a, Message>);

fn pane_parts(studio: &Studio, kind: PaneKind) -> PaneParts<'_> {
    match kind {
        PaneKind::Editor => (editor_tabs(studio), Some(editor_controls(studio)), workspace(studio)),
        PaneKind::Console => (single("Console", Some(studio.namespace.clone())), Some(console_controls(studio)), console(studio)),
        PaneKind::Environment => match studio.side {
            Side::Environment => (side_tabs(studio), None, environment(studio)),
            Side::Files => (side_tabs(studio), None, files(studio)),
            Side::Project => (side_tabs(studio), None, project(studio)),
            Side::Outline => (side_tabs(studio), None, outline(studio)),
            Side::Watch => (side_tabs(studio), Some(watch_controls(studio)), watch(studio)),
            Side::Git => (side_tabs(studio), None, git_tab(studio)),
        },
        PaneKind::Viewer => {
            let grid = studio.grid.as_ref();
            (single("Data", grid.map(|g| g.name.clone())), None, viewer(studio))
        }
        PaneKind::Plots => {
            let plot = studio.plot();
            let detail = plot.map(|p| p.title.clone().unwrap_or_else(|| p.describe()));
            (single("Plots", detail), None, plots(studio))
        }
        PaneKind::Assistant => {
            let model = MODELS.iter().find(|m| m.id == studio.assistant.model).map(|m| m.name.trim_start_matches("Claude ").to_string());
            (single("Assistant", model), Some(assistant_controls(studio)), assistant_pane(studio))
        }
    }
}

fn assistant_controls(studio: &Studio) -> Element<'_, Message> {
    let assistant = &studio.assistant;
    row![small("Stop", assistant.live.is_some().then_some(Message::AskStop)), small("Clear", (!assistant.entries.is_empty()).then_some(Message::AskClear))]
        .spacing(2)
        .align_y(Alignment::Center)
        .into()
}

fn markdown_view(studio: &Studio, source: &str) -> Element<'static, Message> {
    let blocks = docs::blocks(source).into_iter().map(|block| render(block, &studio.theme));
    Column::with_children(blocks).spacing(6).into()
}

fn entry_view<'a>(studio: &'a Studio, entry: &'a Entry) -> Element<'a, Message> {
    match entry {
        Entry::You(words) => {
            let rule = container(space::vertical()).width(2).height(Length::Fill).style(theme::cell_rule(true));
            row![rule, container(text(words).size(theme::UI).style(theme::strong)).padding([2, 10]).width(Length::Fill)].into()
        }
        Entry::Reply(markdown) => markdown_view(studio, markdown),
        Entry::Tool(summary) => text(summary).size(theme::FINE).style(theme::muted).into(),
        Entry::Note(note) => text(note).size(theme::META).style(theme::warn).into(),
    }
}

fn approval_view<'a>(studio: &'a Studio, call: &'a Call) -> Element<'a, Message> {
    let source = call.field("code");
    column![
        text("Run this?").size(theme::META).font(fonts::UI_MEDIUM),
        container(code(source, &studio.theme, studio.console_size())).padding([6, 10]).width(Length::Fill).style(theme::code_block),
        row![small("Run", Some(Message::Approve(false))), small("Run without asking", Some(Message::Approve(true))), small("Skip", Some(Message::Deny))].spacing(6),
    ]
    .spacing(6)
    .into()
}

fn composer_binding(press: KeyPress) -> Option<Binding<Message>> {
    if !matches!(press.status, Status::Focused { .. }) {
        return Binding::from_key_press(press);
    }
    match &press.key {
        Key::Named(key::Named::Enter) if !press.modifiers.shift() => Some(Binding::Custom(Message::AskSend)),
        _ => Binding::from_key_press(press),
    }
}

fn assistant_pane(studio: &Studio) -> Element<'_, Message> {
    let assistant = &studio.assistant;
    let mut list = Column::new().spacing(10).width(Length::Fill).padding(Padding { top: 10.0, right: 12.0, bottom: 10.0, left: 12.0 });
    for entry in &assistant.entries {
        list = list.push(entry_view(studio, entry));
    }
    if let Some(live) = &assistant.live
        && !live.text.is_empty()
    {
        list = list.push(markdown_view(studio, &live.text));
    }
    if let Some(call) = &assistant.approval {
        list = list.push(approval_view(studio, call));
    }
    let history = scrollable(list).id(assistant_id()).width(Length::Fill).height(Length::Fill).style(theme::scroll);
    let chips = Row::with_children(Attach::ALL.iter().map(|attach| {
        let on = assistant.attach.contains(attach);
        button(text(attach.label()).size(theme::FINE).font(fonts::UI_MEDIUM)).padding(theme::CONTROL).style(theme::choice(on)).on_press(Message::AttachToggle(*attach)).into()
    }))
    .spacing(4);
    let ready = !assistant.composer.text().trim().is_empty() && !assistant.busy();
    let bar = row![chips, space::horizontal(), small("Send", ready.then_some(Message::AskSend))].spacing(6).align_y(Alignment::Center);
    let composer = text_editor(&assistant.composer)
        .id(composer_id())
        .on_action(Message::Compose)
        .size(theme::UI)
        .padding(theme::FIELD_PAD)
        .height(Length::Shrink)
        .wrapping(text::Wrapping::Word)
        .style(theme::editor)
        .key_binding(composer_binding);
    let foot = column![bar, composer].spacing(6).padding(Padding { top: 6.0, right: 12.0, bottom: 8.0, left: 12.0 });
    column![history, line(), foot].into()
}

fn detached_pane(studio: &Studio, kind: PaneKind) -> Element<'_, Message> {
    let (tabs, controls, body) = pane_parts(studio, kind);
    let attach = small("Attach", studio.detached.iter().find(|(_, k)| *k == kind).map(|(id, _)| Message::AttachPane(*id)));
    let controls: Element<'_, Message> = match controls {
        Some(controls) => row![controls, attach].spacing(6).align_y(Alignment::Center).into(),
        None => attach,
    };
    let content = column![strip(tabs, Some(controls)), container(body).width(Length::Fill).height(Length::Fill)];
    container(content).width(Length::Fill).height(Length::Fill).style(theme::pane).into()
}

pub fn view(studio: &Studio) -> Element<'_, Message> {
    let panes = pane_grid(&studio.panes, |_, kind, _| {
        let (tabs, controls, body) = pane_parts(studio, *kind);
        let maximized = studio.panes.maximized().is_some();
        let detach = tip(glyph_button(Glyph::Detach, Some(Message::DetachPane(*kind))), "Detach");
        let fold = tip(
            glyph_button(if maximized { Glyph::Shrink } else { Glyph::Expand }, Some(if maximized { Message::RestorePanes } else { Message::Maximize(*kind) })),
            if maximized { "Restore" } else { "Maximize" },
        );
        let hide = tip(glyph_button(Glyph::Minus, Some(Message::TogglePane(*kind))), "Hide");
        let pane_buttons = row![detach, fold, hide].spacing(0);
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
const DROPDOWN_WIDTH: f32 = 280.0;
const PALETTE_WIDTH: f32 = 560.0;
const HEADER: Padding = Padding { top: 4.0, right: 6.0, bottom: 4.0, left: 12.0 };

const MENU_PAD: Padding = Padding { top: 2.0, right: 10.0, bottom: 2.0, left: 10.0 };

fn menu_label<'a>(menu: Menu) -> iced::widget::Text<'a> {
    text(menu.label()).size(theme::UI).font(fonts::UI_MEDIUM)
}

fn menu_bar(studio: &Studio) -> Element<'_, Message> {
    let mut bar = Row::new().height(MENU_HEIGHT).align_y(Alignment::Center).padding(Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 6.0 });
    for menu in Menu::ALL {
        let open = studio.menu == Some(menu);
        let head = button(menu_label(menu)).padding(MENU_PAD).style(theme::choice(open)).on_press(Message::MenuToggle(menu));
        let head: Element<'_, Message> = if studio.menu.is_some() { mouse_area(head).on_enter(Message::MenuHover(menu)).into() } else { head.into() };
        bar = bar.push(head);
    }
    container(bar).width(Length::Fill).style(theme::bar).into()
}

fn dropdown(studio: &Studio, menu: Menu) -> Element<'_, Message> {
    let mut lead = Row::new().padding(Padding { top: MENU_HEIGHT, right: 0.0, bottom: 0.0, left: 6.0 }).align_y(Alignment::Start);
    for earlier in Menu::ALL.iter().take_while(|m| **m != menu) {
        let ghost = menu_label(*earlier).style(|_| text::Style { color: Some(Color::TRANSPARENT) });
        lead = lead.push(container(ghost).padding(MENU_PAD));
    }
    let mut list = Column::new().width(DROPDOWN_WIDTH);
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
            text(mark).size(theme::META).style(theme::accent).width(14),
            text(command.name).size(theme::UI).style(label),
            space::horizontal(),
            text(command.keys).size(theme::FINE).style(theme::faint),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        list = list.push(button(content).padding(theme::ITEM).width(Length::Fill).style(theme::row(false)).on_press_maybe(enabled.then_some(Message::MenuPick(index))));
    }
    let panel = container(list.padding(4)).style(theme::popup);
    let placed = lead.push(panel).width(Length::Fill).height(Length::Fill);
    let backdrop = mouse_area(container(space::horizontal()).width(Length::Fill).height(Length::Fill)).on_press(Message::MenuClose);
    stack![backdrop, placed].into()
}

fn palette_layer<'a>(studio: &'a Studio, palette: &'a Palette) -> Element<'a, Message> {
    let found = commands::matching(studio, &palette.query);
    let input = text_input("Command, or :line", &palette.query)
        .id(palette_id())
        .on_input(Message::PaletteQuery)
        .on_submit(Message::PaletteRun(None))
        .size(theme::UI)
        .padding(theme::FIELD_PAD)
        .style(theme::field)
        .width(Length::Fill);
    let mut list = Column::new().width(Length::Fill);
    for (index, command) in found.iter().enumerate().take(12) {
        let label: TextStyle = if command.message.is_some() { theme::strong } else { theme::faint };
        let content = row![
            text(command.name.clone()).size(theme::UI).style(label),
            space::horizontal(),
            text(command.keys).size(theme::FINE).style(theme::faint),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let press = command.message.is_some().then_some(Message::PaletteRun(Some(index)));
        list = list.push(button(content).padding(theme::ITEM).width(Length::Fill).style(theme::row(index == palette.selected)).on_press_maybe(press));
    }
    if found.is_empty() {
        list = list.push(container(text("Nothing matches").size(theme::UI).style(theme::muted)).padding(theme::ITEM));
    }
    let panel = container(column![input, list].spacing(6).padding(8)).width(PALETTE_WIDTH).style(theme::popup);
    let placed = container(panel).padding(Padding { top: 72.0, right: 0.0, bottom: 0.0, left: 0.0 }).width(Length::Fill).height(Length::Fill).center_x(Length::Fill);
    let backdrop = mouse_area(container(space::horizontal()).width(Length::Fill).height(Length::Fill)).on_press(Message::PaletteClose);
    stack![backdrop, placed].into()
}

fn line<'a>() -> Element<'a, Message> {
    rule::horizontal(1).style(theme::separator).into()
}

fn strip<'a>(tabs: Vec<TabItem>, controls: Option<Element<'a, Message>>) -> Element<'a, Message> {
    let mut labels = Row::new().height(30).align_y(Alignment::Center);
    for tab in tabs {
        let mut label = row![text(tab.label).size(theme::UI).font(fonts::UI_MEDIUM)].spacing(8).align_y(Alignment::Center);
        if let Some(detail) = tab.detail {
            label = label.push(text(detail).size(theme::META).style(theme::muted));
        }
        let closable = tab.close.is_some();
        let pick = button(container(label).height(Length::Fill).align_y(Alignment::Center))
            .padding(Padding { top: 0.0, right: if closable { 4.0 } else { 10.0 }, bottom: 0.0, left: 10.0 })
            .height(Length::Fill)
            .style(theme::tab_button(tab.active))
            .on_press_maybe(tab.select);
        let mut inner = Row::new().align_y(Alignment::Center).height(Length::Fill).push(pick);
        if let Some(close) = tab.close {
            let cross = button(icon(Glyph::Close, Ink::Muted)).padding([0, 4]).style(theme::ghost).on_press(close);
            inner = inner.push(container(cross).padding(Padding { top: 0.0, right: 4.0, bottom: 0.0, left: 0.0 }));
        }
        labels = labels.push(container(inner).height(Length::Fill).style(theme::tab_shell(tab.active)));
    }
    let hidden = Scrollbar::new().width(0).scroller_width(0).margin(0);
    let tabs = scrollable(labels).direction(Direction::Horizontal(hidden)).width(Length::Fill).height(30).style(theme::scroll);
    let mut bar = Row::new().height(30).align_y(Alignment::Center).push(tabs);
    if let Some(controls) = controls {
        bar = bar.push(container(controls).padding([0, 6]));
    }
    column![bar, line()].into()
}

fn small<'a>(label: &'a str, message: Option<Message>) -> Element<'a, Message> {
    button(text(label).font(fonts::UI_MEDIUM).size(theme::META)).padding(theme::CONTROL).style(theme::ghost).on_press_maybe(message).into()
}

fn glyph_button<'a>(glyph: Glyph, message: Option<Message>) -> Element<'a, Message> {
    button(icon(glyph, Ink::Muted)).padding([3, 3]).style(theme::ghost).on_press_maybe(message).into()
}

fn tip<'a>(content: impl Into<Element<'a, Message>>, hint: &'a str) -> Element<'a, Message> {
    let bubble = container(text(hint).size(theme::META)).padding(theme::FIELD_PAD).style(theme::tip);
    tooltip(content, bubble, tooltip::Position::Bottom).gap(4).into()
}

fn tool<'a>(glyph: Glyph, ink: Ink, label: &'a str, hint: &'a str, message: Option<Message>) -> Element<'a, Message> {
    let ink = if message.is_none() { Ink::Muted } else { ink };
    let content = row![icon(glyph, ink), text(label).size(theme::UI).font(fonts::UI_MEDIUM)].spacing(6).align_y(Alignment::Center);
    tip(button(content).padding(theme::TOOL).style(theme::tool).on_press_maybe(message), hint)
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
    container(items).padding(theme::FIELD_PAD).width(Length::Fill).style(theme::bar).into()
}

fn status_bar(studio: &Studio) -> Element<'_, Message> {
    let (line, column) = studio.cursor();
    let status_style: TextStyle = if studio.status.starts_with("Error") {
        theme::alarm
    } else if studio.running {
        theme::accent
    } else {
        theme::muted
    };
    let path = studio.doc().path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    let mut bar = row![text(&studio.status).size(theme::FINE).style(status_style), space::horizontal()]
        .spacing(12)
        .align_y(Alignment::Center);
    let count = studio.doc().problems.len();
    if let Some(problem) = studio.problem_here() {
        bar = bar.push(text(problem.message.clone()).size(theme::FINE).style(theme::alarm));
    } else if count > 0 {
        let label = if count == 1 { "1 problem, F8 to jump".to_string() } else { format!("{count} problems, F8 to jump") };
        bar = bar.push(text(label).size(theme::FINE).style(theme::warn));
    }
    bar = bar.push(text(path).size(theme::FINE).style(theme::faint)).push(text(format!("Ln {line}, Col {column}")).size(theme::FINE).style(theme::muted));
    container(bar).padding(theme::ROW).width(Length::Fill).style(theme::bar).into()
}

fn editor_tabs(studio: &Studio) -> Vec<TabItem> {
    let mut tabs: Vec<TabItem> = Vec::new();
    if studio.start_open {
        tabs.push(TabItem {
            label: "Start".to_string(),
            detail: None,
            active: studio.tab == Tab::Start,
            select: Some(Message::SelectTab(Tab::Start)),
            close: Some(Message::CloseTab(Tab::Start)),
        });
    }
    tabs.extend(studio.docs.iter().enumerate().map(|(i, doc)| TabItem {
        label: doc.label(),
        detail: if doc.stale {
            Some("changed on disk".to_string())
        } else {
            doc.notebook.as_ref().map(|_| "notebook".to_string())
        },
        active: studio.tab == Tab::Doc(i),
        select: Some(Message::SelectTab(Tab::Doc(i))),
        close: Some(Message::CloseTab(Tab::Doc(i))),
    }));
    if studio.help.is_some() {
        tabs.push(TabItem {
            label: "Help".to_string(),
            detail: None,
            active: studio.tab == Tab::Help,
            select: Some(Message::SelectTab(Tab::Help)),
            close: Some(Message::CloseTab(Tab::Help)),
        });
    }
    if studio.settings_open {
        tabs.push(TabItem {
            label: "Settings".to_string(),
            detail: None,
            active: studio.tab == Tab::Settings,
            select: Some(Message::SelectTab(Tab::Settings)),
            close: Some(Message::CloseTab(Tab::Settings)),
        });
    }
    if let Some(diff) = &studio.diff {
        tabs.push(TabItem {
            label: "Diff".to_string(),
            detail: Some(diff.title.clone()),
            active: studio.tab == Tab::Diff,
            select: Some(Message::SelectTab(Tab::Diff)),
            close: Some(Message::CloseTab(Tab::Diff)),
        });
    }
    tabs
}

fn editor_controls(studio: &Studio) -> Element<'_, Message> {
    let help = (studio.tab != Tab::Help).then_some(Message::OpenHelp);
    let mut controls = Row::new().spacing(2);
    if studio.doc().stale && matches!(studio.tab, Tab::Doc(_)) {
        controls = controls.push(small("Reload", Some(Message::Reload)));
    }
    controls.push(small("+", Some(Message::NewFile))).push(small("Help", help)).into()
}

fn folder_name(path: &Path) -> String {
    path.file_name().and_then(|n| n.to_str()).map_or_else(|| path.display().to_string(), str::to_string)
}

fn side_tabs(studio: &Studio) -> Vec<TabItem> {
    let tab = |label: &str, side: Side| TabItem {
        label: label.to_string(),
        detail: None,
        active: studio.side == side,
        select: Some(Message::ShowSide(side)),
        close: None,
    };
    vec![
        tab("Environment", Side::Environment),
        tab("Files", Side::Files),
        tab("Project", Side::Project),
        tab("Outline", Side::Outline),
        tab("Watch", Side::Watch),
        tab("Git", Side::Git),
    ]
}

fn git_tab(studio: &Studio) -> Element<'_, Message> {
    let state = &studio.git;
    let Some(status) = state.status.as_ref().filter(|_| state.root.is_some()) else {
        let can_init = studio.root.is_some() && !state.busy;
        let note = state.error.as_deref().unwrap_or("Not a git repository.");
        return column![hint(note), container(small("Initialize repository", can_init.then_some(Message::GitInit))).padding(theme::GLYPH_PAD)].into();
    };
    let busy = state.busy;
    let mut position = Vec::new();
    if status.ahead > 0 {
        position.push(format!("{} ahead", status.ahead));
    }
    if status.behind > 0 {
        position.push(format!("{} behind", status.behind));
    }
    let header = row![
        text(&status.branch).size(theme::UI).font(fonts::UI_MEDIUM),
        text(position.join(", ")).size(theme::FINE).style(theme::muted),
        space::horizontal(),
        small("Pull", (!busy && status.upstream.is_some()).then_some(Message::GitPull)),
        small("Push", (!busy).then_some(Message::GitPush)),
        small("Refresh", (!busy).then_some(Message::GitRefresh)),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .padding(HEADER);
    let mut list = Column::new().push(header).push(line()).width(Length::Fill);
    if let Some(error) = &state.error {
        list = list.push(container(text(error).size(theme::META).style(theme::alarm)).padding(theme::HEAD));
    }
    let staged: Vec<(&git::Entry, Change)> = status.staged().collect();
    let unstaged: Vec<(&git::Entry, Change)> = status.unstaged().collect();
    let commit_ready = !busy && !staged.is_empty() && !state.message.trim().is_empty();
    let message = text_input("Commit message", &state.message)
        .on_input(Message::GitMessage)
        .on_submit(Message::GitCommit)
        .size(theme::UI)
        .padding(theme::FIELD_PAD)
        .style(theme::field)
        .width(Length::Fill);
    list = list.push(row![message, small("Commit", commit_ready.then_some(Message::GitCommit))].spacing(6).align_y(Alignment::Center).padding(theme::HEAD)).push(line());
    if staged.is_empty() && unstaged.is_empty() {
        list = list.push(hint("Nothing to commit."));
    }
    if !staged.is_empty() {
        list = list.push(section_row(format!("Staged {}", staged.len()), small("Unstage all", (!busy).then_some(Message::GitUnstageAll))));
        for (entry, change) in staged {
            list = list.push(change_row(entry, change, true, busy));
        }
    }
    if !unstaged.is_empty() {
        list = list.push(section_row(format!("Changes {}", unstaged.len()), small("Stage all", (!busy).then_some(Message::GitStageAll))));
        for (entry, change) in unstaged {
            list = list.push(change_row(entry, change, false, busy));
        }
    }
    if !state.log.is_empty() {
        list = list.push(section_row("Recent commits".to_string(), space::horizontal().into()));
        for commit in &state.log {
            let row = row![
                text(&commit.hash).font(fonts::MONO).size(theme::META).style(theme::faint).width(64),
                text(&commit.subject).size(theme::UI).style(theme::strong).width(Length::Fill),
                text(&commit.when).size(theme::FINE).style(theme::faint),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .padding(theme::ROW);
            list = list.push(row);
        }
    }
    scrollable(list.padding(Padding { top: 0.0, right: 0.0, bottom: 8.0, left: 0.0 })).height(Length::Fill).style(theme::scroll).into()
}

fn section_row<'a>(label: String, action: Element<'a, Message>) -> Element<'a, Message> {
    row![text(label).size(theme::FINE).font(fonts::UI_SEMIBOLD).style(theme::muted), space::horizontal(), action]
        .align_y(Alignment::Center)
        .padding(Padding { top: 9.0, right: 6.0, bottom: 2.0, left: 12.0 })
        .into()
}

fn change_row<'a>(entry: &'a git::Entry, change: Change, staged: bool, busy: bool) -> Element<'a, Message> {
    let mark: TextStyle = if staged { theme::accent } else { theme::warn };
    let letter = text(change.letter()).font(fonts::MONO).size(theme::META).style(mark).width(16);
    let name = text(&entry.path).font(fonts::MONO).size(theme::UI).style(theme::strong).width(Length::Fill);
    let pick = button(row![letter, name].spacing(6).align_y(Alignment::Center))
        .padding(theme::ROW)
        .width(Length::Fill)
        .style(theme::row(false))
        .on_press(Message::GitDiff(entry.path.clone(), staged, Some(change)));
    let mut actions = Row::new().align_y(Alignment::Center);
    if staged {
        actions = actions.push(tip(glyph_button(Glyph::Minus, (!busy).then_some(Message::GitUnstage(entry.path.clone()))), "Unstage"));
    } else {
        let discard = (!busy && change != Change::Conflict).then_some(Message::GitDiscard(entry.path.clone(), change));
        actions = actions
            .push(tip(glyph_button(Glyph::Plus, (!busy).then_some(Message::GitStage(entry.path.clone()))), "Stage"))
            .push(tip(glyph_button(Glyph::Close, discard), "Discard"));
    }
    row![pick, actions].align_y(Alignment::Center).padding(Padding { top: 0.0, right: 6.0, bottom: 0.0, left: 0.0 }).into()
}

fn diff_view<'a>(studio: &'a Studio, diff: &'a Diff) -> Element<'a, Message> {
    let mut header = row![text(&diff.title).font(fonts::MONO).size(theme::UI).style(theme::strong), space::horizontal()].spacing(6).align_y(Alignment::Center).padding(HEADER);
    if diff.proposal.is_some() {
        header = header.push(small("Apply", Some(Message::ApplyProposal))).push(small("Discard", Some(Message::DiscardProposal)));
    }
    header = header.push(small("Copy", Some(Message::CopyText(diff.text.clone()))));
    let size = studio.code_size();
    let lines = diff.text.lines().map(|shown| {
        let style: TextStyle = if shown.starts_with("+++") || shown.starts_with("---") || shown.starts_with("diff ") || shown.starts_with("index ") {
            theme::faint
        } else if shown.starts_with('+') {
            theme::good
        } else if shown.starts_with('-') {
            theme::alarm
        } else if shown.starts_with("@@") {
            theme::accent
        } else {
            theme::strong
        };
        text(shown).font(fonts::MONO).size(size).line_height(LineHeight::Absolute(Pixels(studio.line_height()))).style(style).wrapping(text::Wrapping::None).into()
    });
    let body = Column::with_children(lines).padding(Padding { top: EDITOR_PAD, right: 16.0, bottom: EDITOR_PAD, left: 16.0 });
    let scroller = scrollable(body).direction(Direction::Both { vertical: Scrollbar::new(), horizontal: Scrollbar::new() }).width(Length::Fill).height(Length::Fill).style(theme::scroll);
    column![header, line(), scroller].into()
}

fn console_controls(studio: &Studio) -> Element<'_, Message> {
    let find: Element<'_, Message> = match &studio.console_find {
        Some(query) => {
            let field = text_input("Find in console", query)
                .id(console_find_id())
                .on_input(Message::ConsoleQuery)
                .size(theme::META)
                .padding(theme::CONTROL)
                .style(theme::field)
                .width(theme::FIELD_SMALL);
            row![field, glyph_button(Glyph::Close, Some(Message::ConsoleFind(false)))].spacing(2).align_y(Alignment::Center).into()
        }
        None => small("Find", Some(Message::ConsoleFind(true))),
    };
    let picked = studio.console_pick.and_then(|i| studio.console.get(i)).map(|line| Message::CopyText(line.text.trim_end_matches('\n').to_string()));
    row![small("Copy", picked), find, small("Clear", Some(Message::ClearConsole))].spacing(2).align_y(Alignment::Center).into()
}

fn outline(studio: &Studio) -> Element<'_, Message> {
    let items = forms::outline(&studio.doc().content.text());
    if items.is_empty() {
        return hint("No definitions.");
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
            text(item.kind.label()).size(theme::FINE).style(kind).width(44),
            text(item.label).font(fonts::MONO).size(theme::UI).style(theme::strong),
            space::horizontal(),
            text(format!("{}", item.line + 1)).size(theme::FINE).style(theme::faint),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        list = list.push(button(content).padding(theme::ROW).width(Length::Fill).style(theme::row(at == Some(i))).on_press(Message::JumpTo(item.offset)));
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
        .size(theme::UI)
        .padding(theme::FIELD_PAD)
        .style(theme::field)
        .width(Length::Fill);
    row![
        text(label).size(theme::META).style(theme::muted),
        field,
        small("OK", Some(Message::ConfirmNaming)),
        small("Cancel", Some(Message::CancelNaming)),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding(theme::HEAD)
    .into()
}

fn project(studio: &Studio) -> Element<'_, Message> {
    let mut top = Column::new().width(Length::Fill);
    if let Some(naming) = studio.naming.as_ref().filter(|n| n.kind == NamingKind::Project) {
        top = top.push(naming_row("Project name", "folder name", &naming.text)).push(line());
    }
    let Some(info) = &studio.project else {
        let mut column = top.push(hint("No project.scm here or above."));
        let mut actions = row![small("New project…", Some(Message::NewProject))].spacing(2);
        if studio.root.is_some() {
            actions = actions.push(small("Create a project here", Some(Message::CreateProject)));
        }
        column = column.push(container(actions).padding(theme::GLYPH_PAD));
        return column.into();
    };
    let header = row![
        text(&info.name).size(theme::UI).font(fonts::UI_MEDIUM),
        text(&info.version).size(theme::META).style(theme::muted),
        space::horizontal(),
        project_controls(studio),
        small("Open main", Some(Message::PickEntry(info.main.clone()))),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding(HEADER);
    let mut list = top.push(header).push(line());
    if let Some(busy) = &studio.busy {
        list = list.push(container(text(format!("{busy}…")).size(theme::META).style(theme::accent)).padding(theme::HEAD));
    }
    if let Some(naming) = studio.naming.as_ref().filter(|n| n.kind == NamingKind::Package) {
        list = list.push(naming_row("Add", "name, name@version, or name ../path", &naming.text)).push(line());
    }
    if info.dependencies.is_empty() {
        return list.push(hint("No packages.")).into();
    }
    let heading = |label: &'static str| text(label).size(theme::FINE).font(fonts::UI_MEDIUM).style(theme::muted);
    let columns = row![
        heading("Package").width(Length::FillPortion(3)),
        heading("Version").width(56),
        heading("License").width(Length::FillPortion(2)),
        heading("Status").width(Length::FillPortion(2)),
    ]
    .spacing(10)
    .padding(theme::HEAD);
    list = list.push(columns).push(line());
    for dependency in &info.dependencies {
        let style: TextStyle = match dependency.status {
            schematic::project::Status::Vendored | schematic::project::Status::Local => theme::fine,
            schematic::project::Status::Changed | schematic::project::Status::Unlocked => theme::warn,
            schematic::project::Status::Missing => theme::alarm,
        };
        let mut name = row![text(&dependency.name).font(fonts::MONO).size(theme::UI).style(theme::strong)].spacing(6).align_y(Alignment::Center);
        if !dependency.direct {
            name = name.push(text(format!("via {}", dependency.required_by.join(", "))).size(theme::FINE).style(theme::faint));
        }
        let (license, license_style): (&str, TextStyle) = match &dependency.license {
            Some(license) => (license, theme::muted),
            None => ("no license", theme::faint),
        };
        let content = row![
            container(name).width(Length::FillPortion(3)),
            text(dependency.version.clone().unwrap_or_default()).size(theme::META).style(theme::muted).width(56),
            text(license).size(theme::META).style(license_style).width(Length::FillPortion(2)),
            text(dependency.status.label()).size(theme::META).style(style).width(Length::FillPortion(2)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let picked = studio.picked_package.as_deref() == Some(dependency.name.as_str());
        list = list.push(
            button(content).padding(theme::ROW).width(Length::Fill).style(theme::row(picked)).on_press(Message::PickPackage(dependency.name.clone())),
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
        let open = container(small("Open a folder…", Some(Message::OpenFolder))).padding(theme::GLYPH_PAD);
        return column![hint("No folder open."), open].into();
    };
    let header = row![
        text(folder_name(root)).size(theme::UI).font(fonts::UI_MEDIUM),
        space::horizontal(),
        file_controls(studio),
        small("Refresh", Some(Message::RefreshFiles)),
        small("Change…", Some(Message::OpenFolder)),
    ]
    .spacing(2)
    .align_y(Alignment::Center)
    .padding(HEADER);
    let mut list = Column::new().push(header).push(line()).width(Length::Fill);
    if let Some(naming) = studio.naming.as_ref().filter(|n| matches!(n.kind, NamingKind::File | NamingKind::Folder | NamingKind::Rename(_))) {
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
        let mut content = row![text(chevron).size(theme::FINE).style(theme::faint).width(12), text(&entry.name).size(theme::UI).style(style)]
            .spacing(4)
            .align_y(Alignment::Center);
        if let Some((change, staged)) = studio.badge(&entry.path) {
            let mark: TextStyle = if staged { theme::accent } else { theme::warn };
            let letter = if entry.dir { "•" } else { change.letter() };
            content = content.push(space::horizontal()).push(text(letter).font(fonts::MONO).size(theme::FINE).style(mark));
        }
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
    let suggesting = studio.assistant.suggestion.is_some();
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
            if suggesting && !command && !shift {
                return Some(Binding::Custom(Message::AcceptSuggestion));
            }
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
        (Tab::Settings, _) if studio.settings_open => settings_page(studio),
        (Tab::Start, _) if studio.start_open => start_page(studio),
        (Tab::Diff, _) if studio.diff.is_some() => diff_view(studio, studio.diff.as_ref().expect("a diff")),
        _ => match (&studio.doc().notebook, &studio.find) {
            (Some(notebook), _) => notebook_view(studio, notebook),
            (None, Some(find)) => column![find_bar(studio, find), line(), editor(studio)].into(),
            (None, None) => editor(studio),
        },
    }
}

fn notebook_view<'a>(studio: &'a Studio, notebook: &'a Notebook) -> Element<'a, Message> {
    let ready = studio.worker.is_some();
    let count = notebook.cells.len();
    let header = row![
        small("Run all", ready.then_some(Message::RunAllCells)),
        small("Clear outputs", Some(Message::ClearOutputs)),
        small("+ Code", Some(Message::AddCell(count, Kind::Code))),
        small("+ Text", Some(Message::AddCell(count, Kind::Text))),
        space::horizontal(),
        small("Export…", Some(Message::ExportNotebook)),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .padding(HEADER);
    let cells = notebook.cells.iter().enumerate().map(|(i, cell)| cell_view(studio, cell, i, i == notebook.focused, count));
    let list = Column::with_children(cells).spacing(10).padding(Padding { top: 10.0, right: 16.0, bottom: 24.0, left: 16.0 }).width(Length::Fill);
    column![header, line(), scrollable(list).width(Length::Fill).height(Length::Fill).style(theme::scroll)].into()
}

fn cell_view<'a>(studio: &'a Studio, cell: &'a Cell, i: usize, focused: bool, count: usize) -> Element<'a, Message> {
    let ready = studio.worker.is_some();
    let code = cell.kind == Kind::Code;
    let mut tools = Row::new().spacing(2).align_y(Alignment::Center).push(space::horizontal());
    if cell.running {
        tools = tools.push(text("running").size(theme::FINE).style(theme::accent));
    } else if let Some(ms) = cell.elapsed {
        tools = tools.push(text(duration(ms)).size(theme::FINE).style(theme::faint));
    }
    tools = if code {
        tools.push(tip(glyph_button(Glyph::Play, (ready && !cell.running).then_some(Message::RunCell(i))), "Run"))
    } else {
        tools.push(small(if cell.editing { "Done" } else { "Edit" }, Some(if cell.editing { Message::DoneText(i) } else { Message::EditText(i) })))
    };
    tools = tools
        .push(small(if code { "As text" } else { "As code" }, Some(Message::ToggleCellKind(i))))
        .push(glyph_button(Glyph::Up, (i > 0).then_some(Message::MoveCell(i, -1))))
        .push(glyph_button(Glyph::Down, (i + 1 < count).then_some(Message::MoveCell(i, 1))))
        .push(glyph_button(Glyph::Close, (count > 1).then_some(Message::DeleteCell(i))))
        .push(small("+", Some(Message::AddCell(i + 1, Kind::Code))));
    let body: Element<'a, Message> = match (cell.kind, cell.editing) {
        (Kind::Code, _) => text_editor(&cell.content)
            .on_action(move |action| Message::CellEdit(i, action))
            .font(fonts::MONO)
            .size(studio.code_size())
            .line_height(LineHeight::Absolute(Pixels(studio.line_height())))
            .padding(Padding { top: 6.0, right: 10.0, bottom: 6.0, left: 10.0 })
            .height(Length::Shrink)
            .wrapping(if studio.wrap { text::Wrapping::Word } else { text::Wrapping::None })
            .style(theme::editor)
            .highlight_with::<highlight::Schematic>(cell.marks.clone(), highlight::format)
            .key_binding(cell_binding(cell, i))
            .into(),
        (Kind::Text, true) => text_editor(&cell.content)
            .on_action(move |action| Message::CellEdit(i, action))
            .size(theme::UI)
            .padding(Padding { top: 6.0, right: 10.0, bottom: 6.0, left: 10.0 })
            .height(Length::Shrink)
            .wrapping(text::Wrapping::Word)
            .style(theme::editor)
            .key_binding(text_cell_binding(i))
            .into(),
        (Kind::Text, false) => {
            let source = cell.content.text();
            let blocks: Vec<Element<'a, Message>> = docs::blocks(&source).into_iter().map(|block| render(block, &studio.theme)).collect();
            let rendered: Element<'a, Message> = if blocks.is_empty() {
                text("Empty").size(theme::META).style(theme::faint).into()
            } else {
                Column::with_children(blocks).spacing(6).into()
            };
            mouse_area(container(rendered).width(Length::Fill).padding([4, 2])).on_press(Message::EditText(i)).into()
        }
    };
    let mut column = column![tools, body].spacing(6);
    for output in &cell.outputs {
        column = column.push(output_view(studio, output));
    }
    let rule = container(space::vertical()).width(2).height(Length::Fill).style(theme::cell_rule(focused));
    row![rule, container(column).padding(Padding { top: 2.0, right: 10.0, bottom: 6.0, left: 10.0 }).width(Length::Fill)].into()
}

fn output_view<'a>(studio: &'a Studio, output: &'a Output) -> Element<'a, Message> {
    let size = studio.console_size();
    match output {
        Output::Printed(shown) => text(shown.trim_end_matches('\n')).font(fonts::MONO).size(size).style(theme::strong).into(),
        Output::Result(shown) => code(shown, &studio.theme, size),
        Output::Error(shown) => text(shown).font(fonts::MONO).size(size).style(theme::alarm).into(),
        Output::Chart(plot) => canvas(Chart { plot, view: PlotView::default() }).width(Length::Fill).height(260).into(),
        Output::Picture { description, .. } => text(format!("{description} (run to draw)")).size(theme::META).style(theme::faint).into(),
    }
}

fn cell_binding(cell: &Cell, i: usize) -> impl Fn(KeyPress) -> Option<Binding<Message>> + '_ {
    move |press| {
        let command = press.modifiers.command();
        let shift = press.modifiers.shift();
        if matches!(press.key, Key::Named(key::Named::Enter)) {
            if command && shift {
                return Some(Binding::Custom(Message::RunAllCells));
            }
            if command {
                return Some(Binding::Custom(Message::RunCell(i)));
            }
            if shift {
                return Some(Binding::Custom(Message::RunCellAndAdvance(i)));
            }
        }
        if !matches!(press.status, Status::Focused { .. }) {
            return Binding::from_key_press(press);
        }
        if matches!(press.key, Key::Named(key::Named::Tab)) {
            return Some(Binding::Sequence(vec![Binding::Insert(' '), Binding::Insert(' ')]));
        }
        let plain_enter = matches!(press.key, Key::Named(key::Named::Enter)) && !command && !shift;
        if plain_enter || matches!(press.key, Key::Character(_) | Key::Named(key::Named::Backspace)) {
            let text = cell.content.text();
            let position = cell.content.cursor().position;
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

fn text_cell_binding(i: usize) -> impl Fn(KeyPress) -> Option<Binding<Message>> {
    move |press| match &press.key {
        Key::Named(key::Named::Escape) => Some(Binding::Custom(Message::DoneText(i))),
        Key::Named(key::Named::Enter) if press.modifiers.command() => Some(Binding::Custom(Message::DoneText(i))),
        _ => Binding::from_key_press(press),
    }
}

fn start_page(studio: &Studio) -> Element<'_, Message> {
    let action = |label: &'static str, message: Message| button(text(label).size(theme::UI).font(fonts::UI_MEDIUM)).padding(theme::TOOL).style(theme::tool).on_press(message);
    let actions = row![
        action("New file", Message::NewFile),
        action("Open file…", Message::Open),
        action("Open folder…", Message::OpenFolder),
        action("New project…", Message::NewProject),
    ]
    .spacing(6);
    let entry = |path: &Path, message: Message| -> Element<'_, Message> {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
        let place = path.parent().map(|p| p.display().to_string()).unwrap_or_default();
        let content = row![text(name).font(fonts::MONO).size(theme::UI).style(theme::strong), text(place).size(theme::FINE).style(theme::faint)]
            .spacing(10)
            .align_y(Alignment::Center);
        button(content).padding(theme::ROW).width(Length::Fill).style(theme::row(false)).on_press(message).into()
    };
    let mut page = column![text("Schematic Studio").size(theme::GLYPH + 4.0).font(fonts::UI_SEMIBOLD), actions].spacing(14).padding(Padding { top: 16.0, right: 20.0, bottom: 24.0, left: 20.0 }).max_width(680);
    let files: Vec<&PathBuf> = studio.recent.iter().filter(|p| p.is_file()).take(8).collect();
    if !files.is_empty() {
        let mut list = Column::new().push(heading("Recent files")).spacing(2);
        for path in files {
            list = list.push(entry(path, Message::OpenPath(path.clone())));
        }
        page = page.push(list);
    }
    let projects: Vec<&PathBuf> = studio.projects.iter().filter(|p| p.is_dir()).take(8).collect();
    if !projects.is_empty() {
        let mut list = Column::new().push(heading("Recent projects")).spacing(2);
        for path in projects {
            list = list.push(entry(path, Message::OpenProject(path.clone())));
        }
        page = page.push(list);
    }
    scrollable(page).width(Length::Fill).height(Length::Fill).style(theme::scroll).into()
}

fn setting<'a>(label: &'a str, note: &'a str, control: Element<'a, Message>) -> Element<'a, Message> {
    let mut words = Column::new().push(text(label).size(theme::UI).font(fonts::UI_MEDIUM)).spacing(2).width(Length::Fill);
    if !note.is_empty() {
        words = words.push(text(note).size(theme::FINE).style(theme::muted));
    }
    row![words, control].spacing(16).align_y(Alignment::Center).into()
}

fn toggle<'a>(on: bool, message: Message) -> Element<'a, Message> {
    button(text(if on { "On" } else { "Off" }).size(theme::META).font(fonts::UI_MEDIUM)).padding(theme::TOOL).style(theme::choice(on)).on_press(message).into()
}

fn heading<'a>(label: &'a str) -> Element<'a, Message> {
    container(text(label).size(theme::GLYPH).font(fonts::UI_SEMIBOLD)).padding(Padding { top: 10.0, right: 0.0, bottom: 0.0, left: 0.0 }).into()
}

fn settings_page(studio: &Studio) -> Element<'_, Message> {
    let current = theme::name_of(&studio.theme);
    let choice = |label: &'static str, name: &'static str| {
        button(text(label).size(theme::META).font(fonts::UI_MEDIUM)).padding(theme::TOOL).style(theme::choice(current == name)).on_press(Message::SetTheme(name.to_string()))
    };
    let percent = |value: f32| text(format!("{:.0}%", value * 100.0)).size(theme::UI).width(48).align_x(Alignment::Center);
    let themes = row![choice("Light", "light"), choice("Dark", "dark"), choice("Warm", "warm"), choice("High contrast", "contrast")].spacing(4);
    let scale = row![
        small("−", (studio.scale > 0.81).then_some(Message::ScaleBy(-1))),
        percent(studio.scale),
        small("+", (studio.scale < 1.59).then_some(Message::ScaleBy(1))),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let zoom = row![
        small("−", (studio.zoom > 0.71).then_some(Message::ZoomOut)),
        percent(studio.zoom),
        small("+", (studio.zoom < 1.79).then_some(Message::ZoomIn)),
        small("Reset", ((studio.zoom - 1.0).abs() > 0.01).then_some(Message::ZoomReset)),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let mut page = column![
        heading("Appearance"),
        setting("Theme", "", themes.into()),
        setting("Interface size", "", scale.into()),
        setting("Code size", "Ctrl+= and Ctrl+-", zoom.into()),
        heading("Editor"),
        setting("Word wrap", "", toggle(studio.wrap, Message::ToggleWrap)),
        heading("Files"),
        setting("Reopen files at start", "", toggle(studio.restore, Message::ToggleRestore)),
        setting("Keep backups", "Previous versions, kept when saving over a file", toggle(studio.backups, Message::ToggleBackups)),
    ]
    .spacing(14)
    .padding(Padding { top: 8.0, right: 20.0, bottom: 24.0, left: 20.0 })
    .max_width(680);
    let assistant = &studio.assistant;
    let key_note = if assistant.key.is_some() {
        "Set. ANTHROPIC_API_KEY takes precedence when it is set."
    } else {
        "Not set. Kept in credentials.scm beside the settings; ANTHROPIC_API_KEY takes precedence."
    };
    let key_row = row![
        text_input("Key", &assistant.key_field)
            .secure(true)
            .on_input(Message::KeyInput)
            .on_submit(Message::KeySave)
            .size(theme::UI)
            .padding(theme::FIELD_PAD)
            .style(theme::field)
            .width(theme::FIELD),
        small("Save", (!assistant.key_field.trim().is_empty()).then_some(Message::KeySave)),
        small("Remove", assistant.key.is_some().then_some(Message::KeyRemove)),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let models = Row::with_children(MODELS.iter().map(|model| {
        let label = text(model.name.trim_start_matches("Claude ")).size(theme::META).font(fonts::UI_MEDIUM);
        button(label).padding(theme::TOOL).style(theme::choice(assistant.model == model.id)).on_press(Message::SetModel(model.id.to_string())).into()
    }))
    .spacing(4);
    let efforts = Row::with_children(EFFORTS.iter().map(|effort| {
        let label = text(*effort).size(theme::META).font(fonts::UI_MEDIUM);
        button(label).padding(theme::TOOL).style(theme::choice(assistant.effort == *effort)).on_press(Message::SetEffort(effort.to_string())).into()
    }))
    .spacing(4);
    page = page
        .push(heading("Assistant"))
        .push(setting("API key", key_note, key_row.into()))
        .push(setting("Model", "", models.into()))
        .push(setting("Effort", "", efforts.into()))
        .push(setting("Ask before running code", "", toggle(assistant.ask_before_running, Message::ToggleAskBeforeRunning)))
        .push(setting("Attach table columns", "", toggle(assistant.attach_data, Message::ToggleAttachData)))
        .push(setting("Suggest completions", "Tab accepts, Esc dismisses", toggle(assistant.completions, Message::ToggleCompletions)));
    if studio.persistent
        && let Some(path) = settings::path() {
            page = page.push(text(format!("Kept in {}", path.display())).size(theme::FINE).style(theme::faint));
        }
    scrollable(page).width(Length::Fill).height(Length::Fill).style(theme::scroll).into()
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
        .size(theme::UI)
        .padding(theme::FIELD_PAD)
        .style(theme::field)
        .width(theme::FIELD);
    let case = button(text("Aa").size(theme::META).font(fonts::UI_MEDIUM)).padding(theme::CONTROL).style(theme::choice(find.case)).on_press(Message::FindCase);
    let mut first = row![
        query,
        text(where_at).size(theme::META).style(theme::muted).width(90),
        glyph_button(Glyph::Up, (count > 0).then_some(Message::FindStep(-1))),
        glyph_button(Glyph::Down, (count > 0).then_some(Message::FindStep(1))),
        tip(case, "Match case"),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if !find.replacing {
        first = first.push(small("Replace…", Some(Message::FindOpen(true))));
    }
    first = first.push(space::horizontal()).push(glyph_button(Glyph::Close, Some(Message::FindClose)));
    let mut bar = Column::new().push(first).spacing(6).padding(theme::BAR);
    if find.replacing {
        let replacement = text_input("Replace with", &find.replacement)
            .on_input(Message::FindReplacement)
            .on_submit(Message::FindReplace)
            .size(theme::UI)
            .padding(theme::FIELD_PAD)
            .style(theme::field)
            .width(theme::FIELD);
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
    let line_height = studio.line_height();
    let numbers = Column::with_children((0..lines).map(|i| {
        let flagged = doc.marks.problems.iter().any(|(line, _)| *line == i);
        let style: TextStyle = if flagged { theme::alarm } else if i == current { theme::strong } else { theme::faint };
        text(format!("{:>digits$}", i + 1))
            .font(fonts::MONO)
            .size(studio.code_size())
            .line_height(LineHeight::Absolute(Pixels(line_height)))
            .style(style)
            .into()
    }))
    .padding(Padding { top: EDITOR_PAD, right: 10.0, bottom: EDITOR_PAD, left: 14.0 });
    let code = text_editor(&doc.content)
        .id(editor_text_id())
        .on_action(Message::Edit)
        .font(fonts::MONO)
        .size(studio.code_size())
        .line_height(LineHeight::Absolute(Pixels(line_height)))
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
    let overlay = popup(studio, Source::Editor).or_else(|| studio.assistant.suggestion.as_deref().map(|suggestion| suggestion_popup(studio, suggestion)));
    let Some((popup, rows)) = overlay else { return view };
    let cursor = doc.content.cursor().position;
    let advance = CODE_ADVANCE * studio.zoom;
    let gutter = digits as f32 * advance + 24.0;
    let x = (gutter + 6.0 + cursor.column as f32 * advance).min((studio.editor_width - POPUP_WIDTH - 4.0).max(0.0));
    let height = rows as f32 * 24.0 + 8.0;
    let below = EDITOR_PAD + (cursor.line as f32 + 1.0) * line_height - doc.offset + 2.0;
    let y = if studio.editor_height > 0.0 && below + height > studio.editor_height {
        (below - line_height - height - 4.0).max(0.0)
    } else {
        below
    };
    let layer = container(popup).padding(Padding { top: y, right: 0.0, bottom: 0.0, left: x }).width(Length::Fill).height(Length::Fill);
    stack![view, layer].into()
}

const POPUP_WIDTH: f32 = 400.0;
const CODE_ADVANCE: f32 = 8.1;
const CONSOLE_ADVANCE: f32 = 7.8;

fn suggestion_popup<'a>(studio: &'a Studio, suggestion: &str) -> (Element<'a, Message>, usize) {
    let body = container(code(suggestion, &studio.theme, studio.code_size())).padding(6).max_width(POPUP_WIDTH + 160.0).style(theme::popup);
    (body.into(), suggestion.lines().count().max(1))
}

fn popup(studio: &Studio, source: Source) -> Option<(Element<'_, Message>, usize)> {
    let completion = studio.completion.as_ref().filter(|c| c.source == source)?;
    let rows = completion.items.iter().enumerate().map(|(i, item)| {
        let content = row![
            text(&item.name).font(fonts::MONO).size(theme::UI).style(theme::strong),
            text(&item.detail).size(theme::FINE).style(theme::muted).wrapping(text::Wrapping::None),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        button(content)
            .padding(theme::ROW)
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
        header.into_iter().enumerate().map(|(i, cell)| text(cell).size(theme::FINE).font(fonts::UI_SEMIBOLD).style(theme::muted).width(width(i)).into()),
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
        Block::Quote(text) => container(prose(&text, theme, 13.0)).padding(theme::HEAD).width(Length::Fill).style(theme::quote).into(),
        Block::List(items) => {
            let rows = items.into_iter().map(|item| {
                row![text("•").size(theme::CONSOLE).style(theme::muted).width(16), prose(&item, theme, theme::CONSOLE)].spacing(4).align_y(Alignment::Start).into()
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
        return hint("No matches.");
    }
    let mut column = Column::new().spacing(16).padding(Padding { top: 8.0, right: 20.0, bottom: 24.0, left: 20.0 }).max_width(760);
    for entry in found.into_iter().take(40) {
        let mut block = Column::new().spacing(4).push(text(entry.name.clone()).font(fonts::MONO_BOLD).size(theme::GLYPH).style(theme::accent));
        for found in &entry.uses {
            block = block
                .push(code(&found.signatures.join(", "), theme, 12.5))
                .push(prose(&found.description, theme, 12.5))
                .push(text(found.section.clone()).size(theme::FINE).style(theme::faint));
        }
        column = column.push(block);
    }
    column.into()
}

fn help(studio: &Studio) -> Element<'_, Message> {
    let Some(help) = &studio.help else {
        return hint("No page open.");
    };
    let dark = theme::is_dark(&studio.theme);
    let page = |label: &'static str, page: Page| {
        button(text(label).size(theme::META).font(fonts::UI_MEDIUM)).padding(theme::CONTROL).style(theme::choice(help.page == page)).on_press(Message::HelpPage(page))
    };
    let search = text_input("Search the reference", &help.query)
        .on_input(Message::HelpQuery)
        .size(theme::UI)
        .padding(theme::CONTROL)
        .style(theme::field)
        .width(Length::Fill);
    let bar = row![page("Guide", Page::Guide), page("Reference", Page::Reference), search].spacing(6).padding(theme::BAR).align_y(Alignment::Center);
    let body: Element<'_, Message> = if help.query.trim().is_empty() {
        let page = help.page;
        lazy((page, dark), move |_| document(page, dark)).into()
    } else {
        matches(&help.query, &studio.theme)
    };
    column![bar, line(), scrollable(body).width(Length::Fill).height(Length::Fill).style(theme::scroll)].into()
}

fn console(studio: &Studio) -> Element<'_, Message> {
    let size = studio.console_size();
    let query = studio.console_find.as_deref().map(str::to_lowercase).filter(|q| !q.is_empty());
    let picked = studio.console_pick;
    let shown = studio.console.iter().enumerate().filter(move |(_, line)| query.as_ref().is_none_or(|q| line.text.to_lowercase().contains(q)));
    let lines = shown.map(|(i, line)| {
        let (marker, marker_style, style): (&str, TextStyle, TextStyle) = match line.kind {
            LineKind::Input => ("›", theme::accent, theme::strong),
            LineKind::Output => ("", theme::faint, theme::strong),
            LineKind::Result => ("=", theme::faint, theme::strong),
            LineKind::Error => ("!", theme::alarm, theme::alarm),
            LineKind::Note => ("", theme::faint, theme::muted),
        };
        let content = line.text.trim_end_matches('\n');
        let body: Element<'_, Message> = match line.kind {
            LineKind::Input | LineKind::Result => code(content, &studio.theme, size),
            _ => text(content).font(fonts::MONO).size(size).style(style).width(Length::Fill).into(),
        };
        let row = row![container(text(marker).font(fonts::MONO_BOLD).size(size).style(marker_style)).width(14), body].spacing(8);
        button(row).padding([1, 4]).width(Length::Fill).style(theme::row(picked == Some(i))).on_press(Message::PickLine(i)).into()
    });
    let history = scrollable(column(lines).spacing(1).padding([8, 8]).width(Length::Fill))
        .id(console_id())
        .height(Length::Fill)
        .style(theme::scroll);
    let prompt = if studio.pending.is_empty() { "›" } else { "…" };
    let input = text_editor(&studio.input)
        .id(console_text_id())
        .on_action(Message::Input)
        .font(fonts::MONO)
        .size(size)
        .padding(Padding { top: 7.0, right: 0.0, bottom: 7.0, left: 0.0 })
        .style(theme::editor)
        .highlight_with::<highlight::Schematic>(studio.input_marks.clone(), highlight::format)
        .key_binding(console_binding(studio));
    let marker = container(text(prompt).font(fonts::MONO_BOLD).size(size).style(theme::accent))
        .width(14)
        .padding(Padding { top: 7.0, right: 0.0, bottom: 7.0, left: 0.0 });
    let entry = row![marker, input].spacing(8).padding([0, 12]);
    let body: Element<'_, Message> = column![history, line(), entry].into();
    let Some((popup, _)) = popup(studio, Source::Console) else { return body };
    let cursor = studio.input.cursor().position;
    let x = 34.0 + cursor.column as f32 * CONSOLE_ADVANCE * studio.zoom;
    let bottom = 16.0 + studio.input.line_count().max(1) as f32 * 18.0 * studio.zoom;
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
        return hint("Nothing defined.");
    }
    let selected = studio.grid.as_ref().map(|g| g.name.as_str());
    let heading = |label: &'static str| text(label).size(theme::FINE).font(fonts::UI_MEDIUM).style(theme::muted);
    let mut list = Column::new().width(Length::Fill);
    if let Some(binding) = selected.and_then(|name| studio.bindings.iter().find(|b| b.name == name)) {
        let table = binding.kind == "table";
        let actions = row![
            text(&binding.name).font(fonts::MONO).size(theme::UI).style(theme::strong),
            space::horizontal(),
            small("Describe", (table && studio.worker.is_some()).then_some(Message::DescribeBinding(binding.name.clone()))),
            small("Watch", Some(Message::WatchBinding(binding.name.clone()))),
            small("Copy name", Some(Message::CopyText(binding.name.clone()))),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .padding(HEADER);
        list = list.push(actions).push(line());
    }
    let header = row![
        heading("Name").width(Length::FillPortion(2)),
        heading("Type").width(72),
        heading("Value").width(Length::FillPortion(3)),
    ]
    .spacing(10)
    .padding(theme::HEAD);
    list = list.push(header).push(line());
    for (group, members) in groups {
        let label = text(group.label()).size(theme::FINE).font(fonts::UI_SEMIBOLD).style(theme::muted);
        list = list.push(container(label).padding(Padding { top: 9.0, right: 12.0, bottom: 2.0, left: 12.0 }));
        for binding in members {
            let content = row![
                text(&binding.name).font(fonts::MONO).size(theme::UI).style(theme::strong).width(Length::FillPortion(2)),
                text(&binding.kind).size(theme::META).style(theme::muted).width(72),
                text(&binding.summary).size(theme::META).style(theme::muted).width(Length::FillPortion(3)),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            let chosen = selected == Some(binding.name.as_str());
            list = list.push(
                button(content)
                    .padding(theme::ROW)
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
    let label = text(format!("{first}–{last} of {}", grid.total)).size(theme::META).style(theme::muted);
    row![
        label,
        glyph_button(Glyph::Left, (grid.offset > 0).then_some(Message::Page(-1))),
        glyph_button(Glyph::Right, (grid.offset + PAGE < grid.total).then_some(Message::Page(1))),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn data_header<'a>(studio: &'a Studio, grid: &'a Grid) -> Element<'a, Message> {
    let filter = text_input("Filter rows", &studio.grid_filter).on_input(Message::GridFilter).size(theme::META).padding(theme::CONTROL).style(theme::field).width(theme::FIELD_SMALL);
    let copy = small("Copy", (!grid.rows.is_empty()).then_some(Message::CopyRows));
    let csv = small("CSV…", studio.table_shown().then_some(Message::ExportCsv));
    let sorted = grid.sort.as_ref().map(|(column, _)| column.clone()).filter(|column| {
        grid.columns.iter().position(|c| c == column).and_then(|c| grid.numeric.get(c).copied()).unwrap_or(false)
    });
    let histogram = small("Histogram", sorted.map(Message::PlotColumn));
    row![filter, tip(copy, "Copy rows"), csv, histogram, space::horizontal(), pager(grid)]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding(HEADER)
        .into()
}

fn viewer(studio: &Studio) -> Element<'_, Message> {
    let Some(grid) = &studio.grid else {
        return hint("Nothing selected.");
    };
    column![data_header(studio, grid), line(), grid_table(studio, grid)].into()
}

fn grid_table<'a>(studio: &'a Studio, grid: &'a Grid) -> Element<'a, Message> {
    let index = grid.columns.first().is_some_and(|c| c == "#");
    let selected = &studio.selected_rows;
    let columns = grid.columns.iter().enumerate().map(|(c, name)| {
        let numeric = grid.numeric.get(c).copied().unwrap_or(false);
        let sortable = !(index && c == 0);
        let sorted = grid.sort.as_ref().filter(|(column, _)| column == name).map(|(_, descending)| *descending);
        let arrow = match sorted {
            Some(false) => " ▲",
            Some(true) => " ▼",
            None => "",
        };
        let label_style: TextStyle = if sorted.is_some() { theme::accent } else { theme::muted };
        let label = text(format!("{name}{arrow}")).font(fonts::UI_SEMIBOLD).size(theme::FINE).style(label_style);
        let head = button(label).padding(theme::CELL).style(theme::ghost).on_press_maybe(sortable.then(|| Message::SortBy(name.clone())));
        let stats = grid.stats.get(c).map(String::as_str).unwrap_or_default();
        let header: Element<'_, Message> = if stats.is_empty() { head.into() } else { tip(head, stats) };
        let longest = grid.rows.iter().map(|row| row[c].chars().count()).max().unwrap_or(0).max(name.chars().count() + 2);
        let width = (longest as f32 * 7.6 + 24.0).clamp(56.0, 360.0);
        let style: TextStyle = if index && c == 0 { theme::faint } else { theme::strong };
        let column = table::column(header, move |r: usize| {
            let cell = text(&grid.rows[r][c]).font(fonts::MONO).size(theme::UI).style(style);
            button(cell).padding(theme::CELL).style(theme::row(selected.contains(&r))).on_press(Message::SelectRow(r))
        })
        .width(width);
        if numeric { column.align_x(Alignment::End) } else { column }
    });
    let body = table(columns, 0..grid.rows.len()).padding_x(6).padding_y(2);
    let both = Direction::Both { vertical: Scrollbar::default(), horizontal: Scrollbar::default() };
    scrollable(body).direction(both).width(Length::Fill).height(Length::Fill).style(theme::scroll).into()
}

fn watch_controls(_studio: &Studio) -> Element<'_, Message> {
    small("+ Watch", Some(Message::StartNaming(NamingKind::Watch)))
}

fn watch(studio: &Studio) -> Element<'_, Message> {
    let mut list = Column::new().width(Length::Fill);
    if let Some(naming) = studio.naming.as_ref().filter(|n| n.kind == NamingKind::Watch) {
        list = list.push(naming_row("Watch", "expression", &naming.text)).push(line());
    }
    if studio.watches.is_empty() {
        return list.push(hint("No watches.")).into();
    }
    for (i, source) in studio.watches.iter().enumerate() {
        let found = studio.watched.iter().find(|w| w.source == *source);
        let value: Element<'_, Message> = match found {
            Some(Watched { value: Ok(shown), .. }) => code(shown, &studio.theme, 12.5),
            Some(Watched { value: Err(error), .. }) => text(error).size(theme::META).style(theme::alarm).into(),
            None => text("…").size(theme::META).style(theme::faint).into(),
        };
        let content = row![
            container(code(source, &studio.theme, 12.5)).width(Length::FillPortion(2)),
            container(value).width(Length::FillPortion(3)),
            glyph_button(Glyph::Close, Some(Message::RemoveWatch(i))),
        ]
        .spacing(10)
        .align_y(Alignment::Start)
        .padding(Padding { top: 4.0, right: 6.0, bottom: 4.0, left: 12.0 });
        list = list.push(content).push(line());
    }
    scrollable(list).height(Length::Fill).style(theme::scroll).into()
}

fn plot_header(studio: &Studio) -> Element<'_, Message> {
    let count = studio.plots.len();
    let mut bar = Row::new().spacing(6).align_y(Alignment::Center).padding(HEADER);
    if count >= 2 {
        bar = bar
            .push(glyph_button(Glyph::Left, (studio.plot_at > 0).then_some(Message::PlotBack)))
            .push(text(format!("{} of {count}", studio.plot_at + 1)).size(theme::META).style(theme::muted))
            .push(glyph_button(Glyph::Right, (studio.plot_at + 1 < count).then_some(Message::PlotForward)));
    }
    bar.push(space::horizontal())
        .push(small("Reset view", (!studio.plot_view.is_fresh()).then_some(Message::PlotReset)))
        .push(small("Copy SVG", Some(Message::CopyPlot)))
        .push(small("Save as SVG", Some(Message::SavePlot)))
        .into()
}

fn plots(studio: &Studio) -> Element<'_, Message> {
    let Some(plot) = studio.plot() else { return hint("No charts.") };
    let chart: Element<'_, Message> = canvas(Chart { plot, view: studio.plot_view.clone() }).width(Length::Fill).height(Length::Fill).into();
    let mut body = column![plot_header(studio), line(), chart];
    if studio.plots.len() >= 2 {
        let thumbs = studio.plots.iter().enumerate().map(|(i, plot)| canvas(Thumb { plot, index: i, selected: i == studio.plot_at }).width(140).height(80).into());
        let strip = scrollable(row(thumbs).spacing(6).padding([6, 12])).direction(Direction::Horizontal(Scrollbar::new())).style(theme::scroll);
        body = body.push(line()).push(strip);
    }
    body.into()
}

fn hint<'a>(message: &'a str) -> Element<'a, Message> {
    container(text(message).size(theme::UI).style(theme::faint)).padding(12).into()
}
