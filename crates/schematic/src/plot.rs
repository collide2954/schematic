use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bar,
    Line,
    Scatter,
    Histogram,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub name: Option<String>,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plot {
    pub kind: Kind,
    pub title: Option<String>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub categories: Vec<String>,
    pub dates: bool,
    pub step: f64,
    pub series: Vec<Series>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    Series(usize),
    Axis,
    Grid,
    Text,
    Muted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect { x: f32, y: f32, w: f32, h: f32, paint: Paint },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, paint: Paint, width: f32 },
    Polyline { points: Vec<(f32, f32)>, paint: Paint, width: f32 },
    Circle { x: f32, y: f32, r: f32, paint: Paint },
    Text { x: f32, y: f32, text: String, size: f32, anchor: Anchor, paint: Paint, bold: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub shapes: Vec<Shape>,
}

const BAR_WIDTH: usize = 40;
const GRID_W: usize = 40;
const GRID_H: usize = 10;
const MARKS: [char; 4] = ['•', '○', '▪', '▫'];
pub const SERIES_COLORS: [&str; 6] = ["#0f7b86", "#7b3fe4", "#b4540f", "#c0286e", "#2458d3", "#2a8a5a"];

pub fn number(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") }
}

pub fn ticks(lo: f64, hi: f64, count: usize) -> (Vec<f64>, f64) {
    if !(hi > lo) || !lo.is_finite() || !hi.is_finite() {
        return (vec![lo], 1.0);
    }
    let raw = (hi - lo) / count.max(1) as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let step = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    } * magnitude;
    let start = (lo / step).floor() * step;
    let end = (hi / step).ceil() * step;
    let n = ((end - start) / step).round() as usize;
    ((0..=n).map(|i| start + i as f64 * step).collect(), step)
}

pub fn tick_label(value: f64, step: f64, dates: bool) -> String {
    if dates {
        return crate::date::iso(value.round() as i32);
    }
    if value.fract() == 0.0 {
        return number(value);
    }
    let decimals = (-step.log10().floor()).max(0.0) as usize;
    format!("{value:.decimals$}")
}

pub fn histogram(values: &[f64], bins: usize) -> Plot {
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let (min, width) = if values.is_empty() {
        (0.0, 1.0)
    } else if max > min {
        (min, (max - min) / bins as f64)
    } else {
        (min, 1.0)
    };
    let mut counts = vec![0.0; bins];
    for &value in values {
        let index = ((value - min) / width).floor().max(0.0) as usize;
        counts[index.min(bins - 1)] += 1.0;
    }
    let edges: Vec<f64> = (0..bins).map(|i| min + i as f64 * width).collect();
    let categories = edges.iter().map(|lo| format!("{}–{}", number(*lo), number(lo + width))).collect();
    Plot {
        kind: Kind::Histogram,
        title: None,
        x_label: None,
        y_label: Some("count".to_string()),
        categories,
        dates: false,
        step: width,
        series: vec![Series { name: None, xs: edges, ys: counts }],
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("{n} {word}") } else { format!("{n} {word}s") }
}

impl Plot {
    pub fn new(kind: Kind) -> Plot {
        Plot {
            kind,
            title: None,
            x_label: None,
            y_label: None,
            categories: Vec::new(),
            dates: false,
            step: 0.0,
            series: Vec::new(),
        }
    }

    fn points(&self) -> usize {
        self.series.iter().map(|s| s.xs.len()).sum()
    }

    pub fn describe(&self) -> String {
        match self.kind {
            Kind::Bar => format!("bar chart with {}", plural(self.points(), "bar")),
            Kind::Line if self.series.len() > 1 => format!("line chart with {}", plural(self.series.len(), "series")),
            Kind::Line => format!("line chart with {}", plural(self.points(), "point")),
            Kind::Scatter => format!("scatter with {}", plural(self.points(), "point")),
            Kind::Histogram => format!("histogram with {}", plural(self.points(), "bin")),
        }
    }

    fn x_text(&self, x: f64) -> String {
        if self.dates { crate::date::iso(x.round() as i32) } else { number(x) }
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        if let Some(title) = &self.title {
            out.push_str(title);
            out.push('\n');
        }
        match self.kind {
            Kind::Bar | Kind::Histogram => self.text_bars(&mut out),
            Kind::Line | Kind::Scatter => self.text_grid(&mut out),
        }
        out
    }

