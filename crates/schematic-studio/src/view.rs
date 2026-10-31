use iced::keyboard::{Key, key};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text_editor::{Binding, KeyPress, Status};
use iced::widget::{
    Column, button, column, container, pane_grid, row, rule, scrollable, space, table, text, text_editor, tooltip,
};
use iced::{Alignment, Element, Font, Length, Padding, Theme};

use crate::app::{LineKind, Message, PAGE, PaneKind, Studio, console_id};
use crate::theme::Tint;
use crate::worker::Grid;
use crate::{fonts, highlight, theme};

type TextStyle = fn(&Theme) -> text::Style;
type ButtonStyle = fn(&Theme, button::Status) -> button::Style;

pub fn view(studio: &Studio) -> Element<'_, Message> {
    let panes = pane_grid(&studio.panes, |_, kind, _| {
        let (label, subtitle, controls, body): (&str, Option<String>, Option<Element<'_, Message>>, Element<'_, Message>) =
            match kind {
                PaneKind::Editor => ("EDITOR", Some(editor_label(studio)), None, editor(studio)),
                PaneKind::Console => {
                    ("CONSOLE", None, Some(small("Clear", theme::ghost, Some(Message::ClearConsole))), console(studio))
                }
                PaneKind::Environment => ("ENVIRONMENT", environment_label(studio), None, environment(studio)),
                PaneKind::Viewer => {
                    let grid = studio.grid.as_ref();
                    ("DATA", grid.map(|g| g.name.clone()), grid.map(pager), viewer(studio))
                }
            };
        let mut header =
            row![text(label).font(fonts::UI_SEMIBOLD).size(11).style(theme::muted)].spacing(10).align_y(Alignment::Center);
        if let Some(subtitle) = subtitle {
            header = header.push(text(subtitle).size(12).style(theme::strong));
        }
        let mut bar = pane_grid::TitleBar::new(header).padding([7, 12]).style(theme::card_header);
        if let Some(controls) = controls {
            bar = bar.controls(controls);
        }
        pane_grid::Content::new(body).title_bar(bar).style(theme::card)
    })
    .spacing(10)
    .style(theme::panes)
    .on_resize(10, Message::PaneResized)
    .on_drag(Message::PaneDragged);
    let workspace = container(panes).padding(10).width(Length::Fill).height(Length::Fill).style(theme::canvas);
    column![toolbar(studio), line(), workspace, line(), status_bar(studio)].into()
}

fn line<'a>() -> Element<'a, Message> {
    rule::horizontal(1).style(theme::separator).into()
}

fn small<'a>(label: &'a str, style: ButtonStyle, message: Option<Message>) -> Element<'a, Message> {
    button(text(label).font(fonts::UI_MEDIUM).size(12)).padding([3, 10]).style(style).on_press_maybe(message).into()
}

fn action<'a>(label: &'a str, hint: &'a str, style: ButtonStyle, message: Option<Message>) -> Element<'a, Message> {
    let control = button(text(label).font(fonts::UI_MEDIUM).size(13)).padding([6, 12]).style(style).on_press_maybe(message);
    let bubble = container(text(hint).size(12)).padding([4, 8]).style(theme::tip);
    tooltip(control, bubble, tooltip::Position::Bottom).gap(6).into()
}

fn divider<'a>() -> Element<'a, Message> {
    container(rule::vertical(1).style(theme::separator)).height(22).padding([0, 4]).into()
}

fn toolbar(studio: &Studio) -> Element<'_, Message> {
    let ready = studio.worker.is_some();
    let brand = row![
        text("Schematic").font(fonts::UI_SEMIBOLD).size(15).style(theme::strong),
        text("Studio").size(15).style(theme::muted)
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let run = action("Run form", "Ctrl+Enter", theme::primary, ready.then_some(Message::RunForm));
    let run_file = action("Run file", "Ctrl+Shift+Enter", theme::subtle, ready.then_some(Message::RunFile));
    let stop = action("Stop", "Interrupt the running code", theme::danger, studio.running.then_some(Message::Stop));
    let open = action("Open", "Ctrl+O", theme::subtle, Some(Message::Open));
    let save = action("Save", "Ctrl+S", theme::subtle, Some(Message::Save));
    let save_as = action("Save as", "Ctrl+Shift+S", theme::ghost, Some(Message::SaveAs));
    let mode = if theme::is_dark(&studio.theme) { "Light" } else { "Dark" };
    let toggle = action(mode, "Switch the theme", theme::ghost, Some(Message::ToggleTheme));
    let bar = row![brand, divider(), run, run_file, stop, space::horizontal(), open, save, save_as, divider(), toggle]
        .spacing(8)
        .align_y(Alignment::Center);
    container(bar).padding([8, 14]).width(Length::Fill).style(theme::bar).into()
}

fn status_bar(studio: &Studio) -> Element<'_, Message> {
    let (line, column) = studio.cursor();
    let dot_style: TextStyle = if studio.running { theme::accent } else { theme::faint };
    let status_style: TextStyle = if studio.status.starts_with("Error") { theme::alarm } else { theme::muted };
    let path = studio.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    let bar = row![
        text("•").size(14).style(dot_style),
        text(&studio.status).size(12).style(status_style),
        space::horizontal(),
        text(path).size(12).style(theme::faint),
        text(format!("Ln {line}, Col {column}")).size(12).style(theme::muted),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    container(bar).padding([5, 14]).width(Length::Fill).style(theme::bar).into()
}

fn editor_label(studio: &Studio) -> String {
    format!("{}{}", studio.file_name(), if studio.dirty { " •" } else { "" })
}

