use std::rc::Rc;

use super::collections::{expects, items};
use super::native;
use super::strings::as_str;
use crate::error::Error;
use crate::plot::{self, BoxStats, Kind, Plot, Series};
use crate::symbol::Symbol;
use crate::table::Table;
use crate::value::{Shown, Value};
use crate::vm::{Arity, Globals};

pub fn install(globals: &mut Globals) {
    native(globals, "bar-chart", Arity::Between(2, 4), |args| chart("bar-chart", Kind::Bar, args));
    native(globals, "line-chart", Arity::Between(2, 4), |args| chart("line-chart", Kind::Line, args));
    native(globals, "scatter", Arity::Between(2, 4), |args| chart("scatter", Kind::Scatter, args));
    native(globals, "area-chart", Arity::Between(2, 4), |args| chart("area-chart", Kind::Area, args));
    native(globals, "pie-chart", Arity::Between(2, 4), |args| chart("pie-chart", Kind::Pie, args));
    native(globals, "histogram", Arity::Between(1, 3), histogram);
    native(globals, "box-plot", Arity::Between(1, 4), box_plot);
    native(globals, "plot?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Plot(_)))));
    native(globals, "to-svg", Arity::Exact(1), |args| Ok(Value::string(&as_plot("to-svg", &args[0])?.svg())));
    native(globals, "save-plot", Arity::Exact(2), save_plot);
}

fn as_plot<'a>(name: &str, value: &'a Value) -> Result<&'a Plot, Error> {
    match value {
        Value::Plot(plot) => Ok(plot),
        other => Err(expects(name, "a plot", other)),
    }
}

fn save_plot(args: &[Value]) -> Result<Value, Error> {
    let plot = as_plot("save-plot", &args[0])?;
    let path = as_str("save-plot", &args[1])?;
    std::fs::write(path, plot.svg()).map_err(|e| Error::new("io-error", format!("cannot write {path}: {e}")))?;
    Ok(Value::Nil)
}

#[derive(Default)]
struct Options {
    title: Option<String>,
    x_label: Option<String>,
    y_label: Option<String>,
    by: Option<Symbol>,
    facet: Option<Symbol>,
    bins: Option<usize>,
    stack: bool,
    horizontal: bool,
    donut: bool,
    labels: bool,
    x_range: Option<(f64, f64)>,
    y_range: Option<(f64, f64)>,
    x_log: bool,
    y_log: bool,
    x_format: Option<String>,
    y_format: Option<String>,
    hlines: Vec<f64>,
    vlines: Vec<f64>,
}

fn axis_number(name: &str, key: &str, value: &Value) -> Result<f64, Error> {
    match value {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(x) => Ok(*x),
        Value::Date(d) => Ok(*d as f64),
        other => Err(expects(name, &format!("numbers or dates for :{key}"), other)),
    }
}

impl Options {
    fn apply(&self, plot: &mut Plot) {
        plot.stacked = self.stack;
        plot.horizontal = self.horizontal;
        plot.donut = self.donut;
        plot.labels = self.labels;
        plot.x_range = self.x_range;
        plot.y_range = self.y_range;
        plot.x_log = self.x_log;
        plot.y_log = self.y_log;
        plot.x_format = self.x_format.clone();
        plot.y_format = self.y_format.clone();
        plot.hlines = self.hlines.clone();
        plot.vlines = self.vlines.clone();
        plot.title = self.title.clone().or_else(|| plot.title.take());
        plot.x_label = self.x_label.clone().or_else(|| plot.x_label.take());
        plot.y_label = self.y_label.clone().or_else(|| plot.y_label.take());
    }
}

