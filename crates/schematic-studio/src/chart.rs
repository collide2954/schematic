use iced::advanced::text::Alignment;
use iced::alignment::Vertical;
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Frame, Geometry, LineDash, Path, Program, Stroke};
use iced::{Color, Pixels, Point, Rectangle, Renderer, Size, Theme};
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

impl Program<Message> for Chart<'_> {
    type State = ();

    fn draw(&self, _state: &(), renderer: &Renderer, theme: &Theme, bounds: Rectangle, _cursor: Cursor) -> Vec<Geometry> {
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
                        stroke = stroke.with_line_dash(LineDash { segments: &[4.0, 3.0], offset: 0 });
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
        vec![frame.into_geometry()]
    }
}
