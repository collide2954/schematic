use std::cell::RefCell;
use std::rc::Rc;

use super::collections::{expects, items};
use super::native;
use super::strings::as_str;
use crate::error::Error;
use crate::plot::{self, AxisSpec, BoxStats, Layer, LegendAt, Mark, Marker, Note, Plot, Rgb, Rule, Series, Side};
use crate::symbol::Symbol;
use crate::table::Table;
use crate::value::{Shown, Value};
use crate::vm::{Arity, Globals};

macro_rules! layer_natives {
    ($globals:expr, $(($name:literal, $mark:expr)),* $(,)?) => {
        $(native($globals, $name, Arity::Between(2, 5), |args| layer_call($name, $mark, args));)*
    };
}

pub fn install(globals: &mut Globals) {
    layer_natives!(
        globals,
        ("bars", Mark::Bar),
        ("bar-chart", Mark::Bar),
        ("lines", Mark::Line),
        ("line-chart", Mark::Line),
        ("steps", Mark::Step),
        ("points", Mark::Point),
        ("scatter", Mark::Point),
        ("areas", Mark::Area),
        ("area-chart", Mark::Area),
        ("lollipops", Mark::Lollipop),
        ("pie", Mark::Pie),
        ("pie-chart", Mark::Pie),
    );
    native(globals, "histogram", Arity::Between(1, 4), |args| distribution("histogram", Mark::Histogram, args));
    native(globals, "density", Arity::Between(1, 4), |args| distribution("density", Mark::Density, args));
    native(globals, "box-plot", Arity::Between(1, 5), |args| box_plot("box-plot", args));
    native(globals, "boxes", Arity::Between(1, 5), |args| box_plot("boxes", args));
    native(globals, "heatmap", Arity::Between(4, 6), heatmap);
    native(globals, "plot", Arity::Between(0, 1), |args| {
        let mut plot = fresh_plot();
        if let Some(value) = args.first() {
            apply_plot_options("plot", &mut plot, &options("plot", Some(value))?);
        }
        Ok(Value::Plot(Rc::new(plot)))
    });
    native(globals, "title", Arity::Between(2, 3), |args| {
        with_plot("title", args, |plot| {
            plot.title = Some(text_of(&args[1]));
            plot.subtitle = args.get(2).map(text_of);
            Ok(())
        })
    });
    native(globals, "subtitle", Arity::Exact(2), |args| {
        with_plot("subtitle", args, |plot| {
            plot.subtitle = Some(text_of(&args[1]));
            Ok(())
        })
    });
    native(globals, "caption", Arity::Exact(2), |args| {
        with_plot("caption", args, |plot| {
            plot.caption = Some(text_of(&args[1]));
            Ok(())
        })
    });
    native(globals, "hline", Arity::Between(2, 3), |args| rule("hline", args, false));
    native(globals, "vline", Arity::Between(2, 3), |args| rule("vline", args, true));
    native(globals, "annotate", Arity::Exact(4), |args| {
        with_plot("annotate", args, |plot| {
            let x = match &args[1] {
                Value::Str(_) | Value::Keyword(_) | Value::Symbol(_) | Value::Bool(_) => {
                    let label = Shown(&args[1]).to_string();
                    plot.categories.iter().position(|c| *c == label).ok_or_else(|| Error::new("value-error", format!("annotate: no category {label}")))? as f64
                }
                other => axis_number("annotate", "x", other)?,
            };
            let y = axis_number("annotate", "y", &args[2])?;
            plot.notes.push(Note { x, y, text: text_of(&args[3]) });
            Ok(())
        })
    });
    native(globals, "x-axis", Arity::Exact(2), |args| axis_call("x-axis", args, |plot| &mut plot.x));
    native(globals, "y-axis", Arity::Exact(2), |args| axis_call("y-axis", args, |plot| &mut plot.y));
    native(globals, "right-axis", Arity::Exact(2), |args| axis_call("right-axis", args, |plot| &mut plot.y2));
    native(globals, "legend", Arity::Exact(2), |args| {
        with_plot("legend", args, |plot| {
            plot.legend = legend_at("legend", &args[1])?;
            Ok(())
        })
    });
    native(globals, "palette", Arity::Exact(2), |args| {
        with_plot("palette", args, |plot| {
            plot.palette = palette_of("palette", &args[1])?;
            Ok(())
        })
    });
    native(globals, "trend", Arity::Between(1, 2), |args| with_plot("trend", args, |plot| add_trend("trend", plot, args.get(1))));
    native(globals, "smooth", Arity::Exact(2), |args| {
        with_plot("smooth", args, |plot| {
            let window = match &args[1] {
                Value::Int(n) if *n > 0 => *n as usize,
                other => return Err(expects("smooth", "a positive window", other)),
            };
            add_rolling(plot, window);
            Ok(())
        })
    });
    native(globals, "figure", Arity::Between(1, 2), figure);
    native(globals, "plot?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Plot(_)))));
    native(globals, "to-svg", Arity::Between(1, 2), |args| {
        let plot = as_plot("to-svg", &args[0])?;
        let (width, height, dark) = export_options("to-svg", args.get(1))?;
        Ok(Value::string(&plot.svg_sized(width, height, dark)))
    });
    native(globals, "save-plot", Arity::Between(2, 3), save_plot);
    native(globals, "plot-theme!", Arity::Exact(1), |args| {
        let options = options("plot-theme!", Some(&args[0]))?;
        THEME.with(|theme| {
            let mut theme = theme.borrow_mut();
            if let Some(palette) = options.palette.clone() {
                theme.palette = palette;
            }
            if let Some(grid) = options.grid {
                theme.grid = grid;
            }
            if let Some(legend) = options.legend {
                theme.legend = legend;
            }
        });
        Ok(Value::Nil)
    });
    native(globals, "plot-theme", Arity::Exact(0), |_| {
        Ok(THEME.with(|theme| {
            let theme = theme.borrow();
            Value::map([
                (Value::keyword("palette"), Value::vector(theme.palette.iter().map(|c| Value::string(&c.hex())))),
                (Value::keyword("grid"), Value::Bool(theme.grid)),
                (Value::keyword("legend"), Value::keyword(legend_name(theme.legend))),
            ])
        }))
    });
}

struct Theme {
    palette: Vec<Rgb>,
    grid: bool,
    legend: LegendAt,
}

thread_local! {
    static THEME: RefCell<Theme> = const { RefCell::new(Theme { palette: Vec::new(), grid: true, legend: LegendAt::Auto }) };
}

fn fresh_plot() -> Plot {
    let mut plot = Plot::new();
    THEME.with(|theme| {
        let theme = theme.borrow();
        plot.palette = theme.palette.clone();
        plot.grid = theme.grid;
        plot.legend = theme.legend;
    });
    plot
}

fn legend_name(at: LegendAt) -> &'static str {
    match at {
        LegendAt::Auto => "auto",
        LegendAt::Top => "top",
        LegendAt::Right => "right",
        LegendAt::Bottom => "bottom",
        LegendAt::Hidden => "none",
    }
}

