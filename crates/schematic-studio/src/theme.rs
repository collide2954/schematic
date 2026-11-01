use iced::theme::Palette;
use iced::widget::{button, container, pane_grid, rule, scrollable, text, text_editor, text_input};
use iced::{Background, Border, Color, Theme};

#[derive(Clone, Copy, Debug)]
pub struct Tones {
    pub canvas: Color,
    pub surface: Color,
    pub raised: Color,
    pub hover: Color,
    pub line: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub accent: Color,
    pub accent_text: Color,
    pub danger: Color,
    pub warning: Color,
    pub success: Color,
    pub comment: Color,
    pub string: Color,
    pub number: Color,
    pub keyword: Color,
    pub special: Color,
    pub constant: Color,
    pub brackets: [Color; 3],
    pub tooltip: Color,
    pub tooltip_text: Color,
}

const fn rgb(hex: u32) -> Color {
    Color {
        r: ((hex >> 16) & 0xFF) as f32 / 255.0,
        g: ((hex >> 8) & 0xFF) as f32 / 255.0,
        b: (hex & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

pub const fn fade(color: Color, a: f32) -> Color {
    Color { a, ..color }
}

pub const LIGHT: Tones = Tones {
    canvas: rgb(0xE3E6EB),
    surface: rgb(0xFFFFFF),
    raised: rgb(0xF1F3F6),
    hover: rgb(0xE8ECF1),
    line: rgb(0xDADFE6),
    text: rgb(0x1A2230),
    muted: rgb(0x66728A),
    faint: rgb(0x9AA5B8),
    accent: rgb(0x0F7B86),
    accent_text: rgb(0xFFFFFF),
    danger: rgb(0xC83C32),
    warning: rgb(0xB7791F),
    success: rgb(0x2F8F5B),
    comment: rgb(0x8B95A7),
    string: rgb(0x2A8A5A),
    number: rgb(0xB4540F),
    keyword: rgb(0x7B3FE4),
    special: rgb(0x2458D3),
    constant: rgb(0xC0286E),
    brackets: [rgb(0x56637A), rgb(0x3D6ACB), rgb(0xB0731E)],
    tooltip: rgb(0x1A2230),
    tooltip_text: rgb(0xF5F7FA),
};

pub const DARK: Tones = Tones {
    canvas: rgb(0x0C0E12),
    surface: rgb(0x16191F),
    raised: rgb(0x1C2027),
    hover: rgb(0x252A33),
    line: rgb(0x272D37),
    text: rgb(0xE3E8EF),
    muted: rgb(0x8C97AA),
    faint: rgb(0x5B6678),
    accent: rgb(0x3EC6CF),
    accent_text: rgb(0x0B1A1C),
    danger: rgb(0xF0695E),
    warning: rgb(0xE2A93B),
    success: rgb(0x55C98A),
    comment: rgb(0x6F7A8E),
    string: rgb(0x8BD39C),
    number: rgb(0xF0A868),
    keyword: rgb(0xC7A2FF),
    special: rgb(0x86B4FF),
    constant: rgb(0xF58FB8),
    brackets: [rgb(0xA0AABB), rgb(0x74A6F0), rgb(0xDDB36E)],
    tooltip: rgb(0xE3E8EF),
    tooltip_text: rgb(0x171B21),
};

fn palette(t: &Tones) -> Palette {
    Palette { background: t.surface, text: t.text, primary: t.accent, success: t.success, warning: t.warning, danger: t.danger }
}

pub fn light() -> Theme {
    Theme::custom("Schematic Light", palette(&LIGHT))
}

pub fn dark() -> Theme {
    Theme::custom("Schematic Dark", palette(&DARK))
}

pub fn is_dark(theme: &Theme) -> bool {
    theme.extended_palette().is_dark
}

pub fn tones(theme: &Theme) -> &'static Tones {
    if is_dark(theme) { &DARK } else { &LIGHT }
}

pub fn toggled(theme: &Theme) -> Theme {
    if is_dark(theme) { light() } else { dark() }
}

fn rounded(radius: f32) -> Border {
    Border { radius: radius.into(), ..Border::default() }
}

fn filled(color: Color) -> container::Style {
    container::Style { background: Some(color.into()), ..container::Style::default() }
}

pub fn bar(theme: &Theme) -> container::Style {
    let t = tones(theme);
    container::Style { text_color: Some(t.text), ..filled(t.surface) }
}

pub fn pane(theme: &Theme) -> container::Style {
    filled(tones(theme).surface)
}

pub fn gaps(theme: &Theme) -> container::Style {
    filled(tones(theme).line)
}

pub fn strip(theme: &Theme) -> container::Style {
    filled(tones(theme).raised)
}

pub fn tab(theme: &Theme) -> container::Style {
    let t = tones(theme);
    container::Style { text_color: Some(t.text), ..filled(t.surface) }
}

pub fn tip(theme: &Theme) -> container::Style {
    let t = tones(theme);
    container::Style { text_color: Some(t.tooltip_text), border: rounded(5.0), ..filled(t.tooltip) }
}

fn plain(background: Option<Color>, text_color: Color) -> button::Style {
    button::Style {
        background: background.map(Background::from),
        text_color,
        border: rounded(4.0),
        shadow: iced::Shadow::default(),
        snap: true,
    }
}

pub fn tool(theme: &Theme, status: button::Status) -> button::Style {
    let t = tones(theme);
    match status {
        button::Status::Active => plain(None, t.text),
        button::Status::Hovered => plain(Some(t.hover), t.text),
        button::Status::Pressed => plain(Some(t.line), t.text),
        button::Status::Disabled => plain(None, t.faint),
    }
}

pub fn ghost(theme: &Theme, status: button::Status) -> button::Style {
    let t = tones(theme);
    match status {
        button::Status::Active => plain(None, t.muted),
        button::Status::Hovered => plain(Some(t.hover), t.text),
        button::Status::Pressed => plain(Some(t.line), t.text),
        button::Status::Disabled => plain(None, t.faint),
    }
}

pub fn row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = tones(theme);
        let background = match status {
            _ if selected => Some(fade(t.accent, 0.12)),
            button::Status::Hovered | button::Status::Pressed => Some(t.hover),
            _ => None,
        };
        button::Style { border: Border::default(), ..plain(background, t.text) }
    }
}

