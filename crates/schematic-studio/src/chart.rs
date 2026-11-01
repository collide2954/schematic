use std::f32::consts::TAU;

use iced::advanced::text::Alignment;
use iced::alignment::Vertical;
use iced::mouse::{self, Cursor, ScrollDelta};
use iced::widget::canvas::{self, Action, Frame, Geometry, LineDash, Path, Program, Stroke};
use iced::{Color, Event, Pixels, Point, Rectangle, Renderer, Size, Theme, Vector};
use schematic::plot::{Anchor, Paint, Plot, Scene, Shape, arc_points};

use crate::app::Message;
use crate::{fonts, theme};

pub const MAX_ZOOM: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlotView {
    pub zoom: f32,
    pub pan: (f32, f32),
}

impl Default for PlotView {
    fn default() -> PlotView {
        PlotView { zoom: 1.0, pan: (0.0, 0.0) }
    }
}

impl PlotView {
    pub fn clamped(self, size: Size) -> PlotView {
        let zoom = self.zoom.clamp(1.0, MAX_ZOOM);
        let x = self.pan.0.clamp(size.width - size.width * zoom, 0.0);
        let y = self.pan.1.clamp(size.height - size.height * zoom, 0.0);
        PlotView { zoom, pan: (x, y) }
    }

    pub fn zoomed(self, factor: f32, about: Point, size: Size) -> PlotView {
        let zoom = (self.zoom * factor).clamp(1.0, MAX_ZOOM);
        let ratio = zoom / self.zoom;
        let pan = (about.x - (about.x - self.pan.0) * ratio, about.y - (about.y - self.pan.1) * ratio);
        PlotView { zoom, pan }.clamped(size)
    }

    pub fn panned(self, by: Vector, size: Size) -> PlotView {
        PlotView { zoom: self.zoom, pan: (self.pan.0 + by.x, self.pan.1 + by.y) }.clamped(size)
    }
}

pub struct Chart<'a> {
    pub plot: &'a Plot,
    pub view: PlotView,
}

pub struct Thumb<'a> {
    pub plot: &'a Plot,
    pub index: usize,
    pub selected: bool,
}

#[derive(Debug, Default)]
pub struct Pointer {
    pub at: Option<Point>,
    pub dragging: Option<Point>,
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

fn paint_color(theme: &Theme, paint: &Paint) -> Color {
    let t = theme::tones(theme);
    match paint {
        Paint::Series(i) => [t.accent, t.keyword, t.number, t.constant, t.special, t.string][i % 6],
        Paint::Axis => t.muted,
        Paint::Grid => t.line,
        Paint::Text => t.text,
        Paint::Muted => t.muted,
    }
}

fn paint_scene(frame: &mut Frame, scene: &Scene, theme: &Theme, with_text: bool) {
    let color = |paint: &Paint| paint_color(theme, paint);
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
            Shape::Text { .. } if !with_text => {}
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
}

fn wheel_factor(delta: ScrollDelta) -> f32 {
    let lines = match delta {
        ScrollDelta::Lines { y, .. } => y,
        ScrollDelta::Pixels { y, .. } => y / 40.0,
    };
    1.15f32.powf(lines)
}

impl Program<Message> for Chart<'_> {
    type State = Pointer;

