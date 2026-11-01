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
    bins: Option<usize>,
    stack: bool,
    horizontal: bool,
    donut: bool,
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
    options.title = text("title")?;
    options.x_label = text("x-label")?;
    options.y_label = text("y-label")?;
    options.stack = flag("stack")?;
    options.horizontal = flag("horizontal")?;
    options.donut = flag("donut")?;
    options.by = match map.get(&Value::keyword("by")) {
        None | Some(Value::Nil) => None,
        Some(Value::Keyword(column)) => Some(*column),
        Some(other) => return Err(expects(name, "a column keyword for :by", other)),
    };
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

fn chart(name: &str, kind: Kind, args: &[Value]) -> Result<Value, Error> {
    let mut plot = Plot::new(kind);
    let (xs, ys, by, options): (Vec<Value>, Vec<Value>, Option<Vec<Value>>, Options) = match &args[0] {
        Value::Table(table) => {
            let (x_name, xs) = column_values(name, table, args.get(1).unwrap_or(&Value::Nil))?;
            let y_key = args.get(2).ok_or_else(|| Error::new("value-error", format!("{name} expects an x and a y column")))?;
            let (y_name, ys) = column_values(name, table, y_key)?;
            let options = options(name, args.get(3))?;
            let by = match options.by {
                Some(column) => Some(column_values(name, table, &Value::Keyword(column))?.1),
                None => None,
            };
            plot.x_label = Some(x_name.name().to_string());
            plot.y_label = Some(y_name.name().to_string());
            (xs, ys, by, options)
        }
        Value::Vector(_) | Value::List(_) | Value::Array(_) => {
            let xs: Vec<Value> = items(name, &args[0])?.collect();
            let ys: Vec<Value> = items(name, &args[1])?.collect();
            (xs, ys, None, options(name, args.get(2))?)
        }
        other => return Err(expects(name, "a table or a collection", other)),
    };
    if xs.len() != ys.len() {
        let message = format!("{name} expects x and y of the same length, got {} and {}", xs.len(), ys.len());
        return Err(Error::new("value-error", message));
    }
    plot.title = options.title.or(plot.title);
    plot.x_label = options.x_label.or(plot.x_label);
    plot.y_label = options.y_label.or(plot.y_label);
    plot.stacked = options.stack;
    plot.horizontal = options.horizontal;
    plot.donut = options.donut;
    let mut groups: Vec<(Option<String>, Vec<usize>)> = Vec::new();
    for i in 0..xs.len() {
        let key = by.as_ref().map(|keys| Shown(&keys[i]).to_string());
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
    Ok(Value::Plot(Rc::new(plot)))
}

fn histogram(args: &[Value]) -> Result<Value, Error> {
    let (values, label, rest): (Vec<Value>, Option<String>, &[Value]) = match &args[0] {
        Value::Table(table) => {
            let key = args.get(1).ok_or_else(|| Error::new("value-error", "histogram expects a column"))?;
            let (name, values) = column_values("histogram", table, key)?;
            (values, Some(name.name().to_string()), &args[2..])
        }
        other => (items("histogram", other)?.collect(), None, &args[1..]),
    };
    let options = options("histogram", rest.first())?;
    let mut plot = plot::histogram(&numbers("histogram", &values)?, options.bins.unwrap_or(10));
    plot.title = options.title;
    plot.x_label = options.x_label.or(label);
    plot.y_label = options.y_label.or(plot.y_label);
    Ok(Value::Plot(Rc::new(plot)))
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
                    let mut buckets: Vec<(String, Vec<Value>)> = Vec::new();
                    for (key, value) in groups.iter().zip(&values) {
                        let label = Shown(key).to_string();
                        match buckets.iter_mut().find(|(l, _)| *l == label) {
                            Some((_, bucket)) => bucket.push(value.clone()),
                            None => buckets.push((label, vec![value.clone()])),
                        }
                    }
                    for (label, bucket) in buckets {
                        plot.boxes.push(BoxStats::of(&label, &numbers(name, &bucket)?));
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
    let options = options(name, rest.first())?;
    plot.title = options.title;
    plot.x_label = options.x_label.or(plot.x_label);
    plot.y_label = options.y_label.or(plot.y_label);
    plot.categories = plot.boxes.iter().map(|b| b.label.clone()).collect();
    Ok(Value::Plot(Rc::new(plot)))
}
