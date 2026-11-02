use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::fmt::Write;

use crate::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Bar,
    Line,
    Step,
    Area,
    Point,
    Lollipop,
    Histogram,
    Box,
    Pie,
    Heat,
    Density,
}

impl Mark {
    fn starts_at_zero(self) -> bool {
        matches!(self, Mark::Bar | Mark::Histogram | Mark::Lollipop | Mark::Area | Mark::Density)
    }

    pub fn categorical(self) -> bool {
        matches!(self, Mark::Bar | Mark::Lollipop | Mark::Pie | Mark::Heat | Mark::Box)
    }

    pub fn label(self) -> &'static str {
        match self {
            Mark::Bar => "bar chart",
            Mark::Line => "line chart",
            Mark::Step => "step chart",
            Mark::Area => "area chart",
            Mark::Point => "scatter",
            Mark::Lollipop => "lollipop chart",
            Mark::Histogram => "histogram",
            Mark::Box => "box plot",
            Mark::Pie => "pie chart",
            Mark::Heat => "heatmap",
            Mark::Density => "density plot",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    Circle,
    Square,
    Diamond,
    Triangle,
}

pub const MARKERS: [Marker; 4] = [Marker::Circle, Marker::Square, Marker::Diamond, Marker::Triangle];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegendAt {
    Auto,
    Top,
    Right,
    Bottom,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn parse(text: &str) -> Option<Rgb> {
        let hex = text.trim().strip_prefix('#')?;
        let digits: Vec<u8> = hex.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect::<Option<Vec<_>>>()?;
        match digits.as_slice() {
            [r, g, b] => Some(Rgb(r * 17, g * 17, b * 17)),
            [r1, r2, g1, g2, b1, b2] => Some(Rgb(r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2)),
            _ => None,
        }
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)).round() as u8;
        Rgb(lerp(self.0, other.0), lerp(self.1, other.1), lerp(self.2, other.2))
    }
}

pub const SERIES_COLORS: [&str; 6] = ["#0072b2", "#e69f00", "#009e73", "#d55e00", "#cc79a7", "#56b4e9"];