    fn update(&self, state: &mut Pointer, event: &Event, bounds: Rectangle, cursor: Cursor) -> Option<Action<Message>> {
        let inside = cursor.position_in(bounds);
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                state.at = inside;
                if let (Some(from), Some(to)) = (state.dragging, inside) {
                    state.dragging = Some(to);
                    return Some(Action::publish(Message::PlotView(self.view.panned(to - from, bounds.size()))).and_capture());
                }
                Some(Action::request_redraw())
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                state.at = None;
                Some(Action::request_redraw())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if inside.is_some() && self.view.zoom > 1.0 => {
                state.dragging = inside;
                Some(Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging.is_some() => {
                state.dragging = None;
                Some(Action::capture())
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let about = inside?;
                let view = self.view.zoomed(wheel_factor(*delta), about, bounds.size());
                (view != self.view).then(|| Action::publish(Message::PlotView(view)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(&self, state: &Pointer, renderer: &Renderer, theme: &Theme, bounds: Rectangle, _cursor: Cursor) -> Vec<Geometry> {
        let t = theme::tones(theme);
        let view = self.view.clamped(bounds.size());
        let scene = self.plot.scene(bounds.width * view.zoom, bounds.height * view.zoom);
        let mut frame = Frame::new(renderer, bounds.size());
        let (pan_x, pan_y) = view.pan;
        frame.with_save(|frame| {
            frame.translate(Vector::new(pan_x, pan_y));
            paint_scene(frame, &scene, theme, true);
        });
        let Some(point) = state.at else { return vec![frame.into_geometry()] };
        if state.dragging.is_none() {
            let guide = Stroke::default().with_color(theme::fade(t.muted, 0.6)).with_width(1.0);
            let dashed = Stroke { line_dash: LineDash { segments: &[3.0, 3.0], offset: 0 }, ..guide };
            frame.stroke(&Path::line(Point::new(point.x, 0.0), Point::new(point.x, bounds.height)), dashed);
            frame.stroke(&Path::line(Point::new(0.0, point.y), Point::new(bounds.width, point.y)), dashed);
        }
        if let Some(text) = hit(&scene.shapes, point.x - pan_x, point.y - pan_y).map(str::to_string) {
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

    fn mouse_interaction(&self, state: &Pointer, bounds: Rectangle, cursor: Cursor) -> mouse::Interaction {
        if state.dragging.is_some() {
            mouse::Interaction::Grabbing
        } else if self.view.zoom > 1.0 && cursor.is_over(bounds) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}

impl Program<Message> for Thumb<'_> {
    type State = ();

    fn update(&self, _state: &mut (), event: &Event, bounds: Rectangle, cursor: Cursor) -> Option<Action<Message>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if cursor.is_over(bounds) => {
                Some(Action::publish(Message::PlotSelect(self.index)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(&self, _state: &(), renderer: &Renderer, theme: &Theme, bounds: Rectangle, _cursor: Cursor) -> Vec<Geometry> {
        let t = theme::tones(theme);
        let mut frame = Frame::new(renderer, bounds.size());
        let inset = 4.0;
        let inner = Size::new(bounds.width - 2.0 * inset, bounds.height - 2.0 * inset);
        let scene = self.plot.scene(inner.width * 3.0, inner.height * 3.0);
        frame.with_save(|frame| {
            frame.translate(Vector::new(inset, inset));
            frame.scale(1.0_f32 / 3.0);
            paint_scene(frame, &scene, theme, false);
        });
        let border = Path::rounded_rectangle(Point::new(0.5, 0.5), Size::new(bounds.width - 1.0, bounds.height - 1.0), 4.0.into());
        let color = if self.selected { t.accent } else { t.line };
        frame.stroke(&border, Stroke::default().with_color(color).with_width(if self.selected { 2.0 } else { 1.0 }));
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, _state: &(), bounds: Rectangle, cursor: Cursor) -> mouse::Interaction {
        if cursor.is_over(bounds) { mouse::Interaction::Pointer } else { mouse::Interaction::default() }
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

    #[test]
    fn views_zoom_about_the_pointer_and_stay_inside_the_canvas() {
        let size = Size::new(400.0, 200.0);
        let view = PlotView::default().zoomed(2.0, Point::new(100.0, 50.0), size);
        assert_eq!((view.zoom, view.pan), (2.0, (-100.0, -50.0)));
        let moved = view.panned(Vector::new(500.0, 10.0), size);
        assert_eq!(moved.pan, (0.0, -40.0));
        let far = view.panned(Vector::new(-900.0, -900.0), size);
        assert_eq!(far.pan, (-400.0, -200.0));
        let back = view.zoomed(0.5, Point::new(0.0, 0.0), size);
        assert_eq!(back, PlotView::default());
        assert_eq!(PlotView { zoom: 40.0, pan: (0.0, 0.0) }.clamped(size).zoom, MAX_ZOOM);
        assert!(wheel_factor(ScrollDelta::Lines { x: 0.0, y: 1.0 }) > 1.0 && wheel_factor(ScrollDelta::Pixels { x: 0.0, y: -40.0 }) < 1.0);
    }
}
