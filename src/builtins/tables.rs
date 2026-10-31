use std::rc::Rc;

use super::collections::{expects, items, out_of_bounds, seq_items};
use super::native;
use crate::array::Array;
use crate::error::Error;
use crate::symbol::Symbol;
use crate::table::Table;
use crate::value::Value;
use crate::vm::{Arity, Globals};

pub fn install(globals: &mut Globals) {
    native(globals, "table", Arity::AtLeast(0), table);
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