pub fn named_palette(name: &str) -> Option<Vec<Rgb>> {
    let hexes: &[&str] = match name {
        "default" => &SERIES_COLORS,
        "warm" => &["#b4540f", "#c0286e", "#d9a21b", "#8f3b1b", "#e2725b", "#a1468f"],
        "cool" => &["#0f7b86", "#2458d3", "#3c8dbc", "#5b6fd1", "#1e9c8e", "#4f5bd5"],
        "earth" => &["#6b4f2a", "#2a8a5a", "#b4540f", "#4a6b3a", "#8a6d3b", "#1f6f5f"],
        "gray" => &["#1a2230", "#56637a", "#8a94a6", "#b5bcc8", "#d0d5dd", "#66728a"],
        "bright" => &["#e63946", "#f4a261", "#2a9d8f", "#457b9d", "#8338ec", "#ffbe0b"],
        _ => return None,
    };
    Some(hexes.iter().filter_map(|h| Rgb::parse(h)).collect())
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Series {
    pub name: Option<String>,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub sizes: Vec<f64>,
    pub lows: Vec<f64>,
    pub highs: Vec<f64>,
    pub texts: Vec<String>,
}

impl Series {
    pub fn new(name: Option<String>, xs: Vec<f64>, ys: Vec<f64>) -> Series {
        Series { name, xs, ys, ..Series::default() }
    }
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
pub struct Layer {
    pub mark: Mark,
    pub series: Vec<Series>,
    pub boxes: Vec<BoxStats>,
    pub bands: Vec<String>,
    pub side: Side,
    pub stacked: bool,
    pub normalized: bool,
    pub labels: bool,
    pub dashed: bool,
    pub width: f32,
    pub opacity: f32,
    pub marker: Marker,
    pub markers: bool,
    pub dots: bool,
    pub color: Option<Rgb>,
    pub step: f64,
    pub donut: bool,
    pub first_color: usize,
}

impl Layer {
    pub fn new(mark: Mark) -> Layer {
        Layer {
            mark,
            series: Vec::new(),
            boxes: Vec::new(),
            bands: Vec::new(),
            side: Side::Left,
            stacked: false,
            normalized: false,
            labels: false,
            dashed: false,
            width: 2.0,
            opacity: 1.0,
            marker: Marker::Circle,
            markers: false,
            dots: true,
            color: None,
            step: 0.0,
            donut: false,
            first_color: 0,
        }
    }

    fn points(&self) -> usize {
        self.series.iter().map(|s| s.xs.len()).sum()
    }

    pub fn describe(&self) -> String {
        let many = self.series.len() > 1;
        match self.mark {
            Mark::Bar | Mark::Lollipop => format!("{} with {}", self.mark.label(), plural(self.points(), "bar")),
            Mark::Line | Mark::Step | Mark::Area | Mark::Density if many => format!("{} with {}", self.mark.label(), plural(self.series.len(), "series")),
            Mark::Line | Mark::Step | Mark::Area | Mark::Point | Mark::Density => format!("{} with {}", self.mark.label(), plural(self.points(), "point")),
            Mark::Histogram => format!("histogram with {}", plural(self.points(), "bin")),
            Mark::Box => format!("box plot with {}", plural(self.boxes.len(), "group")),
            Mark::Pie => format!("pie chart with {}", plural(self.points(), "slice")),
            Mark::Heat => format!("heatmap with {}", plural(self.points(), "cell")),
        }
    }

    fn stack_totals(&self) -> BTreeMap<u64, f64> {
        let mut totals: BTreeMap<u64, f64> = BTreeMap::new();
        for series in &self.series {
            for (x, y) in series.xs.iter().zip(&series.ys) {
                if *y > 0.0 {
                    *totals.entry(x.to_bits()).or_default() += y;
                }
            }
        }
        totals
    }

    fn stacks(&self) -> bool {
        self.stacked || (self.mark == Mark::Area && self.series.len() > 1)
    }

    fn value_extent(&self, visible: &dyn Fn(usize) -> bool) -> (f64, f64) {
        let fold = |values: &mut dyn Iterator<Item = f64>| values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
        let shown = || self.series.iter().enumerate().filter(|(i, _)| visible(*i)).map(|(_, s)| s);
        match self.mark {
            Mark::Box => {
                let mut all = self.boxes.iter().flat_map(|b| b.outliers.iter().copied().chain([b.low, b.high]));
                fold(&mut all)
            }
            Mark::Bar | Mark::Area if self.stacks() => {
                let totals = self.stack_totals();
                let (lo, _) = fold(&mut shown().flat_map(|s| s.ys.iter().copied()));
                let (_, hi) = fold(&mut totals.into_values());
                (lo.min(0.0), hi)
            }
            _ => {
                let mut values = shown().flat_map(|s| s.ys.iter().copied().chain(s.lows.iter().copied()).chain(s.highs.iter().copied()));
                fold(&mut values)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct AxisSpec {
    pub label: Option<String>,
    pub range: Option<(f64, f64)>,
    pub log: bool,
    pub format: Option<String>,
    pub ticks: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub vertical: bool,
    pub value: f64,
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub x: f64,
    pub y: f64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plot {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub caption: Option<String>,
    pub x: AxisSpec,
    pub y: AxisSpec,
    pub y2: AxisSpec,
    pub categories: Vec<String>,
    pub dates: bool,
    pub horizontal: bool,
    pub layers: Vec<Layer>,
    pub rules: Vec<Rule>,
    pub notes: Vec<Note>,
    pub legend: LegendAt,
    pub palette: Vec<Rgb>,
    pub grid: bool,
    pub panels: Vec<Plot>,
    pub columns: Option<usize>,
}

impl Default for Plot {
    fn default() -> Plot {
        Plot::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paint {
    Series(usize),
    Rgb(Rgb),
    Heat(f32),
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
    Rect { x: f32, y: f32, w: f32, h: f32, paint: Paint, alpha: f32, tip: Option<String> },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, paint: Paint, width: f32, dashed: bool },
    Polyline { points: Vec<(f32, f32)>, paint: Paint, width: f32, dashed: bool },
    Polygon { points: Vec<(f32, f32)>, paint: Paint, alpha: f32, tip: Option<String> },
    Circle { x: f32, y: f32, r: f32, paint: Paint, alpha: f32, tip: Option<String> },
    Arc { cx: f32, cy: f32, r: f32, inner: f32, start: f32, end: f32, paint: Paint, tip: Option<String> },
    Text { x: f32, y: f32, text: String, size: f32, anchor: Anchor, paint: Paint, bold: bool, angle: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegendEntry {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub series: usize,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub x: f32,
    pub y: f32,
    pub series: usize,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub key: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub shapes: Vec<Shape>,
    pub legend: Vec<LegendEntry>,
    pub samples: Vec<Sample>,
    pub picks: Vec<Pick>,
}

const BAR_WIDTH: usize = 40;
const GRID_W: usize = 40;
const GRID_H: usize = 10;
const MARKS: [char; 4] = ['•', '○', '▪', '▫'];
const EIGHTHS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];
const CHAR_W: f32 = 6.2;

#[derive(Clone, Copy)]
struct TextStyle {
    fine: bool,
    color: bool,
}

impl TextStyle {
    const PLAIN: TextStyle = TextStyle { fine: false, color: false };

    fn tint(self, rgb: Option<Rgb>, text: &str) -> String {
        match rgb {
            Some(Rgb(r, g, b)) if self.color && !text.is_empty() => format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m"),
            _ => text.to_string(),
        }
    }

    fn shade(self, back: Rgb, text: &str) -> String {
        if !self.color {
            return text.to_string();
        }
        let Rgb(r, g, b) = back;
        let ink = if (r as u32 + g as u32 + b as u32) < 330 { "255;255;255" } else { "26;34;48" };
        format!("\x1b[48;2;{r};{g};{b}m\x1b[38;2;{ink}m{text}\x1b[0m")
    }
}

fn rgb_of(paint: &Paint) -> Option<Rgb> {
    match paint {
        Paint::Series(i) => Rgb::parse(SERIES_COLORS[i % SERIES_COLORS.len()]),
        Paint::Rgb(rgb) => Some(*rgb),
        _ => None,
    }
}

fn bar_glyphs(share: f64, fine: bool) -> String {
    if !fine {
        return "█".repeat((share * BAR_WIDTH as f64).round() as usize);
    }
    let eighths = (share * BAR_WIDTH as f64 * 8.0).round() as usize;
    let mut out = "█".repeat(eighths / 8);
    if !eighths.is_multiple_of(8) {
        out.push(EIGHTHS[eighths % 8 - 1]);
    }
    out
}

fn braille_bit(column: usize, row: usize) -> u8 {
    match (column % 2, row % 4) {
        (0, 0) => 0x01,
        (0, 1) => 0x02,
        (0, 2) => 0x04,
        (0, _) => 0x40,
        (_, 0) => 0x08,
        (_, 1) => 0x10,
        (_, 2) => 0x20,
        _ => 0x80,
    }
}

fn traced_cells(mark: Mark, cells: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for pair in cells.windows(2) {
        out.push(pair[0]);
        if mark == Mark::Step {
            out.push((pair[1].0, pair[0].1));
        }
    }
    out.extend(cells.last().copied());
    out
}

pub fn number(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") }
}

pub fn compact(x: f64) -> String {
    let magnitude = x.abs();
    let (scaled, suffix) = if magnitude >= 1e9 {
        (x / 1e9, "B")
    } else if magnitude >= 1e6 {
        (x / 1e6, "M")
    } else if magnitude >= 1e3 {
        (x / 1e3, "K")
    } else {
        return number(x);
    };
    let text = if scaled.fract() == 0.0 || scaled.abs() >= 100.0 { format!("{}", scaled.round() as i64) } else { format!("{scaled:.1}") };
    format!("{}{suffix}", text.trim_end_matches(".0"))
}

pub fn ticks(lo: f64, hi: f64, count: usize) -> (Vec<f64>, f64) {
    if hi.partial_cmp(&lo) != Some(std::cmp::Ordering::Greater) || !lo.is_finite() || !hi.is_finite() {
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
    histogram_in(values, min, width, bins)
}

pub fn histogram_in(values: &[f64], min: f64, width: f64, bins: usize) -> Plot {
    let mut counts = vec![0.0; bins];
    for &value in values {
        let index = ((value - min) / width).floor().max(0.0) as usize;
        counts[index.min(bins - 1)] += 1.0;
    }
    let edges: Vec<f64> = (0..bins).map(|i| min + i as f64 * width).collect();
    let mut plot = Plot::new();
    plot.y.label = Some("count".to_string());
    plot.categories = edges.iter().map(|lo| format!("{}–{}", number(*lo), number(lo + width))).collect();
    let mut layer = Layer::new(Mark::Histogram);
    layer.step = width;
    layer.series = vec![Series::new(None, edges, counts)];
    plot.layers.push(layer);
    plot
}

pub fn density(values: &[f64], bandwidth: Option<f64>, points: usize) -> Series {
    let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    let n = finite.len();
    if n == 0 {
        return Series::default();
    }
    let mean = finite.iter().sum::<f64>() / n as f64;
    let variance = finite.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n.max(1) as f64;
    let deviation = variance.sqrt();
    let h = bandwidth.unwrap_or_else(|| {
        let silverman = 1.06 * deviation * (n as f64).powf(-0.2);
        if silverman > 0.0 { silverman } else { 1.0 }
    });
    let min = finite.iter().copied().fold(f64::INFINITY, f64::min) - 3.0 * h;
    let max = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max) + 3.0 * h;
    let count = points.max(2);
    let xs: Vec<f64> = (0..count).map(|i| min + (max - min) * i as f64 / (count - 1) as f64).collect();
    let norm = 1.0 / (n as f64 * h * (2.0 * std::f64::consts::PI).sqrt());
    let ys = xs.iter().map(|x| finite.iter().map(|v| (-0.5 * ((x - v) / h).powi(2)).exp()).sum::<f64>() * norm).collect();
    Series::new(None, xs, ys)
}

pub fn linear_fit(xs: &[f64], ys: &[f64]) -> Option<(f64, f64)> {
    let n = xs.len().min(ys.len());
    if n < 2 {
        return None;
    }
    let (sx, sy) = (xs[..n].iter().sum::<f64>(), ys[..n].iter().sum::<f64>());
    let (mx, my) = (sx / n as f64, sy / n as f64);
    let sxx: f64 = xs[..n].iter().map(|x| (x - mx).powi(2)).sum();
    if sxx == 0.0 {
        return None;
    }
    let sxy: f64 = xs[..n].iter().zip(&ys[..n]).map(|(x, y)| (x - mx) * (y - my)).sum();
    let slope = sxy / sxx;
    Some((slope, my - slope * mx))
}

pub fn rolling_mean(ys: &[f64], window: usize) -> Vec<f64> {
    let window = window.max(1);
    ys.iter()
        .enumerate()
        .map(|(i, _)| {
            let start = (i + 1).saturating_sub(window);
            let slice = &ys[start..=i];
            slice.iter().sum::<f64>() / slice.len() as f64
        })
        .collect()
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("{n} {word}") } else { format!("{n} {word}s") }
}

pub fn shared_ranges(panels: &[Plot]) -> ((f64, f64), Option<(f64, f64)>) {
    let Some(first) = panels.first() else { return ((0.0, 1.0), None) };
    let fold = |pairs: &mut dyn Iterator<Item = (f64, f64)>| pairs.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (a, b)| (lo.min(a), hi.max(b)));
    let (vlo, vhi) = fold(&mut panels.iter().map(|p| p.value_extent(Side::Left, &[])));
    let from_zero = first.layers.iter().any(|l| l.mark.starts_at_zero());
    let value = if first.y.log {
        (vlo.max(f64::MIN_POSITIVE), vhi)
    } else {
        let axis = Axis::numeric(vlo, vhi, 5, from_zero);
        (axis.lo, axis.hi)
    };
    let numeric = first.categories.is_empty() && first.layers.iter().any(|l| matches!(l.mark, Mark::Line | Mark::Step | Mark::Point | Mark::Area | Mark::Density));
    let x = numeric.then(|| {
        let mut extents = panels.iter().map(|p| {
            let mut xs = p.layers.iter().flat_map(|l| l.series.iter().flat_map(|s| s.xs.iter().copied()));
            xs.next().map_or((f64::INFINITY, f64::NEG_INFINITY), |x| xs.fold((x, x), |(lo, hi), v| (lo.min(v), hi.max(v))))
        });
        let (lo, hi) = fold(&mut extents);
        if first.x.log {
            (lo, hi)
        } else {
            let axis = Axis::hugging(lo, hi, 5);
            (axis.lo, axis.hi)
        }
    });
    (value, x)
}

fn whole(values: impl Iterator<Item = f64>) -> bool {
    let mut any = false;
    for value in values {
        if value.fract() != 0.0 {
            return false;
        }
        any = true;
    }
    any
}

fn translate(shape: Shape, dx: f32, dy: f32) -> Shape {
    let shift = |points: Vec<(f32, f32)>| points.into_iter().map(|(x, y)| (x + dx, y + dy)).collect();
    match shape {
        Shape::Rect { x, y, w, h, paint, alpha, tip } => Shape::Rect { x: x + dx, y: y + dy, w, h, paint, alpha, tip },
        Shape::Line { x1, y1, x2, y2, paint, width, dashed } => Shape::Line { x1: x1 + dx, y1: y1 + dy, x2: x2 + dx, y2: y2 + dy, paint, width, dashed },
        Shape::Polyline { points, paint, width, dashed } => Shape::Polyline { points: shift(points), paint, width, dashed },
        Shape::Polygon { points, paint, alpha, tip } => Shape::Polygon { points: shift(points), paint, alpha, tip },
        Shape::Circle { x, y, r, paint, alpha, tip } => Shape::Circle { x: x + dx, y: y + dy, r, paint, alpha, tip },
        Shape::Arc { cx, cy, r, inner, start, end, paint, tip } => Shape::Arc { cx: cx + dx, cy: cy + dy, r, inner, start, end, paint, tip },
        Shape::Text { x, y, text, size, anchor, paint, bold, angle } => Shape::Text { x: x + dx, y: y + dy, text, size, anchor, paint, bold, angle },
    }
}

fn shift_scene(scene: Scene, dx: f32, dy: f32) -> Scene {
    Scene {
        width: scene.width,
        height: scene.height,
        shapes: scene.shapes.into_iter().map(|s| translate(s, dx, dy)).collect(),
        legend: scene.legend.into_iter().map(|e| LegendEntry { x: e.x + dx, y: e.y + dy, ..e }).collect(),
        samples: scene.samples.into_iter().map(|s| Sample { x: s.x + dx, y: s.y + dy, ..s }).collect(),
        picks: scene.picks.into_iter().map(|p| Pick { x: p.x + dx, y: p.y + dy, ..p }).collect(),
    }
}

struct Axis {
    lo: f64,
    hi: f64,
    ticks: Vec<f64>,
    step: f64,
    log: bool,
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
        Axis { lo: ticks[0], hi: *ticks.last().unwrap(), ticks, step, log: false }
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
        Axis { lo, hi, ticks, step, log: false }
    }

    fn fixed(lo: f64, hi: f64, count: usize, log: bool) -> Axis {
        if log {
            return Axis::log(lo, hi);
        }
        let (lo, hi) = if hi > lo { (lo, hi) } else { (lo, lo + 1.0) };
        let (ticks, step) = ticks(lo, hi, count);
        let within = ticks.into_iter().filter(|t| *t >= lo - step * 1e-9 && *t <= hi + step * 1e-9).collect();
        Axis { lo, hi, ticks: within, step, log: false }
    }

    fn log(lo: f64, hi: f64) -> Axis {
        let lo = if lo.is_finite() { lo.max(f64::MIN_POSITIVE) } else { 1.0 };
        let hi = if hi.is_finite() && hi > lo { hi } else { lo * 10.0 };
        let multiples: &[f64] = if (hi / lo).log10() < 2.0 { &[1.0, 2.0, 5.0] } else { &[1.0] };
        let mut ticks = Vec::new();
        for exponent in (lo.log10().floor() as i32)..=(hi.log10().ceil() as i32) {
            for multiple in multiples {
                let tick = multiple * 10f64.powi(exponent);
                if tick >= lo * 0.999 && tick <= hi * 1.001 {
                    ticks.push(tick);
                }
            }
        }
        Axis { lo, hi, ticks, step: 1.0, log: true }
    }

    fn whole(mut self) -> Axis {
        if !self.log && self.step < 1.0 {
            self.ticks = (self.lo.ceil() as i64..=self.hi.floor() as i64).map(|i| i as f64).collect();
            self.step = 1.0;
        }
        self
    }

    fn for_values(spec: &AxisSpec, lo: f64, hi: f64, count: usize, include_zero: bool, hug: bool) -> Axis {
        let count = spec.ticks.unwrap_or(count).max(1);
        match (spec.range, spec.log) {
            (Some((a, b)), log) => Axis::fixed(a, b, count, log),
            (None, true) => Axis::log(lo, hi),
            (None, false) if hug => Axis::hugging(lo, hi, count),
            (None, false) => Axis::numeric(lo, hi, count, include_zero),
        }
    }

    fn place(&self, value: f64, from: f32, to: f32) -> f32 {
        let fraction = if self.log {
            (value.max(f64::MIN_POSITIVE).log10() - self.lo.log10()) / (self.hi.log10() - self.lo.log10())
        } else {
            (value - self.lo) / (self.hi - self.lo)
        };
        from + fraction as f32 * (to - from)
    }

    fn holds(&self, value: f64) -> bool {
        value >= self.lo && value <= self.hi
    }

    fn label(&self, value: f64, dates: bool, format: Option<&str>) -> String {
        if let Some(pattern) = format {
            if pattern == "compact" {
                return compact(value);
            }
            if dates {
                return crate::date::format(value.round() as i32, pattern);
            }
            if let Some(text) = format_tick(value, Some(pattern)) {
                return text;
            }
        }
        if self.log { number(value) } else { tick_label(value, self.step, dates) }
    }
}

pub fn format_tick(value: f64, spec: Option<&str>) -> Option<String> {
    let spec = spec?;
    if spec == "compact" {
        return Some(compact(value));
    }
    let argument = if value.fract() == 0.0 && value.abs() < 1e15 { Value::Int(value as i64) } else { Value::Float(value) };
    match crate::builtins::strings::format_values(&Value::string(spec), &[argument]) {
        Ok(Value::Str(text)) => Some(text.to_string()),
        _ => None,
    }
}

fn text_shape(x: f32, y: f32, text: String, size: f32, anchor: Anchor, paint: Paint) -> Shape {
    Shape::Text { x, y, text, size, anchor, paint, bold: false, angle: 0.0 }
}

fn marker_shape(marker: Marker, x: f32, y: f32, r: f32, paint: Paint, alpha: f32, tip: Option<String>) -> Shape {
    match marker {
        Marker::Circle => Shape::Circle { x, y, r, paint, alpha, tip },
        Marker::Square => Shape::Polygon { points: vec![(x - r, y - r), (x + r, y - r), (x + r, y + r), (x - r, y + r)], paint, alpha, tip },
        Marker::Diamond => {
            let d = r * 1.3;
            Shape::Polygon { points: vec![(x, y - d), (x + d, y), (x, y + d), (x - d, y)], paint, alpha, tip }
        }
        Marker::Triangle => {
            let d = r * 1.25;
            Shape::Polygon { points: vec![(x, y - d), (x + d, y + d * 0.8), (x - d, y + d * 0.8)], paint, alpha, tip }
        }
    }
}

struct Frame {
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
}

impl Plot {
    pub fn new() -> Plot {
        Plot {
            title: None,
            subtitle: None,
            caption: None,
            x: AxisSpec::default(),
            y: AxisSpec::default(),
            y2: AxisSpec::default(),
            categories: Vec::new(),
            dates: false,
            horizontal: false,
            layers: Vec::new(),
            rules: Vec::new(),
            notes: Vec::new(),
            legend: LegendAt::Auto,
            palette: Vec::new(),
            grid: true,
            panels: Vec::new(),
            columns: None,
        }
    }

    pub fn mark(&self) -> Option<Mark> {
        self.layers.first().map(|l| l.mark)
    }

    pub fn describe(&self) -> String {
        if let Some(first) = self.panels.first() {
            return format!("{} of {}", plural(self.panels.len(), "panel"), first.describe());
        }
        match self.layers.as_slice() {
            [] => "empty plot".to_string(),
            [one] => one.describe(),
            [first, rest @ ..] => format!("{} and {} more layer{}", first.describe(), rest.len(), if rest.len() == 1 { "" } else { "s" }),
        }
    }

    pub fn entries(&self) -> Vec<(usize, usize, String)> {
        let mut out = Vec::new();
        for (li, layer) in self.layers.iter().enumerate() {
            for (si, series) in layer.series.iter().enumerate() {
                if let Some(name) = &series.name {
                    out.push((li, si, name.clone()));
                }
            }
        }
        out
    }

    fn global_index(&self, layer: usize, series: usize) -> usize {
        self.layers[..layer].iter().map(|l| l.series.len()).sum::<usize>() + series
    }

    fn paint(&self, layer: &Layer, series: usize) -> Paint {
        if let Some(color) = layer.color {
            return Paint::Rgb(color);
        }
        let index = layer.first_color + series;
        if self.palette.is_empty() { Paint::Series(index) } else { Paint::Rgb(self.palette[index % self.palette.len()]) }
    }

    fn x_text(&self, x: f64) -> String {
        if self.dates { crate::date::iso(x.round() as i32) } else { number(x) }
    }

    pub fn value_extent(&self, side: Side, hidden: &[usize]) -> (f64, f64) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for (li, layer) in self.layers.iter().enumerate().filter(|(_, l)| l.side == side) {
            let visible = |si: usize| !hidden.contains(&self.global_index(li, si));
            let (a, b) = layer.value_extent(&visible);
            lo = lo.min(a);
            hi = hi.max(b);
        }
        (lo, hi)
    }

    pub fn text(&self) -> String {
        self.styled_text(TextStyle::PLAIN)
    }

    pub fn terminal_text(&self, color: bool) -> String {
        self.styled_text(TextStyle { fine: true, color })
    }

    fn styled_text(&self, style: TextStyle) -> String {
        let mut out = String::new();
        if let Some(title) = &self.title {
            out.push_str(title);
            out.push('\n');
        }
        if self.panels.is_empty() {
            self.body_text(&mut out, style);
            return out;
        }
        for (i, panel) in self.panels.iter().enumerate() {
            if i > 0 {
                out.push_str("\n\n");
            }
            let _ = writeln!(out, "── {} ──", panel.title.clone().unwrap_or_default());
            panel.body_text(&mut out, style);
        }
        out
    }

    fn body_text(&self, out: &mut String, style: TextStyle) {
        let mut grid_layers: Vec<&Layer> = Vec::new();
        let mut first = true;
        for layer in &self.layers {
            match layer.mark {
                Mark::Line | Mark::Step | Mark::Point | Mark::Area | Mark::Density => {
                    grid_layers.push(layer);
                    continue;
                }
                _ => {}
            }
            if !first {
                out.push('\n');
            }
            first = false;
            match layer.mark {
                Mark::Bar | Mark::Histogram | Mark::Lollipop => self.text_bars(layer, out, style),
                Mark::Pie => self.text_pie(layer, out, style),
                Mark::Box => Self::text_boxes(layer, out),
                Mark::Heat => self.text_heat(layer, out, style),
                _ => {}
            }
        }
        if !grid_layers.is_empty() {
            if !first {
                out.push('\n');
            }
            self.text_grid(&grid_layers, out, style);
        }
    }

    fn bar_rows(&self, layer: &Layer) -> Vec<(String, f64, usize)> {
        let multi = layer.series.len() > 1;
        let mut rows = Vec::new();
        if layer.mark == Mark::Histogram {
            for (si, series) in layer.series.iter().enumerate() {
                for (i, y) in series.ys.iter().enumerate() {
                    rows.push((self.categories.get(i).cloned().unwrap_or_default(), *y, si));
                }
            }
            return rows;
        }
        for (c, category) in self.categories.iter().enumerate() {
            for (si, series) in layer.series.iter().enumerate() {
                let Some(i) = series.xs.iter().position(|x| *x as usize == c) else { continue };
                let label = match (&series.name, multi) {
                    (Some(name), true) => format!("{category} {name}"),
                    _ => category.clone(),
                };
                rows.push((label, series.ys[i], si));
            }
        }
        rows
    }

    fn write_bars(&self, layer: &Layer, out: &mut String, rows: &[(String, f64, String, usize)], style: TextStyle) {
        let width = rows.iter().map(|(label, ..)| label.chars().count()).max().unwrap_or(0);
        let max = rows.iter().map(|(_, v, ..)| v.abs()).fold(0.0, f64::max);
        for (i, (label, value, suffix, series)) in rows.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let share = if max > 0.0 { value.abs() / max } else { 0.0 };
            let bar = style.tint(rgb_of(&self.paint(layer, *series)), &bar_glyphs(share, style.fine));
            let _ = write!(out, "{label:<width$} {bar} {}{suffix}", number(*value));
        }
    }

    fn text_bars(&self, layer: &Layer, out: &mut String, style: TextStyle) {
        let rows: Vec<(String, f64, String, usize)> = self.bar_rows(layer).into_iter().map(|(l, v, s)| (l, v, String::new(), s)).collect();
        self.write_bars(layer, out, &rows, style);
    }

    fn text_pie(&self, layer: &Layer, out: &mut String, style: TextStyle) {
        let Some(series) = layer.series.first() else { return };
        let total: f64 = series.ys.iter().sum();
        let rows: Vec<(String, f64, String, usize)> = series
            .xs
            .iter()
            .zip(&series.ys)
            .enumerate()
            .map(|(i, (x, y))| {
                let label = self.categories.get(*x as usize).cloned().unwrap_or_default();
                let share = if total > 0.0 { (y / total * 100.0).round() } else { 0.0 };
                (label, *y, format!(" ({share}%)"), i)
            })
            .collect();
        self.write_bars(layer, out, &rows, style);
    }

    fn text_heat(&self, layer: &Layer, out: &mut String, style: TextStyle) {
        let Some(series) = layer.series.first() else { return };
        let width = layer.bands.iter().map(|b| b.chars().count()).max().unwrap_or(0);
        let (lo, hi) = series.sizes.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        let _ = write!(out, "{:<width$}", "");
        for category in &self.categories {
            let _ = write!(out, " {category:>8}");
        }
        for (r, band) in layer.bands.iter().enumerate() {
            let _ = write!(out, "\n{band:<width$}");
            for c in 0..self.categories.len() {
                let found = series.xs.iter().zip(&series.ys).zip(&series.sizes).find(|((x, y), _)| **x as usize == c && **y as usize == r);
                let cell = match found {
                    Some((_, v)) => {
                        let t = if hi > lo { ((v - lo) / (hi - lo)) as f32 } else { 0.5 };
                        style.shade(Rgb(232, 240, 248).mix(Rgb(11, 61, 107), t), &format!("{:>8}", number(*v)))
                    }
                    None => format!("{:>8}", ""),
                };
                let _ = write!(out, " {cell}");
            }
        }
    }

    fn text_boxes(layer: &Layer, out: &mut String) {
        let width = layer.boxes.iter().map(|b| b.label.chars().count()).max().unwrap_or(0);
        let lo = layer.boxes.iter().map(|b| b.outliers.iter().copied().fold(b.low, f64::min)).fold(f64::INFINITY, f64::min);
        let hi = layer.boxes.iter().map(|b| b.outliers.iter().copied().fold(b.high, f64::max)).fold(f64::NEG_INFINITY, f64::max);
        let col = |v: f64| if hi > lo { ((v - lo) / (hi - lo) * (GRID_W - 1) as f64).round() as usize } else { GRID_W / 2 };
        for (i, b) in layer.boxes.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let mut cells = [' '; GRID_W];
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
            let _ = write!(out, "{:<width$} {glyphs} {} {} {} {} {}", b.label, number(b.low), number(b.q1), number(b.median), number(b.q3), number(b.high));
        }
    }

    fn text_grid(&self, layers: &[&Layer], out: &mut String, style: TextStyle) {
        let series: Vec<(&Layer, &Series)> = layers.iter().flat_map(|l| l.series.iter().map(move |s| (*l, s))).collect();
        let colors: Vec<Option<Rgb>> = layers.iter().flat_map(|l| (0..l.series.len()).map(move |si| rgb_of(&self.paint(l, si)))).collect();
        let points: Vec<(usize, f64, f64)> = series.iter().enumerate().flat_map(|(i, (_, s))| s.xs.iter().zip(&s.ys).map(move |(x, y)| (i, *x, *y))).collect();
        if points.is_empty() {
            out.push_str("(no points)");
            return;
        }
        let (xmin, xmax) = points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
        let (ymin, ymax) = points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.2), hi.max(p.2)));
        let braille = style.fine && (style.color || series.len() == 1);
        let rows = if braille { braille_rows(&series, &colors, (xmin, xmax), (ymin, ymax), style) } else { glyph_rows(&series, &points, (xmin, xmax), (ymin, ymax)) };
        let (lo, hi) = (number(ymin), number(ymax));
        let width = lo.chars().count().max(hi.chars().count());
        for (r, line) in rows.iter().enumerate() {
            let label = if r == 0 { hi.as_str() } else if r == GRID_H - 1 { lo.as_str() } else { "" };
            let _ = writeln!(out, "{label:>width$} |{line}");
        }
        let _ = writeln!(out, "{:>width$} +{}", "", "-".repeat(GRID_W));
        let (left, right) = (self.x_text(xmin), self.x_text(xmax));
        let gap = GRID_W.saturating_sub(left.chars().count() + right.chars().count());
        let _ = write!(out, "{:>width$}  {left}{}{right}", "", " ".repeat(gap));
        if series.len() > 1 {
            let legend: Vec<String> = series
                .iter()
                .enumerate()
                .map(|(i, (_, s))| {
                    let mark = if braille { style.tint(colors[i], "━━") } else { MARKS[i % MARKS.len()].to_string() };
                    format!("{mark} {}", s.name.clone().unwrap_or_default())
                })
                .collect();
            let _ = write!(out, "\n{}", legend.join("  "));
        }
    }

    pub fn scene(&self, width: f32, height: f32) -> Scene {
        self.scene_hiding(width, height, &[])
    }

    pub fn scene_hiding(&self, width: f32, height: f32, hidden: &[usize]) -> Scene {
        if self.panels.is_empty() { self.body_scene(width, height, false, hidden) } else { self.panel_scene(width, height) }
    }
}

