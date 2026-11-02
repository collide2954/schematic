use std::f32::consts::TAU;

use iced::advanced::text::Alignment;
use iced::alignment::Vertical;
use iced::mouse::{self, Cursor, ScrollDelta};
use iced::widget::canvas::{self, Action, Frame, Geometry, LineDash, Path, Program, Stroke};
use iced::{Color, Event, Pixels, Point, Rectangle, Renderer, Size, Theme, Vector};
use schematic::plot::{Anchor, Paint, Plot, Rgb, SERIES_COLORS, Scene, Shape, arc_points};

use crate::app::Message;
use crate::{fonts, theme};

pub const MAX_ZOOM: f32 = 8.0;
const CLICK_SLOP: f32 = 4.0;
const BRUSH_MIN: f32 = 10.0;
const READOUT_REACH: f32 = 24.0;

#[derive(Debug, Clone, PartialEq)]
pub struct PlotView {
    pub zoom: f32,
    pub pan: (f32, f32),
    pub hidden: Vec<usize>,
}

impl Default for PlotView {
    fn default() -> PlotView {
        PlotView { zoom: 1.0, pan: (0.0, 0.0), hidden: Vec::new() }
    }
}

impl PlotView {
    pub fn is_fresh(&self) -> bool {
        *self == PlotView::default()
    }

    pub fn clamped(mut self, size: Size) -> PlotView {
        let zoom = self.zoom.clamp(1.0, MAX_ZOOM);
        let x = self.pan.0.clamp(size.width - size.width * zoom, 0.0);
        let y = self.pan.1.clamp(size.height - size.height * zoom, 0.0);
        self.zoom = zoom;
        self.pan = (x, y);
        self
    }

    pub fn zoomed(self, factor: f32, about: Point, size: Size) -> PlotView {
        let zoom = (self.zoom * factor).clamp(1.0, MAX_ZOOM);
        let ratio = zoom / self.zoom;
        let pan = (about.x - (about.x - self.pan.0) * ratio, about.y - (about.y - self.pan.1) * ratio);
        PlotView { zoom, pan, hidden: self.hidden }.clamped(size)
    }

    pub fn panned(self, by: Vector, size: Size) -> PlotView {
        let pan = (self.pan.0 + by.x, self.pan.1 + by.y);
        PlotView { zoom: self.zoom, pan, hidden: self.hidden }.clamped(size)
    }

    pub fn framed(self, region: Rectangle, size: Size) -> PlotView {
        let zoom = (size.width / region.width.max(1.0)).min(size.height / region.height.max(1.0)).min(MAX_ZOOM) * self.zoom;
        let ratio = zoom / self.zoom;
        let pan = (self.pan.0 * ratio - region.x * ratio + (size.width - region.width * ratio) / 2.0, self.pan.1 * ratio - region.y * ratio + (size.height - region.height * ratio) / 2.0);
        PlotView { zoom, pan, hidden: self.hidden }.clamped(size)
    }

