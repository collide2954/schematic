use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bar,
    Line,
    Scatter,
    Histogram,
    Area,
    Box,
    Pie,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub name: Option<String>,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoxStats {
    pub label: String,
    pub low: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub high: f64,
    pub outliers: Vec<f64>,
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
    pub boxes: Vec<BoxStats>,
    pub stacked: bool,
    pub horizontal: bool,
    pub donut: bool,
    pub x_range: Option<(f64, f64)>,
    pub y_range: Option<(f64, f64)>,
    pub x_log: bool,
    pub y_log: bool,
    pub x_format: Option<String>,
    pub y_format: Option<String>,
    pub hlines: Vec<f64>,
    pub vlines: Vec<f64>,
    pub labels: bool,
    pub panels: Vec<Plot>,
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
    Rect { x: f32, y: f32, w: f32, h: f32, paint: Paint, tip: Option<String> },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, paint: Paint, width: f32, dashed: bool },
    Polyline { points: Vec<(f32, f32)>, paint: Paint, width: f32 },
    Polygon { points: Vec<(f32, f32)>, paint: Paint },
    Circle { x: f32, y: f32, r: f32, paint: Paint, tip: Option<String> },
    Arc { cx: f32, cy: f32, r: f32, inner: f32, start: f32, end: f32, paint: Paint, tip: Option<String> },
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

fn quantile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let position = p * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    sorted[lower] + (sorted[upper] - sorted[lower]) * (position - lower as f64)
}

impl BoxStats {
    pub fn of(label: &str, values: &[f64]) -> BoxStats {
        let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
        sorted.sort_by(f64::total_cmp);
        let (q1, median, q3) = (quantile(&sorted, 0.25), quantile(&sorted, 0.5), quantile(&sorted, 0.75));
        let reach = 1.5 * (q3 - q1);
        let (lower, upper) = (q1 - reach, q3 + reach);
        let inside = || sorted.iter().copied().filter(|v| *v >= lower && *v <= upper);
        BoxStats {
            label: label.to_string(),
            low: inside().fold(f64::INFINITY, f64::min),
            q1,
            median,
            q3,
            high: inside().fold(f64::NEG_INFINITY, f64::max),
            outliers: sorted.iter().copied().filter(|v| *v < lower || *v > upper).collect(),
        }
    }
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
    let mut plot = Plot::new(Kind::Histogram);
    plot.y_label = Some("count".to_string());
    plot.categories = categories;
    plot.step = width;
    plot.series = vec![Series { name: None, xs: edges, ys: counts }];
    plot
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("{n} {word}") } else { format!("{n} {word}s") }
}

struct Axis {
    lo: f64,
    hi: f64,
    ticks: Vec<f64>,
    step: f64,
}

impl Axis {
    fn numeric(lo: f64, hi: f64, count: usize, include_zero: bool) -> Axis {
        let (mut lo, mut hi) = if lo.is_finite() && hi.is_finite() { (lo, hi) } else { (0.0, 1.0) };
        if include_zero {
            lo = lo.min(0.0);
            hi = hi.max(0.0);
        }
        if hi <= lo {
            hi = lo + 1.0;
        }
        let (ticks, step) = ticks(lo, hi, count);
        Axis { lo: ticks[0], hi: *ticks.last().unwrap(), ticks, step }
    }

    fn hugging(lo: f64, hi: f64, count: usize) -> Axis {
        let (mut lo, mut hi) = if lo.is_finite() && hi.is_finite() { (lo, hi) } else { (0.0, 1.0) };
        if hi <= lo {
            hi = lo + 1.0;
        }
        let (ticks, step) = ticks(lo, hi, count);
        let pad = (hi - lo) * 0.03;
        lo -= pad;
        hi += pad;
        let ticks = ticks.into_iter().filter(|t| *t >= lo && *t <= hi).collect();
        Axis { lo, hi, ticks, step }
    }