fn glyph_rows(series: &[(&Layer, &Series)], points: &[(usize, f64, f64)], (xmin, xmax): (f64, f64), (ymin, ymax): (f64, f64)) -> Vec<String> {
    let col = |x: f64| if xmax > xmin { ((x - xmin) / (xmax - xmin) * (GRID_W - 1) as f64).round() as usize } else { GRID_W / 2 };
    let row = |y: f64| {
        let up = if ymax > ymin { ((y - ymin) / (ymax - ymin) * (GRID_H - 1) as f64).round() as usize } else { GRID_H / 2 };
        GRID_H - 1 - up
    };
    let mut grid = vec![vec![' '; GRID_W]; GRID_H];
    for (layer, s) in series {
        if !matches!(layer.mark, Mark::Line | Mark::Step | Mark::Area | Mark::Density) {
            continue;
        }
        let mut cells: Vec<(usize, usize)> = s.xs.iter().zip(&s.ys).map(|(x, y)| (col(*x), row(*y))).collect();
        cells.sort();
        for pair in traced_cells(layer.mark, &cells).windows(2) {
            for (c, r) in segment(pair[0], pair[1]) {
                if grid[r][c] == ' ' {
                    grid[r][c] = '·';
                }
            }
        }
    }
    for (i, x, y) in points {
        if series[*i].0.mark != Mark::Density {
            grid[row(*y)][col(*x)] = MARKS[i % MARKS.len()];
        }
    }
    grid.iter().map(|cells| cells.iter().collect::<String>().trim_end().to_string()).collect()
}

