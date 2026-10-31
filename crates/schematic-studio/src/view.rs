use iced::keyboard::{Key, key};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text_editor::{Binding, KeyPress};
use iced::widget::{
    button, column, container, pane_grid, pick_list, row, scrollable, space, table, text, text_editor, text_input,
};
use iced::{Alignment, Border, Element, Font, Length, Theme};
use schematic_studio::highlight;

use crate::{LineKind, Message, PAGE, PaneKind, Studio, console_id};

pub fn view(studio: &Studio) -> Element<'_, Message> {
    let panes = pane_grid(&studio.panes, |_, kind, _| {
        let (title, body, controls): (&str, Element<'_, Message>, Option<Element<'_, Message>>) = match kind {
            PaneKind::Editor => ("Editor", editor(studio), Some(text(studio.file_label()).size(12).into())),
            PaneKind::Console => ("Console", console(studio), Some(small_button("Clear", Some(Message::ClearConsole)))),
            PaneKind::Environment => ("Environment", environment(studio), None),
            PaneKind::Viewer => ("Data", viewer(studio), Some(viewer_controls(studio))),
        };
        let mut bar = pane_grid::TitleBar::new(text(title).size(13)).padding(6).style(title_style);
        if let Some(controls) = controls {
            bar = bar.controls(controls);
        }
        pane_grid::Content::new(body).title_bar(bar).style(pane_style)
    })
    .spacing(6)
    .on_resize(8, Message::PaneResized)
    .on_drag(Message::PaneDragged);

    let status = text(&studio.status).size(12).style(text::secondary);
    column![toolbar(studio), panes, status].spacing(6).padding(8).into()
}

fn small_button<'a>(label: &'a str, message: Option<Message>) -> Element<'a, Message> {
    button(text(label).size(12)).padding([2, 8]).on_press_maybe(message).into()
}

fn toolbar(studio: &Studio) -> Element<'_, Message> {
    let ready = studio.worker.is_some();
    let run = button(text("Run form").size(13)).on_press_maybe(ready.then_some(Message::RunForm));
    let run_file = button(text("Run file").size(13)).on_press_maybe(ready.then_some(Message::RunFile));
    let stop = button(text("Stop").size(13)).style(button::danger).on_press_maybe(studio.running.then_some(Message::Stop));
    let path = text_input("path/to/script.scm", &studio.path)
        .on_input(Message::PathChanged)
        .on_submit(Message::Open)
        .size(13)
        .width(360);
    let open = button(text("Open").size(13)).on_press(Message::Open);
    let save = button(text("Save").size(13)).on_press(Message::Save);
    let theme = pick_list(Theme::ALL, Some(studio.theme.clone()), Message::ThemeChanged).text_size(13);
    row![run, run_file, stop, space::horizontal(), path, open, save, theme]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
}

fn key_binding(press: KeyPress) -> Option<Binding<Message>> {
    let command = press.modifiers.command();
    let custom = match &press.key {
        Key::Named(key::Named::Enter) if command => {
            Some(if press.modifiers.shift() { Message::RunFile } else { Message::RunForm })
        }
        Key::Character(c) if command && c.as_str() == "s" => Some(Message::Save),
        _ => None,
    };
    if let Some(message) = custom {
        return Some(Binding::Custom(message));
    }
    if matches!(press.key, Key::Named(key::Named::Tab)) {
        return Some(Binding::Sequence(vec![Binding::Insert(' '), Binding::Insert(' ')]));
    }
    Binding::from_key_press(press)
}

fn editor(studio: &Studio) -> Element<'_, Message> {
    text_editor(&studio.editor)
        .on_action(Message::Edit)
        .font(Font::MONOSPACE)
        .size(14)
        .height(Length::Fill)
        .wrapping(text::Wrapping::None)
        .highlight_with::<highlight::Schematic>((), highlight::format)
        .key_binding(key_binding)
        .into()
}