fn as_plot<'a>(name: &str, value: &'a Value) -> Result<&'a Plot, Error> {
    match value {
        Value::Plot(plot) => Ok(plot),
        other => Err(expects(name, "a plot", other)),
    }
}

fn with_plot(name: &str, args: &[Value], change: impl FnOnce(&mut Plot) -> Result<(), Error>) -> Result<Value, Error> {
    let mut plot = as_plot(name, &args[0])?.clone();
    change(&mut plot)?;
    Ok(Value::Plot(Rc::new(plot)))
}

fn text_of(value: &Value) -> String {
    match value {
        Value::Str(s) => s.to_string(),
        other => Shown(other).to_string(),
    }
}

fn export_options(name: &str, value: Option<&Value>) -> Result<(f32, f32, bool), Error> {
    let Some(value) = value else { return Ok((640.0, 400.0, false)) };
    let Value::Map(map) = value else { return Err(expects(name, "an options map", value)) };
    let size = |key: &str, fallback: f32| -> Result<f32, Error> {
        match map.get(&Value::keyword(key)) {
            None | Some(Value::Nil) => Ok(fallback),
            Some(Value::Int(n)) if *n > 0 => Ok(*n as f32),
            Some(Value::Float(x)) if *x > 0.0 => Ok(*x as f32),
            Some(other) => Err(expects(name, &format!("a positive :{key}"), other)),
        }
    };
    let dark = match map.get(&Value::keyword("theme")) {
        Some(Value::Keyword(k)) => &*k.name() == "dark",
        _ => matches!(map.get(&Value::keyword("dark")), Some(Value::Bool(true))),
    };
    Ok((size("width", 640.0)?, size("height", 400.0)?, dark))
}

fn save_plot(args: &[Value]) -> Result<Value, Error> {
    let plot = as_plot("save-plot", &args[0])?;
    let path = as_str("save-plot", &args[1])?;
    let (width, height, dark) = export_options("save-plot", args.get(2))?;
    std::fs::write(path, plot.svg_sized(width, height, dark)).map_err(|e| Error::new("io-error", format!("cannot write {path}: {e}")))?;
    Ok(Value::Nil)
}

#[derive(Default)]
struct Options {
    title: Option<String>,
    subtitle: Option<String>,
    caption: Option<String>,
    x_label: Option<String>,
    y_label: Option<String>,
    by: Option<Symbol>,
    facet: Option<Symbol>,
    size: Option<Symbol>,
    low: Option<Symbol>,
    high: Option<Symbol>,
    text: Option<Symbol>,
    bins: Option<usize>,
    bandwidth: Option<f64>,
    stack: bool,
    normalize: bool,
    horizontal: bool,
    donut: bool,
    labels: bool,
    step: bool,
    lollipop: bool,
    right: bool,
    dash: bool,
    shapes: bool,
    dots: Option<bool>,
    trend: bool,
    rolling: Option<usize>,
    width: Option<f32>,
    opacity: Option<f32>,
    color: Option<Rgb>,
    marker: Option<Marker>,
    sort: Option<bool>,
    top: Option<usize>,
    x_range: Option<(f64, f64)>,
    y_range: Option<(f64, f64)>,
    x_log: bool,
    y_log: bool,
    x_format: Option<String>,
    y_format: Option<String>,
    x_ticks: Option<usize>,
    y_ticks: Option<usize>,
    hlines: Vec<f64>,
    vlines: Vec<f64>,
    legend: Option<LegendAt>,
    palette: Option<Vec<Rgb>>,
    grid: Option<bool>,
}

fn axis_number(name: &str, key: &str, value: &Value) -> Result<f64, Error> {
    match value {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(x) => Ok(*x),
        Value::Date(d) => Ok(*d as f64),
        other => Err(expects(name, &format!("numbers or dates for :{key}"), other)),
    }
}

fn legend_at(name: &str, value: &Value) -> Result<LegendAt, Error> {
    match value {
        Value::Keyword(k) => Ok(match &*k.name() {
            "top" => LegendAt::Top,
            "right" => LegendAt::Right,
            "bottom" => LegendAt::Bottom,
            "none" | "hidden" => LegendAt::Hidden,
            "auto" => LegendAt::Auto,
            _ => return Err(expects(name, ":top, :right, :bottom, :none, or :auto for the legend", value)),
        }),
        Value::Bool(false) | Value::Nil => Ok(LegendAt::Hidden),
        other => Err(expects(name, ":top, :right, :bottom, :none, or :auto for the legend", other)),
    }
}

fn palette_of(name: &str, value: &Value) -> Result<Vec<Rgb>, Error> {
    match value {
        Value::Keyword(k) => plot::named_palette(&k.name()).ok_or_else(|| expects(name, "a palette name such as :warm, :cool, :earth, :gray, or :bright", value)),
        Value::Nil => Ok(Vec::new()),
        other => items(name, other)?
            .map(|color| match &color {
                Value::Str(s) => Rgb::parse(s).ok_or_else(|| expects(name, "colors written as #rrggbb", &color)),
                _ => Err(expects(name, "colors written as #rrggbb", &color)),
            })
            .collect(),
    }
}