fn braille_rows(series: &[(&Layer, &Series)], colors: &[Option<Rgb>], (xmin, xmax): (f64, f64), (ymin, ymax): (f64, f64), style: TextStyle) -> Vec<String> {
    let (w, h) = (GRID_W * 2, GRID_H * 4);
    let col = |x: f64| if xmax > xmin { ((x - xmin) / (xmax - xmin) * (w - 1) as f64).round() as usize } else { w / 2 };
    let row = |y: f64| {
        let up = if ymax > ymin { ((y - ymin) / (ymax - ymin) * (h - 1) as f64).round() as usize } else { h / 2 };
        h - 1 - up
    };
    let mut dots = vec![vec![0u8; GRID_W]; GRID_H];
    let mut owners = vec![vec![None; GRID_W]; GRID_H];
    let mut set = |c: usize, r: usize, series: usize| {
        if c < w && r < h {
            dots[r / 4][c / 2] |= braille_bit(c, r);
            owners[r / 4][c / 2] = Some(series);
        }
    };
    for (i, (layer, s)) in series.iter().enumerate() {
        let mut cells: Vec<(usize, usize)> = s.xs.iter().zip(&s.ys).map(|(x, y)| (col(*x), row(*y))).collect();
        cells.sort();
        if matches!(layer.mark, Mark::Line | Mark::Step | Mark::Area | Mark::Density) {
            for pair in traced_cells(layer.mark, &cells).windows(2) {
                set(pair[0].0, pair[0].1, i);
                for (c, r) in segment(pair[0], pair[1]) {
                    set(c, r, i);
                }
            }
        }
        for (c, r) in &cells {
            set(*c, *r, i);
            if layer.mark == Mark::Point {
                set(c + 1, *r, i);
                set(*c, r + 1, i);
                set(c + 1, r + 1, i);
            }
        }
    }
    dots.iter()
        .zip(&owners)
        .map(|(cells, owners)| {
            let mut line = String::new();
            let mut run = String::new();
            let mut current = None;
            for (mask, owner) in cells.iter().zip(owners) {
                if *owner != current {
                    line.push_str(&style.tint(current.and_then(|o: usize| colors[o]), &run));
                    run.clear();
                    current = *owner;
                }
                run.push(if *mask == 0 { ' ' } else { char::from_u32(0x2800 + *mask as u32).unwrap_or(' ') });
            }
            line.push_str(&style.tint(current.and_then(|o| colors[o]), &run));
            line.trim_end().to_string()
        })
        .collect()
}

impl Plot {

    fn panel_scene(&self, width: f32, height: f32) -> Scene {
        let mut scene = Scene { width, height, ..Scene::default() };
        let tiny = height < 120.0 || width < 160.0;
        let mut top = 0.0;
        if let (Some(title), false) = (&self.title, tiny) {
            scene.shapes.push(Shape::Text { x: width / 2.0, y: 18.0, text: title.clone(), size: 15.0, anchor: Anchor::Middle, paint: Paint::Text, bold: true, angle: 0.0 });
            top = 32.0;
        }
        let (top_label, bottom_label) = if tiny { (None, None) } else { self.axis_labels() };
        if let Some(label) = top_label {
            scene.shapes.push(text_shape(self.left_margin(), top + 11.0, label, 12.0, Anchor::Start, Paint::Muted));
            top += 22.0;
        }
        let mut bottom = 0.0;
        if let Some(label) = bottom_label {
            scene.shapes.push(text_shape(width / 2.0, height - 10.0, label, 12.0, Anchor::Middle, Paint::Muted));
            bottom = 18.0;
        }
        let n = self.panels.len();
        let avail = height - top - bottom;
        let room = |cols: usize| (width / cols as f32 - 82.0).min(avail / n.div_ceil(cols) as f32 - 64.0);
        let square = ((n as f32).sqrt().ceil() as usize).max(1);
        let cols = self
            .columns
            .filter(|c| *c > 0)
            .unwrap_or_else(|| (1..=n).max_by(|a, b| room(*a).total_cmp(&room(*b)).then_with(|| a.abs_diff(square).cmp(&b.abs_diff(square)).reverse())).unwrap_or(1))
            .min(n.max(1));
        let rows = n.div_ceil(cols);
        let (cell_w, cell_h) = (width / cols as f32, avail / rows as f32);
        for (i, panel) in self.panels.iter().enumerate() {
            let (dx, dy) = ((i % cols) as f32 * cell_w, top + (i / cols) as f32 * cell_h);
            let inner = shift_scene(panel.body_scene(cell_w, cell_h, true, &[]), dx, dy);
            scene.shapes.extend(inner.shapes);
            scene.legend.extend(inner.legend);
            scene.samples.extend(inner.samples);
            scene.picks.extend(inner.picks);
        }
        scene
    }

    fn axis_labels(&self) -> (Option<String>, Option<String>) {
        let (value_label, category_label) = (self.y.label.clone(), self.x.label.clone());
        if self.horizontal { (category_label, value_label) } else { (value_label, category_label) }
    }

    fn left_margin(&self) -> f32 {
        if self.horizontal {
            let longest = self.categories.iter().map(|c| c.chars().count()).max().unwrap_or(0) as f32;
            20.0 + longest * 7.0
        } else if self.mark() == Some(Mark::Heat) {
            let longest = self.layers.iter().flat_map(|l| l.bands.iter()).map(|b| b.chars().count()).max().unwrap_or(0) as f32;
            16.0 + longest * 7.0
        } else {
            58.0
        }
    }

