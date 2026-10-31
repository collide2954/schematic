use iced::Font;
use iced::font::{Style, Weight};

pub const UI: Font = Font::with_name("Inter");
pub const UI_MEDIUM: Font = Font { weight: Weight::Medium, ..UI };
pub const UI_SEMIBOLD: Font = Font { weight: Weight::Semibold, ..UI };
pub const MONO: Font = Font::with_name("JetBrains Mono");
pub const MONO_BOLD: Font = Font { weight: Weight::Bold, ..MONO };
pub const MONO_ITALIC: Font = Font { style: Style::Italic, ..MONO };

pub const FILES: [&[u8]; 6] = [
    include_bytes!("../fonts/Inter-Regular.ttf"),
    include_bytes!("../fonts/Inter-Medium.ttf"),
    include_bytes!("../fonts/Inter-SemiBold.ttf"),
    include_bytes!("../fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../fonts/JetBrainsMono-Italic.ttf"),
];