fn options(name: &str, value: Option<&Value>) -> Result<Options, Error> {
    let mut options = Options::default();
    let Some(value) = value else { return Ok(options) };
    let Value::Map(map) = value else { return Err(expects(name, "an options map", value)) };
    let text = |key: &str| -> Result<Option<String>, Error> {
        match map.get(&Value::keyword(key)) {
            None | Some(Value::Nil) => Ok(None),
            Some(Value::Str(s)) => Ok(Some(s.to_string())),
            Some(other) => Ok(Some(Shown(other).to_string())),
        }
    };
    let flag = |key: &str| -> Result<bool, Error> {
        match map.get(&Value::keyword(key)) {
            None | Some(Value::Nil) | Some(Value::Bool(false)) => Ok(false),
            Some(Value::Bool(true)) => Ok(true),
            Some(other) => Err(expects(name, &format!("true or false for :{key}"), other)),
        }
    };
    let column = |key: &str| -> Result<Option<Symbol>, Error> {
        match map.get(&Value::keyword(key)) {
            None | Some(Value::Nil) => Ok(None),
            Some(Value::Keyword(column)) => Ok(Some(*column)),
            Some(other) => Err(expects(name, &format!("a column keyword for :{key}"), other)),
        }
    };
    let range = |key: &str| -> Result<Option<(f64, f64)>, Error> {
        let Some(value) = map.get(&Value::keyword(key)) else { return Ok(None) };
        if matches!(value, Value::Nil) {
            return Ok(None);
        }
        let bounds: Vec<Value> = items(name, value).ok().map(Iterator::collect).unwrap_or_default();
        let [low, high] = bounds.as_slice() else {
            return Err(Error::new("value-error", format!("{name} expects :{key} to be [low high]")));
        };
        Ok(Some((axis_number(name, key, low)?, axis_number(name, key, high)?)))
    };
    let scale = |key: &str| -> Result<bool, Error> {
        match map.get(&Value::keyword(key)) {
            None | Some(Value::Nil) => Ok(false),
            Some(Value::Keyword(k)) if &*k.name() == "linear" => Ok(false),
            Some(Value::Keyword(k)) if &*k.name() == "log" => Ok(true),
            Some(other) => Err(expects(name, &format!(":linear or :log for :{key}"), other)),
        }
    };
    let lines = |key: &str| -> Result<Vec<f64>, Error> {
        match map.get(&Value::keyword(key)) {
            None | Some(Value::Nil) => Ok(Vec::new()),
            Some(single @ (Value::Int(_) | Value::Float(_) | Value::Date(_))) => Ok(vec![axis_number(name, key, single)?]),
            Some(many) => items(name, many)?.map(|v| axis_number(name, key, &v)).collect(),
        }
    };
    options.title = text("title")?;
    options.x_label = text("x-label")?;
    options.y_label = text("y-label")?;
    options.by = column("by")?;
    options.facet = column("facet")?;
    options.stack = flag("stack")?;
    options.horizontal = flag("horizontal")?;
    options.donut = flag("donut")?;
    options.labels = flag("labels")?;
    options.x_range = range("x-range")?;
    options.y_range = range("y-range")?;
    options.x_log = scale("x-scale")?;
    options.y_log = scale("y-scale")?;
    options.x_format = text("x-format")?;
    options.y_format = text("y-format")?;
    options.hlines = lines("hline")?;
    options.vlines = lines("vline")?;
    options.bins = match map.get(&Value::keyword("bins")) {
        None | Some(Value::Nil) => None,
        Some(Value::Int(n)) if *n > 0 => Some(*n as usize),
        Some(_) => return Err(Error::new("value-error", format!("{name} expects a positive number of bins"))),
    };
    Ok(options)
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

fn facet_groups(keys: &[Value]) -> Vec<(String, Vec<usize>)> {
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

fn share_axes(parent: &mut Plot) {
    let (value, x) = plot::shared_ranges(&parent.panels);
    for panel in &mut parent.panels {
        panel.y_range.get_or_insert(value);
        if panel.x_range.is_none() {
            panel.x_range = x;
        }
    }
}

fn pick(values: &[Value], rows: &[usize]) -> Vec<Value> {
    rows.iter().map(|i| values[*i].clone()).collect()
}

fn build(
    name: &str,
    kind: Kind,
    mut plot: Plot,
    xs: &[Value],
    ys: &[Value],
    by: Option<&[Value]>,
    seed: Option<&[String]>,
) -> Result<Plot, Error> {
    if let Some(seed) = seed {
        plot.categories = seed.to_vec();
    }
    let mut groups: Vec<(Option<String>, Vec<usize>)> = Vec::new();
    for i in 0..xs.len() {
        let key = by.map(|keys| Shown(&keys[i]).to_string());
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, rows)) => rows.push(i),
            None => groups.push((key, vec![i])),
        }
    }
    let categorical = matches!(kind, Kind::Bar | Kind::Pie);
    let mut dates = None;
    for (key, rows) in groups {
        let mut series = Series { name: key, xs: Vec::new(), ys: Vec::new() };
        for i in rows {
            if matches!(xs[i], Value::Nil) || matches!(ys[i], Value::Nil) {
                continue;
            }
            let y = number(name, "y", &ys[i])?;
            let x = match (categorical, &xs[i]) {
                (true, label) => {
                    let label = Shown(label).to_string();
                    let index = plot.categories.iter().position(|c| *c == label).unwrap_or_else(|| {
                        plot.categories.push(label);
                        plot.categories.len() - 1
                    });
                    index as f64
                }
                (false, Value::Date(d)) if dates != Some(false) => {
                    dates = Some(true);
                    *d as f64
                }
                (false, Value::Int(_) | Value::Float(_)) if dates != Some(true) => {
                    dates = Some(false);
                    number(name, "x", &xs[i])?
                }
                (false, other) => return Err(expects(name, "numbers or dates for x", other)),
            };
            series.xs.push(x);
            series.ys.push(y);
        }
        if matches!(kind, Kind::Line | Kind::Area) {
            let mut order: Vec<usize> = (0..series.xs.len()).collect();
            order.sort_by(|a, b| series.xs[*a].total_cmp(&series.xs[*b]));
            series.xs = order.iter().map(|i| series.xs[*i]).collect();
            series.ys = order.iter().map(|i| series.ys[*i]).collect();
        }
        plot.series.push(series);
    }
    plot.dates = dates == Some(true);
    for series in &plot.series {
        let bad = |values: &[f64], log: bool| log.then(|| values.iter().copied().find(|v| *v <= 0.0)).flatten();
        if let Some(v) = bad(&series.ys, plot.y_log).or_else(|| bad(&series.xs, plot.x_log && !categorical)) {
            return Err(Error::new("value-error", format!("{name} expects positive values on a log scale, got {}", plot::number(v))));
        }
    }
    if kind == Kind::Pie {
        let mut totals = vec![0.0; plot.categories.len()];
        for series in &plot.series {
            for (x, y) in series.xs.iter().zip(&series.ys) {
                if *y < 0.0 {
                    return Err(Error::new("value-error", format!("{name} expects positive values, got {}", plot::number(*y))));
                }
                totals[*x as usize] += y;
            }
        }
        plot.series = vec![Series { name: None, xs: (0..totals.len()).map(|i| i as f64).collect(), ys: totals }];
    }
    Ok(plot)
}