fn options(name: &str, value: Option<&Value>) -> Result<Options, Error> {
    let mut options = Options::default();
    let Some(value) = value else { return Ok(options) };
    let Value::Map(map) = value else { return Err(expects(name, "an options map", value)) };
    let get = |key: &str| map.get(&Value::keyword(key)).filter(|v| !matches!(v, Value::Nil));
    let text = |key: &str| -> Option<String> {
        get(key).map(|v| match v {
            Value::Str(s) => s.to_string(),
            Value::Keyword(k) if key.ends_with("format") => k.name().to_string(),
            other => Shown(other).to_string(),
        })
    };
    let flag = |key: &str| -> Result<bool, Error> {
        match get(key) {
            None | Some(Value::Bool(false)) => Ok(false),
            Some(Value::Bool(true)) => Ok(true),
            Some(other) => Err(expects(name, &format!("true or false for :{key}"), other)),
        }
    };
    let column = |key: &str| -> Result<Option<Symbol>, Error> {
        match get(key) {
            None => Ok(None),
            Some(Value::Keyword(column)) => Ok(Some(*column)),
            Some(other) => Err(expects(name, &format!("a column keyword for :{key}"), other)),
        }
    };
    let range = |key: &str| -> Result<Option<(f64, f64)>, Error> {
        let Some(value) = get(key) else { return Ok(None) };
        let bounds: Vec<Value> = items(name, value).ok().map(Iterator::collect).unwrap_or_default();
        let [low, high] = bounds.as_slice() else {
            return Err(Error::new("value-error", format!("{name} expects :{key} to be [low high]")));
        };
        Ok(Some((axis_number(name, key, low)?, axis_number(name, key, high)?)))
    };
    let scale = |key: &str| -> Result<bool, Error> {
        match get(key) {
            None => Ok(false),
            Some(Value::Keyword(k)) if &*k.name() == "linear" => Ok(false),
            Some(Value::Keyword(k)) if &*k.name() == "log" => Ok(true),
            Some(other) => Err(expects(name, &format!(":linear or :log for :{key}"), other)),
        }
    };
    let lines = |key: &str| -> Result<Vec<f64>, Error> {
        match get(key) {
            None => Ok(Vec::new()),
            Some(single @ (Value::Int(_) | Value::Float(_) | Value::Date(_))) => Ok(vec![axis_number(name, key, single)?]),
            Some(many) => items(name, many)?.map(|v| axis_number(name, key, &v)).collect(),
        }
    };
    let count = |key: &str, what: &str| -> Result<Option<usize>, Error> {
        match get(key) {
            None => Ok(None),
            Some(Value::Int(n)) if *n > 0 => Ok(Some(*n as usize)),
            Some(_) => Err(Error::new("value-error", format!("{name} expects a positive number of {what}"))),
        }
    };
    let float = |key: &str| -> Result<Option<f32>, Error> {
        match get(key) {
            None => Ok(None),
            Some(Value::Int(n)) => Ok(Some(*n as f32)),
            Some(Value::Float(x)) => Ok(Some(*x as f32)),
            Some(other) => Err(expects(name, &format!("a number for :{key}"), other)),
        }
    };
    options.title = text("title");
    options.subtitle = text("subtitle");
    options.caption = text("caption");
    options.x_label = text("x-label");
    options.y_label = text("y-label");
    options.by = column("by")?;
    options.facet = column("facet")?;
    options.size = column("size")?;
    options.low = column("low")?;
    options.high = column("high")?;
    options.text = column("text")?;
    options.stack = flag("stack")?;
    options.normalize = flag("normalize")?;
    options.horizontal = flag("horizontal")?;
    options.donut = flag("donut")?;
    options.labels = flag("labels")?;
    options.step = flag("step")?;
    options.lollipop = flag("lollipop")?;
    options.dash = flag("dash")?;
    options.shapes = flag("shapes")?;
    options.dots = match get("dots") {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(other) => return Err(expects(name, "true or false for :dots", other)),
    };
    options.trend = flag("trend")?;
    options.right = match get("axis") {
        None => false,
        Some(Value::Keyword(k)) if &*k.name() == "right" => true,
        Some(Value::Keyword(k)) if &*k.name() == "left" => false,
        Some(other) => return Err(expects(name, ":left or :right for :axis", other)),
    };
    options.width = float("width")?;
    options.opacity = float("opacity")?;
    options.color = match get("color") {
        None => None,
        Some(Value::Str(s)) => Some(Rgb::parse(s).ok_or_else(|| expects(name, "a color written as #rrggbb", &Value::Str(s.clone())))?),
        Some(other) => return Err(expects(name, "a color written as #rrggbb", other)),
    };
    options.marker = match get("shape") {
        None => None,
        Some(shape @ Value::Keyword(k)) => Some(match &*k.name() {
            "circle" => Marker::Circle,
            "square" => Marker::Square,
            "diamond" => Marker::Diamond,
            "triangle" => Marker::Triangle,
            _ => return Err(expects(name, ":circle, :square, :diamond, or :triangle for :shape", shape)),
        }),
        Some(other) => return Err(expects(name, ":circle, :square, :diamond, or :triangle for :shape", other)),
    };
    options.sort = match get("sort") {
        None => None,
        Some(Value::Keyword(k)) if &*k.name() == "asc" => Some(false),
        Some(Value::Keyword(k)) if &*k.name() == "desc" => Some(true),
        Some(other) => return Err(expects(name, ":asc or :desc for :sort", other)),
    };
    options.top = count("top", "categories")?;
    options.rolling = count("rolling", "points")?;
    options.x_range = range("x-range")?;
    options.y_range = range("y-range")?;
    options.x_log = scale("x-scale")?;
    options.y_log = scale("y-scale")?;
    options.x_format = text("x-format");
    options.y_format = text("y-format");
    options.x_ticks = count("x-ticks", "ticks")?;
    options.y_ticks = count("y-ticks", "ticks")?;
    options.hlines = lines("hline")?;
    options.vlines = lines("vline")?;
    options.legend = get("legend").map(|v| legend_at(name, v)).transpose()?;
    options.palette = get("palette").map(|v| palette_of(name, v)).transpose()?;
    options.grid = match get("grid") {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(other) => return Err(expects(name, "true or false for :grid", other)),
    };
    options.bins = count("bins", "bins")?;
    options.bandwidth = match get("bandwidth") {
        None => None,
        Some(Value::Int(n)) if *n > 0 => Some(*n as f64),
        Some(Value::Float(x)) if *x > 0.0 => Some(*x),
        Some(other) => return Err(expects(name, "a positive :bandwidth", other)),
    };
    Ok(options)
}

