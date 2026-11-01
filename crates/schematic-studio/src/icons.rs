use iced::mouse::Cursor;
use iced::widget::canvas;
use iced::widget::canvas::{Frame, Geometry, Path, Program, Stroke};
use iced::{Element, Length, Point, Rectangle, Renderer, Size, Theme};

use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Play,
    PlayAll,
    Stop,
    Open,
    Save,
    Sun,
    Moon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Accent,
    Text,
    Muted,
    Danger,
}

pub struct Icon {
    pub glyph: Glyph,
    pub ink: Ink,
}

pub fn icon<'a, Message: 'a>(glyph: Glyph, ink: Ink) -> Element<'a, Message> {
    canvas(Icon { glyph, ink }).width(Length::Fixed(16.0)).height(Length::Fixed(16.0)).into()
}

fn polygon(points: &[(f32, f32)]) -> Path {
    Path::new(|builder| {
        let mut points = points.iter();
        if let Some((x, y)) = points.next() {
            builder.move_to(Point::new(*x, *y));
        }
        for (x, y) in points {
            builder.line_to(Point::new(*x, *y));
        }
        builder.close();
    })
}

fn polyline(points: &[(f32, f32)]) -> Path {
    Path::new(|builder| {
        let mut points = points.iter();
        if let Some((x, y)) = points.next() {
            builder.move_to(Point::new(*x, *y));
        }
        for (x, y) in points {
            builder.line_to(Point::new(*x, *y));
        }
    })
}

impl<Message> Program<Message> for Icon {
    type State = ();

    fn draw(&self, _state: &(), renderer: &Renderer, theme: &Theme, bounds: Rectangle, _cursor: Cursor) -> Vec<Geometry> {
        let t = theme::tones(theme);
        let color = match self.ink {
            Ink::Accent => t.accent,
            Ink::Text => t.text,
            Ink::Muted => t.muted,
            Ink::Danger => t.danger,
        };
        let stroke = || Stroke::default().with_color(color).with_width(1.6);
        let mut frame = Frame::new(renderer, bounds.size());
        match self.glyph {
            Glyph::Play => frame.fill(&polygon(&[(4.0, 2.5), (13.5, 8.0), (4.0, 13.5)]), color),
            Glyph::PlayAll => {
                frame.fill(&polygon(&[(2.5, 3.0), (10.0, 8.0), (2.5, 13.0)]), color);
                frame.fill_rectangle(Point::new(11.5, 3.0), Size::new(2.0, 10.0), color);
            }
            Glyph::Stop => frame.fill(&Path::rectangle(Point::new(3.5, 3.5), Size::new(9.0, 9.0)), color),
            Glyph::Open => {
                frame.stroke(&polygon(&[(2.0, 4.5), (6.0, 4.5), (7.5, 6.5), (14.0, 6.5), (14.0, 13.0), (2.0, 13.0)]), stroke());
            }
            Glyph::Save => {
                frame.stroke(&polyline(&[(2.5, 10.0), (2.5, 13.5), (13.5, 13.5), (13.5, 10.0)]), stroke());
                frame.stroke(&polyline(&[(8.0, 2.5), (8.0, 10.5)]), stroke());
                frame.stroke(&polyline(&[(5.0, 7.5), (8.0, 10.5), (11.0, 7.5)]), stroke());
            }
            Glyph::Sun => {
                let center = Point::new(8.0, 8.0);
                frame.fill(&Path::circle(center, 3.2), color);
                for i in 0..8 {
                    let angle = i as f32 * std::f32::consts::FRAC_PI_4;
                    let (sin, cos) = angle.sin_cos();
                    let from = Point::new(8.0 + cos * 5.0, 8.0 + sin * 5.0);
                    let to = Point::new(8.0 + cos * 7.0, 8.0 + sin * 7.0);
                    frame.stroke(&Path::line(from, to), stroke().with_width(1.4));
                }
            }
            Glyph::Moon => {
                frame.fill(&Path::circle(Point::new(8.0, 8.0), 5.8), color);
                frame.fill(&Path::circle(Point::new(10.6, 6.0), 5.0), t.surface);
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_builds_an_element() {
        let glyphs = [Glyph::Play, Glyph::PlayAll, Glyph::Stop, Glyph::Open, Glyph::Save, Glyph::Sun, Glyph::Moon];
        let elements: Vec<Element<'_, ()>> = glyphs.into_iter().map(|glyph| icon(glyph, Ink::Text)).collect();
        assert_eq!(elements.len(), glyphs.len());
    }
}