pub fn tab_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = tones(theme);
        let (background, color) = match (active, status) {
            (true, _) => (Some(t.surface), t.text),
            (false, button::Status::Hovered | button::Status::Pressed) => (Some(t.hover), t.text),
            (false, _) => (None, t.muted),
        };
        button::Style { border: Border::default(), ..plain(background, color) }
    }
}

pub fn tab_shell(active: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| if active { filled(tones(theme).surface) } else { container::Style::default() }
}

pub fn choice(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let t = tones(theme);
        match (selected, status) {
            (true, _) => plain(Some(fade(t.accent, 0.15)), t.accent),
            (false, button::Status::Hovered | button::Status::Pressed) => plain(Some(t.hover), t.text),
            (false, _) => plain(None, t.muted),
        }
    }
}

pub fn popup(theme: &Theme) -> container::Style {
    let t = tones(theme);
    let shadow = iced::Shadow {
        color: fade(Color::BLACK, if is_dark(theme) { 0.5 } else { 0.14 }),
        offset: iced::Vector::new(0.0, 4.0),
        blur_radius: 12.0,
    };
    container::Style { border: Border { color: t.line, width: 1.0, radius: 6.0.into() }, shadow, ..filled(t.surface) }
}

pub fn code_block(theme: &Theme) -> container::Style {
    container::Style { border: rounded(4.0), ..filled(tones(theme).raised) }
}

pub fn quote(theme: &Theme) -> container::Style {
    let t = tones(theme);
    container::Style { text_color: Some(t.muted), border: rounded(4.0), ..filled(fade(t.accent, 0.07)) }
}

pub fn editor(theme: &Theme, _status: text_editor::Status) -> text_editor::Style {
    let t = tones(theme);
    text_editor::Style {
        background: Color::TRANSPARENT.into(),
        border: Border::default(),
        placeholder: t.faint,
        value: t.text,
        selection: fade(t.accent, 0.25),
    }
}

pub fn field(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let t = tones(theme);
    let outline = match status {
        text_input::Status::Focused { .. } => t.accent,
        text_input::Status::Hovered => t.muted,
        _ => t.line,
    };
    text_input::Style {
        background: t.raised.into(),
        border: Border { color: outline, width: 1.0, radius: 4.0.into() },
        icon: t.muted,
        placeholder: t.faint,
        value: t.text,
        selection: fade(t.accent, 0.25),
    }
}

pub fn scroll(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let t = tones(theme);
    let active = matches!(status, scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. });
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: fade(t.muted, if active { 0.55 } else { 0.3 }).into(),
            border: rounded(4.0),
        },
    };
    scrollable::Style { vertical_rail: rail, horizontal_rail: rail, gap: None, ..scrollable::default(theme, status) }
}

pub fn panes(theme: &Theme) -> pane_grid::Style {
    let t = tones(theme);
    pane_grid::Style {
        hovered_region: pane_grid::Highlight {
            background: fade(t.accent, 0.12).into(),
            border: Border { color: t.accent, width: 1.0, radius: 0.0.into() },
        },
        picked_split: pane_grid::Line { color: t.accent, width: 2.0 },
        hovered_split: pane_grid::Line { color: fade(t.accent, 0.6), width: 2.0 },
    }
}

pub fn separator(theme: &Theme) -> rule::Style {
    rule::Style { color: tones(theme).line, radius: 0.0.into(), fill_mode: rule::FillMode::Full, snap: true }
}

pub fn strong(theme: &Theme) -> text::Style {
    text::Style { color: Some(tones(theme).text) }
}

pub fn muted(theme: &Theme) -> text::Style {
    text::Style { color: Some(tones(theme).muted) }
}

pub fn faint(theme: &Theme) -> text::Style {
    text::Style { color: Some(tones(theme).faint) }
}

pub fn accent(theme: &Theme) -> text::Style {
    text::Style { color: Some(tones(theme).accent) }
}

pub fn alarm(theme: &Theme) -> text::Style {
    text::Style { color: Some(tones(theme).danger) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_themes_toggle_and_know_their_tones() {
        assert!(!is_dark(&light()));
        assert!(is_dark(&dark()));
        assert_eq!(toggled(&light()), dark());
        assert_eq!(tones(&dark()).canvas, DARK.canvas);
        assert_eq!(rgb(0x0F7B86).g, 123.0 / 255.0);
    }
}