fn apply_plot_options(_name: &str, plot: &mut Plot, options: &Options) {
    if options.title.is_some() {
        plot.title = options.title.clone();
    }
    if options.subtitle.is_some() {
        plot.subtitle = options.subtitle.clone();
    }
    if options.caption.is_some() {
        plot.caption = options.caption.clone();
    }
    if options.x_label.is_some() {
        plot.x.label = options.x_label.clone();
    }
    if options.y_label.is_some() {
        plot.y.label = options.y_label.clone();
    }
    if options.x_range.is_some() {
        plot.x.range = options.x_range;
    }
    if options.y_range.is_some() {
        plot.y.range = options.y_range;
    }
    if options.x_log {
        plot.x.log = true;
    }
    if options.y_log {
        plot.y.log = true;
    }
    if options.x_format.is_some() {
        plot.x.format = options.x_format.clone();
    }
    if options.y_format.is_some() {
        plot.y.format = options.y_format.clone();
    }
    if options.x_ticks.is_some() {
        plot.x.ticks = options.x_ticks;
    }
    if options.y_ticks.is_some() {
        plot.y.ticks = options.y_ticks;
    }
    if options.horizontal {
        plot.horizontal = true;
    }
    plot.rules.extend(options.hlines.iter().map(|v| Rule { vertical: false, value: *v, label: None }));
    plot.rules.extend(options.vlines.iter().map(|v| Rule { vertical: true, value: *v, label: None }));
    if let Some(legend) = options.legend {
        plot.legend = legend;
    }
    if let Some(palette) = &options.palette {
        plot.palette = palette.clone();
    }
    if let Some(grid) = options.grid {
        plot.grid = grid;
    }
}

fn apply_layer_options(layer: &mut Layer, options: &Options) {
    layer.stacked = options.stack || options.normalize;
    layer.normalized = options.normalize;
    layer.labels = options.labels;
    layer.donut = options.donut;
    layer.dashed = options.dash;
    layer.markers = options.shapes;
    layer.dots = options.dots.unwrap_or(!options.dash);
    layer.side = if options.right { Side::Right } else { Side::Left };
    if let Some(width) = options.width {
        layer.width = width.max(0.5);
    }
    if let Some(opacity) = options.opacity {
        layer.opacity = opacity.clamp(0.05, 1.0);
    }
    layer.color = options.color;
    if let Some(marker) = options.marker {
        layer.marker = marker;
    }
    if options.step && layer.mark == Mark::Line {
        layer.mark = Mark::Step;
    }
    if options.lollipop && layer.mark == Mark::Bar {
        layer.mark = Mark::Lollipop;
    }
}

fn axis_call(name: &str, args: &[Value], pick: impl Fn(&mut Plot) -> &mut AxisSpec) -> Result<Value, Error> {
    with_plot(name, args, |plot| {
        let Value::Map(map) = &args[1] else { return Err(expects(name, "an options map", &args[1])) };
        let get = |key: &str| map.get(&Value::keyword(key)).filter(|v| !matches!(v, Value::Nil));
        let axis = pick(plot);
        if let Some(label) = get("label") {
            axis.label = Some(text_of(label));
        }
        if let Some(value) = get("range") {
            let bounds: Vec<Value> = items(name, value).ok().map(Iterator::collect).unwrap_or_default();
            let [low, high] = bounds.as_slice() else {
                return Err(Error::new("value-error", format!("{name} expects :range to be [low high]")));
            };
            axis.range = Some((axis_number(name, "range", low)?, axis_number(name, "range", high)?));
        }
        match get("scale") {
            None => {}
            Some(Value::Keyword(k)) if &*k.name() == "log" => axis.log = true,
            Some(Value::Keyword(k)) if &*k.name() == "linear" => axis.log = false,
            Some(other) => return Err(expects(name, ":linear or :log for :scale", other)),
        }
        if let Some(format) = get("format") {
            axis.format = Some(match format {
                Value::Keyword(k) => k.name().to_string(),
                other => text_of(other),
            });
        }
        match get("ticks") {
            None => {}
            Some(Value::Int(n)) if *n > 0 => axis.ticks = Some(*n as usize),
            Some(_) => return Err(Error::new("value-error", format!("{name} expects a positive number of ticks"))),
        }
        Ok(())
    })
}

fn rule(name: &str, args: &[Value], vertical: bool) -> Result<Value, Error> {
    with_plot(name, args, |plot| {
        let values: Vec<f64> = match &args[1] {
            single @ (Value::Int(_) | Value::Float(_) | Value::Date(_)) => vec![axis_number(name, "value", single)?],
            many => items(name, many)?.map(|v| axis_number(name, "value", &v)).collect::<Result<Vec<_>, _>>()?,
        };
        let label = args.get(2).map(text_of);
        for value in values {
            plot.rules.push(Rule { vertical, value, label: label.clone() });
        }
        Ok(())
    })
}

fn column_values(name: &str, table: &Table, key: &Value) -> Result<(Symbol, Vec<Value>), Error> {
    let Value::Keyword(column) = key else { return Err(expects(name, "a column keyword", key)) };
    let array = table.column(*column).ok_or_else(|| Table::missing(*column))?;
    Ok((*column, array.iter().collect()))
}

fn number(name: &str, what: &str, value: &Value) -> Result<f64, Error> {
    match value {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(x) => Ok(*x),
        other => Err(expects(name, &format!("numbers for {what}"), other)),
    }
}

fn numbers(name: &str, values: &[Value]) -> Result<Vec<f64>, Error> {
    let mut out = Vec::new();
    for value in values {
        match value {
            Value::Nil => {}
            Value::Int(i) => out.push(*i as f64),
            Value::Float(x) => out.push(*x),
            other => return Err(expects(name, "numbers", other)),
        }
    }
    Ok(out)
}

fn groups_of(keys: &[Value]) -> Vec<(String, Vec<usize>)> {
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, key) in keys.iter().enumerate() {
        let label = Shown(key).to_string();
        match groups.iter_mut().find(|(l, _)| *l == label) {
            Some((_, rows)) => rows.push(i),
            None => groups.push((label, vec![i])),
        }
    }
    groups
}

fn pick(values: &[Value], rows: &[usize]) -> Vec<Value> {
    rows.iter().map(|i| values[*i].clone()).collect()
}

fn share_axes(parent: &mut Plot) {
    let (value, x) = plot::shared_ranges(&parent.panels);
    for panel in &mut parent.panels {
        panel.y.range.get_or_insert(value);
        if panel.x.range.is_none() {
            panel.x.range = x;
        }
    }
}