    fn bar_rows(&self) -> Vec<(String, f64)> {
        let multi = self.series.len() > 1;
        let mut rows = Vec::new();
        if self.kind == Kind::Histogram {
            for series in &self.series {
                for (i, y) in series.ys.iter().enumerate() {
                    rows.push((self.categories.get(i).cloned().unwrap_or_default(), *y));
                }
            }
            return rows;
        }
        for (c, category) in self.categories.iter().enumerate() {
            for series in &self.series {
                let Some(i) = series.xs.iter().position(|x| *x as usize == c) else { continue };
                let label = match (&series.name, multi) {
                    (Some(name), true) => format!("{category} {name}"),
                    _ => category.clone(),
                };
                rows.push((label, series.ys[i]));
            }
        }
        rows
    }

    fn text_bars(&self, out: &mut String) {
        let rows = self.bar_rows();
        let width = rows.iter().map(|(label, _)| label.chars().count()).max().unwrap_or(0);
        let max = rows.iter().map(|(_, v)| v.abs()).fold(0.0, f64::max);
        for (i, (label, value)) in rows.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let n = if max > 0.0 { (value.abs() / max * BAR_WIDTH as f64).round() as usize } else { 0 };
            let _ = write!(out, "{label:<width$} {} {}", "█".repeat(n), number(*value));
        }
    }

    fn text_grid(&self, out: &mut String) {
        let points: Vec<(usize, f64, f64)> = self
            .series
            .iter()
            .enumerate()
            .flat_map(|(i, s)| s.xs.iter().zip(&s.ys).map(move |(x, y)| (i, *x, *y)))
            .collect();
        if points.is_empty() {
            out.push_str("(no points)");
            return;
        }
        let (xmin, xmax) = points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
        let (ymin, ymax) = points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.2), hi.max(p.2)));
        let col = |x: f64| if xmax > xmin { ((x - xmin) / (xmax - xmin) * (GRID_W - 1) as f64).round() as usize } else { GRID_W / 2 };
        let row = |y: f64| {
            let up = if ymax > ymin { ((y - ymin) / (ymax - ymin) * (GRID_H - 1) as f64).round() as usize } else { GRID_H / 2 };
            GRID_H - 1 - up
        };
        let mut grid = vec![vec![' '; GRID_W]; GRID_H];
        if self.kind == Kind::Line {
            for series in &self.series {
                let mut cells: Vec<(usize, usize)> = series.xs.iter().zip(&series.ys).map(|(x, y)| (col(*x), row(*y))).collect();
                cells.sort();
                for pair in cells.windows(2) {
                    for (c, r) in segment(pair[0], pair[1]) {
                        if grid[r][c] == ' ' {
                            grid[r][c] = '·';
                        }
                    }
                }
            }
        }
        for (i, x, y) in &points {
            grid[row(*y)][col(*x)] = MARKS[i % MARKS.len()];
        }
        let (lo, hi) = (number(ymin), number(ymax));
        let width = lo.chars().count().max(hi.chars().count());
        for (r, cells) in grid.iter().enumerate() {
            let label = if r == 0 { hi.as_str() } else if r == GRID_H - 1 { lo.as_str() } else { "" };
            let line: String = cells.iter().collect();
            let _ = writeln!(out, "{label:>width$} |{}", line.trim_end());
        }
        let _ = writeln!(out, "{:>width$} +{}", "", "-".repeat(GRID_W));
        let (left, right) = (self.x_text(xmin), self.x_text(xmax));
        let gap = GRID_W.saturating_sub(left.chars().count() + right.chars().count());
        let _ = write!(out, "{:>width$}  {left}{}{right}", "", " ".repeat(gap));
        if self.series.len() > 1 {
            let legend: Vec<String> = self
                .series
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{} {}", MARKS[i % MARKS.len()], s.name.clone().unwrap_or_default()))
                .collect();
            let _ = write!(out, "\n{}", legend.join("  "));
        }
    }

    pub fn scene(&self, width: f32, height: f32) -> Scene {
        let mut shapes = Vec::new();
        let multi = self.series.len() > 1;
        let top = 16.0 + if self.title.is_some() { 24.0 } else { 0.0 } + if multi || self.y_label.is_some() { 22.0 } else { 0.0 };
        let bottom = 34.0 + if self.x_label.is_some() { 18.0 } else { 0.0 };
        let right = if self.dates { 40.0 } else { 24.0 };
        let (x0, x1, y0, y1) = (58.0f32, width - right, top, height - bottom);
        let (plot_w, plot_h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        if let Some(title) = &self.title {
            shapes.push(Shape::Text { x: width / 2.0, y: 18.0, text: title.clone(), size: 15.0, anchor: Anchor::Middle, paint: Paint::Text, bold: true });
        }
        if let Some(label) = &self.y_label {
            shapes.push(Shape::Text { x: x0, y: y0 - 11.0, text: label.clone(), size: 12.0, anchor: Anchor::Start, paint: Paint::Muted, bold: false });
        }
        if let Some(label) = &self.x_label {
            shapes.push(Shape::Text { x: (x0 + x1) / 2.0, y: height - 10.0, text: label.clone(), size: 12.0, anchor: Anchor::Middle, paint: Paint::Muted, bold: false });
        }
        let ys: Vec<f64> = self.series.iter().flat_map(|s| s.ys.iter().copied()).collect();
        let bars = matches!(self.kind, Kind::Bar | Kind::Histogram);
        let (mut ymin, mut ymax) = ys.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), y| (lo.min(*y), hi.max(*y)));
        if ys.is_empty() {
            (ymin, ymax) = (0.0, 1.0);
        }
        if bars {
            ymin = ymin.min(0.0);
            ymax = ymax.max(0.0);
        }
        if ymax <= ymin {
            ymax = ymin + 1.0;
        }
        let (yticks, ystep) = ticks(ymin, ymax, ((plot_h / 40.0) as usize).clamp(2, 6));
        let (ylo, yhi) = (yticks[0], *yticks.last().unwrap());
        let sy = |y: f64| y1 - ((y - ylo) / (yhi - ylo)) as f32 * plot_h;
        for tick in &yticks {
            let y = sy(*tick);
            shapes.push(Shape::Line { x1: x0, y1: y, x2: x1, y2: y, paint: Paint::Grid, width: 1.0 });
            shapes.push(Shape::Text { x: x0 - 8.0, y, text: tick_label(*tick, ystep, false), size: 11.0, anchor: Anchor::End, paint: Paint::Muted, bold: false });
        }
        match self.kind {
            Kind::Bar => {
                let n = self.categories.len().max(1);
                let group = plot_w / n as f32;
                let slot = group * 0.72 / self.series.len().max(1) as f32;
                let base = sy(0.0);
                for (si, series) in self.series.iter().enumerate() {
                    for (x, y) in series.xs.iter().zip(&series.ys) {
                        let left = x0 + *x as f32 * group + group * 0.14 + si as f32 * slot;
                        let top = sy(*y);
                        shapes.push(Shape::Rect { x: left, y: top.min(base), w: slot.max(1.0) - 1.0, h: (top - base).abs(), paint: Paint::Series(si) });
                    }
                }
                for (c, category) in self.categories.iter().enumerate() {
                    shapes.push(Shape::Text { x: x0 + (c as f32 + 0.5) * group, y: y1 + 14.0, text: category.clone(), size: 11.0, anchor: Anchor::Middle, paint: Paint::Muted, bold: false });
                }
            }
            Kind::Histogram => {
                let Some(series) = self.series.first() else { return Scene { width, height, shapes } };
                let xlo = series.xs.first().copied().unwrap_or(0.0);
                let xhi = series.xs.last().copied().unwrap_or(0.0) + self.step;
                let span = (xhi - xlo).max(f64::EPSILON);
                let sx = |x: f64| x0 + ((x - xlo) / span) as f32 * plot_w;
                let base = sy(0.0);
                for (x, y) in series.xs.iter().zip(&series.ys) {
                    let (left, right) = (sx(*x), sx(x + self.step));
                    shapes.push(Shape::Rect { x: left + 0.5, y: sy(*y), w: (right - left - 1.0).max(1.0), h: base - sy(*y), paint: Paint::Series(0) });
                }
                let mut edges: Vec<f64> = series.xs.clone();
                edges.push(xhi);
                let (labels, step) = if edges.len() <= 12 { (edges, self.step) } else { ticks(xlo, xhi, 6) };
                for edge in labels {
                    shapes.push(Shape::Text { x: sx(edge), y: y1 + 14.0, text: tick_label(edge, step, false), size: 11.0, anchor: Anchor::Middle, paint: Paint::Muted, bold: false });
                }
            }
            Kind::Line | Kind::Scatter => {
                let xs: Vec<f64> = self.series.iter().flat_map(|s| s.xs.iter().copied()).collect();
                let (mut xmin, mut xmax) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
                if xs.is_empty() {
                    (xmin, xmax) = (0.0, 1.0);
                }
                if xmax <= xmin {
                    xmax = xmin + 1.0;
                }
                let count = ((plot_w / if self.dates { 90.0 } else { 60.0 }) as usize).clamp(2, 6);
                let (xticks, xstep) = ticks(xmin, xmax, count);
                let pad = (xmax - xmin) * 0.03;
                let (xlo, xhi) = (xmin - pad, xmax + pad);
                let sx = |x: f64| x0 + ((x - xlo) / (xhi - xlo)) as f32 * plot_w;
                for tick in xticks.iter().filter(|t| **t >= xlo && **t <= xhi) {
                    shapes.push(Shape::Text { x: sx(*tick), y: y1 + 14.0, text: tick_label(*tick, xstep, self.dates), size: 11.0, anchor: Anchor::Middle, paint: Paint::Muted, bold: false });
                }
                for (si, series) in self.series.iter().enumerate() {
                    let points: Vec<(f32, f32)> = series.xs.iter().zip(&series.ys).map(|(x, y)| (sx(*x), sy(*y))).collect();
                    if self.kind == Kind::Line && points.len() > 1 {
                        shapes.push(Shape::Polyline { points: points.clone(), paint: Paint::Series(si), width: 2.0 });
                    }
                    if self.kind == Kind::Scatter || points.len() <= 60 {
                        let r = if self.kind == Kind::Scatter { 3.5 } else { 2.5 };
                        shapes.extend(points.iter().map(|(x, y)| Shape::Circle { x: *x, y: *y, r, paint: Paint::Series(si) }));
                    }
                }
            }
        }
        shapes.push(Shape::Line { x1: x0, y1: y0, x2: x0, y2: y1, paint: Paint::Axis, width: 1.0 });
        shapes.push(Shape::Line { x1: x0, y1: y1, x2: x1, y2: y1, paint: Paint::Axis, width: 1.0 });
        if multi {
            let mut right = x1;
            for (si, series) in self.series.iter().enumerate().rev() {
                let name = series.name.clone().unwrap_or_default();
                let text_w = name.chars().count() as f32 * 6.5;
                right -= text_w;
                shapes.push(Shape::Text { x: right, y: y0 - 11.0, text: name, size: 11.0, anchor: Anchor::Start, paint: Paint::Muted, bold: false });
                right -= 16.0;
                shapes.push(Shape::Rect { x: right, y: y0 - 16.0, w: 10.0, h: 10.0, paint: Paint::Series(si) });
                right -= 14.0;
            }
        }
        Scene { width, height, shapes }
    }

    pub fn svg(&self) -> String {
        let scene = self.scene(640.0, 400.0);
        let color = |paint: &Paint| match paint {
            Paint::Series(i) => SERIES_COLORS[i % SERIES_COLORS.len()],
            Paint::Axis => "#56637a",
            Paint::Grid => "#e3e7ee",
            Paint::Text => "#1a2230",
            Paint::Muted => "#66728a",
        };
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" font-family=\"sans-serif\">\n",
            w = scene.width,
            h = scene.height
        );
        for shape in &scene.shapes {
            match shape {
                Shape::Rect { x, y, w, h, paint } => {
                    let _ = writeln!(out, "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"{}\"/>", color(paint));
                }
                Shape::Line { x1, y1, x2, y2, paint, width } => {
                    let _ = writeln!(out, "<line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" stroke=\"{}\" stroke-width=\"{width}\"/>", color(paint));
                }
                Shape::Polyline { points, paint, width } => {
                    let points: Vec<String> = points.iter().map(|(x, y)| format!("{x:.1},{y:.1}")).collect();
                    let _ = writeln!(out, "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{width}\"/>", points.join(" "), color(paint));
                }
                Shape::Circle { x, y, r, paint } => {
                    let _ = writeln!(out, "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{r}\" fill=\"{}\"/>", color(paint));
                }
                Shape::Text { x, y, text, size, anchor, paint, bold } => {
                    let anchor = match anchor {
                        Anchor::Start => "start",
                        Anchor::Middle => "middle",
                        Anchor::End => "end",
                    };
                    let weight = if *bold { " font-weight=\"bold\"" } else { "" };
                    let _ = writeln!(
                        out,
                        "<text x=\"{x:.1}\" y=\"{y:.1}\" text-anchor=\"{anchor}\" dominant-baseline=\"middle\" font-size=\"{size}\" fill=\"{}\"{weight}>{}</text>",
                        color(paint),
                        escape(text)
                    );
                }
            }
        }
        out.push_str("</svg>\n");
        out
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn segment((c0, r0): (usize, usize), (c1, r1): (usize, usize)) -> Vec<(usize, usize)> {
    let (mut x, mut y) = (c0 as i64, r0 as i64);
    let (x1, y1) = (c1 as i64, r1 as i64);
    let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
    let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
    let mut err = dx + dy;
    let mut cells = Vec::new();
    while (x, y) != (x1, y1) {
        cells.push((x as usize, y as usize));
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
    cells.retain(|cell| *cell != (c0, r0));
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_land_on_round_numbers() {
        assert_eq!(ticks(0.0, 42.0, 5), (vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0], 10.0));
        let (fractional, step) = ticks(0.0, 1.0, 5);
        let labels: Vec<String> = fractional.iter().map(|t| tick_label(*t, step, false)).collect();
        assert_eq!(labels, ["0", "0.2", "0.4", "0.6", "0.8", "1"]);
        assert_eq!(ticks(5.0, 5.0, 5), (vec![5.0], 1.0));
        assert_eq!(tick_label(19723.0, 10.0, true), "2024-01-01");
        assert_eq!(ticks(-3.0, 3.0, 4).0, [-4.0, -2.0, 0.0, 2.0, 4.0]);
    }

    #[test]
    fn histograms_bin_values_and_clamp_the_maximum() {
        let plot = histogram(&[1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 9.0], 4);
        assert_eq!(plot.series[0].ys, [3.0, 3.0, 0.0, 1.0]);
        assert_eq!(plot.series[0].xs, [1.0, 3.0, 5.0, 7.0]);
        assert_eq!(plot.categories, ["1–3", "3–5", "5–7", "7–9"]);
        assert_eq!(plot.describe(), "histogram with 4 bins");
        assert_eq!(histogram(&[2.0, 2.0], 3).series[0].ys, [2.0, 0.0, 0.0]);
        assert_eq!(histogram(&[], 2).series[0].ys, [0.0, 0.0]);
    }

    #[test]
    fn scenes_place_bars_within_the_plot_area() {
        let mut plot = Plot::new(Kind::Bar);
        plot.categories = vec!["a".into(), "b".into()];
        plot.series = vec![Series { name: None, xs: vec![0.0, 1.0], ys: vec![1.0, 2.0] }];
        let scene = plot.scene(640.0, 400.0);
        let rects: Vec<&Shape> = scene.shapes.iter().filter(|s| matches!(s, Shape::Rect { .. })).collect();
        assert_eq!(rects.len(), 2);
        let Shape::Rect { x, y, w, h, .. } = rects[1] else { unreachable!() };
        assert!(*x > 58.0 && x + w < 622.0 && *y >= 16.0 && y + h <= 366.1, "{rects:?}");
        let Shape::Rect { h: short, .. } = rects[0] else { unreachable!() };
        assert!((h / short - 2.0).abs() < 0.01);
        assert_eq!(plot.describe(), "bar chart with 2 bars");
        let svg = plot.svg();
        assert!(svg.starts_with("<svg xmlns") && svg.ends_with("</svg>\n"));
        assert_eq!(escape("a<b&c"), "a&lt;b&amp;c");
    }

    #[test]
    fn segments_connect_grid_cells() {
        assert_eq!(segment((0, 0), (3, 0)), [(1, 0), (2, 0)]);
        assert_eq!(segment((0, 3), (3, 0)), [(1, 2), (2, 1)]);
        assert!(segment((5, 5), (5, 5)).is_empty());
    }
}