    pub fn toggled(mut self, series: usize) -> PlotView {
        match self.hidden.iter().position(|s| *s == series) {
            Some(at) => {
                self.hidden.remove(at);
            }
            None => self.hidden.push(series),
        }
        self
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
    pub pressed: Option<Point>,
    pub dragging: Option<Point>,
    pub brush: Option<(Point, Point)>,
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
        Shape::Polygon { points, tip: Some(tip), .. } => {
            let (x0, x1) = points.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
            let (y0, y1) = points.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
            (px >= x0 - 2.0 && px <= x1 + 2.0 && py >= y0 - 2.0 && py <= y1 + 2.0).then_some(tip.as_str())
        }
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

pub fn readout(scene: &Scene, px: f32, py: f32) -> Option<(f32, Vec<String>)> {
    let nearest = scene.samples.iter().min_by(|a, b| (a.x - px).abs().total_cmp(&(b.x - px).abs()))?;
    if (nearest.x - px).abs() > READOUT_REACH {
        return None;
    }
    let _ = py;
    let mut lines: Vec<(usize, String)> = scene.samples.iter().filter(|s| (s.x - nearest.x).abs() < 0.5).map(|s| (s.series, s.text.clone())).collect();
    lines.sort_by_key(|(series, _)| *series);
    lines.dedup();
    Some((nearest.x, lines.into_iter().map(|(_, text)| text).collect()))
}

fn lerp(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: 1.0 }
}

fn paint_color(theme: &Theme, paint: &Paint) -> Color {
    let t = theme::tones(theme);
    match paint {
        Paint::Series(i) => {
            let rgb = Rgb::parse(SERIES_COLORS[i % SERIES_COLORS.len()]).unwrap_or(Rgb(0, 0, 0));
            Color::from_rgb8(rgb.0, rgb.1, rgb.2)
        }
        Paint::Rgb(rgb) => Color::from_rgb8(rgb.0, rgb.1, rgb.2),
        Paint::Heat(level) => {
            if *level < 0.6 {
                lerp(t.raised, t.accent, level / 0.6)
            } else {
                lerp(t.accent, Color::from_rgb8(11, 61, 107), (level - 0.6) / 0.4)
            }
        }
        Paint::Axis => t.muted,
        Paint::Grid => t.line,
        Paint::Text => t.text,
        Paint::Muted => t.muted,
    }
}

fn paint_scene(frame: &mut Frame, scene: &Scene, theme: &Theme, with_text: bool) {
    let color = |paint: &Paint| paint_color(theme, paint);
    let faded = |paint: &Paint, alpha: f32| if alpha < 1.0 { theme::fade(color(paint), alpha) } else { color(paint) };
    for shape in &scene.shapes {
        match shape {
            Shape::Rect { x, y, w, h, paint, alpha, .. } => {
                frame.fill_rectangle(Point::new(*x, *y), Size::new(*w, *h), faded(paint, *alpha));
            }
            Shape::Line { x1, y1, x2, y2, paint, width, dashed } => {
                let path = Path::line(Point::new(*x1, *y1), Point::new(*x2, *y2));
                let mut stroke = Stroke::default().with_color(color(paint)).with_width(*width);
                if *dashed {
                    stroke.line_dash = LineDash { segments: &[4.0, 3.0], offset: 0 };
                }
                frame.stroke(&path, stroke);
            }
            Shape::Polyline { points, paint, width, dashed } => {
                let mut stroke = Stroke::default().with_color(color(paint)).with_width(*width);
                if *dashed {
                    stroke.line_dash = LineDash { segments: &[6.0, 4.0], offset: 0 };
                }
                frame.stroke(&path_of(points, false), stroke);
            }
            Shape::Polygon { points, paint, alpha, .. } => {
                frame.fill(&path_of(points, true), faded(paint, *alpha));
            }
            Shape::Circle { x, y, r, paint, alpha, .. } => frame.fill(&Path::circle(Point::new(*x, *y), *r), faded(paint, *alpha)),
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
            Shape::Text { x, y, text, size, anchor, paint, bold, angle } => {
                let label = canvas::Text {
                    content: text.clone(),
                    position: if *angle == 0.0 { Point::new(*x, *y) } else { Point::ORIGIN },
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
                };
                if *angle == 0.0 {
                    frame.fill_text(label);
                } else {
                    frame.with_save(|frame| {
                        frame.translate(Vector::new(*x, *y));
                        frame.rotate(angle.to_radians());
                        frame.fill_text(label);
                    });
                }
            }
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

fn bubble(frame: &mut Frame, theme: &Theme, lines: &[String], anchor: Point, bounds: Rectangle) {
    let t = theme::tones(theme);
    let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 6.6 + 16.0;
    let height = 10.0 + lines.len() as f32 * 16.0;
    let x = if anchor.x + 14.0 + width > bounds.width { anchor.x - 14.0 - width } else { anchor.x + 14.0 };
    let y = (anchor.y - height / 2.0).clamp(4.0, (bounds.height - height - 4.0).max(4.0));
    frame.fill(&Path::rounded_rectangle(Point::new(x, y), Size::new(width, height), 5.0.into()), t.tooltip);
    for (i, line) in lines.iter().enumerate() {
        frame.fill_text(canvas::Text {
            content: line.clone(),
            position: Point::new(x + 8.0, y + 13.0 + i as f32 * 16.0),
            color: t.tooltip_text,
            size: Pixels(12.0),
            font: fonts::UI,
            align_x: Alignment::Left,
            align_y: Vertical::Center,
            ..canvas::Text::default()
        });
    }
}

impl Chart<'_> {
    fn scene_for(&self, bounds: Rectangle) -> (Scene, PlotView) {
        let view = self.view.clone().clamped(bounds.size());
        let scene = self.plot.scene_hiding(bounds.width * view.zoom, bounds.height * view.zoom, &view.hidden);
        (scene, view)
    }

    fn click(&self, bounds: Rectangle, at: Point) -> Option<Message> {
        let (scene, view) = self.scene_for(bounds);
        let (sx, sy) = (at.x - view.pan.0, at.y - view.pan.1);
        if let Some(entry) = scene.legend.iter().find(|e| sx >= e.x && sx <= e.x + e.w && sy >= e.y && sy <= e.y + e.h) {
            return Some(Message::PlotToggle(entry.series));
        }
        scene.picks.iter().rev().find(|p| sx >= p.x && sx <= p.x + p.w && sy >= p.y && sy <= p.y + p.h).map(|p| Message::PlotPick(p.key.clone()))
    }
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
                    return Some(Action::publish(Message::PlotView(self.view.clone().panned(to - from, bounds.size()))).and_capture());
                }
                if let (Some(start), Some(to)) = (state.pressed, inside)
                    && self.view.zoom <= 1.0
                    && (to - start).x.hypot((to - start).y) > CLICK_SLOP {
                        state.brush = Some((start, to));
                    }
                Some(Action::request_redraw())
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                state.at = None;
                Some(Action::request_redraw())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if inside.is_some() => {
                state.pressed = inside;
                if self.view.zoom > 1.0 {
                    state.dragging = inside;
                }
                Some(Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.pressed.is_some() => {
                let start = state.pressed.take();
                let brush = state.brush.take();
                let was_dragging = state.dragging.take().is_some();
                if let Some((a, b)) = brush {
                    let region = Rectangle::new(Point::new(a.x.min(b.x), a.y.min(b.y)), Size::new((a.x - b.x).abs(), (a.y - b.y).abs()));
                    if region.width >= BRUSH_MIN && region.height >= BRUSH_MIN {
                        return Some(Action::publish(Message::PlotView(self.view.clone().framed(region, bounds.size()))).and_capture());
                    }
                    return Some(Action::request_redraw());
                }
                if was_dragging {
                    return Some(Action::capture());
                }
                let at = inside.or(start)?;
                match self.click(bounds, at) {
                    Some(message) => Some(Action::publish(message).and_capture()),
                    None => Some(Action::request_redraw()),
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let about = inside?;
                let view = self.view.clone().zoomed(wheel_factor(*delta), about, bounds.size());
                (view != self.view).then(|| Action::publish(Message::PlotView(view)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(&self, state: &Pointer, renderer: &Renderer, theme: &Theme, bounds: Rectangle, _cursor: Cursor) -> Vec<Geometry> {
        let t = theme::tones(theme);
        let (scene, view) = self.scene_for(bounds);
        let mut frame = Frame::new(renderer, bounds.size());
        let (pan_x, pan_y) = view.pan;
        frame.with_save(|frame| {
            frame.translate(Vector::new(pan_x, pan_y));
            paint_scene(frame, &scene, theme, true);
        });
        if let Some((a, b)) = state.brush {
            let region = Rectangle::new(Point::new(a.x.min(b.x), a.y.min(b.y)), Size::new((a.x - b.x).abs(), (a.y - b.y).abs()));
            frame.fill_rectangle(region.position(), region.size(), theme::fade(t.accent, 0.12));
            frame.stroke(&Path::rectangle(region.position(), region.size()), Stroke::default().with_color(t.accent).with_width(1.0));
            return vec![frame.into_geometry()];
        }
        let Some(point) = state.at else { return vec![frame.into_geometry()] };
        if state.dragging.is_some() {
            return vec![frame.into_geometry()];
        }
        let guide = Stroke { line_dash: LineDash { segments: &[3.0, 3.0], offset: 0 }, ..Stroke::default().with_color(theme::fade(t.muted, 0.6)).with_width(1.0) };
        if let Some((x, lines)) = readout(&scene, point.x - pan_x, point.y - pan_y) {
            let gx = x + pan_x;
            frame.stroke(&Path::line(Point::new(gx, 0.0), Point::new(gx, bounds.height)), guide);
            bubble(&mut frame, theme, &lines, Point::new(gx, point.y), bounds);
            return vec![frame.into_geometry()];
        }
        frame.stroke(&Path::line(Point::new(point.x, 0.0), Point::new(point.x, bounds.height)), guide);
        frame.stroke(&Path::line(Point::new(0.0, point.y), Point::new(bounds.width, point.y)), guide);
        if let Some(text) = hit(&scene.shapes, point.x - pan_x, point.y - pan_y).map(str::to_string) {
            bubble(&mut frame, theme, &[text], point, bounds);
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, state: &Pointer, bounds: Rectangle, cursor: Cursor) -> mouse::Interaction {
        if state.dragging.is_some() {
            mouse::Interaction::Grabbing
        } else if state.brush.is_some() {
            mouse::Interaction::Crosshair
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
        let scene = self.plot.scene(inner.width, inner.height);
        frame.with_save(|frame| {
            frame.translate(Vector::new(inset, inset));
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
    use schematic::plot::Sample;

    #[test]
    fn hits_find_the_mark_under_the_pointer() {
        let shapes = vec![
            Shape::Rect { x: 10.0, y: 10.0, w: 20.0, h: 30.0, paint: Paint::Series(0), alpha: 1.0, tip: Some("bar".into()) },
            Shape::Circle { x: 100.0, y: 100.0, r: 3.0, paint: Paint::Series(0), alpha: 1.0, tip: Some("point".into()) },
            Shape::Arc { cx: 200.0, cy: 200.0, r: 50.0, inner: 0.0, start: 0.0, end: 1.5, paint: Paint::Series(1), tip: Some("slice".into()) },
            Shape::Rect { x: 0.0, y: 0.0, w: 5.0, h: 5.0, paint: Paint::Grid, alpha: 1.0, tip: None },
            Shape::Polygon { points: vec![(300.0, 300.0), (310.0, 300.0), (305.0, 310.0)], paint: Paint::Series(2), alpha: 1.0, tip: Some("triangle".into()) },
        ];
        assert_eq!(hit(&shapes, 15.0, 20.0), Some("bar"));
        assert_eq!(hit(&shapes, 104.0, 100.0), Some("point"));
        assert_eq!(hit(&shapes, 110.0, 100.0), None);
        assert_eq!(hit(&shapes, 230.0, 230.0), Some("slice"));
        assert_eq!(hit(&shapes, 230.0, 170.0), None);
        assert_eq!(hit(&shapes, 2.0, 2.0), None);
        assert_eq!(hit(&shapes, 305.0, 305.0), Some("triangle"));
    }

    #[test]
    fn views_zoom_about_the_pointer_frame_regions_and_toggle_series() {
        let size = Size::new(400.0, 200.0);
        let view = PlotView::default().zoomed(2.0, Point::new(100.0, 50.0), size);
        assert_eq!((view.zoom, view.pan), (2.0, (-100.0, -50.0)));
        let moved = view.clone().panned(Vector::new(500.0, 10.0), size);
        assert_eq!(moved.pan, (0.0, -40.0));
        let far = view.clone().panned(Vector::new(-900.0, -900.0), size);
        assert_eq!(far.pan, (-400.0, -200.0));
        let back = view.zoomed(0.5, Point::new(0.0, 0.0), size);
        assert!(back.is_fresh());
        assert_eq!(PlotView { zoom: 40.0, ..PlotView::default() }.clamped(size).zoom, MAX_ZOOM);
        assert!(wheel_factor(ScrollDelta::Lines { x: 0.0, y: 1.0 }) > 1.0 && wheel_factor(ScrollDelta::Pixels { x: 0.0, y: -40.0 }) < 1.0);
        let framed = PlotView::default().framed(Rectangle::new(Point::new(100.0, 50.0), Size::new(200.0, 100.0)), size);
        assert_eq!((framed.zoom, framed.pan), (2.0, (-200.0, -100.0)));
        let toggled = PlotView::default().toggled(2).toggled(1).toggled(2);
        assert_eq!(toggled.hidden, [1]);
        assert!(!toggled.is_fresh());
    }

    #[test]
    fn readouts_gather_every_series_at_the_nearest_x() {
        let scene = Scene {
            samples: vec![
                Sample { x: 100.0, y: 10.0, series: 1, text: "b: 2".into() },
                Sample { x: 100.0, y: 20.0, series: 0, text: "a: 1".into() },
                Sample { x: 200.0, y: 30.0, series: 0, text: "a: 3".into() },
            ],
            ..Scene::default()
        };
        assert_eq!(readout(&scene, 110.0, 0.0), Some((100.0, vec!["a: 1".to_string(), "b: 2".to_string()])));
        assert_eq!(readout(&scene, 190.0, 0.0), Some((200.0, vec!["a: 3".to_string()])));
        assert_eq!(readout(&scene, 150.0, 0.0), None);
    }
}