struct Columns {
    xs: Vec<Value>,
    ys: Vec<Value>,
    by: Option<Vec<Value>>,
    facet: Option<Vec<Value>>,
    sizes: Option<Vec<Value>>,
    lows: Option<Vec<Value>>,
    highs: Option<Vec<Value>>,
    texts: Option<Vec<Value>>,
    x_name: Option<String>,
    y_name: Option<String>,
    options: Options,
}

fn columns(name: &str, args: &[Value]) -> Result<Columns, Error> {
    match args.first() {
        Some(Value::Table(table)) => {
            let (x_name, xs) = column_values(name, table, args.get(1).unwrap_or(&Value::Nil))?;
            let y_key = args.get(2).ok_or_else(|| Error::new("value-error", format!("{name} expects an x and a y column")))?;
            let (y_name, ys) = column_values(name, table, y_key)?;
            let options = options(name, args.get(3))?;
            let keys = |column: Option<Symbol>| column.map(|c| column_values(name, table, &Value::Keyword(c)).map(|(_, v)| v)).transpose();
            Ok(Columns {
                xs,
                ys,
                by: keys(options.by)?,
                facet: keys(options.facet)?,
                sizes: keys(options.size)?,
                lows: keys(options.low)?,
                highs: keys(options.high)?,
                texts: keys(options.text)?,
                x_name: Some(x_name.name().to_string()),
                y_name: Some(y_name.name().to_string()),
                options,
            })
        }
        Some(Value::Vector(_) | Value::List(_) | Value::Array(_)) => {
            let options = options(name, args.get(2))?;
            if options.facet.is_some() {
                return Err(Error::new("value-error", format!("{name} expects a table for :facet")));
            }
            if options.by.is_some() || options.size.is_some() || options.low.is_some() || options.high.is_some() || options.text.is_some() {
                return Err(Error::new("value-error", format!("{name} expects a table for column options")));
            }
            let xs: Vec<Value> = items(name, &args[0])?.collect();
            let ys: Vec<Value> = args.get(1).map(|v| items(name, v)).transpose()?.map(Iterator::collect).unwrap_or_default();
            Ok(Columns { xs, ys, by: None, facet: None, sizes: None, lows: None, highs: None, texts: None, x_name: None, y_name: None, options })
        }
        Some(other) => Err(expects(name, "a table or a collection", other)),
        None => Err(Error::new("value-error", format!("{name} expects data"))),
    }
}

fn category_index(plot: &mut Plot, label: String) -> f64 {
    plot.categories.iter().position(|c| *c == label).unwrap_or_else(|| {
        plot.categories.push(label);
        plot.categories.len() - 1
    }) as f64
}

fn build_layer(name: &str, mark: Mark, plot: &mut Plot, columns: &Columns, seed: Option<&[String]>) -> Result<Layer, Error> {
    if let Some(seed) = seed {
        plot.categories = seed.to_vec();
    }
    let (xs, ys) = (&columns.xs, &columns.ys);
    if xs.len() != ys.len() {
        return Err(Error::new("value-error", format!("{name} expects x and y of the same length, got {} and {}", xs.len(), ys.len())));
    }
    let mut groups: Vec<(Option<String>, Vec<usize>)> = Vec::new();
    for i in 0..xs.len() {
        let key = columns.by.as_ref().map(|keys| Shown(&keys[i]).to_string());
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, rows)) => rows.push(i),
            None => groups.push((key, vec![i])),
        }
    }
    let categorical = mark.categorical() || !plot.categories.is_empty() || xs.iter().any(|x| matches!(x, Value::Str(_) | Value::Keyword(_) | Value::Symbol(_) | Value::Bool(_)));
    let mut layer = Layer::new(mark);
    let mut dates = plot.dates.then_some(true);
    for (key, rows) in groups {
        let mut series = Series::new(key, Vec::new(), Vec::new());
        for i in rows {
            if matches!(xs[i], Value::Nil) || matches!(ys[i], Value::Nil) {
                continue;
            }
            let y = number(name, "y", &ys[i])?;
            let x = if categorical {
                category_index(plot, Shown(&xs[i]).to_string())
            } else {
                match &xs[i] {
                    Value::Date(d) if dates != Some(false) => {
                        dates = Some(true);
                        *d as f64
                    }
                    Value::Int(_) | Value::Float(_) if dates != Some(true) => {
                        dates = Some(false);
                        number(name, "x", &xs[i])?
                    }
                    other => return Err(expects(name, "numbers or dates for x", other)),
                }
            };
            series.xs.push(x);
            series.ys.push(y);
            if let Some(sizes) = &columns.sizes {
                series.sizes.push(number(name, "size", &sizes[i]).unwrap_or(0.0));
            }
            if let (Some(lows), Some(highs)) = (&columns.lows, &columns.highs) {
                series.lows.push(number(name, "low", &lows[i])?);
                series.highs.push(number(name, "high", &highs[i])?);
            }
            if let Some(texts) = &columns.texts {
                series.texts.push(Shown(&texts[i]).to_string());
            }
        }
        if matches!(mark, Mark::Line | Mark::Step | Mark::Area) && !categorical {
            let mut order: Vec<usize> = (0..series.xs.len()).collect();
            order.sort_by(|a, b| series.xs[*a].total_cmp(&series.xs[*b]));
            let reorder = |values: &Vec<f64>| -> Vec<f64> { order.iter().map(|i| values[*i]).collect() };
            series.xs = reorder(&series.xs);
            series.ys = reorder(&series.ys);
            if !series.sizes.is_empty() {
                series.sizes = reorder(&series.sizes);
            }
            if !series.lows.is_empty() {
                series.lows = reorder(&series.lows);
                series.highs = reorder(&series.highs);
            }
            if !series.texts.is_empty() {
                series.texts = order.iter().map(|i| series.texts[*i].clone()).collect();
            }
        }
        layer.series.push(series);
    }
    if !categorical {
        plot.dates = dates == Some(true);
    }
    let log_check = |values: &[f64], log: bool| log.then(|| values.iter().copied().find(|v| *v <= 0.0)).flatten();
    for series in &layer.series {
        if let Some(v) = log_check(&series.ys, plot.y.log || columns.options.y_log).or_else(|| log_check(&series.xs, (plot.x.log || columns.options.x_log) && !categorical)) {
            return Err(Error::new("value-error", format!("{name} expects positive values on a log scale, got {}", plot::number(v))));
        }
    }
    if mark == Mark::Pie {
        let mut totals = vec![0.0; plot.categories.len()];
        for series in &layer.series {
            for (x, y) in series.xs.iter().zip(&series.ys) {
                if *y < 0.0 {
                    return Err(Error::new("value-error", format!("{name} expects positive values, got {}", plot::number(*y))));
                }
                totals[*x as usize] += y;
            }
        }
        layer.series = vec![Series::new(None, (0..totals.len()).map(|i| i as f64).collect(), totals)];
    }
    apply_layer_options(&mut layer, &columns.options);
    if layer.normalized {
        normalize(&mut layer);
    }
    Ok(layer)
}

