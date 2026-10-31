use std::cmp::Ordering;
use std::rc::Rc;

use super::collections::{expects, items, out_of_bounds, select_mask, seq_items};
use super::functional::compare_values;
use super::{native, native_vm};
use crate::array::Array;
use crate::error::Error;
use crate::map::Map;
use crate::symbol::Symbol;
use crate::table::Table;
use crate::value::Value;
use crate::vm::{Arity, Globals, Vm};

pub fn install(globals: &mut Globals) {
    native(globals, "table", Arity::AtLeast(0), table);
    native(globals, "select", Arity::AtLeast(2), select);
    native(globals, "rename", Arity::Exact(2), rename);
    native_vm(globals, "where", Arity::Exact(2), where_fn);
    native(globals, "order-by", Arity::Between(2, 3), order_by);
    native(globals, "head", Arity::Between(1, 2), |args| slice("head", args, true));
    native(globals, "tail", Arity::Between(1, 2), |args| slice("tail", args, false));
    native_vm(globals, "aggregate", Arity::Between(2, 3), aggregate);
    native(globals, "table?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Table(_)))));
    native(globals, "columns", Arity::Exact(1), |args| {
        Ok(Value::vector(as_table("columns", &args[0])?.names().iter().map(|n| Value::Keyword(*n))))
    });
    native(globals, "nrows", Arity::Exact(1), |args| Ok(Value::Int(as_table("nrows", &args[0])?.nrows() as i64)));
    native(globals, "col", Arity::Exact(2), col);
    native(globals, "row", Arity::Exact(2), row);
    native(globals, "rows", Arity::Exact(1), |args| Ok(Value::vector(as_table("rows", &args[0])?.rows())));
}

pub fn as_table<'a>(name: &str, value: &'a Value) -> Result<&'a Table, Error> {
    match value {
        Value::Table(t) => Ok(t),
        other => Err(expects(name, "a table", other)),
    }
}

pub fn column_name(name: &str, value: &Value) -> Result<Symbol, Error> {
    match value {
        Value::Keyword(s) | Value::Symbol(s) => Ok(s.root()),
        Value::Str(s) => Ok(Symbol::intern(s)),
        other => Err(expects(name, "a keyword column name", other)),
    }
}

pub fn column_from(value: &Value, nrows: Option<usize>) -> Result<Rc<Array>, Error> {
    match value {
        Value::Array(a) => Ok(a.clone()),
        Value::Vector(_) | Value::List(_) => Ok(Rc::new(Array::from_values(seq_items(value).unwrap())?)),
        scalar => match nrows {
            Some(n) => Ok(Rc::new(Array::from_values(std::iter::repeat_n(scalar.clone(), n))?)),
            None => Err(expects("table", "a column of values", scalar)),
        },
    }
}