type Inputs = (Vec<Value>, Vec<Value>, Option<Vec<Value>>, Option<Vec<Value>>, Options);

fn chart(name: &str, kind: Kind, args: &[Value]) -> Result<Value, Error> {
    let mut base = Plot::new(kind);
    let (xs, ys, by, facet, options): Inputs = match &args[0] {
        Value::Table(table) => {
            let (x_name, xs) = column_values(name, table, args.get(1).unwrap_or(&Value::Nil))?;
            let y_key = args.get(2).ok_or_else(|| Error::new("value-error", format!("{name} expects an x and a y column")))?;
            let (y_name, ys) = column_values(name, table, y_key)?;
            let options = options(name, args.get(3))?;
            let keys = |column: Option<Symbol>| column.map(|c| column_values(name, table, &Value::Keyword(c)).map(|(_, v)| v)).transpose();
            let (by, facet) = (keys(options.by)?, keys(options.facet)?);
            base.x_label = Some(x_name.name().to_string());
            base.y_label = Some(y_name.name().to_string());
            (xs, ys, by, facet, options)
        }
        Value::Vector(_) | Value::List(_) | Value::Array(_) => {
            let options = options(name, args.get(2))?;
            if options.facet.is_some() {
                return Err(Error::new("value-error", format!("{name} expects a table for :facet")));
            }
            let xs: Vec<Value> = items(name, &args[0])?.collect();
            let ys: Vec<Value> = items(name, &args[1])?.collect();
            (xs, ys, None, None, options)
        }
        other => return Err(expects(name, "a table or a collection", other)),
    };
    if xs.len() != ys.len() {
        let message = format!("{name} expects x and y of the same length, got {} and {}", xs.len(), ys.len());
        return Err(Error::new("value-error", message));
    }
    options.apply(&mut base);
    let Some(facet) = facet else {
        return Ok(Value::Plot(Rc::new(build(name, kind, base, &xs, &ys, by.as_deref(), None)?)));
    };
    let full = build(name, kind, base.clone(), &xs, &ys, by.as_deref(), None)?;
    base.categories = full.categories.clone();
    let mut parent = base.clone();
    for (label, rows) in facet_groups(&facet) {
        let sub_by = by.as_ref().map(|keys| pick(keys, &rows));
        let mut panel = build(name, kind, base.clone(), &pick(&xs, &rows), &pick(&ys, &rows), sub_by.as_deref(), Some(&full.categories))?;
        panel.title = Some(label);
        parent.panels.push(panel);
    }
    share_axes(&mut parent);
    Ok(Value::Plot(Rc::new(parent)))
}