fn normalize(layer: &mut Layer) {
    let mut totals: std::collections::BTreeMap<u64, f64> = std::collections::BTreeMap::new();
    for series in &layer.series {
        for (x, y) in series.xs.iter().zip(&series.ys) {
            *totals.entry(x.to_bits()).or_default() += y.abs();
        }
    }
    for series in &mut layer.series {
        for (x, y) in series.xs.iter().zip(series.ys.iter_mut()) {
            let total = totals.get(&x.to_bits()).copied().unwrap_or(0.0);
            *y = if total > 0.0 { *y / total * 100.0 } else { 0.0 };
        }
    }
}

fn reorder_categories(plot: &mut Plot, layer: &mut Layer, descending: Option<bool>, top: Option<usize>) {
    if plot.categories.is_empty() || (descending.is_none() && top.is_none()) {
        return;
    }
    let mut totals = vec![0.0; plot.categories.len()];
    for series in &layer.series {
        for (x, y) in series.xs.iter().zip(&series.ys) {
            totals[*x as usize] += y;
        }
    }
    let mut order: Vec<usize> = (0..plot.categories.len()).collect();
    if let Some(descending) = descending {
        order.sort_by(|a, b| {
            let cmp = totals[*a].total_cmp(&totals[*b]);
            if descending { cmp.reverse() } else { cmp }
        });
    } else if top.is_some() {
        order.sort_by(|a, b| totals[*b].total_cmp(&totals[*a]));
    }
    let kept = top.map_or(order.len(), |n| n.min(order.len()));
    let mut mapping = vec![None; plot.categories.len()];
    for (new, old) in order.iter().take(kept).enumerate() {
        mapping[*old] = Some(new);
    }
    let other = (kept < order.len()).then_some(kept);
    let mut categories: Vec<String> = order.iter().take(kept).map(|i| plot.categories[*i].clone()).collect();
    if other.is_some() {
        categories.push("Other".to_string());
    }
    for series in &mut layer.series {
        let mut merged: Vec<(f64, f64)> = Vec::new();
        let mut extra = (0.0, false);
        for (x, y) in series.xs.iter().zip(&series.ys) {
            match mapping[*x as usize] {
                Some(new) => merged.push((new as f64, *y)),
                None => extra = (extra.0 + y, true),
            }
        }
        if let (Some(slot), true) = (other, extra.1) {
            merged.push((slot as f64, extra.0));
        }
        merged.sort_by(|a, b| a.0.total_cmp(&b.0));
        series.xs = merged.iter().map(|(x, _)| *x).collect();
        series.ys = merged.iter().map(|(_, y)| *y).collect();
        series.sizes.clear();
        series.lows.clear();
        series.highs.clear();
        series.texts.clear();
    }
    plot.categories = categories;
}

fn series_count(plot: &Plot) -> usize {
    plot.layers.iter().map(|l| l.series.len()).sum()
}

fn add_trend(name: &str, plot: &mut Plot, options: Option<&Value>) -> Result<(), Error> {
    let options = self::options(name, options)?;
    let Some(source) = plot.layers.iter().rev().find(|l| !matches!(l.mark, Mark::Pie | Mark::Heat | Mark::Box | Mark::Histogram | Mark::Density)) else {
        return Err(Error::new("value-error", format!("{name} needs a layer of points, lines, or bars")));
    };
    let mut layer = Layer::new(Mark::Line);
    layer.first_color = source.first_color;
    layer.side = source.side;
    layer.dashed = true;
    layer.dots = false;
    layer.width = 1.5;
    layer.color = options.color;
    for series in &source.series {
        let Some((slope, intercept)) = plot::linear_fit(&series.xs, &series.ys) else { continue };
        let (lo, hi) = series.xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
        layer.series.push(Series::new(None, vec![lo, hi], vec![slope * lo + intercept, slope * hi + intercept]));
    }
    plot.layers.push(layer);
    Ok(())
}

fn add_rolling(plot: &mut Plot, window: usize) {
    let Some(source) = plot.layers.iter().rev().find(|l| matches!(l.mark, Mark::Line | Mark::Step | Mark::Point | Mark::Area | Mark::Bar)) else { return };
    let mut layer = Layer::new(Mark::Line);
    layer.first_color = source.first_color;
    layer.side = source.side;
    layer.dots = false;
    layer.width = 2.5;
    for series in &source.series {
        let name = Some(match &series.name {
            Some(name) => format!("{name} ({window}-point mean)"),
            None => format!("{window}-point mean"),
        });
        layer.series.push(Series::new(name, series.xs.clone(), plot::rolling_mean(&series.ys, window)));
    }
    plot.layers.push(layer);
}

fn split_target(name: &str, args: &[Value]) -> Result<(Plot, Vec<Value>), Error> {
    match args.first() {
        Some(Value::Plot(plot)) => {
            if !plot.panels.is_empty() {
                return Err(Error::new("value-error", format!("{name}: add layers before :facet, not after")));
            }
            Ok(((**plot).clone(), args[1..].to_vec()))
        }
        _ => Ok((fresh_plot(), args.to_vec())),
    }
}