fn from_rows(rows: &Value) -> Result<Table, Error> {
    let rows: Vec<Value> = items("table", rows)?.collect();
    let mut names: Vec<Symbol> = Vec::new();
    for row in &rows {
        let Value::Map(m) = row else {
            return Err(expects("table", "row maps", row));
        };
        for key in m.keys() {
            let name = column_name("table", key)?;
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    let mut pairs = Vec::with_capacity(names.len());
    for name in names {
        let key = Value::Keyword(name);
        let values = rows.iter().map(|row| match row {
            Value::Map(m) => m.get(&key).cloned().unwrap_or(Value::Nil),
            _ => Value::Nil,
        });
        pairs.push((name, Rc::new(Array::from_values(values)?)));
    }
    Table::new(pairs)
}

fn from_pairs(pairs: &[Value]) -> Result<Table, Error> {
    let mut table = Table::default();
    for pair in pairs.chunks(2) {
        let name = column_name("table", &pair[0])?;
        let nrows = if table.ncols() > 0 { Some(table.nrows()) } else { None };
        table.set_column(name, column_from(&pair[1], nrows)?)?;
    }
    Ok(table)
}

fn table(args: &[Value]) -> Result<Value, Error> {
    let built = match args {
        [] => Table::default(),
        [Value::Table(t)] => (**t).clone(),
        [Value::Map(m)] => {
            let flat: Vec<Value> = m.iter().flat_map(|(k, v)| [k.clone(), v.clone()]).collect();
            from_pairs(&flat)?
        }
        [Value::Vector(_) | Value::List(_)] => from_rows(&args[0])?,
        pairs if pairs.len() % 2 == 0 && pairs.iter().step_by(2).all(|k| matches!(k, Value::Keyword(_))) => {
            from_pairs(pairs)?
        }
        _ => {
            let shown = if args.len() == 1 { args[0].to_string() } else { format!("{} arguments", args.len()) };
            return Err(Error::type_error(format!(
                "table expects a map of columns, a vector of row maps, or keyword and column pairs, got {shown}"
            )));
        }
    };
    Ok(Value::Table(Rc::new(built)))
}

fn col(args: &[Value]) -> Result<Value, Error> {
    let table = as_table("col", &args[0])?;
    let name = column_name("col", &args[1])?;
    match table.column(name) {
        Some(c) => Ok(Value::Array(c.clone())),
        None => Err(Error::new("value-error", format!("table has no column :{name}"))),
    }
}

fn column_names(name: &str, args: &[Value]) -> Result<Vec<Symbol>, Error> {
    match args {
        [Value::Vector(_) | Value::List(_)] => items(name, &args[0])?.map(|v| column_name(name, &v)).collect(),
        _ => args.iter().map(|v| column_name(name, v)).collect(),
    }
}

fn select(args: &[Value]) -> Result<Value, Error> {
    let table = as_table("select", &args[0])?;
    let names = column_names("select", &args[1..])?;
    Ok(Value::Table(Rc::new(table.select(&names)?)))
}

fn rename(args: &[Value]) -> Result<Value, Error> {
    let mut table = as_table("rename", &args[0])?.clone();
    let Value::Map(renames) = &args[1] else {
        return Err(expects("rename", "a map from old to new column names", &args[1]));
    };
    for (from, to) in renames.iter() {
        table = table.rename(column_name("rename", from)?, column_name("rename", to)?)?;
    }
    Ok(Value::Table(Rc::new(table)))
}

fn where_fn(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let Value::Table(table) = &args[0] else {
        return select_mask(&args[0], &args[1]);
    };
    let keep: Vec<usize> = match &args[1] {
        Value::Array(_) | Value::Vector(_) | Value::List(_) => {
            let flags: Vec<Value> = items("where", &args[1])?.collect();
            if flags.len() != table.nrows() {
                let message = format!("mask length {} does not match table length {}", flags.len(), table.nrows());
                return Err(Error::new("value-error", message));
            }
            flags.iter().enumerate().filter(|(_, f)| f.is_truthy()).map(|(i, _)| i).collect()
        }
        f @ (Value::Closure(_) | Value::Native(_) | Value::Keyword(_)) => {
            let mut keep = Vec::new();
            for (i, row) in table.rows().enumerate() {
                if vm.call_value(f, &[row])?.is_truthy() {
                    keep.push(i);
                }
            }
            keep
        }
        other => return Err(expects("where", "a mask or a predicate", other)),
    };
    Ok(Value::Table(Rc::new(table.take_rows(&keep))))
}

fn order_by(args: &[Value]) -> Result<Value, Error> {
    let table = as_table("order-by", &args[0])?;
    let descending = match args.get(2) {
        None => false,
        Some(Value::Keyword(k)) if &*k.name() == "desc" => true,
        Some(Value::Keyword(k)) if &*k.name() == "asc" => false,
        Some(other) => return Err(expects("order-by", ":asc or :desc", other)),
    };
    let names = column_names("order-by", &args[1..2])?;
    let mut keys = Vec::with_capacity(names.len());
    for name in names {
        keys.push(table.column(name).ok_or_else(|| Table::missing(name))?.clone());
    }
    let mut indices: Vec<usize> = (0..table.nrows()).collect();
    indices.sort_by(|&a, &b| {
        for key in &keys {
            let ordering = compare_values("order-by", &key.get(a).unwrap(), &key.get(b).unwrap()).unwrap_or(Ordering::Equal);
            if ordering != Ordering::Equal {
                return if descending { ordering.reverse() } else { ordering };
            }
        }
        Ordering::Equal
    });
    Ok(Value::Table(Rc::new(table.take_rows(&indices))))
}

fn key_columns(table: &Table, spec: &Value) -> Result<Vec<Symbol>, Error> {
    let names: Vec<Symbol> = match spec {
        Value::Vector(_) | Value::List(_) => {
            items("group-by", spec)?.map(|v| column_name("group-by", &v)).collect::<Result<_, _>>()?
        }
        other => vec![column_name("group-by", other)?],
    };
    for name in &names {
        table.column(*name).ok_or_else(|| Table::missing(*name))?;
    }
    Ok(names)
}

fn group_rows(table: &Table, names: &[Symbol]) -> Vec<(Value, Vec<usize>)> {
    let columns: Vec<&Rc<Array>> = names.iter().map(|n| table.column(*n).unwrap()).collect();
    let mut groups: Vec<(Value, Vec<usize>)> = Vec::new();
    let mut positions = Map::new();
    for i in 0..table.nrows() {
        let key = if columns.len() == 1 {
            columns[0].get(i).unwrap()
        } else {
            Value::vector(columns.iter().map(|c| c.get(i).unwrap()))
        };
        match positions.get(&key) {
            Some(Value::Int(p)) => groups[*p as usize].1.push(i),
            _ => {
                positions.insert_mut(key.clone(), Value::Int(groups.len() as i64));
                groups.push((key, vec![i]));
            }
        }
    }
    groups
}

pub fn group_by_table(table: &Table, spec: &Value) -> Result<Value, Error> {
    let names = key_columns(table, spec)?;
    let pairs = group_rows(table, &names)
        .into_iter()
        .map(|(key, rows)| (key, Value::Table(Rc::new(table.take_rows(&rows)))));
    Ok(Value::map(pairs))
}

fn apply_aggregate(vm: &mut Vm, how: &Value, group: &Table) -> Result<Value, Error> {
    let whole = || Value::Table(Rc::new(group.clone()));
    match how {
        Value::Vector(v) => {
            let parts: Vec<Value> = v.iter().cloned().collect();
            match parts.as_slice() {
                [f] => vm.call_value(f, &[whole()]),
                [f, col] => {
                    let name = column_name("aggregate", col)?;
                    let column = group.column(name).ok_or_else(|| Table::missing(name))?;
                    vm.call_value(f, &[Value::Array(column.clone())])
                }
                _ => Err(expects("aggregate", "[function] or [function column]", how)),
            }
        }
        f @ (Value::Closure(_) | Value::Native(_) | Value::Keyword(_)) => vm.call_value(f, &[whole()]),
        other => Err(expects("aggregate", "[function column] pairs", other)),
    }
}

fn aggregate(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let table = as_table("aggregate", &args[0])?;
    let (by, spec) = match args.len() {
        3 => (Some(&args[1]), &args[2]),
        _ => (None, &args[1]),
    };
    let Value::Map(spec) = spec else {
        return Err(expects("aggregate", "a map from new column names to [function column] pairs", spec));
    };
    let names = match by {
        Some(spec) => key_columns(table, spec)?,
        None => Vec::new(),
    };
    let groups = if names.is_empty() {
        vec![(Value::Nil, (0..table.nrows()).collect())]
    } else {
        group_rows(table, &names)
    };
    let mut specs = Vec::with_capacity(spec.len());
    for (k, how) in spec.iter() {
        specs.push((column_name("aggregate", k)?, how.clone()));
    }
    let mut out_names: Vec<Symbol> = names.clone();
    out_names.extend(specs.iter().map(|(name, _)| *name));
    let mut out_values: Vec<Vec<Value>> = vec![Vec::with_capacity(groups.len()); out_names.len()];
    for (key, rows) in &groups {
        let group = table.take_rows(rows);
        match names.len() {
            0 => {}
            1 => out_values[0].push(key.clone()),
            _ => {
                for (c, part) in seq_items(key).unwrap().enumerate() {
                    out_values[c].push(part);
                }
            }
        }
        for (j, (_, how)) in specs.iter().enumerate() {
            let value = apply_aggregate(vm, how, &group)?;
            out_values[names.len() + j].push(value);
        }
    }
    let mut pairs = Vec::with_capacity(out_names.len());
    for (name, values) in out_names.into_iter().zip(out_values) {
        pairs.push((name, Rc::new(Array::from_values(values)?)));
    }
    Ok(Value::Table(Rc::new(Table::new(pairs)?)))
}

fn slice(name: &str, args: &[Value], from_start: bool) -> Result<Value, Error> {
    let table = as_table(name, &args[0])?;
    let n = match args.get(1) {
        None => 5,
        Some(Value::Int(n)) if *n >= 0 => *n as usize,
        Some(other) => return Err(expects(name, "a non-negative count", other)),
    };
    let total = table.nrows();
    let n = n.min(total);
    let indices: Vec<usize> = if from_start { (0..n).collect() } else { (total - n..total).collect() };
    Ok(Value::Table(Rc::new(table.take_rows(&indices))))
}

fn row(args: &[Value]) -> Result<Value, Error> {
    let table = as_table("row", &args[0])?;
    let Value::Int(i) = &args[1] else {
        return Err(expects("row", "an integer index", &args[1]));
    };
    usize::try_from(*i)
        .ok()
        .and_then(|i| table.row(i))
        .ok_or_else(|| out_of_bounds(*i, "table", table.nrows()))
}