    fn body_scene(&self, width: f32, height: f32, bare: bool, hidden: &[usize]) -> Scene {
        if self.mark() == Some(Mark::Pie) {
            return self.pie_scene(width, height);
        }
        let mut scene = Scene { width, height, ..Scene::default() };
        let tiny = bare && (height < 120.0 || width < 160.0);
        let entries: Vec<(usize, usize, String)> = self
            .layers
            .iter()
            .enumerate()
            .flat_map(|(li, l)| l.series.iter().enumerate().filter_map(move |(si, s)| s.name.clone().map(|n| (li, si, n))))
            .collect();
        let series_count: usize = self.layers.iter().map(|l| l.series.len()).sum();
        let legend_at = match self.legend {
            _ if tiny || entries.is_empty() => LegendAt::Hidden,
            LegendAt::Auto if series_count > 1 => LegendAt::Top,
            LegendAt::Auto => LegendAt::Hidden,
            other => other,
        };
        let (top_label, bottom_label) = if bare { (None, None) } else { self.axis_labels() };
        let title_room = match &self.title {
            None => 0.0,
            Some(_) if tiny => 0.0,
            Some(_) if bare => 14.0,
            Some(_) => 24.0,
        } + if self.subtitle.is_some() && !bare { 16.0 } else { 0.0 };
        let legend_room = if legend_at == LegendAt::Top || top_label.is_some() { 22.0 } else { 0.0 };
        let legend_width = if legend_at == LegendAt::Right { entries.iter().map(|(_, _, n)| n.chars().count()).max().unwrap_or(0) as f32 * 6.5 + 34.0 } else { 0.0 };
        let legend_bottom = if legend_at == LegendAt::Bottom { 22.0 } else { 0.0 };
        let caption_room = if self.caption.is_some() && !bare { 18.0 } else { 0.0 };
        let any_labels = self.layers.iter().any(|l| l.labels);
        let (label_above, label_beside) = if any_labels { if self.horizontal { (0.0, 28.0) } else { (12.0, 0.0) } } else { (0.0, 0.0) };
        let right_axis = self.layers.iter().any(|l| l.side == Side::Right);
        let edge = if self.dates { 40.0 } else { 24.0 };
        let left = if tiny { 6.0 } else { self.left_margin() };
        let right = if tiny { 6.0 } else { edge + label_beside + legend_width + if right_axis { 46.0 } else { 0.0 } };
        let longest_category = self.categories.iter().map(|c| c.chars().count()).max().unwrap_or(0) as f32;
        let band_estimate = (width - left - right) / self.categories.len().max(1) as f32;
        let rotate = !self.horizontal && !self.categories.is_empty() && self.mark() != Some(Mark::Histogram) && longest_category * CHAR_W > band_estimate - 6.0;
        let rotated_room = if rotate { (longest_category * CHAR_W * 0.64 + 6.0).min(70.0) } else { 0.0 };
        let top = if tiny { 6.0 } else { 16.0 + title_room + legend_room + label_above };
        let bottom = if tiny { 6.0 } else { 34.0 + if bottom_label.is_some() { 18.0 } else { 0.0 } + legend_bottom + caption_room + rotated_room };
        if !bare && height - bottom - top < 60.0 {
            return self.body_scene(width, height, true, hidden);
        }
        let frame = Frame { x0: left, x1: (width - right).max(left + 10.0), y0: top, y1: (height - bottom).max(top + 10.0) };
        let (x0, x1, y0, y1) = (frame.x0, frame.x1, frame.y0, frame.y1);
        let (plot_w, plot_h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        if let (Some(title), false) = (&self.title, tiny) {
            let (y, size) = if bare { (14.0, 13.0) } else { (18.0, 15.0) };
            scene.shapes.push(Shape::Text { x: width / 2.0, y, text: title.clone(), size, anchor: Anchor::Middle, paint: Paint::Text, bold: true, angle: 0.0 });
        }
        if let (Some(subtitle), false) = (&self.subtitle, bare) {
            scene.shapes.push(text_shape(width / 2.0, 36.0, subtitle.clone(), 12.0, Anchor::Middle, Paint::Muted));
        }
        if let Some(label) = top_label {
            scene.shapes.push(text_shape(x0, y0 - 11.0, label, 12.0, Anchor::Start, Paint::Muted));
        }
        if let Some(label) = bottom_label {
            scene.shapes.push(text_shape((x0 + x1) / 2.0, height - 10.0 - caption_room, label, 12.0, Anchor::Middle, Paint::Muted));
        }
        if let (Some(caption), false) = (&self.caption, bare) {
            scene.shapes.push(text_shape(x0, height - 8.0, caption.clone(), 11.0, Anchor::Start, Paint::Muted));
        }
        if self.mark() == Some(Mark::Heat) {
            self.heat_shapes(&mut scene, &frame);
            self.finish_axes(&mut scene, &frame);
            return scene;
        }
        let from_zero = self.layers.iter().any(|l| l.side == Side::Left && l.mark.starts_at_zero());
        let (vlo, vhi) = self.value_extent(Side::Left, hidden);
        let value_count = (((if self.horizontal { plot_w } else { plot_h }) / 40.0) as usize).clamp(2, 6);
        let mut value_axis = Axis::for_values(&self.y, vlo, vhi, value_count, from_zero, false);
        let whole_values = self.layers.iter().filter(|l| l.side == Side::Left).all(|l| match l.mark {
            Mark::Box => whole(l.boxes.iter().flat_map(|b| [b.low, b.q1, b.median, b.q3, b.high])),
            _ => whole(l.series.iter().flat_map(|s| s.ys.iter().copied())),
        }) && !self.layers.is_empty();
        if whole_values {
            value_axis = value_axis.whole();
        }
        let right_axis_scale = right_axis.then(|| {
            let (lo, hi) = self.value_extent(Side::Right, hidden);
            let zero = self.layers.iter().any(|l| l.side == Side::Right && l.mark.starts_at_zero());
            Axis::for_values(&self.y2, lo, hi, value_count, zero, false)
        });
        let horizontal = self.horizontal;
        let value_at = |axis: &Axis, v: f64| if horizontal { axis.place(v, x0, x1) } else { axis.place(v, y1, y0) };
        for tick in &value_axis.ticks {
            let label = value_axis.label(*tick, false, self.y.format.as_deref());
            if horizontal {
                let x = value_at(&value_axis, *tick);
                if self.grid {
                    scene.shapes.push(Shape::Line { x1: x, y1: y0, x2: x, y2: y1, paint: Paint::Grid, width: 1.0, dashed: false });
                }
                scene.shapes.push(text_shape(x, y1 + 14.0, label, 11.0, Anchor::Middle, Paint::Muted));
            } else {
                let y = value_at(&value_axis, *tick);
                if self.grid {
                    scene.shapes.push(Shape::Line { x1: x0, y1: y, x2: x1, y2: y, paint: Paint::Grid, width: 1.0, dashed: false });
                }
                scene.shapes.push(text_shape(x0 - 8.0, y, label, 11.0, Anchor::End, Paint::Muted));
            }
        }
        if let Some(axis) = &right_axis_scale {
            for tick in &axis.ticks {
                let label = axis.label(*tick, false, self.y2.format.as_deref());
                scene.shapes.push(text_shape(x1 + 8.0, value_at(axis, *tick), label, 11.0, Anchor::Start, Paint::Muted));
            }
            if let Some(label) = &self.y2.label {
                scene.shapes.push(text_shape(x1 + 8.0, y0 - 11.0, label.clone(), 12.0, Anchor::Start, Paint::Muted));
            }
        }
        let categorical = !self.categories.is_empty();
        let n = self.categories.len().max(1);
        let band = if horizontal { plot_h / n as f32 } else { plot_w / n as f32 };
        let x_axis = (!categorical).then(|| {
            let histogram = self.layers.iter().find(|l| l.mark == Mark::Histogram);
            let xs: Vec<f64> = self.layers.iter().flat_map(|l| l.series.iter().flat_map(|s| s.xs.iter().copied())).collect();
            let (mut xmin, mut xmax) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
            if let Some(layer) = histogram {
                xmax = xmax.max(layer.series.first().and_then(|s| s.xs.last()).copied().unwrap_or(0.0) + layer.step);
                if self.x.range.is_none() {
                    return Axis::fixed(xmin, xmax, 6, false);
                }
            }
            if !xmin.is_finite() {
                xmin = 0.0;
                xmax = 1.0;
            }
            let count = ((plot_w / if self.dates { 90.0 } else { 60.0 }) as usize).clamp(2, 6);
            let mut axis = Axis::for_values(&self.x, xmin, xmax, count, false, true);
            if whole(xs.iter().copied()) {
                axis = axis.whole();
            }
            axis
        });
        let position = |x: f64| -> f32 {
            match &x_axis {
                Some(axis) => axis.place(x, x0, x1),
                None if horizontal => y0 + (x as f32 + 0.5) * band,
                None => x0 + (x as f32 + 0.5) * band,
            }
        };
        if let Some(axis) = &x_axis {
            let histogram = self.layers.iter().find(|l| l.mark == Mark::Histogram);
            match histogram {
                Some(layer) if self.x.range.is_none() => {
                    let series = layer.series.first();
                    let mut edges: Vec<f64> = series.map(|s| s.xs.clone()).unwrap_or_default();
                    edges.push(axis.hi);
                    let (labels, step) = if edges.len() <= 12 { (edges, layer.step) } else { ticks(axis.lo, axis.hi, 6) };
                    for edge in labels {
                        scene.shapes.push(text_shape(position(edge), y1 + 14.0, tick_label(edge, step, false), 11.0, Anchor::Middle, Paint::Muted));
                    }
                }
                _ => {
                    for tick in &axis.ticks {
                        let label = axis.label(*tick, self.dates, self.x.format.as_deref());
                        scene.shapes.push(text_shape(position(*tick), y1 + 14.0, label, 11.0, Anchor::Middle, Paint::Muted));
                    }
                }
            }
        } else {
            for (c, category) in self.categories.iter().enumerate() {
                let center = position(c as f64);
                if horizontal {
                    scene.shapes.push(text_shape(x0 - 8.0, center, category.clone(), 11.0, Anchor::End, Paint::Muted));
                } else if rotate {
                    scene.shapes.push(Shape::Text { x: center + 4.0, y: y1 + 10.0, text: category.clone(), size: 11.0, anchor: Anchor::End, paint: Paint::Muted, bold: false, angle: -40.0 });
                } else {
                    scene.shapes.push(text_shape(center, y1 + 14.0, category.clone(), 11.0, Anchor::Middle, Paint::Muted));
                }
            }
        }
        for rule in self.rules.iter().filter(|r| r.vertical) {
            let Some(axis) = &x_axis else { continue };
            if !axis.holds(rule.value) {
                continue;
            }
            let x = position(rule.value);
            scene.shapes.push(Shape::Line { x1: x, y1: y0, x2: x, y2: y1, paint: Paint::Axis, width: 1.0, dashed: true });
            if let Some(label) = &rule.label {
                scene.shapes.push(text_shape(x + 4.0, y0 + 8.0, label.clone(), 10.5, Anchor::Start, Paint::Muted));
            }
        }
        for (li, layer) in self.layers.iter().enumerate() {
            let axis = match (layer.side, &right_axis_scale) {
                (Side::Right, Some(axis)) => axis,
                _ => &value_axis,
            };
            let base = if axis.log { axis.lo } else { 0.0 };
            let visible: Vec<bool> = (0..layer.series.len()).map(|si| !hidden.contains(&self.global_index(li, si))).collect();
            let context = LayerContext { frame: &frame, band, position: &position, value_at: &|v| value_at(axis, v), baseline: base, visible: &visible, layer_index: li };
            match layer.mark {
                Mark::Bar | Mark::Lollipop => self.bar_shapes(&mut scene, layer, &context),
                Mark::Box => self.box_shapes(&mut scene, layer, &context),
                Mark::Histogram => self.histogram_shapes(&mut scene, layer, &context),
                Mark::Line | Mark::Step | Mark::Point | Mark::Area | Mark::Density => self.path_shapes(&mut scene, layer, &context),
                Mark::Pie | Mark::Heat => {}
            }
        }
        for rule in self.rules.iter().filter(|r| !r.vertical) {
            if !value_axis.holds(rule.value) {
                continue;
            }
            let v = value_at(&value_axis, rule.value);
            let line = if horizontal {
                Shape::Line { x1: v, y1: y0, x2: v, y2: y1, paint: Paint::Axis, width: 1.0, dashed: true }
            } else {
                Shape::Line { x1: x0, y1: v, x2: x1, y2: v, paint: Paint::Axis, width: 1.0, dashed: true }
            };
            scene.shapes.push(line);
            if let Some(label) = &rule.label {
                if horizontal {
                    scene.shapes.push(text_shape(v + 4.0, y0 + 8.0, label.clone(), 10.5, Anchor::Start, Paint::Muted));
                } else {
                    scene.shapes.push(text_shape(x1 - 4.0, v - 7.0, label.clone(), 10.5, Anchor::End, Paint::Muted));
                }
            }
        }
        for note in &self.notes {
            let px = if horizontal { value_at(&value_axis, note.y) } else { position(note.x) };
            let py = if horizontal { position(note.x) } else { value_at(&value_axis, note.y) };
            scene.shapes.push(Shape::Text { x: px + 6.0, y: py - 6.0, text: note.text.clone(), size: 11.0, anchor: Anchor::Start, paint: Paint::Text, bold: false, angle: 0.0 });
            scene.shapes.push(Shape::Circle { x: px, y: py, r: 2.5, paint: Paint::Text, alpha: 1.0, tip: None });
        }
        self.finish_axes(&mut scene, &frame);
        if right_axis {
            scene.shapes.push(Shape::Line { x1, y1: y0, x2: x1, y2: y1, paint: Paint::Axis, width: 1.0, dashed: false });
        }
        self.legend_shapes(&mut scene, &entries, legend_at, &frame, width, height, caption_room, hidden);
        scene
    }

    fn finish_axes(&self, scene: &mut Scene, frame: &Frame) {
        let Frame { x0, x1, y0, y1 } = *frame;
        scene.shapes.push(Shape::Line { x1: x0, y1: y0, x2: x0, y2: y1, paint: Paint::Axis, width: 1.0, dashed: false });
        scene.shapes.push(Shape::Line { x1: x0, y1, x2: x1, y2: y1, paint: Paint::Axis, width: 1.0, dashed: false });
    }

    #[allow(clippy::too_many_arguments)]
    fn legend_shapes(&self, scene: &mut Scene, entries: &[(usize, usize, String)], at: LegendAt, frame: &Frame, width: f32, height: f32, caption_room: f32, hidden: &[usize]) {
        let paint_of = |li: usize, si: usize| {
            let paint = self.paint(&self.layers[li], si);
            if hidden.contains(&self.global_index(li, si)) { Paint::Grid } else { paint }
        };
        match at {
            LegendAt::Hidden | LegendAt::Auto => {}
            LegendAt::Top | LegendAt::Bottom => {
                let y = if at == LegendAt::Top { frame.y0 - 11.0 } else { height - 12.0 - caption_room - if self.x.label.is_some() { 18.0 } else { 0.0 } };
                let mut right = frame.x1;
                for (li, si, name) in entries.iter().rev() {
                    let text_w = name.chars().count() as f32 * 6.5;
                    right -= text_w;
                    scene.shapes.push(text_shape(right, y, name.clone(), 11.0, Anchor::Start, Paint::Muted));
                    right -= 16.0;
                    scene.shapes.push(Shape::Rect { x: right, y: y - 5.0, w: 10.0, h: 10.0, paint: paint_of(*li, *si), alpha: 1.0, tip: None });
                    scene.legend.push(LegendEntry { x: right, y: y - 8.0, w: 16.0 + text_w, h: 16.0, series: self.global_index(*li, *si), name: name.clone() });
                    right -= 14.0;
                }
            }
            LegendAt::Right => {
                let x = frame.x1 + 14.0 + if self.layers.iter().any(|l| l.side == Side::Right) { 46.0 } else { 0.0 };
                let mut y = frame.y0 + 8.0;
                for (li, si, name) in entries {
                    scene.shapes.push(Shape::Rect { x, y: y - 5.0, w: 10.0, h: 10.0, paint: paint_of(*li, *si), alpha: 1.0, tip: None });
                    scene.shapes.push(text_shape(x + 16.0, y, name.clone(), 11.0, Anchor::Start, Paint::Muted));
                    scene.legend.push(LegendEntry { x, y: y - 8.0, w: width - x, h: 16.0, series: self.global_index(*li, *si), name: name.clone() });
                    y += 18.0;
                }
            }
        }
    }

    fn bar_shapes(&self, scene: &mut Scene, layer: &Layer, context: &LayerContext) {
        let LayerContext { frame, band, position, value_at, baseline, visible, .. } = *context;
        let Frame { x0, y0, .. } = *frame;
        let _ = (x0, y0);
        let n = self.categories.len().max(1);
        let shown: Vec<usize> = (0..layer.series.len()).filter(|i| visible[*i]).collect();
        let slots = if layer.stacks() { 1 } else { shown.len().max(1) };
        let slot = band * 0.72 / slots as f32;
        let mut positive = vec![baseline; n];
        let mut negative = vec![0.0f64; n];
        let value_text = |v: f64| format_tick(v, self.y.format.as_deref()).unwrap_or_else(|| number(v));
        for (slot_index, si) in shown.iter().enumerate() {
            let series = &layer.series[*si];
            let paint = self.paint(layer, *si);
            for (k, (x, y)) in series.xs.iter().zip(&series.ys).enumerate() {
                let c = *x as usize;
                let (from, to) = if layer.stacks() {
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
                    (baseline, y.max(baseline).max(*y))
                };
                let center = position(c as f64) - band / 2.0 + band * 0.14 + if layer.stacks() { slot / 2.0 } else { slot_index as f32 * slot + slot / 2.0 };
                let category = self.categories.get(c).cloned().unwrap_or_default();
                let tip = Some(match &series.name {
                    Some(name) if layer.series.len() > 1 => format!("{category} {name}: {}", number(*y)),
                    _ => format!("{category}: {}", number(*y)),
                });
                let (a, b) = (value_at(from), value_at(to));
                let thickness = slot.max(1.0) - 1.0;
                if layer.mark == Mark::Lollipop {
                    if self.horizontal {
                        scene.shapes.push(Shape::Line { x1: a, y1: center, x2: b, y2: center, paint, width: 2.0, dashed: false });
                        scene.shapes.push(Shape::Circle { x: b, y: center, r: 5.0, paint, alpha: layer.opacity, tip: tip.clone() });
                        scene.picks.push(Pick { x: b - 5.0, y: center - 5.0, w: 10.0, h: 10.0, key: category.clone() });
                    } else {
                        scene.shapes.push(Shape::Line { x1: center, y1: a, x2: center, y2: b, paint, width: 2.0, dashed: false });
                        scene.shapes.push(Shape::Circle { x: center, y: b, r: 5.0, paint, alpha: layer.opacity, tip: tip.clone() });
                        scene.picks.push(Pick { x: center - 5.0, y: b - 5.0, w: 10.0, h: 10.0, key: category.clone() });
                    }
                } else if self.horizontal {
                    let row = center - thickness / 2.0;
                    scene.shapes.push(Shape::Rect { x: a.min(b), y: row, w: (a - b).abs(), h: thickness, paint, alpha: layer.opacity, tip });
                    scene.picks.push(Pick { x: a.min(b), y: row, w: (a - b).abs(), h: thickness, key: category.clone() });
                } else {
                    let column = center - thickness / 2.0;
                    scene.shapes.push(Shape::Rect { x: column, y: a.min(b), w: thickness, h: (a - b).abs(), paint, alpha: layer.opacity, tip });
                    scene.picks.push(Pick { x: column, y: a.min(b), w: thickness, h: (a - b).abs(), key: category.clone() });
                }
                if layer.labels && !layer.stacks() {
                    if self.horizontal {
                        scene.shapes.push(text_shape(a.max(b) + 5.0, center, value_text(*y), 10.5, Anchor::Start, Paint::Muted));
                    } else {
                        scene.shapes.push(text_shape(center, a.min(b) - 8.0, value_text(*y), 10.5, Anchor::Middle, Paint::Muted));
                    }
                }
                if let (Some(low), Some(high)) = (series.lows.get(k), series.highs.get(k)) {
                    self.range_shapes(scene, center, value_at(*low), value_at(*high), Paint::Axis);
                }
            }
        }
        if layer.labels && layer.stacks() {
            for (c, total) in positive.iter().enumerate().filter(|(_, total)| **total > baseline) {
                let center = position(c as f64) - band / 2.0 + band * 0.14 + slot.max(1.0) / 2.0;
                if self.horizontal {
                    scene.shapes.push(text_shape(value_at(*total) + 5.0, center, value_text(*total), 10.5, Anchor::Start, Paint::Muted));
                } else {
                    scene.shapes.push(text_shape(center, value_at(*total) - 8.0, value_text(*total), 10.5, Anchor::Middle, Paint::Muted));
                }
            }
        }
    }

    fn range_shapes(&self, scene: &mut Scene, center: f32, a: f32, b: f32, paint: Paint) {
        let cap = 4.0;
        if self.horizontal {
            scene.shapes.push(Shape::Line { x1: a, y1: center, x2: b, y2: center, paint, width: 1.2, dashed: false });
            scene.shapes.push(Shape::Line { x1: a, y1: center - cap, x2: a, y2: center + cap, paint, width: 1.2, dashed: false });
            scene.shapes.push(Shape::Line { x1: b, y1: center - cap, x2: b, y2: center + cap, paint, width: 1.2, dashed: false });
        } else {
            scene.shapes.push(Shape::Line { x1: center, y1: a, x2: center, y2: b, paint, width: 1.2, dashed: false });
            scene.shapes.push(Shape::Line { x1: center - cap, y1: a, x2: center + cap, y2: a, paint, width: 1.2, dashed: false });
            scene.shapes.push(Shape::Line { x1: center - cap, y1: b, x2: center + cap, y2: b, paint, width: 1.2, dashed: false });
        }
    }

    fn box_shapes(&self, scene: &mut Scene, layer: &Layer, context: &LayerContext) {
        let LayerContext { frame, band, position, value_at, .. } = *context;
        let half = band * 0.25;
        for (i, b) in layer.boxes.iter().enumerate() {
            let center = position(i as f64);
            let paint = self.paint(layer, i);
            let line = |x1: f32, y1: f32, x2: f32, y2: f32| Shape::Line { x1, y1, x2, y2, paint: Paint::Axis, width: 1.5, dashed: false };
            scene.shapes.push(line(center, value_at(b.high), center, value_at(b.q3)));
            scene.shapes.push(line(center, value_at(b.q1), center, value_at(b.low)));
            scene.shapes.push(line(center - half / 2.0, value_at(b.high), center + half / 2.0, value_at(b.high)));
            scene.shapes.push(line(center - half / 2.0, value_at(b.low), center + half / 2.0, value_at(b.low)));
            let tip = format!("{}: {} to {}, median {}", b.label, number(b.q1), number(b.q3), number(b.median));
            let (top, bottom) = (value_at(b.q3), value_at(b.q1));
            scene.shapes.push(Shape::Rect { x: center - half, y: top, w: half * 2.0, h: bottom - top, paint, alpha: layer.opacity, tip: Some(tip) });
            scene.picks.push(Pick { x: center - half, y: top, w: half * 2.0, h: bottom - top, key: b.label.clone() });
            scene.shapes.push(Shape::Line { x1: center - half, y1: value_at(b.median), x2: center + half, y2: value_at(b.median), paint: Paint::Text, width: 2.0, dashed: false });
            for outlier in &b.outliers {
                scene.shapes.push(Shape::Circle { x: center, y: value_at(*outlier), r: 3.0, paint, alpha: 1.0, tip: Some(format!("{}: {}", b.label, number(*outlier))) });
            }
        }
        let _ = frame;
    }

    fn histogram_shapes(&self, scene: &mut Scene, layer: &Layer, context: &LayerContext) {
        let LayerContext { position, value_at, baseline, visible, .. } = *context;
        let value_text = |v: f64| format_tick(v, self.y.format.as_deref()).unwrap_or_else(|| number(v));
        for (si, series) in layer.series.iter().enumerate().filter(|(si, _)| visible[*si]) {
            let paint = self.paint(layer, si);
            let base = value_at(baseline);
            for (i, (x, y)) in series.xs.iter().zip(&series.ys).enumerate() {
                let (left, right) = (position(*x), position(x + layer.step));
                let category = self.categories.get(i).cloned().unwrap_or_default();
                let tip = format!("{category}: {}", number(*y));
                let top = value_at(y.max(baseline));
                let w = (right - left - 1.0).max(1.0);
                scene.shapes.push(Shape::Rect { x: left + 0.5, y: top, w, h: base - top, paint, alpha: layer.opacity, tip: Some(tip) });
                scene.picks.push(Pick { x: left + 0.5, y: top, w, h: base - top, key: category });
                if layer.labels {
                    scene.shapes.push(text_shape((left + right) / 2.0, top - 8.0, value_text(*y), 10.5, Anchor::Middle, Paint::Muted));
                }
            }
        }
    }

    fn path_shapes(&self, scene: &mut Scene, layer: &Layer, context: &LayerContext) {
        let LayerContext { frame, position, value_at, baseline, visible, layer_index, .. } = *context;
        let stack = layer.stacks();
        let mut base: BTreeMap<u64, f64> = BTreeMap::new();
        let max_size = layer.series.iter().flat_map(|s| s.sizes.iter().copied()).fold(0.0, f64::max);
        let largest = ((frame.x1 - frame.x0).min(frame.y1 - frame.y0) * 0.1).max(2.0);
        let value_text = |v: f64| format_tick(v, self.y.format.as_deref()).unwrap_or_else(|| number(v));
        for (si, series) in layer.series.iter().enumerate() {
            if !visible[si] {
                continue;
            }
            let paint = self.paint(layer, si);
            let global = self.global_index(layer_index, si);
            let floors: Vec<f64> = series.xs.iter().map(|x| if stack { base.get(&x.to_bits()).copied().unwrap_or(0.0) } else { baseline }).collect();
            let tops: Vec<f64> = series.ys.iter().zip(&floors).map(|(y, f)| y + f).collect();
            let points: Vec<(f32, f32)> = series.xs.iter().zip(&tops).map(|(x, y)| (position(*x), value_at(*y))).collect();
            let traced: Vec<(f32, f32)> = if layer.mark == Mark::Step {
                let mut out = Vec::with_capacity(points.len() * 2);
                for pair in points.windows(2) {
                    out.push(pair[0]);
                    out.push((pair[1].0, pair[0].1));
                }
                if let Some(last) = points.last() {
                    out.push(*last);
                }
                out
            } else {
                points.clone()
            };
            if matches!(layer.mark, Mark::Area | Mark::Density) && !points.is_empty() {
                let mut polygon = traced.clone();
                let mut lower: Vec<(f32, f32)> = series.xs.iter().zip(&floors).map(|(x, f)| (position(*x), value_at(*f))).collect();
                lower.reverse();
                polygon.extend(lower);
                scene.shapes.push(Shape::Polygon { points: polygon, paint, alpha: 0.35 * layer.opacity, tip: None });
            }
            if matches!(layer.mark, Mark::Line | Mark::Step | Mark::Area | Mark::Density) && traced.len() > 1 {
                scene.shapes.push(Shape::Polyline { points: traced.clone(), paint, width: layer.width, dashed: layer.dashed });
            }
            let marker = if layer.markers { MARKERS[(layer.first_color + si) % MARKERS.len()] } else { layer.marker };
            let dots = layer.mark == Mark::Point || (matches!(layer.mark, Mark::Line | Mark::Step) && layer.dots && points.len() <= 60);
            for (k, ((x, y), (px, py))) in series.xs.iter().zip(&series.ys).zip(&points).enumerate() {
                let text = match &series.name {
                    Some(name) => format!("{name}: {}", number(*y)),
                    None => number(*y),
                };
                scene.samples.push(Sample { x: *px, y: *py, series: global, text: format!("{} · {text}", self.x_text(*x)) });
                if let (Some(low), Some(high)) = (series.lows.get(k), series.highs.get(k)) {
                    self.range_shapes(scene, *px, value_at(*low), value_at(*high), paint);
                }
                if dots {
                    let r = match series.sizes.get(k) {
                        Some(size) if max_size > 0.0 => (3.0 + 15.0 * (size / max_size).sqrt() as f32).min(largest),
                        _ if layer.mark == Mark::Point => 3.5,
                        _ => 2.5,
                    };
                    let tip = Some(match series.texts.get(k) {
                        Some(label) => format!("{label}: {}, {}", self.x_text(*x), number(*y)),
                        None => format!("{}, {}", self.x_text(*x), number(*y)),
                    });
                    scene.shapes.push(marker_shape(marker, *px, *py, r, paint, layer.opacity, tip));
                    let key = series.texts.get(k).cloned().unwrap_or_else(|| self.x_text(*x));
                    scene.picks.push(Pick { x: px - r, y: py - r, w: 2.0 * r, h: 2.0 * r, key });
                }
                if layer.labels {
                    let label = series.texts.get(k).cloned().unwrap_or_else(|| value_text(*y));
                    scene.shapes.push(text_shape(*px, py - 9.0, label, 10.5, Anchor::Middle, Paint::Muted));
                }
            }
            if stack {
                for (x, top) in series.xs.iter().zip(&tops) {
                    base.insert(x.to_bits(), *top);
                }
            }
        }
    }

    fn heat_shapes(&self, scene: &mut Scene, frame: &Frame) {
        let Frame { x0, x1, y0, y1 } = *frame;
        let Some(layer) = self.layers.iter().find(|l| l.mark == Mark::Heat) else { return };
        let Some(series) = layer.series.first() else { return };
        let cols = self.categories.len().max(1);
        let rows = layer.bands.len().max(1);
        let (cell_w, cell_h) = ((x1 - x0) / cols as f32, (y1 - y0) / rows as f32);
        let (lo, hi) = series.sizes.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        for ((x, y), value) in series.xs.iter().zip(&series.ys).zip(&series.sizes) {
            let (c, r) = (*x as usize, *y as usize);
            let t = if hi > lo { ((value - lo) / (hi - lo)) as f32 } else { 0.5 };
            let px = x0 + c as f32 * cell_w;
            let py = y0 + r as f32 * cell_h;
            let tip = format!("{} × {}: {}", self.categories.get(c).cloned().unwrap_or_default(), layer.bands.get(r).cloned().unwrap_or_default(), number(*value));
            scene.shapes.push(Shape::Rect { x: px + 0.5, y: py + 0.5, w: (cell_w - 1.0).max(1.0), h: (cell_h - 1.0).max(1.0), paint: Paint::Heat(t), alpha: 1.0, tip: Some(tip) });
            scene.picks.push(Pick { x: px, y: py, w: cell_w, h: cell_h, key: self.categories.get(c).cloned().unwrap_or_default() });
            if layer.labels || cell_w > 48.0 {
                let paint = if t > 0.55 { Paint::Rgb(Rgb(255, 255, 255)) } else { Paint::Text };
                scene.shapes.push(text_shape(px + cell_w / 2.0, py + cell_h / 2.0, number(*value), 10.5, Anchor::Middle, paint));
            }
        }
        for (c, category) in self.categories.iter().enumerate() {
            scene.shapes.push(text_shape(x0 + (c as f32 + 0.5) * cell_w, y1 + 14.0, category.clone(), 11.0, Anchor::Middle, Paint::Muted));
        }
        for (r, band) in layer.bands.iter().enumerate() {
            scene.shapes.push(text_shape(x0 - 8.0, y0 + (r as f32 + 0.5) * cell_h, band.clone(), 11.0, Anchor::End, Paint::Muted));
        }
    }

    fn pie_scene(&self, width: f32, height: f32) -> Scene {
        let mut scene = Scene { width, height, ..Scene::default() };
        let top = 16.0 + if self.title.is_some() { 24.0 } else { 0.0 };
        if let Some(title) = &self.title {
            scene.shapes.push(Shape::Text { x: width / 2.0, y: 18.0, text: title.clone(), size: 15.0, anchor: Anchor::Middle, paint: Paint::Text, bold: true, angle: 0.0 });
        }
        let Some(layer) = self.layers.first() else { return scene };
        let Some(series) = layer.series.first() else { return scene };
        let total: f64 = series.ys.iter().sum();
        let (cx, cy) = (width * 0.4, top + (height - top) / 2.0);
        let r = ((height - top) / 2.0 - 28.0).min(width * 0.24).max(10.0);
        let inner = if layer.donut { r * 0.55 } else { 0.0 };
        let mut angle = -FRAC_PI_2;
        for (i, (x, y)) in series.xs.iter().zip(&series.ys).enumerate() {
            let sweep = if total > 0.0 { (y / total) as f32 * TAU } else { 0.0 };
            let label = self.categories.get(*x as usize).cloned().unwrap_or_default();
            let share = if total > 0.0 { (y / total * 100.0).round() } else { 0.0 };
            let tip = format!("{label}: {} ({share}%)", number(*y));
            scene.shapes.push(Shape::Arc { cx, cy, r, inner, start: angle, end: angle + sweep, paint: self.paint(layer, i), tip: Some(tip) });
            let mid = angle + sweep / 2.0;
            let (lx, ly) = (cx + (r + 14.0) * mid.cos(), cy + (r + 14.0) * mid.sin());
            let anchor = if mid.cos() >= 0.0 { Anchor::Start } else { Anchor::End };
            scene.shapes.push(text_shape(lx, ly, format!("{label} {share}%"), 11.0, anchor, Paint::Muted));
            angle += sweep;
        }
        scene
    }

    pub fn svg(&self) -> String {
        self.svg_sized(640.0, 400.0, false)
    }

    pub fn svg_sized(&self, width: f32, height: f32, dark: bool) -> String {
        let scene = self.scene(width, height);
        let series_colors: Vec<Rgb> = SERIES_COLORS.iter().filter_map(|h| Rgb::parse(h)).collect();
        let (axis, grid, text, muted, back) = if dark { ("#8a94a6", "#2a313d", "#e6e9ef", "#9aa3b2", "#161a21") } else { ("#56637a", "#e3e7ee", "#1a2230", "#66728a", "#ffffff") };
        let heat_lo = if dark { Rgb(30, 40, 52) } else { Rgb(232, 240, 248) };
        let heat_hi = Rgb(11, 61, 107);
        let color = |paint: &Paint| -> String {
            match paint {
                Paint::Series(i) => series_colors[i % series_colors.len()].hex(),
                Paint::Rgb(rgb) => rgb.hex(),
                Paint::Heat(t) => heat_lo.mix(heat_hi, *t).hex(),
                Paint::Axis => axis.to_string(),
                Paint::Grid => grid.to_string(),
                Paint::Text => text.to_string(),
                Paint::Muted => muted.to_string(),
            }
        };
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" font-family=\"sans-serif\">\n",
            w = scene.width,
            h = scene.height
        );
        if dark {
            let _ = writeln!(out, "<rect width=\"{}\" height=\"{}\" fill=\"{back}\"/>", scene.width, scene.height);
        }
        let opacity = |alpha: f32| if alpha < 1.0 { format!(" fill-opacity=\"{alpha:.2}\"") } else { String::new() };
        for shape in &scene.shapes {
            match shape {
                Shape::Rect { x, y, w, h, paint, alpha, .. } => {
                    let _ = writeln!(out, "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"{}\"{}/>", color(paint), opacity(*alpha));
                }
                Shape::Line { x1, y1, x2, y2, paint, width, dashed } => {
                    let dash = if *dashed { " stroke-dasharray=\"4 3\"" } else { "" };
                    let _ = writeln!(out, "<line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" stroke=\"{}\" stroke-width=\"{width}\"{dash}/>", color(paint));
                }
                Shape::Polyline { points, paint, width, dashed } => {
                    let dash = if *dashed { " stroke-dasharray=\"6 4\"" } else { "" };
                    let _ = writeln!(out, "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{width}\"{dash}/>", points_attr(points), color(paint));
                }
                Shape::Polygon { points, paint, alpha, .. } => {
                    let _ = writeln!(out, "<polygon points=\"{}\" fill=\"{}\"{}/>", points_attr(points), color(paint), opacity(*alpha));
                }
                Shape::Circle { x, y, r, paint, alpha, .. } => {
                    let _ = writeln!(out, "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{r}\" fill=\"{}\"{}/>", color(paint), opacity(*alpha));
                }
                Shape::Arc { cx, cy, r, inner, start, end, paint, .. } => {
                    let _ = writeln!(out, "<path d=\"{}\" fill=\"{}\"/>", arc_path(*cx, *cy, *r, *inner, *start, *end), color(paint));
                }
                Shape::Text { x, y, text, size, anchor, paint, bold, angle } => {
                    let anchor = match anchor {
                        Anchor::Start => "start",
                        Anchor::Middle => "middle",
                        Anchor::End => "end",
                    };
                    let weight = if *bold { " font-weight=\"bold\"" } else { "" };
                    let rotate = if *angle != 0.0 { format!(" transform=\"rotate({angle:.0} {x:.1} {y:.1})\"") } else { String::new() };
                    let _ = writeln!(
                        out,
                        "<text x=\"{x:.1}\" y=\"{y:.1}\" text-anchor=\"{anchor}\" dominant-baseline=\"middle\" font-size=\"{size}\" fill=\"{}\"{weight}{rotate}>{}</text>",
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

struct LayerContext<'a> {
    frame: &'a Frame,
    band: f32,
    position: &'a dyn Fn(f64) -> f32,
    value_at: &'a dyn Fn(f64) -> f32,
    baseline: f64,
    visible: &'a [bool],
    layer_index: usize,
}

impl Clone for LayerContext<'_> {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for LayerContext<'_> {}

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

    fn bars(categories: &[&str], values: &[f64]) -> Plot {
        let mut plot = Plot::new();
        plot.categories = categories.iter().map(|c| c.to_string()).collect();
        let mut layer = Layer::new(Mark::Bar);
        layer.series = vec![Series::new(None, (0..values.len()).map(|i| i as f64).collect(), values.to_vec())];
        plot.layers.push(layer);
        plot
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
    fn ticks_land_on_round_numbers() {
        assert_eq!(ticks(0.0, 42.0, 5), (vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0], 10.0));
        let (fractional, step) = ticks(0.0, 1.0, 5);
        let labels: Vec<String> = fractional.iter().map(|t| tick_label(*t, step, false)).collect();
        assert_eq!(labels, ["0", "0.2", "0.4", "0.6", "0.8", "1"]);
        assert_eq!(ticks(5.0, 5.0, 5), (vec![5.0], 1.0));
        assert_eq!(tick_label(19723.0, 10.0, true), "2024-01-01");
        assert_eq!(ticks(-3.0, 3.0, 4).0, [-4.0, -2.0, 0.0, 2.0, 4.0]);
        assert_eq!([compact(950.0), compact(1500.0), compact(2_000_000.0), compact(3.26e9), compact(120_000.0)], ["950", "1.5K", "2M", "3.3B", "120K"]);
    }

    #[test]
    fn histograms_bin_values_and_clamp_the_maximum() {
        let plot = histogram(&[1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 9.0], 4);
        assert_eq!(plot.layers[0].series[0].ys, [3.0, 3.0, 0.0, 1.0]);
        assert_eq!(plot.layers[0].series[0].xs, [1.0, 3.0, 5.0, 7.0]);
        assert_eq!(plot.categories, ["1–3", "3–5", "5–7", "7–9"]);
        assert_eq!(plot.describe(), "histogram with 4 bins");
        assert_eq!(histogram(&[2.0, 2.0], 3).layers[0].series[0].ys, [2.0, 0.0, 0.0]);
        assert_eq!(histogram(&[], 2).layers[0].series[0].ys, [0.0, 0.0]);
    }

    #[test]
    fn facet_grids_fit_the_space_they_are_given() {
        let mut plot = Plot::new();
        for title in ["a", "b", "c"] {
            let mut panel = bars(&["x"], &[1.0]);
            panel.title = Some(title.into());
            plot.panels.push(panel);
        }
        let title_ys = |plot: &Plot, width: f32, height: f32| -> Vec<f32> {
            let mut ys: Vec<f32> = plot
                .scene(width, height)
                .shapes
                .iter()
                .filter_map(|s| match s {
                    Shape::Text { y, bold: true, .. } => Some(*y),
                    _ => None,
                })
                .collect();
            ys.dedup();
            ys
        };
        assert_eq!(title_ys(&plot, 480.0, 220.0).len(), 1);
        assert_eq!(title_ys(&plot, 640.0, 400.0).len(), 2);
        assert_eq!(title_ys(&plot, 400.0, 900.0).len(), 3);
        plot.columns = Some(1);
        assert_eq!(title_ys(&plot, 640.0, 400.0).len(), 3, "explicit columns win");
    }

    #[test]
    fn scenes_place_bars_within_the_plot_area() {
        let plot = bars(&["a", "b"], &[1.0, 2.0]);
        let scene = plot.scene(640.0, 400.0);
        let rects: Vec<&Shape> = scene.shapes.iter().filter(|s| matches!(s, Shape::Rect { .. })).collect();
        assert_eq!(rects.len(), 2);
        let Shape::Rect { x, y, w, h, .. } = rects[1] else { unreachable!() };
        assert!(*x > 58.0 && x + w < 622.0 && *y >= 16.0 && y + h <= 366.1, "{rects:?}");
        let Shape::Rect { h: short, .. } = rects[0] else { unreachable!() };
        assert!((h / short - 2.0).abs() < 0.01);
        assert_eq!(plot.describe(), "bar chart with 2 bars");
        assert_eq!(scene.picks.len(), 2);
        assert_eq!(scene.picks[0].key, "a");
        let svg = plot.svg();
        assert!(svg.starts_with("<svg xmlns") && svg.ends_with("</svg>\n"));
        assert_eq!(escape("a<b&c"), "a&lt;b&amp;c");
        assert!(plot.svg_sized(200.0, 100.0, true).contains("fill=\"#161a21\""));
    }

    #[test]
    fn stacked_bars_sit_on_top_of_each_other() {
        let mut plot = bars(&["a"], &[1.0]);
        plot.layers[0].series.push(Series::new(Some("y".into()), vec![0.0], vec![2.0]));
        plot.layers[0].series[0].name = Some("x".into());
        plot.layers[0].stacked = true;
        let bars: Vec<_> = rects(&plot.scene(640.0, 400.0)).into_iter().filter(|r| r.2 > 20.0).collect();
        assert_eq!(bars.len(), 2);
        let (first, second) = (bars[0], bars[1]);
        assert!((first.0 - second.0).abs() < 0.01 && (first.2 - second.2).abs() < 0.01, "{bars:?}");
        assert!(((second.1 + second.3) - first.1).abs() < 0.6, "{bars:?}");
        assert!((second.3 / first.3 - 2.0).abs() < 0.05, "{bars:?}");
        let scene = plot.scene(640.0, 400.0);
        assert_eq!(scene.legend.len(), 2);
        let hidden = plot.scene_hiding(640.0, 400.0, &[1]);
        assert_eq!(rects(&hidden).into_iter().filter(|r| r.2 > 20.0).count(), 1, "a hidden series draws nothing");
    }

    #[test]
    fn horizontal_bars_grow_along_x() {
        let mut plot = bars(&["a", "b"], &[1.0, 2.0]);
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
        let mut plot = bars(&["a", "b", "c"], &[1.0, 1.0, 2.0]);
        plot.layers[0].mark = Mark::Pie;
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
    fn area_polygons_close_on_the_baseline_and_steps_stair() {
        let mut plot = Plot::new();
        let mut layer = Layer::new(Mark::Area);
        layer.series = vec![Series::new(None, vec![1.0, 2.0, 3.0], vec![1.0, 4.0, 2.0])];
        plot.layers.push(layer);
        let scene = plot.scene(640.0, 400.0);
        let polygon = scene.shapes.iter().find_map(|s| match s {
            Shape::Polygon { points, .. } => Some(points.clone()),
            _ => None,
        });
        let points = polygon.expect("an area polygon");
        assert_eq!(points.len(), 6);
        assert!((points[3].1 - points[5].1).abs() < 0.01, "{points:?}");
        plot.layers[0].mark = Mark::Step;
        let steps = plot.scene(640.0, 400.0).shapes.iter().find_map(|s| match s {
            Shape::Polyline { points, .. } => Some(points.clone()),
            _ => None,
        });
        assert_eq!(steps.expect("a step line").len(), 5);
        assert_eq!(plot.scene(640.0, 400.0).samples.len(), 3);
    }

    #[test]
    fn layers_share_scales_and_a_right_axis() {
        let mut plot = bars(&["a", "b"], &[10.0, 20.0]);
        plot.layers[0].series[0].name = Some("units".into());
        let mut line = Layer::new(Mark::Line);
        line.series = vec![Series::new(Some("share".into()), vec![0.0, 1.0], vec![0.25, 0.75])];
        line.side = Side::Right;
        line.first_color = 1;
        plot.layers.push(line);
        plot.y2.format = Some("{:.0%}".into());
        let svg = plot.svg();
        assert!(svg.contains("%<") && svg.contains(">20<"), "{svg}");
        assert_eq!(svg.matches("<polyline").count(), 1);
        assert_eq!(plot.describe(), "bar chart with 2 bars and 1 more layer");
        let scene = plot.scene(640.0, 400.0);
        let mut listed: Vec<usize> = scene.legend.iter().map(|e| e.series).collect();
        listed.sort_unstable();
        assert_eq!(listed, [0, 1]);
        assert!(scene.samples.iter().all(|s| s.series == 1));
    }

    #[test]
    fn density_and_fits_are_smooth_and_linear() {
        let series = density(&[1.0, 2.0, 3.0], None, 50);
        assert_eq!(series.xs.len(), 50);
        let area: f64 = series.ys.iter().sum::<f64>() * (series.xs[1] - series.xs[0]);
        assert!((area - 1.0).abs() < 0.05, "{area}");
        assert_eq!(linear_fit(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0]), Some((2.0, 0.0)));
        assert_eq!(linear_fit(&[1.0], &[2.0]), None);
        assert_eq!(rolling_mean(&[1.0, 3.0, 5.0, 7.0], 2), [1.0, 2.0, 4.0, 6.0]);
    }

    #[test]
    fn colors_parse_and_palettes_paint_series() {
        assert_eq!(Rgb::parse("#0f7b86"), Some(Rgb(15, 123, 134)));
        assert_eq!(Rgb::parse("#fff"), Some(Rgb(255, 255, 255)));
        assert_eq!(Rgb::parse("teal"), None);
        assert_eq!(Rgb(0, 0, 0).mix(Rgb(255, 255, 255), 0.5).hex(), "#808080");
        assert_eq!(named_palette("warm").map(|p| p.len()), Some(6));
        let mut plot = bars(&["a", "b"], &[1.0, 2.0]);
        plot.palette = vec![Rgb(1, 2, 3)];
        assert!(plot.svg().contains("fill=\"#010203\""));
        plot.layers[0].color = Some(Rgb(9, 9, 9));
        assert!(plot.svg().contains("fill=\"#090909\""));
    }

    #[test]
    fn long_category_labels_turn_and_heatmaps_tile() {
        let names: Vec<String> = (0..12).map(|i| format!("category number {i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let plot = bars(&refs, &[1.0; 12]);
        let angled = plot.scene(640.0, 400.0).shapes.iter().filter(|s| matches!(s, Shape::Text { angle, .. } if *angle != 0.0)).count();
        assert_eq!(angled, 12);
        assert!(plot.svg().contains("rotate(-40"));
        let mut heat = Plot::new();
        heat.categories = vec!["x".into(), "y".into()];
        let mut layer = Layer::new(Mark::Heat);
        layer.bands = vec!["p".into(), "q".into()];
        let mut series = Series::new(None, vec![0.0, 1.0, 0.0, 1.0], vec![0.0, 0.0, 1.0, 1.0]);
        series.sizes = vec![1.0, 2.0, 3.0, 4.0];
        layer.series = vec![series];
        heat.layers.push(layer);
        let scene = heat.scene(640.0, 400.0);
        assert_eq!(rects(&scene).len(), 4);
        assert!(scene.shapes.iter().any(|s| matches!(s, Shape::Rect { paint: Paint::Heat(t), .. } if (*t - 1.0).abs() < 0.001)));
        assert!(heat.text().contains("q") && heat.text().contains("4"));
        assert_eq!(heat.describe(), "heatmap with 4 cells");
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
        assert_eq!(format_tick(2500.0, Some("compact")), Some("2.5K".to_string()));
    }

    #[test]
    fn terminal_text_uses_eighth_blocks_braille_and_colors() {
        let plot = bars(&["a", "b"], &[1.0, 3.0]);
        let plain = plot.text();
        assert!(plain.contains("█ 1") && !plain.contains('▍') && !plain.contains("\x1b["), "{plain}");
        let fine = plot.terminal_text(false);
        assert!(fine.contains("▍ 1") && !fine.contains("\x1b["), "{fine}");
        let colored = plot.terminal_text(true);
        assert!(colored.contains("\x1b[38;2;") && colored.contains("▍\x1b[0m 1"), "{colored}");
        let mut lines = Plot::new();
        let mut layer = Layer::new(Mark::Line);
        layer.series.push(Series::new(Some("a".into()), vec![1.0, 2.0, 3.0], vec![1.0, 2.0, 3.0]));
        layer.series.push(Series::new(Some("b".into()), vec![1.0, 2.0, 3.0], vec![3.0, 2.0, 1.0]));
        lines.layers.push(layer);
        let braille = |text: &str| text.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c));
        let grid = lines.terminal_text(true);
        assert!(braille(&grid) && grid.contains("━━\x1b[0m a") && grid.contains("━━\x1b[0m b"), "{grid}");
        assert!(!braille(&lines.text()) && lines.text().contains("• a  ○ b"));
        assert!(!braille(&lines.terminal_text(false)));
        lines.layers[0].series.pop();
        assert!(braille(&lines.terminal_text(false)));
    }

    #[test]
    fn segments_connect_grid_cells() {
        assert_eq!(segment((0, 0), (3, 0)), [(1, 0), (2, 0)]);
        assert_eq!(segment((0, 3), (3, 0)), [(1, 2), (2, 1)]);
        assert!(segment((5, 5), (5, 5)).is_empty());
    }
}