fn finish_layer(name: &str, plot: &mut Plot, mut layer: Layer, columns: &Columns, new_plot: bool) -> Result<(), Error> {
    if layer.mark == Mark::Heat || plot.layers.iter().any(|l| matches!(l.mark, Mark::Pie | Mark::Heat)) {
        return Err(Error::new("value-error", format!("{name}: a pie or heatmap cannot share a plot with other layers")));
    }
    if layer.mark == Mark::Pie && !plot.layers.is_empty() {
        return Err(Error::new("value-error", format!("{name}: a pie cannot share a plot with other layers")));
    }
    if new_plot {
        reorder_categories(plot, &mut layer, columns.options.sort, columns.options.top);
    }
    if let [only] = layer.series.as_mut_slice()
        && only.name.is_none()
        && layer.mark != Mark::Pie {
            only.name = columns.y_name.clone();
        }
    layer.first_color = series_count(plot);
    if layer.normalized {
        if plot.y.format.is_none() {
            plot.y.format = Some("{:.0}%".to_string());
        }
        if plot.y.range.is_none() {
            plot.y.range = Some((0.0, 100.0));
        }
    }
    if plot.layers.is_empty() {
        if plot.x.label.is_none() {
            plot.x.label = columns.x_name.clone();
        }
        if plot.y.label.is_none() {
            plot.y.label = columns.y_name.clone();
        }
    } else if layer.side == Side::Right && plot.y2.label.is_none() {
        plot.y2.label = columns.y_name.clone();
    }
    plot.layers.push(layer);
    apply_plot_options(name, plot, &columns.options);
    if columns.options.trend {
        add_trend(name, plot, None)?;
    }
    if let Some(window) = columns.options.rolling {
        add_rolling(plot, window);
    }
    Ok(())
}

fn layer_call(name: &str, mark: Mark, args: &[Value]) -> Result<Value, Error> {
    let (mut plot, rest) = split_target(name, args)?;
    let new_plot = plot.layers.is_empty();
    let columns = columns(name, &rest)?;
    let Some(facet) = columns.facet.clone() else {
        let layer = build_layer(name, mark, &mut plot, &columns, None)?;
        finish_layer(name, &mut plot, layer, &columns, new_plot)?;
        return Ok(Value::Plot(Rc::new(plot)));
    };
    if !new_plot {
        return Err(Error::new("value-error", format!("{name}: :facet only works on the first layer")));
    }
    let mut full = plot.clone();
    let mut whole = build_layer(name, mark, &mut full, &columns, None)?;
    reorder_categories(&mut full, &mut whole, columns.options.sort, columns.options.top);
    let mut parent = plot.clone();
    parent.categories = full.categories.clone();
    apply_plot_options(name, &mut parent, &columns.options);
    parent.x.label = parent.x.label.clone().or_else(|| columns.x_name.clone());
    parent.y.label = parent.y.label.clone().or_else(|| columns.y_name.clone());
    for (label, rows) in groups_of(&facet) {
        let subset = Columns {
            xs: pick(&columns.xs, &rows),
            ys: pick(&columns.ys, &rows),
            by: columns.by.as_ref().map(|keys| pick(keys, &rows)),
            facet: None,
            sizes: columns.sizes.as_ref().map(|v| pick(v, &rows)),
            lows: columns.lows.as_ref().map(|v| pick(v, &rows)),
            highs: columns.highs.as_ref().map(|v| pick(v, &rows)),
            texts: columns.texts.as_ref().map(|v| pick(v, &rows)),
            x_name: columns.x_name.clone(),
            y_name: columns.y_name.clone(),
            options: Options { sort: None, top: None, ..options(name, rest.get(3))? },
        };
        let mut panel = plot.clone();
        let layer = build_layer(name, mark, &mut panel, &subset, Some(&full.categories))?;
        finish_layer(name, &mut panel, layer, &subset, false)?;
        panel.title = Some(label);
        parent.panels.push(panel);
    }
    share_axes(&mut parent);
    Ok(Value::Plot(Rc::new(parent)))
}

fn distribution(name: &str, mark: Mark, args: &[Value]) -> Result<Value, Error> {
    let (mut plot, rest) = split_target(name, args)?;
    let new_plot = plot.layers.is_empty();
    let (values, label, options_value, table): (Vec<Value>, Option<String>, Option<&Value>, Option<&Table>) = match rest.first() {
        Some(Value::Table(table)) => {
            let key = rest.get(1).ok_or_else(|| Error::new("value-error", format!("{name} expects a column")))?;
            let (column, values) = column_values(name, table, key)?;
            (values, Some(column.name().to_string()), rest.get(2), Some(table))
        }
        Some(other) => (items(name, other)?.collect(), None, rest.get(1), None),
        None => return Err(Error::new("value-error", format!("{name} expects data"))),
    };
    let options = options(name, options_value)?;
    let keys = |column: Option<Symbol>| -> Result<Option<Vec<Value>>, Error> {
        match (column, table) {
            (Some(column), Some(table)) => Ok(Some(column_values(name, table, &Value::Keyword(column))?.1)),
            (Some(_), None) => Err(Error::new("value-error", format!("{name} expects a table for :facet"))),
            (None, _) => Ok(None),
        }
    };
    let facet = keys(options.facet)?;
    let by = keys(options.by)?;
    let bins = options.bins.unwrap_or(10);
    let finish = |plot: &mut Plot, values: &[f64], groups: Option<&[(String, Vec<usize>)]>, bounds: Option<(f64, f64)>| -> Result<(), Error> {
        let mut layer = match mark {
            Mark::Histogram => {
                let built = match bounds {
                    Some((min, width)) => plot::histogram_in(values, min, width, bins),
                    None => plot::histogram(values, bins),
                };
                plot.categories = built.categories.clone();
                if plot.y.label.is_none() {
                    plot.y.label = Some("count".to_string());
                }
                built.layers.into_iter().next().unwrap_or_else(|| Layer::new(Mark::Histogram))
            }
            _ => {
                let mut layer = Layer::new(Mark::Density);
                match groups {
                    Some(groups) => {
                        for (label, rows) in groups {
                            let subset: Vec<f64> = rows.iter().filter_map(|i| values.get(*i).copied()).collect();
                            let mut series = plot::density(&subset, options.bandwidth, 120);
                            series.name = Some(label.clone());
                            layer.series.push(series);
                        }
                    }
                    None => layer.series.push(plot::density(values, options.bandwidth, 120)),
                }
                if plot.y.label.is_none() {
                    plot.y.label = Some("density".to_string());
                }
                layer
            }
        };
        apply_layer_options(&mut layer, &options);
        layer.first_color = series_count(plot);
        if plot.layers.is_empty() && plot.x.label.is_none() {
            plot.x.label = label.clone();
        }
        plot.layers.push(layer);
        apply_plot_options(name, plot, &options);
        Ok(())
    };
    let all = numbers(name, &values)?;
    let grouped: Option<Vec<(String, Vec<usize>)>> = by.as_ref().map(|keys| {
        let kept: Vec<usize> = (0..values.len()).filter(|i| !matches!(values[*i], Value::Nil)).collect();
        let keys: Vec<Value> = kept.iter().map(|i| keys[*i].clone()).collect();
        groups_of(&keys)
    });
    let Some(facet) = facet else {
        finish(&mut plot, &all, grouped.as_deref(), None)?;
        return Ok(Value::Plot(Rc::new(plot)));
    };
    if !new_plot {
        return Err(Error::new("value-error", format!("{name}: :facet only works on the first layer")));
    }
    let reference = plot::histogram(&all, bins);
    let bounds = (mark == Mark::Histogram).then(|| (reference.layers[0].series[0].xs.first().copied().unwrap_or(0.0), reference.layers[0].step));
    let mut parent = plot.clone();
    apply_plot_options(name, &mut parent, &options);
    parent.x.label = parent.x.label.clone().or_else(|| label.clone());
    parent.y.label = parent.y.label.clone().or_else(|| Some(if mark == Mark::Histogram { "count" } else { "density" }.to_string()));
    for (title, rows) in groups_of(&facet) {
        let subset = numbers(name, &pick(&values, &rows))?;
        let mut panel = plot.clone();
        finish(&mut panel, &subset, None, bounds)?;
        panel.title = Some(title);
        parent.panels.push(panel);
    }
    share_axes(&mut parent);
    Ok(Value::Plot(Rc::new(parent)))
}

