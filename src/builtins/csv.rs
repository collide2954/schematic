use std::rc::Rc;

use super::collections::expects;
use super::native;
use super::strings::as_str;
use super::tables::as_table;
use crate::array::Array;
use crate::error::Error;
use crate::symbol::Symbol;
use crate::table::Table;
use crate::value::{Shown, Value};
use crate::vm::{Arity, Globals};

pub fn install(globals: &mut Globals) {
    native(globals, "parse-csv", Arity::Between(1, 2), |args| {
        let text = as_str("parse-csv", &args[0])?;
        parse(text, separator(args.get(1))?)
    });
    native(globals, "to-csv", Arity::Between(1, 2), |args| {
        let table = as_table("to-csv", &args[0])?;
        Ok(Value::string(&serialize(table, separator(args.get(1))?)))
    });
    native(globals, "read-csv", Arity::Between(1, 2), |args| {
        let path = as_str("read-csv", &args[0])?;
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::new("io-error", format!("cannot read {path}: {e}")))?;
        parse(&text, separator(args.get(1))?)
    });
    native(globals, "write-csv", Arity::Between(2, 3), |args| {
        let table = as_table("write-csv", &args[0])?;
        let path = as_str("write-csv", &args[1])?;
        std::fs::write(path, serialize(table, separator(args.get(2))?))
            .map_err(|e| Error::new("io-error", format!("cannot write {path}: {e}")))?;
        Ok(Value::Nil)
    });
}

fn separator(options: Option<&Value>) -> Result<char, Error> {
    let Some(options) = options else { return Ok(',') };
    let Value::Map(m) = options else {
        return Err(expects("csv", "an options map", options));
    };
    match m.get(&Value::keyword("sep")) {
        None => Ok(','),
        Some(Value::Str(s)) if s.chars().count() == 1 => Ok(s.chars().next().unwrap()),
        Some(other) => Err(expects("csv", "a single-character :sep", other)),
    }
}

fn records(text: &str, sep: char) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => in_quotes = true,
            '\r' => {}
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            c if c == sep => record.push(std::mem::take(&mut field)),
            c => field.push(c),
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

fn infer(values: &[Option<&str>]) -> Vec<Value> {
    let present = || values.iter().flatten();
    if present().all(|v| v.parse::<i64>().is_ok()) {
        return values.iter().map(|v| v.map_or(Value::Nil, |s| Value::Int(s.parse().unwrap()))).collect();
    }
    if present().all(|v| v.parse::<f64>().is_ok()) {
        return values.iter().map(|v| v.map_or(Value::Nil, |s| Value::Float(s.parse().unwrap()))).collect();
    }
    if present().all(|v| v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("false")) {
        return values.iter().map(|v| v.map_or(Value::Nil, |s| Value::Bool(s.eq_ignore_ascii_case("true")))).collect();
    }
    values.iter().map(|v| v.map_or(Value::Nil, Value::string)).collect()
}

fn parse(text: &str, sep: char) -> Result<Value, Error> {
    let mut records = records(text, sep).into_iter();
    let Some(header) = records.next() else {
        return Ok(Value::Table(Rc::new(Table::default())));
    };
    let rows: Vec<Vec<String>> = records.collect();
    for (i, row) in rows.iter().enumerate() {
        if row.len() != header.len() {
            let message = format!("csv row {} has {} fields but the header has {}", i + 2, row.len(), header.len());
            return Err(Error::new("value-error", message));
        }
    }
    let mut pairs = Vec::with_capacity(header.len());
    for (c, name) in header.iter().enumerate() {
        let raw: Vec<Option<&str>> = rows.iter().map(|row| Some(row[c].as_str()).filter(|s| !s.is_empty())).collect();
        let column = Array::from_values(infer(&raw))?;
        pairs.push((Symbol::intern(name.trim()), Rc::new(column)));
    }
    Ok(Value::Table(Rc::new(Table::new(pairs)?)))
}

fn quote_field(text: &str, sep: char) -> String {
    if text.contains([sep, '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

fn serialize(table: &Table, sep: char) -> String {
    let mut out = String::new();
    let header: Vec<String> = table.names().iter().map(|n| quote_field(&n.name(), sep)).collect();
    out.push_str(&header.join(&sep.to_string()));
    out.push('\n');
    for row in 0..table.nrows() {
        let cells: Vec<String> = table
            .columns()
            .map(|(_, column)| match column.get(row).unwrap() {
                Value::Nil => String::new(),
                value => quote_field(&Shown(&value).to_string(), sep),
            })
            .collect();
        out.push_str(&cells.join(&sep.to_string()));
        out.push('\n');
    }
    out
}
