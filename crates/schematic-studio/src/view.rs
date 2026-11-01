use iced::keyboard::{Key, key};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text::LineHeight;
use iced::widget::text_editor::{Binding, KeyPress, Status};
use iced::widget::{
    Column, button, canvas, column, container, pane_grid, row, rule, scrollable, sensor, space, table, text, text_editor,
    tooltip,
};
use iced::{Alignment, Element, Font, Length, Padding, Pixels, Theme};

use crate::app::{EDITOR_PAD, LINE_HEIGHT, LineKind, Message, PAGE, PaneKind, Studio, console_id, editor_id};
use crate::chart::Chart;
use crate::icons::{Glyph, Ink, icon};
use crate::worker::Grid;
use crate::{fonts, highlight, theme};

type TextStyle = fn(&Theme) -> text::Style;

pub fn view(studio: &Studio) -> Element<'_, Message> {
    let panes = pane_grid(&studio.panes, |_, kind, _| {
        let (name, detail, controls, body): (&str, Option<String>, Option<Element<'_, Message>>, Element<'_, Message>) =
            match kind {
                PaneKind::Editor => ("Editor", Some(editor_label(studio)), None, editor(studio)),
                PaneKind::Console => ("Console", None, Some(small("Clear", Some(Message::ClearConsole))), console(studio)),
                PaneKind::Environment => ("Environment", environment_label(studio), None, environment(studio)),
                PaneKind::Viewer => {
                    let grid = studio.grid.as_ref();
                    ("Data", grid.map(|g| g.name.clone()), grid.map(pager), viewer(studio))
                }
                PaneKind::Plots => {
                    let plot = studio.plot();
                    let detail = plot.map(|p| p.title.clone().unwrap_or_else(|| p.describe()));
                    ("Plots", detail, Some(plot_controls(studio)), plots(studio))
                }
            };
        let bar = pane_grid::TitleBar::new(strip(name, detail, controls)).style(theme::strip);
        pane_grid::Content::new(body).title_bar(bar).style(theme::pane)
    })
    .spacing(1)
    .style(theme::panes)
    .on_resize(6, Message::PaneResized)
    .on_drag(Message::PaneDragged);
    let workspace = container(panes).width(Length::Fill).height(Length::Fill).style(theme::gaps);
    column![toolbar(studio), line(), workspace, line(), status_bar(studio)].into()
}

fn line<'a>() -> Element<'a, Message> {
    rule::horizontal(1).style(theme::separator).into()
}

fn strip<'a>(name: &'a str, detail: Option<String>, controls: Option<Element<'a, Message>>) -> Element<'a, Message> {
    let mut label = row![text(name).size(12.5).font(fonts::UI_MEDIUM)].spacing(8).align_y(Alignment::Center);
    if let Some(detail) = detail {
        label = label.push(text(detail).size(12).style(theme::muted));
    }
    let tab = container(label).padding([0, 12]).height(Length::Fill).align_y(Alignment::Center).style(theme::tab);
    let mut bar = row![tab, space::horizontal()].height(30).align_y(Alignment::Center);
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
    let path = studio.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    let bar = row![
        text("•").size(13).style(dot_style),
        text(&studio.status).size(11.5).style(status_style),
        space::horizontal(),
        text(path).size(11.5).style(theme::faint),
        text(format!("Ln {line}, Col {column}")).size(11.5).style(theme::muted),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    container(bar).padding([3, 10]).width(Length::Fill).style(theme::bar).into()
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
    let lines = studio.editor.line_count().max(1);
    let current = studio.editor.cursor().position.line;
    let digits = lines.to_string().len().max(2);
    let numbers = Column::with_children((0..lines).map(|i| {
        let style: TextStyle = if i == current { theme::strong } else { theme::faint };
        text(format!("{:>digits$}", i + 1))
            .font(fonts::MONO)
            .size(13.5)
            .line_height(LineHeight::Absolute(Pixels(LINE_HEIGHT)))
            .style(style)
            .into()
    }))
    .padding(Padding { top: EDITOR_PAD, right: 10.0, bottom: EDITOR_PAD, left: 14.0 });
    let code = text_editor(&studio.editor)
        .on_action(Message::Edit)
        .placeholder("Open a script or start typing. Ctrl+Enter runs the form at the cursor.")
        .font(fonts::MONO)
        .size(13.5)
        .line_height(LineHeight::Absolute(Pixels(LINE_HEIGHT)))
        .padding(Padding { top: EDITOR_PAD, right: 12.0, bottom: EDITOR_PAD, left: 6.0 })
        .height(Length::Shrink)
        .wrapping(text::Wrapping::None)
        .style(theme::editor)
        .highlight_with::<highlight::Schematic>(studio.marks.clone(), highlight::format)
        .key_binding(editor_binding);
    let body = row![numbers, code].align_y(Alignment::Start).width(Length::Fill);
    let scroller = scrollable(body)
        .id(editor_id())
        .width(Length::Fill)
        .height(Length::Fill)
        .on_scroll(Message::EditorScrolled)
        .style(theme::scroll);
    sensor(scroller).on_show(Message::EditorResized).on_resize(Message::EditorResized).into()
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
    let history = scrollable(column(lines).spacing(3).padding([8, 12]).width(Length::Fill))
        .id(console_id())
        .height(Length::Fill)
        .style(theme::scroll);
    let prompt = if studio.pending.is_empty() { "›" } else { "…" };
    let input = text_editor(&studio.input)
        .on_action(Message::Input)
        .placeholder("Type an expression and press Enter")
        .font(fonts::MONO)
        .size(13)
        .padding(Padding { top: 7.0, right: 0.0, bottom: 7.0, left: 0.0 })
        .style(theme::editor)
        .key_binding(console_binding(studio));
    let marker = container(text(prompt).font(fonts::MONO_BOLD).size(13).style(theme::accent))
        .width(14)
        .padding(Padding { top: 7.0, right: 0.0, bottom: 7.0, left: 0.0 });
    let entry = row![marker, input].spacing(8).padding([0, 12]);
    column![history, line(), entry].into()
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

fn hint(message: &str) -> Element<'_, Message> {
    container(text(message).size(12.5).style(theme::faint)).padding(12).into()
}