fn box_plot(name: &str, args: &[Value]) -> Result<Value, Error> {
    let (mut plot, rest) = split_target(name, args)?;
    let mut layer = Layer::new(Mark::Box);
    let options_value: Option<&Value> = match rest.first() {
        Some(Value::Table(table)) => {
            let keys: Vec<&Value> = rest[1..].iter().take_while(|v| matches!(v, Value::Keyword(_))).collect();
            let (group, value) = match keys.as_slice() {
                [value] => (None, *value),
                [group, value] => (Some(*group), *value),
                _ => return Err(Error::new("value-error", format!("{name} expects a value column and an optional group column"))),
            };
            let (value_name, values) = column_values(name, table, value)?;
            plot.y.label = plot.y.label.clone().or_else(|| Some(value_name.name().to_string()));
            match group {
                None => layer.boxes.push(BoxStats::of("", &numbers(name, &values)?)),
                Some(group) => {
                    let (group_name, groups) = column_values(name, table, group)?;
                    plot.x.label = plot.x.label.clone().or_else(|| Some(group_name.name().to_string()));
                    for (label, rows) in groups_of(&groups) {
                        layer.boxes.push(BoxStats::of(&label, &numbers(name, &pick(&values, &rows))?));
                    }
                }
            }
            rest.get(1 + keys.len())
        }
        Some(other) => {
            let values: Vec<Value> = items(name, other)?.collect();
            layer.boxes.push(BoxStats::of("", &numbers(name, &values)?));
            rest.get(1)
        }
        None => return Err(Error::new("value-error", format!("{name} expects data"))),
    };
    let options = options(name, options_value)?;
    apply_layer_options(&mut layer, &options);
    layer.first_color = series_count(&plot);
    plot.categories = layer.boxes.iter().map(|b| b.label.clone()).collect();
    plot.layers.push(layer);
    apply_plot_options(name, &mut plot, &options);
    Ok(Value::Plot(Rc::new(plot)))
}

fn heatmap(args: &[Value]) -> Result<Value, Error> {
    let name = "heatmap";
    let (mut plot, rest) = split_target(name, args)?;
    if !plot.layers.is_empty() {
        return Err(Error::new("value-error", "heatmap: a heatmap cannot share a plot with other layers"));
    }
    let Some(Value::Table(table)) = rest.first() else {
        return Err(expects(name, "a table", rest.first().unwrap_or(&Value::Nil)));
    };
    let (x_name, xs) = column_values(name, table, rest.get(1).unwrap_or(&Value::Nil))?;
    let (y_name, ys) = column_values(name, table, rest.get(2).unwrap_or(&Value::Nil))?;
    let (_, values) = column_values(name, table, rest.get(3).unwrap_or(&Value::Nil))?;
    let options = options(name, rest.get(4))?;
    let mut layer = Layer::new(Mark::Heat);
    let mut series = Series::default();
    for i in 0..xs.len() {
        if matches!(values[i], Value::Nil) {
            continue;
        }
        let column = category_index(&mut plot, Shown(&xs[i]).to_string());
        let row_label = Shown(&ys[i]).to_string();
        let row = layer.bands.iter().position(|b| *b == row_label).unwrap_or_else(|| {
            layer.bands.push(row_label);
            layer.bands.len() - 1
        });
        series.xs.push(column);
        series.ys.push(row as f64);
        series.sizes.push(number(name, "values", &values[i])?);
    }
    layer.series = vec![series];
    apply_layer_options(&mut layer, &options);
    plot.x.label = Some(x_name.name().to_string());
    plot.y.label = Some(y_name.name().to_string());
    plot.layers.push(layer);
    apply_plot_options(name, &mut plot, &options);
    Ok(Value::Plot(Rc::new(plot)))
}

fn figure(args: &[Value]) -> Result<Value, Error> {
    let name = "figure";
    let mut figure = fresh_plot();
    for value in items(name, &args[0])? {
        let plot = as_plot(name, &value)?.clone();
        if plot.panels.is_empty() {
            figure.panels.push(plot);
        } else {
            figure.panels.extend(plot.panels.clone());
        }
    }
    if figure.panels.is_empty() {
        return Err(Error::new("value-error", "figure expects at least one plot"));
    }
    if let Some(value) = args.get(1) {
        let Value::Map(map) = value else { return Err(expects(name, "an options map", value)) };
        if let Some(title) = map.get(&Value::keyword("title")) {
            figure.title = Some(text_of(title));
        }
        match map.get(&Value::keyword("columns")) {
            None | Some(Value::Nil) => {}
            Some(Value::Int(n)) if *n > 0 => figure.columns = Some(*n as usize),
            Some(_) => return Err(Error::new("value-error", "figure expects a positive number of :columns")),
        }
    }
    Ok(Value::Plot(Rc::new(figure)))
}