fn environment_label(studio: &Studio) -> Option<String> {
    match studio.bindings.len() {
        0 => None,
        1 => Some("1 binding".to_string()),
        n => Some(format!("{n} bindings")),
    }
}

fn editor_binding(press: KeyPress) -> Option<Binding<Message>> {
    let command = press.modifiers.command();
    let shift = press.modifiers.shift();
    let global = match &press.key {
        Key::Named(key::Named::Enter) if command => Some(if shift { Message::RunFile } else { Message::RunForm }),
        Key::Character(c) if command && c.eq_ignore_ascii_case("s") => Some(if shift { Message::SaveAs } else { Message::Save }),
        Key::Character(c) if command && c.eq_ignore_ascii_case("o") => Some(Message::Open),
        _ => None,
    };
    if let Some(message) = global {
        return Some(Binding::Custom(message));
    }
    if matches!(press.key, Key::Named(key::Named::Tab)) && matches!(press.status, Status::Focused { .. }) {
        return Some(Binding::Sequence(vec![Binding::Insert(' '), Binding::Insert(' ')]));
    }
    Binding::from_key_press(press)
}

fn console_binding(studio: &Studio) -> impl Fn(KeyPress) -> Option<Binding<Message>> {
    let cursor = studio.input.cursor().position.line;
    let first_line = cursor == 0;
    let last_line = cursor + 1 >= studio.input.line_count();
    let browsing = studio.recalled.is_some();
    move |press| {
        if !matches!(press.status, Status::Focused { .. }) {
            return None;
        }
        let plain = !press.modifiers.shift() && !press.modifiers.command();
        match &press.key {
            Key::Named(key::Named::Enter) if plain => Some(Binding::Custom(Message::Submit)),
            Key::Named(key::Named::ArrowUp) if plain && first_line => Some(Binding::Custom(Message::HistoryBack)),
            Key::Named(key::Named::ArrowDown) if plain && last_line && browsing => {
                Some(Binding::Custom(Message::HistoryForward))
            }
            Key::Named(key::Named::Tab) => Some(Binding::Sequence(vec![Binding::Insert(' '), Binding::Insert(' ')])),
            _ => Binding::from_key_press(press),
        }
    }
}

fn editor(studio: &Studio) -> Element<'_, Message> {
    text_editor(&studio.editor)
        .on_action(Message::Edit)
        .placeholder("Open a script or start typing. Ctrl+Enter runs the form at the cursor.")
        .font(fonts::MONO)
        .size(13.5)
        .line_height(1.55)
        .padding(12)
        .height(Length::Fill)
        .wrapping(text::Wrapping::None)
        .style(theme::editor)
        .highlight_with::<highlight::Schematic>(studio.marks.clone(), highlight::format)
        .key_binding(editor_binding)
        .into()
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
        row![
            container(text(marker).font(fonts::MONO_BOLD).size(13).style(marker_style)).width(14),
            text(line.text.trim_end_matches('\n')).font(font).size(13).style(style).width(Length::Fill),
        ]
        .spacing(8)
        .into()
    });
    let history = scrollable(column(lines).spacing(4).padding([10, 12]).width(Length::Fill))
        .id(console_id())
        .height(Length::Fill)
        .style(theme::scroll);
    let prompt = if studio.pending.is_empty() { "›" } else { "…" };
    let input = text_editor(&studio.input)
        .on_action(Message::Input)
        .placeholder("Type an expression and press Enter")
        .font(fonts::MONO)
        .size(13)
        .padding(Padding { top: 8.0, right: 0.0, bottom: 8.0, left: 0.0 })
        .style(theme::editor)
        .key_binding(console_binding(studio));
    let marker = container(text(prompt).font(fonts::MONO_BOLD).size(13).style(theme::accent))
        .width(14)
        .padding(Padding { top: 8.0, right: 0.0, bottom: 8.0, left: 0.0 });
    let entry = row![marker, input].spacing(8).padding([0, 12]);
    column![history, line(), entry].into()
}

fn environment(studio: &Studio) -> Element<'_, Message> {
    let groups = studio.groups();
    if groups.is_empty() {
        return hint("Nothing defined yet. Values you define appear here; click one to open it in the data pane.");
    }
    let selected = studio.grid.as_ref().map(|g| g.name.as_str());
    let mut list = Column::new().spacing(2).padding([6, 8]).width(Length::Fill);
    for (group, members) in groups {
        let heading = text(group.label().to_uppercase()).font(fonts::UI_SEMIBOLD).size(10.5).style(theme::faint);
        list = list.push(container(heading).padding(Padding { top: 10.0, right: 8.0, bottom: 4.0, left: 8.0 }));
        for binding in members {
            let tint = match binding.kind.as_str() {
                "table" => Tint::Accent,
                "array" => Tint::Violet,
                "function" => Tint::Amber,
                _ => Tint::Neutral,
            };
            let content = row![
                text(&binding.name).font(fonts::MONO).size(13).style(theme::strong).width(Length::FillPortion(2)),
                container(text(&binding.kind).font(fonts::UI_MEDIUM).size(11)).padding([1, 7]).style(theme::pill(tint)),
                text(&binding.summary).size(12).style(theme::muted).width(Length::FillPortion(3)),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            let chosen = selected == Some(binding.name.as_str());
            list = list.push(
                button(content)
                    .padding([5, 8])
                    .width(Length::Fill)
                    .style(theme::row(chosen))
                    .on_press(Message::Inspect(binding.name.clone())),
            );
        }
    }
    scrollable(list).height(Length::Fill).style(theme::scroll).into()
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

fn hint(message: &str) -> Element<'_, Message> {
    container(text(message).size(13).style(theme::faint)).padding(14).into()
}