fn console(studio: &Studio) -> Element<'_, Message> {
    let lines = studio.console.iter().map(|line| {
        let (prefix, style): (&str, Option<fn(&Theme) -> text::Style>) = match line.kind {
            LineKind::Input => ("> ", None),
            LineKind::Output => ("", None),
            LineKind::Result => ("", Some(text::primary)),
            LineKind::Error => ("", Some(text::danger)),
            LineKind::Note => ("", Some(text::secondary)),
        };
        let mut shown = text(format!("{prefix}{}", line.text.trim_end_matches('\n'))).font(Font::MONOSPACE).size(13);
        if let Some(style) = style {
            shown = shown.style(style);
        }
        Element::from(shown)
    });
    let history = scrollable(column(lines).spacing(2).padding(6).width(Length::Fill)).id(console_id()).height(Length::Fill);
    let prompt = if studio.pending.is_empty() { ">" } else { "..." };
    let input = text_input("Type an expression and press Enter", &studio.input)
        .on_input(Message::InputChanged)
        .on_submit(Message::Submit)
        .font(Font::MONOSPACE)
        .size(13);
    let entry = row![text(prompt).font(Font::MONOSPACE).size(13), input].spacing(6).align_y(Alignment::Center).padding([4, 6]);
    column![history, entry].into()
}

fn environment(studio: &Studio) -> Element<'_, Message> {
    if studio.bindings.is_empty() {
        return hint("Nothing defined yet. Values you define appear here; click one to view it.");
    }
    let rows = studio.bindings.iter().map(|binding| {
        row![
            button(text(&binding.name).font(Font::MONOSPACE).size(13))
                .style(button::text)
                .padding([2, 4])
                .on_press(Message::Inspect(binding.name.clone())),
            text(&binding.kind).size(12).style(text::secondary).width(70),
            text(&binding.summary).size(12).width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    });
    scrollable(column(rows).spacing(2).padding(6).width(Length::Fill)).height(Length::Fill).into()
}

fn viewer_controls(studio: &Studio) -> Element<'_, Message> {
    let Some(grid) = &studio.grid else {
        return space::horizontal().into();
    };
    let first = grid.offset + usize::from(!grid.rows.is_empty());
    let last = grid.offset + grid.rows.len();
    let label = format!("{}: rows {first} to {last} of {}", grid.name, grid.total);
    let previous = small_button("Previous", (grid.offset > 0).then_some(Message::Page(-1)));
    let next = small_button("Next", (grid.offset + PAGE < grid.total).then_some(Message::Page(1)));
    row![text(label).size(12), previous, next].spacing(6).align_y(Alignment::Center).into()
}

fn viewer(studio: &Studio) -> Element<'_, Message> {
    let Some(grid) = &studio.grid else {
        return hint("Click a name in the environment to view it here.");
    };
    let columns = grid.columns.iter().enumerate().map(|(c, name)| {
        let header = text(name).font(Font::MONOSPACE).size(12).style(text::secondary);
        table::column(header, move |r: usize| text(&grid.rows[r][c]).font(Font::MONOSPACE).size(13))
    });
    let body = table(columns, 0..grid.rows.len()).padding_x(8).padding_y(3);
    let both = Direction::Both { vertical: Scrollbar::default(), horizontal: Scrollbar::default() };
    scrollable(body).direction(both).width(Length::Fill).height(Length::Fill).into()
}

fn hint(message: &str) -> Element<'_, Message> {
    container(text(message).size(13).style(text::secondary)).padding(10).into()
}

fn pane_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(palette.background.base.color.into()),
        border: Border { color: palette.background.strong.color, width: 1.0, radius: 6.0.into() },
        ..container::Style::default()
    }
}

fn title_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(palette.background.weak.color.into()),
        text_color: Some(palette.background.weak.text),
        border: Border {
            radius: iced::border::Radius { top_left: 6.0, top_right: 6.0, bottom_right: 0.0, bottom_left: 0.0 },
            ..Border::default()
        },
        ..container::Style::default()
    }
}
