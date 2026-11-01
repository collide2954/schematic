use std::f32::consts::TAU;

use iced::advanced::text::Alignment;
use iced::alignment::Vertical;
use iced::mouse::{self, Cursor};
use iced::widget::canvas::{self, Action, Frame, Geometry, LineDash, Path, Program, Stroke};
use iced::{Color, Event, Pixels, Point, Rectangle, Renderer, Size, Theme};
use schematic::plot::{Anchor, Paint, Plot, Shape, arc_points};

use crate::app::Message;
use crate::{fonts, theme};

pub struct Chart<'a> {
    pub plot: &'a Plot,
}

fn path_of(points: &[(f32, f32)], close: bool) -> Path {
    Path::new(|builder| {
        let mut points = points.iter();
        if let Some((x, y)) = points.next() {
            builder.move_to(Point::new(*x, *y));
        }
        for (x, y) in points {
            builder.line_to(Point::new(*x, *y));
        }
        if close {
            builder.close();
        }
    })
}

pub fn hit(shapes: &[Shape], px: f32, py: f32) -> Option<&str> {
    shapes.iter().rev().find_map(|shape| match shape {
        Shape::Rect { x, y, w, h, tip: Some(tip), .. } if px >= *x && px <= x + w && py >= *y && py <= y + h => Some(tip.as_str()),
        Shape::Circle { x, y, r, tip: Some(tip), .. } if (px - x).hypot(py - y) <= r + 2.5 => Some(tip.as_str()),
        Shape::Arc { cx, cy, r, inner, start, end, tip: Some(tip), .. } => {
            let (dx, dy) = (px - cx, py - cy);
            let distance = dx.hypot(dy);
            let mut angle = dy.atan2(dx);
            while angle < *start {
                angle += TAU;
            }
            (distance >= *inner && distance <= r + 0.5 && angle <= *end).then_some(tip.as_str())
        }
        _ => None,
    })
}

impl Program<Message> for Chart<'_> {
    type State = Option<Point>;

    fn update(&self, state: &mut Option<Point>, event: &Event, bounds: Rectangle, cursor: Cursor) -> Option<Action<Message>> {
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                *state = cursor.position_in(bounds);
                Some(Action::request_redraw())
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                *state = None;
                Some(Action::request_redraw())
            }
            _ => None,
        }
    }

    fn draw(&self, state: &Option<Point>, renderer: &Renderer, theme: &Theme, bounds: Rectangle, _cursor: Cursor) -> Vec<Geometry> {
        let t = theme::tones(theme);
        let color = |paint: &Paint| -> Color {
            match paint {
                Paint::Series(i) => [t.accent, t.keyword, t.number, t.constant, t.special, t.string][i % 6],
                Paint::Axis => t.muted,
                Paint::Grid => t.line,
                Paint::Text => t.text,
                Paint::Muted => t.muted,
            }
        };
        let scene = self.plot.scene(bounds.width, bounds.height);
        let mut frame = Frame::new(renderer, bounds.size());
        for shape in &scene.shapes {
            match shape {
                Shape::Rect { x, y, w, h, paint, .. } => {
                    frame.fill_rectangle(Point::new(*x, *y), Size::new(*w, *h), color(paint));
                }
                Shape::Line { x1, y1, x2, y2, paint, width, dashed } => {
                    let path = Path::line(Point::new(*x1, *y1), Point::new(*x2, *y2));
                    let mut stroke = Stroke::default().with_color(color(paint)).with_width(*width);
                    if *dashed {
                        stroke.line_dash = LineDash { segments: &[4.0, 3.0], offset: 0 };
                    }
                    frame.stroke(&path, stroke);
                }
                Shape::Polyline { points, paint, width } => {
                    frame.stroke(&path_of(points, false), Stroke::default().with_color(color(paint)).with_width(*width));
                }
                Shape::Polygon { points, paint } => {
                    frame.fill(&path_of(points, true), theme::fade(color(paint), 0.35));
                }
                Shape::Circle { x, y, r, paint, .. } => frame.fill(&Path::circle(Point::new(*x, *y), *r), color(paint)),
                Shape::Arc { cx, cy, r, inner, start, end, paint, .. } => {
                    let mut points = arc_points(*cx, *cy, *r, *start, *end);
                    if *inner > 0.0 {
                        points.extend(arc_points(*cx, *cy, *inner, *end, *start));
                    } else {
                        points.push((*cx, *cy));
                    }
                    frame.fill(&path_of(&points, true), color(paint));
                }
                Shape::Text { x, y, text, size, anchor, paint, bold } => frame.fill_text(canvas::Text {
                    content: text.clone(),
                    position: Point::new(*x, *y),
                    color: color(paint),
                    size: Pixels(*size),
                    font: if *bold { fonts::UI_SEMIBOLD } else { fonts::UI },
                    align_x: match anchor {
                        Anchor::Start => Alignment::Left,
                        Anchor::Middle => Alignment::Center,
                        Anchor::End => Alignment::Right,
                    },
                    align_y: Vertical::Center,
                    ..canvas::Text::default()
                }),
            }
        }
        if let Some(tip) = state.and_then(|p| hit(&scene.shapes, p.x, p.y).map(|tip| (p, tip.to_string()))) {
            let (point, text) = tip;
            let width = text.chars().count() as f32 * 6.6 + 16.0;
            let height = 24.0;
            let x = if point.x + 14.0 + width > bounds.width { point.x - 14.0 - width } else { point.x + 14.0 };
            let y = if point.y - 32.0 < 0.0 { point.y + 18.0 } else { point.y - 32.0 };
            frame.fill(&Path::rounded_rectangle(Point::new(x, y), Size::new(width, height), 5.0.into()), t.tooltip);
            frame.fill_text(canvas::Text {
                content: text,
                position: Point::new(x + 8.0, y + height / 2.0),
                color: t.tooltip_text,
                size: Pixels(12.0),
                font: fonts::UI,
                align_x: Alignment::Left,
                align_y: Vertical::Center,
                ..canvas::Text::default()
            });
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_find_the_mark_under_the_pointer() {
        let shapes = vec![
            Shape::Rect { x: 10.0, y: 10.0, w: 20.0, h: 30.0, paint: Paint::Series(0), tip: Some("bar".into()) },
            Shape::Circle { x: 100.0, y: 100.0, r: 3.0, paint: Paint::Series(0), tip: Some("point".into()) },
            Shape::Arc { cx: 200.0, cy: 200.0, r: 50.0, inner: 0.0, start: 0.0, end: 1.5, paint: Paint::Series(1), tip: Some("slice".into()) },
            Shape::Rect { x: 0.0, y: 0.0, w: 5.0, h: 5.0, paint: Paint::Grid, tip: None },
        ];
        assert_eq!(hit(&shapes, 15.0, 20.0), Some("bar"));
        assert_eq!(hit(&shapes, 104.0, 100.0), Some("point"));
        assert_eq!(hit(&shapes, 110.0, 100.0), None);
        assert_eq!(hit(&shapes, 230.0, 230.0), Some("slice"));
        assert_eq!(hit(&shapes, 230.0, 170.0), None);
        assert_eq!(hit(&shapes, 2.0, 2.0), None);
    }
}