    fn place(&self, value: f64, from: f32, to: f32) -> f32 {
        from + ((value - self.lo) / (self.hi - self.lo)) as f32 * (to - from)
    }
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
            boxes: Vec::new(),
            stacked: false,
            horizontal: false,
            donut: false,
            x_range: None,
            y_range: None,
            x_log: false,
            y_log: false,
            x_format: None,
            y_format: None,
            hlines: Vec::new(),
            vlines: Vec::new(),
            labels: false,
            panels: Vec::new(),
        }
    }

    fn points(&self) -> usize {
        self.series.iter().map(|s| s.xs.len()).sum()
    }

    pub fn describe(&self) -> String {
        let many = self.series.len() > 1;
        match self.kind {
            Kind::Bar => format!("bar chart with {}", plural(self.points(), "bar")),
            Kind::Line if many => format!("line chart with {}", plural(self.series.len(), "series")),
            Kind::Line => format!("line chart with {}", plural(self.points(), "point")),
            Kind::Area if many => format!("area chart with {}", plural(self.series.len(), "series")),
            Kind::Area => format!("area chart with {}", plural(self.points(), "point")),
            Kind::Scatter => format!("scatter with {}", plural(self.points(), "point")),
            Kind::Histogram => format!("histogram with {}", plural(self.points(), "bin")),
            Kind::Box => format!("box plot with {}", plural(self.boxes.len(), "group")),
            Kind::Pie => format!("pie chart with {}", plural(self.points(), "slice")),
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
            Kind::Pie => self.text_pie(&mut out),
            Kind::Box => self.text_boxes(&mut out),
            Kind::Line | Kind::Scatter | Kind::Area => self.text_grid(&mut out),
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

    fn write_bars(out: &mut String, rows: &[(String, f64, String)]) {
        let width = rows.iter().map(|(label, _, _)| label.chars().count()).max().unwrap_or(0);
        let max = rows.iter().map(|(_, v, _)| v.abs()).fold(0.0, f64::max);
        for (i, (label, value, suffix)) in rows.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let n = if max > 0.0 { (value.abs() / max * BAR_WIDTH as f64).round() as usize } else { 0 };
            let _ = write!(out, "{label:<width$} {} {}{suffix}", "█".repeat(n), number(*value));
        }
    }

    fn text_bars(&self, out: &mut String) {
        let rows: Vec<(String, f64, String)> = self.bar_rows().into_iter().map(|(l, v)| (l, v, String::new())).collect();
        Self::write_bars(out, &rows);
    }

    fn text_pie(&self, out: &mut String) {
        let Some(series) = self.series.first() else { return };
        let total: f64 = series.ys.iter().sum();
        let rows: Vec<(String, f64, String)> = series
            .xs
            .iter()
            .zip(&series.ys)
            .map(|(x, y)| {
                let label = self.categories.get(*x as usize).cloned().unwrap_or_default();
                let share = if total > 0.0 { (y / total * 100.0).round() } else { 0.0 };
                (label, *y, format!(" ({share}%)"))
            })
            .collect();
        Self::write_bars(out, &rows);
    }

    fn text_boxes(&self, out: &mut String) {
        let width = self.boxes.iter().map(|b| b.label.chars().count()).max().unwrap_or(0);
        let lo = self.boxes.iter().map(|b| b.outliers.iter().copied().fold(b.low, f64::min)).fold(f64::INFINITY, f64::min);
        let hi = self.boxes.iter().map(|b| b.outliers.iter().copied().fold(b.high, f64::max)).fold(f64::NEG_INFINITY, f64::max);
        let col = |v: f64| if hi > lo { ((v - lo) / (hi - lo) * (GRID_W - 1) as f64).round() as usize } else { GRID_W / 2 };
        for (i, b) in self.boxes.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let mut cells = vec![' '; GRID_W];
            for cell in cells.iter_mut().take(col(b.high) + 1).skip(col(b.low)) {
                *cell = '-';
            }
            cells[col(b.low)] = '+';
            cells[col(b.high)] = '+';
            for cell in cells.iter_mut().take(col(b.q3) + 1).skip(col(b.q1)) {
                *cell = '=';
            }
            cells[col(b.q1)] = '[';
            cells[col(b.q3)] = ']';
            cells[col(b.median)] = '|';
            for outlier in &b.outliers {
                cells[col(*outlier)] = 'o';
            }
            let glyphs: String = cells.iter().collect();
            let _ = write!(
                out,
                "{:<width$} {glyphs} {} {} {} {} {}",
                b.label,
                number(b.low),
                number(b.q1),
                number(b.median),
                number(b.q3),
                number(b.high)
            );
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
        if matches!(self.kind, Kind::Line | Kind::Area) {
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

    fn stack_totals(&self) -> Vec<f64> {
        let mut totals: BTreeMap<u64, f64> = BTreeMap::new();
        for series in &self.series {
            for (x, y) in series.xs.iter().zip(&series.ys) {
                if *y > 0.0 {
                    *totals.entry(x.to_bits()).or_default() += y;
                }
            }
        }
        totals.into_values().collect()
    }

    fn value_extent(&self) -> (f64, f64) {
        let fold = |values: &mut dyn Iterator<Item = f64>| values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
        match self.kind {
            Kind::Box => {
                let mut all = self.boxes.iter().flat_map(|b| b.outliers.iter().copied().chain([b.low, b.high]));
                fold(&mut all)
            }
            Kind::Bar | Kind::Area if self.stacked || (self.kind == Kind::Area && self.series.len() > 1) => {
                let totals = self.stack_totals();
                let (lo, _) = fold(&mut self.series.iter().flat_map(|s| s.ys.iter().copied()));
                let (_, hi) = fold(&mut totals.into_iter());
                (lo.min(0.0), hi)
            }
            _ => fold(&mut self.series.iter().flat_map(|s| s.ys.iter().copied())),
        }
    }

    fn text_shape(x: f32, y: f32, text: String, size: f32, anchor: Anchor, paint: Paint) -> Shape {
        Shape::Text { x, y, text, size, anchor, paint, bold: false }
    }

    pub fn scene(&self, width: f32, height: f32) -> Scene {
        if self.kind == Kind::Pie {
            return self.pie_scene(width, height);
        }
        let mut shapes = Vec::new();
        let multi = self.series.len() > 1 && self.kind != Kind::Box;
        let (value_label, category_label) = (self.y_label.clone(), self.x_label.clone());
        let (top_label, bottom_label) =
            if self.horizontal { (category_label.clone(), value_label.clone()) } else { (value_label.clone(), category_label.clone()) };
        let top = 16.0 + if self.title.is_some() { 24.0 } else { 0.0 } + if multi || top_label.is_some() { 22.0 } else { 0.0 };
        let bottom = 34.0 + if bottom_label.is_some() { 18.0 } else { 0.0 };
        let longest = self.categories.iter().map(|c| c.chars().count()).max().unwrap_or(0) as f32;
        let left = if self.horizontal { 20.0 + longest * 7.0 } else { 58.0 };
        let right = if self.dates { 40.0 } else { 24.0 };
        let (x0, x1, y0, y1) = (left, width - right, top, height - bottom);
        let (plot_w, plot_h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        if let Some(title) = &self.title {
            shapes.push(Shape::Text { x: width / 2.0, y: 18.0, text: title.clone(), size: 15.0, anchor: Anchor::Middle, paint: Paint::Text, bold: true });
        }
        if let Some(label) = top_label {
            shapes.push(Self::text_shape(x0, y0 - 11.0, label, 12.0, Anchor::Start, Paint::Muted));
        }
        if let Some(label) = bottom_label {
            shapes.push(Self::text_shape((x0 + x1) / 2.0, height - 10.0, label, 12.0, Anchor::Middle, Paint::Muted));
        }
        let bars = matches!(self.kind, Kind::Bar | Kind::Histogram | Kind::Area);
        let (vlo, vhi) = self.value_extent();
        let value_count = (((if self.horizontal { plot_w } else { plot_h }) / 40.0) as usize).clamp(2, 6);
        let value_axis = Axis::numeric(vlo, vhi, value_count, bars);
        let value_at = |v: f64| if self.horizontal { value_axis.place(v, x0, x1) } else { value_axis.place(v, y1, y0) };
        for tick in &value_axis.ticks {
            let label = tick_label(*tick, value_axis.step, false);
            if self.horizontal {
                let x = value_at(*tick);
                shapes.push(Shape::Line { x1: x, y1: y0, x2: x, y2: y1, paint: Paint::Grid, width: 1.0, dashed: false });
                shapes.push(Self::text_shape(x, y1 + 14.0, label, 11.0, Anchor::Middle, Paint::Muted));
            } else {
                let y = value_at(*tick);
                shapes.push(Shape::Line { x1: x0, y1: y, x2: x1, y2: y, paint: Paint::Grid, width: 1.0, dashed: false });
                shapes.push(Self::text_shape(x0 - 8.0, y, label, 11.0, Anchor::End, Paint::Muted));
            }
        }
        match self.kind {
            Kind::Bar => self.bar_shapes(&mut shapes, (x0, x1, y0, y1), &value_at),
            Kind::Box => self.box_shapes(&mut shapes, (x0, x1, y0, y1), &value_at),
            Kind::Histogram => {
                if let Some(series) = self.series.first() {
                    let xlo = series.xs.first().copied().unwrap_or(0.0);
                    let xhi = series.xs.last().copied().unwrap_or(0.0) + self.step;
                    let span = (xhi - xlo).max(f64::EPSILON);
                    let sx = |x: f64| x0 + ((x - xlo) / span) as f32 * plot_w;
                    let base = value_at(0.0);
                    for (x, y) in series.xs.iter().zip(&series.ys) {
                        let (left, right) = (sx(*x), sx(x + self.step));
                        let tip = format!("{}: {}", self.categories.get(series.xs.iter().position(|v| v == x).unwrap_or(0)).cloned().unwrap_or_default(), number(*y));
                        shapes.push(Shape::Rect { x: left + 0.5, y: value_at(*y), w: (right - left - 1.0).max(1.0), h: base - value_at(*y), paint: Paint::Series(0), tip: Some(tip) });
                    }
                    let mut edges: Vec<f64> = series.xs.clone();
                    edges.push(xhi);
                    let (labels, step) = if edges.len() <= 12 { (edges, self.step) } else { ticks(xlo, xhi, 6) };
                    for edge in labels {
                        shapes.push(Self::text_shape(sx(edge), y1 + 14.0, tick_label(edge, step, false), 11.0, Anchor::Middle, Paint::Muted));
                    }
                }
            }
            Kind::Line | Kind::Scatter | Kind::Area => {
                let xs: Vec<f64> = self.series.iter().flat_map(|s| s.xs.iter().copied()).collect();
                let (xmin, xmax) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
                let count = ((plot_w / if self.dates { 90.0 } else { 60.0 }) as usize).clamp(2, 6);
                let x_axis = Axis::hugging(xmin, xmax, count);
                let sx = |x: f64| x_axis.place(x, x0, x1);
                for tick in &x_axis.ticks {
                    shapes.push(Self::text_shape(sx(*tick), y1 + 14.0, tick_label(*tick, x_axis.step, self.dates), 11.0, Anchor::Middle, Paint::Muted));
                }
                let stack = self.kind == Kind::Area && multi;
                let mut base: BTreeMap<u64, f64> = BTreeMap::new();
                for (si, series) in self.series.iter().enumerate() {
                    let floors: Vec<f64> = series.xs.iter().map(|x| if stack { base.get(&x.to_bits()).copied().unwrap_or(0.0) } else { 0.0 }).collect();
                    let tops: Vec<f64> = series.ys.iter().zip(&floors).map(|(y, f)| y + f).collect();
                    let points: Vec<(f32, f32)> = series.xs.iter().zip(&tops).map(|(x, y)| (sx(*x), value_at(*y))).collect();
                    if self.kind == Kind::Area && !points.is_empty() {
                        let mut polygon = points.clone();
                        let mut lower: Vec<(f32, f32)> = series.xs.iter().zip(&floors).map(|(x, f)| (sx(*x), value_at(*f))).collect();
                        lower.reverse();
                        polygon.extend(lower);
                        shapes.push(Shape::Polygon { points: polygon, paint: Paint::Series(si) });
                    }
                    if matches!(self.kind, Kind::Line | Kind::Area) && points.len() > 1 {
                        shapes.push(Shape::Polyline { points: points.clone(), paint: Paint::Series(si), width: 2.0 });
                    }
                    if self.kind == Kind::Scatter || (self.kind == Kind::Line && points.len() <= 60) {
                        let r = if self.kind == Kind::Scatter { 3.5 } else { 2.5 };
                        for ((x, y), (px, py)) in series.xs.iter().zip(&series.ys).zip(&points) {
                            let tip = Some(format!("{}, {}", self.x_text(*x), number(*y)));
                            shapes.push(Shape::Circle { x: *px, y: *py, r, paint: Paint::Series(si), tip });
                        }
                    }
                    if stack {
                        for (x, top) in series.xs.iter().zip(&tops) {
                            base.insert(x.to_bits(), *top);
                        }
                    }
                }
            }
            Kind::Pie => {}
        }
        shapes.push(Shape::Line { x1: x0, y1: y0, x2: x0, y2: y1, paint: Paint::Axis, width: 1.0, dashed: false });
        shapes.push(Shape::Line { x1: x0, y1: y1, x2: x1, y2: y1, paint: Paint::Axis, width: 1.0, dashed: false });
        if multi {
            let mut right = x1;
            for (si, series) in self.series.iter().enumerate().rev() {
                let name = series.name.clone().unwrap_or_default();
                let text_w = name.chars().count() as f32 * 6.5;
                right -= text_w;
                shapes.push(Self::text_shape(right, y0 - 11.0, name, 11.0, Anchor::Start, Paint::Muted));
                right -= 16.0;
                shapes.push(Shape::Rect { x: right, y: y0 - 16.0, w: 10.0, h: 10.0, paint: Paint::Series(si), tip: None });
                right -= 14.0;
            }
        }
        Scene { width, height, shapes }
    }

    fn bar_shapes(&self, shapes: &mut Vec<Shape>, (x0, x1, y0, y1): (f32, f32, f32, f32), value_at: &dyn Fn(f64) -> f32) {
        let n = self.categories.len().max(1);
        let group = if self.horizontal { (y1 - y0) / n as f32 } else { (x1 - x0) / n as f32 };
        let slots = if self.stacked { 1 } else { self.series.len().max(1) };
        let slot = group * 0.72 / slots as f32;
        let mut positive = vec![0.0f64; n];
        let mut negative = vec![0.0f64; n];
        for (si, series) in self.series.iter().enumerate() {
            for (x, y) in series.xs.iter().zip(&series.ys) {
                let c = *x as usize;
                let (from, to) = if self.stacked {
                    if *y >= 0.0 {
                        let from = positive[c];
                        positive[c] += y;
                        (from, positive[c])
                    } else {
                        let to = negative[c];
                        negative[c] += y;
                        (negative[c], to)
                    }
                } else {
                    (0.0, *y)
                };
                let offset = group * 0.14 + if self.stacked { 0.0 } else { si as f32 * slot };
                let category = self.categories.get(c).cloned().unwrap_or_default();
                let tip = Some(match &series.name {
                    Some(name) if self.series.len() > 1 => format!("{category} {name}: {}", number(*y)),
                    _ => format!("{category}: {}", number(*y)),
                });
                let (a, b) = (value_at(from), value_at(to));
                if self.horizontal {
                    shapes.push(Shape::Rect { x: a.min(b), y: y0 + c as f32 * group + offset, w: (a - b).abs(), h: slot.max(1.0) - 1.0, paint: Paint::Series(si), tip });
                } else {
                    shapes.push(Shape::Rect { x: x0 + c as f32 * group + offset, y: a.min(b), w: slot.max(1.0) - 1.0, h: (a - b).abs(), paint: Paint::Series(si), tip });
                }
            }
        }
        for (c, category) in self.categories.iter().enumerate() {
            if self.horizontal {
                shapes.push(Self::text_shape(x0 - 8.0, y0 + (c as f32 + 0.5) * group, category.clone(), 11.0, Anchor::End, Paint::Muted));
            } else {
                shapes.push(Self::text_shape(x0 + (c as f32 + 0.5) * group, y1 + 14.0, category.clone(), 11.0, Anchor::Middle, Paint::Muted));
            }
        }
    }

    fn box_shapes(&self, shapes: &mut Vec<Shape>, (x0, x1, _y0, y1): (f32, f32, f32, f32), value_at: &dyn Fn(f64) -> f32) {
        let n = self.boxes.len().max(1);
        let group = (x1 - x0) / n as f32;
        let half = group * 0.25;
        for (i, b) in self.boxes.iter().enumerate() {
            let center = x0 + (i as f32 + 0.5) * group;
            let line = |x1: f32, y1: f32, x2: f32, y2: f32| Shape::Line { x1, y1, x2, y2, paint: Paint::Axis, width: 1.5, dashed: false };
            shapes.push(line(center, value_at(b.high), center, value_at(b.q3)));
            shapes.push(line(center, value_at(b.q1), center, value_at(b.low)));
            shapes.push(line(center - half / 2.0, value_at(b.high), center + half / 2.0, value_at(b.high)));
            shapes.push(line(center - half / 2.0, value_at(b.low), center + half / 2.0, value_at(b.low)));
            let tip = format!("{}: {} to {}, median {}", b.label, number(b.q1), number(b.q3), number(b.median));
            shapes.push(Shape::Rect { x: center - half, y: value_at(b.q3), w: half * 2.0, h: value_at(b.q1) - value_at(b.q3), paint: Paint::Series(i), tip: Some(tip) });
            shapes.push(Shape::Line { x1: center - half, y1: value_at(b.median), x2: center + half, y2: value_at(b.median), paint: Paint::Text, width: 2.0, dashed: false });
            for outlier in &b.outliers {
                shapes.push(Shape::Circle { x: center, y: value_at(*outlier), r: 3.0, paint: Paint::Series(i), tip: Some(format!("{}: {}", b.label, number(*outlier))) });
            }
            shapes.push(Self::text_shape(center, y1 + 14.0, b.label.clone(), 11.0, Anchor::Middle, Paint::Muted));
        }
    }

    fn pie_scene(&self, width: f32, height: f32) -> Scene {
        let mut shapes = Vec::new();
        let top = 16.0 + if self.title.is_some() { 24.0 } else { 0.0 };
        if let Some(title) = &self.title {
            shapes.push(Shape::Text { x: width / 2.0, y: 18.0, text: title.clone(), size: 15.0, anchor: Anchor::Middle, paint: Paint::Text, bold: true });
        }
        let Some(series) = self.series.first() else { return Scene { width, height, shapes } };
        let total: f64 = series.ys.iter().sum();
        let (cx, cy) = (width * 0.4, top + (height - top) / 2.0);
        let r = ((height - top) / 2.0 - 28.0).min(width * 0.24).max(10.0);
        let inner = if self.donut { r * 0.55 } else { 0.0 };
        let mut angle = -FRAC_PI_2;
        for (i, (x, y)) in series.xs.iter().zip(&series.ys).enumerate() {
            let sweep = if total > 0.0 { (y / total) as f32 * TAU } else { 0.0 };
            let label = self.categories.get(*x as usize).cloned().unwrap_or_default();
            let share = if total > 0.0 { (y / total * 100.0).round() } else { 0.0 };
            let tip = format!("{label}: {} ({share}%)", number(*y));
            shapes.push(Shape::Arc { cx, cy, r, inner, start: angle, end: angle + sweep, paint: Paint::Series(i), tip: Some(tip) });
            let mid = angle + sweep / 2.0;
            let (lx, ly) = (cx + (r + 14.0) * mid.cos(), cy + (r + 14.0) * mid.sin());
            let anchor = if mid.cos() >= 0.0 { Anchor::Start } else { Anchor::End };
            shapes.push(Self::text_shape(lx, ly, format!("{label} {share}%"), 11.0, anchor, Paint::Muted));
            angle += sweep;
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
                Shape::Rect { x, y, w, h, paint, .. } => {
                    let _ = writeln!(out, "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"{}\"/>", color(paint));
                }
                Shape::Line { x1, y1, x2, y2, paint, width, dashed } => {
                    let dash = if *dashed { " stroke-dasharray=\"4 3\"" } else { "" };
                    let _ = writeln!(out, "<line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" stroke=\"{}\" stroke-width=\"{width}\"{dash}/>", color(paint));
                }
                Shape::Polyline { points, paint, width } => {
                    let _ = writeln!(out, "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{width}\"/>", points_attr(points), color(paint));
                }
                Shape::Polygon { points, paint } => {
                    let _ = writeln!(out, "<polygon points=\"{}\" fill=\"{}\" fill-opacity=\"0.35\"/>", points_attr(points), color(paint));
                }
                Shape::Circle { x, y, r, paint, .. } => {
                    let _ = writeln!(out, "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{r}\" fill=\"{}\"/>", color(paint));
                }
                Shape::Arc { cx, cy, r, inner, start, end, paint, .. } => {
                    let _ = writeln!(out, "<path d=\"{}\" fill=\"{}\"/>", arc_path(*cx, *cy, *r, *inner, *start, *end), color(paint));
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

fn points_attr(points: &[(f32, f32)]) -> String {
    points.iter().map(|(x, y)| format!("{x:.1},{y:.1}")).collect::<Vec<_>>().join(" ")
}

pub fn arc_points(cx: f32, cy: f32, r: f32, start: f32, end: f32) -> Vec<(f32, f32)> {
    let steps = (((end - start).abs() / 0.05).ceil() as usize).max(1);
    (0..=steps)
        .map(|i| {
            let angle = start + (end - start) * i as f32 / steps as f32;
            (cx + r * angle.cos(), cy + r * angle.sin())
        })
        .collect()
}

fn arc_path(cx: f32, cy: f32, r: f32, inner: f32, start: f32, end: f32) -> String {
    let end = end.min(start + TAU - 0.0005);
    let large = u8::from(end - start > std::f32::consts::PI);
    let (x1, y1) = (cx + r * start.cos(), cy + r * start.sin());
    let (x2, y2) = (cx + r * end.cos(), cy + r * end.sin());
    if inner <= 0.0 {
        return format!("M{cx:.1},{cy:.1} L{x1:.1},{y1:.1} A{r:.1},{r:.1} 0 {large} 1 {x2:.1},{y2:.1} Z");
    }
    let (x3, y3) = (cx + inner * end.cos(), cy + inner * end.sin());
    let (x4, y4) = (cx + inner * start.cos(), cy + inner * start.sin());
    format!("M{x1:.1},{y1:.1} A{r:.1},{r:.1} 0 {large} 1 {x2:.1},{y2:.1} L{x3:.1},{y3:.1} A{inner:.1},{inner:.1} 0 {large} 0 {x4:.1},{y4:.1} Z")
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

    fn rects(scene: &Scene) -> Vec<(f32, f32, f32, f32)> {
        scene
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Rect { x, y, w, h, .. } => Some((*x, *y, *w, *h)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn stacked_bars_sit_on_top_of_each_other() {
        let mut plot = Plot::new(Kind::Bar);
        plot.categories = vec!["a".into()];
        plot.series = vec![
            Series { name: Some("x".into()), xs: vec![0.0], ys: vec![1.0] },
            Series { name: Some("y".into()), xs: vec![0.0], ys: vec![2.0] },
        ];
        plot.stacked = true;
        let bars: Vec<_> = rects(&plot.scene(640.0, 400.0)).into_iter().filter(|r| r.2 > 20.0).collect();
        assert_eq!(bars.len(), 2);
        let (first, second) = (bars[0], bars[1]);
        assert!((first.0 - second.0).abs() < 0.01 && (first.2 - second.2).abs() < 0.01, "{bars:?}");
        assert!(((second.1 + second.3) - first.1).abs() < 0.6, "{bars:?}");
        assert!((second.3 / first.3 - 2.0).abs() < 0.05, "{bars:?}");
    }

    #[test]
    fn horizontal_bars_grow_along_x() {
        let mut plot = Plot::new(Kind::Bar);
        plot.categories = vec!["a".into(), "b".into()];
        plot.series = vec![Series { name: None, xs: vec![0.0, 1.0], ys: vec![1.0, 2.0] }];
        plot.horizontal = true;
        let bars = rects(&plot.scene(640.0, 400.0));
        assert_eq!(bars.len(), 2);
        assert!((bars[0].0 - bars[1].0).abs() < 0.01 && bars[0].1 < bars[1].1, "{bars:?}");
        assert!((bars[1].2 / bars[0].2 - 2.0).abs() < 0.05, "{bars:?}");
    }

    #[test]
    fn box_stats_follow_the_quartiles_and_fences() {
        let stats = BoxStats::of("v", &[1.0, 2.0, 3.0, 4.0, 100.0]);
        assert_eq!((stats.q1, stats.median, stats.q3), (2.0, 3.0, 4.0));
        assert_eq!((stats.low, stats.high), (1.0, 4.0));
        assert_eq!(stats.outliers, [100.0]);
        let two = BoxStats::of("v", &[12.0, 30.0]);
        assert_eq!((two.q1, two.median, two.q3, two.low, two.high), (16.5, 21.0, 25.5, 12.0, 30.0));
    }

    #[test]
    fn pie_wedges_cover_the_whole_circle() {
        let mut plot = Plot::new(Kind::Pie);
        plot.categories = vec!["a".into(), "b".into(), "c".into()];
        plot.series = vec![Series { name: None, xs: vec![0.0, 1.0, 2.0], ys: vec![1.0, 1.0, 2.0] }];
        let scene = plot.scene(640.0, 400.0);
        let sweep: f32 = scene
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Arc { start, end, .. } => Some(end - start),
                _ => None,
            })
            .sum();
        assert!((sweep - std::f32::consts::TAU).abs() < 0.001);
        assert_eq!(plot.describe(), "pie chart with 3 slices");
    }

    #[test]
    fn area_polygons_close_on_the_baseline() {
        let mut plot = Plot::new(Kind::Area);
        plot.series = vec![Series { name: None, xs: vec![1.0, 2.0, 3.0], ys: vec![1.0, 4.0, 2.0] }];
        let scene = plot.scene(640.0, 400.0);
        let polygon = scene.shapes.iter().find_map(|s| match s {
            Shape::Polygon { points, .. } => Some(points.clone()),
            _ => None,
        });
        let points = polygon.expect("an area polygon");
        assert_eq!(points.len(), 6);
        assert!((points[3].1 - points[5].1).abs() < 0.01, "{points:?}");
    }

    #[test]
    fn log_axes_tick_at_powers_and_fixed_ranges_hold() {
        assert_eq!(Axis::log(1.0, 1000.0).ticks, [1.0, 10.0, 100.0, 1000.0]);
        assert_eq!(Axis::log(2.0, 50.0).ticks, [2.0, 5.0, 10.0, 20.0, 50.0]);
        let axis = Axis::log(1.0, 100.0);
        assert!((axis.place(10.0, 0.0, 200.0) - 100.0).abs() < 0.01);
        let fixed = Axis::fixed(0.0, 100.0, 5, false);
        assert_eq!((fixed.lo, fixed.hi), (0.0, 100.0));
        assert_eq!(fixed.ticks, [0.0, 20.0, 40.0, 60.0, 80.0, 100.0]);
        assert_eq!(format_tick(1234567.0, Some("{:,}")), Some("1,234,567".to_string()));
        assert_eq!(format_tick(0.5, Some("{:.0%}")), Some("50%".to_string()));
        assert_eq!(format_tick(2.5, Some("{:z}")), None);
    }

    #[test]
    fn segments_connect_grid_cells() {
        assert_eq!(segment((0, 0), (3, 0)), [(1, 0), (2, 0)]);
        assert_eq!(segment((0, 3), (3, 0)), [(1, 2), (2, 1)]);
        assert!(segment((5, 5), (5, 5)).is_empty());
    }
}