fn histogram(args: &[Value]) -> Result<Value, Error> {
    let name = "histogram";
    let (values, label, rest, table): (Vec<Value>, Option<String>, &[Value], Option<&Table>) = match &args[0] {
        Value::Table(table) => {
            let key = args.get(1).ok_or_else(|| Error::new("value-error", "histogram expects a column"))?;
            let (column, values) = column_values(name, table, key)?;
            (values, Some(column.name().to_string()), &args[2..], Some(table))
        }
        other => (items(name, other)?.collect(), None, &args[1..], None),
    };
    let options = options(name, rest.first())?;
    let bins = options.bins.unwrap_or(10);
    let finish = |mut plot: Plot| {
        plot.x_label = label.clone();
        options.apply(&mut plot);
        plot
    };
    let facet = match (options.facet, table) {
        (Some(column), Some(table)) => Some(column_values(name, table, &Value::Keyword(column))?.1),
        (Some(_), None) => return Err(Error::new("value-error", format!("{name} expects a table for :facet"))),
        (None, _) => None,
    };
    let Some(facet) = facet else {
        return Ok(Value::Plot(Rc::new(finish(plot::histogram(&numbers(name, &values)?, bins)))));
    };
    let full = plot::histogram(&numbers(name, &values)?, bins);
    let (min, width) = (full.series[0].xs.first().copied().unwrap_or(0.0), full.step);
    let mut parent = finish(Plot::new(Kind::Histogram));
    for (title, rows) in facet_groups(&facet) {
        let mut panel = plot::histogram_in(&numbers(name, &pick(&values, &rows))?, min, width, bins);
        panel = finish(panel);
        panel.title = Some(title);
        parent.panels.push(panel);
    }
    share_axes(&mut parent);
    Ok(Value::Plot(Rc::new(parent)))
}

fn box_plot(args: &[Value]) -> Result<Value, Error> {
    let name = "box-plot";
    let mut plot = Plot::new(Kind::Box);
    let rest: &[Value] = match &args[0] {
        Value::Table(table) => {
            let keys: Vec<&Value> = args[1..].iter().take_while(|v| matches!(v, Value::Keyword(_))).collect();
            let (group, value) = match keys.as_slice() {
                [value] => (None, *value),
                [group, value] => (Some(*group), *value),
                _ => return Err(Error::new("value-error", format!("{name} expects a value column and an optional group column"))),
            };
            let (value_name, values) = column_values(name, table, value)?;
            plot.y_label = Some(value_name.name().to_string());
            match group {
                None => plot.boxes.push(BoxStats::of("", &numbers(name, &values)?)),
                Some(group) => {
                    let (group_name, groups) = column_values(name, table, group)?;
                    plot.x_label = Some(group_name.name().to_string());
                    for (label, rows) in facet_groups(&groups) {
                        plot.boxes.push(BoxStats::of(&label, &numbers(name, &pick(&values, &rows))?));
                    }
                }
            }
            &args[1 + keys.len()..]
        }
        other => {
            let values: Vec<Value> = items(name, other)?.collect();
            plot.boxes.push(BoxStats::of("", &numbers(name, &values)?));
            &args[1..]
        }
    };
    options(name, rest.first())?.apply(&mut plot);
    plot.categories = plot.boxes.iter().map(|b| b.label.clone()).collect();
    Ok(Value::Plot(Rc::new(plot)))
}
