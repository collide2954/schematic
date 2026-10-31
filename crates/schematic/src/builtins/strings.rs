use std::fmt::Write;
use std::rc::Rc;

use super::collections::{expects, items, owned, take};
use super::{native, native_owned};
use crate::error::Error;
use crate::reader::parse_number;
use crate::symbol::Symbol;
use crate::value::{Shown, Value};
use crate::vm::{Arity, Globals};

pub fn install(globals: &mut Globals) {
    native_owned(globals, "str", Arity::AtLeast(0), str_fn);
    native(globals, "substring", Arity::Between(2, 3), substring);
    native(globals, "string-split", Arity::Between(1, 2), string_split);
    native(globals, "string-join", Arity::Between(1, 2), string_join);
    native(globals, "string-upcase", Arity::Exact(1), |args| {
        Ok(Value::string(&as_str("string-upcase", &args[0])?.to_uppercase()))
    });
    native(globals, "string-downcase", Arity::Exact(1), |args| {
        Ok(Value::string(&as_str("string-downcase", &args[0])?.to_lowercase()))
    });
    native(globals, "string-trim", Arity::Exact(1), |args| Ok(Value::string(as_str("string-trim", &args[0])?.trim())));
    native(globals, "string-contains?", Arity::Exact(2), |args| {
        Ok(Value::Bool(as_str("string-contains?", &args[0])?.contains(as_str("string-contains?", &args[1])?)))
    });
    native(globals, "string-starts-with?", Arity::Exact(2), |args| {
        let name = "string-starts-with?";
        Ok(Value::Bool(as_str(name, &args[0])?.starts_with(as_str(name, &args[1])?)))
    });
    native(globals, "string-ends-with?", Arity::Exact(2), |args| {
        let name = "string-ends-with?";
        Ok(Value::Bool(as_str(name, &args[0])?.ends_with(as_str(name, &args[1])?)))
    });
    native(globals, "string-replace", Arity::Exact(3), |args| {
        let name = "string-replace";
        let replaced = as_str(name, &args[0])?.replace(as_str(name, &args[1])?, as_str(name, &args[2])?);
        Ok(Value::string(&replaced))
    });
    native(globals, "string-index-of", Arity::Exact(2), string_index_of);
    native(globals, "string-repeat", Arity::Exact(2), string_repeat);
    native(globals, "string->number", Arity::Exact(1), string_to_number);
    native(globals, "number->string", Arity::Exact(1), |args| match &args[0] {
        Value::Int(_) | Value::Float(_) => Ok(Value::string(&args[0].to_string())),
        other => Err(expects("number->string", "a number", other)),
    });
    native(globals, "name", Arity::Exact(1), |args| match &args[0] {
        Value::Keyword(s) | Value::Symbol(s) => Ok(Value::string(&s.name())),
        Value::Str(_) => Ok(args[0].clone()),
        other => Err(expects("name", "a keyword, symbol, or string", other)),
    });
    native(globals, "keyword", Arity::Exact(1), |args| match &args[0] {
        Value::Str(s) => Ok(Value::keyword(s)),
        Value::Symbol(s) => Ok(Value::Keyword(*s)),
        Value::Keyword(_) => Ok(args[0].clone()),
        other => Err(expects("keyword", "a string, symbol, or keyword", other)),
    });
    native(globals, "symbol", Arity::Exact(1), |args| match &args[0] {
        Value::Str(s) => Ok(Value::Symbol(Symbol::intern(s))),
        Value::Keyword(s) => Ok(Value::Symbol(*s)),
        Value::Symbol(_) => Ok(args[0].clone()),
        other => Err(expects("symbol", "a string, symbol, or keyword", other)),
    });
}

pub fn as_str<'a>(name: &str, value: &'a Value) -> Result<&'a str, Error> {
    match value {
        Value::Str(s) => Ok(s),
        other => Err(expects(name, "a string", other)),
    }
}

pub fn display(value: &Value) -> String {
    match value {
        Value::Nil => String::new(),
        other => Shown(other).to_string(),
    }
}

fn str_fn(args: &mut [Value]) -> Result<Value, Error> {
    if args.is_empty() {
        return Ok(Value::string(""));
    }
    let mut out = match take(args, 0) {
        Value::Str(s) => owned(s),
        other => display(&other),
    };
    for arg in &args[1..] {
        out.push_str(&display(arg));
    }
    Ok(Value::Str(Rc::new(out)))
}

fn substring(args: &[Value]) -> Result<Value, Error> {
    let s = as_str("substring", &args[0])?;
    let len = s.chars().count() as i64;
    let bound = |value: Option<&Value>, default: i64| match value {
        None => Ok(default),
        Some(Value::Int(i)) => Ok(*i),
        Some(other) => Err(expects("substring", "integer bounds", other)),
    };
    let start = bound(args.get(1), 0)?;
    let end = bound(args.get(2), len)?;
    if start < 0 || end < start || end > len {
        let message = format!("substring range {start} to {end} is out of bounds for a string of length {len}");
        return Err(Error::new("index-error", message));
    }
    let piece: String = s.chars().skip(start as usize).take((end - start) as usize).collect();
    Ok(Value::string(&piece))
}

fn string_split(args: &[Value]) -> Result<Value, Error> {
    let s = as_str("string-split", &args[0])?;
    let parts: Vec<Value> = match args.get(1) {
        None => s.split_whitespace().map(Value::string).collect(),
        Some(separator) => {
            let separator = as_str("string-split", separator)?;
            if separator.is_empty() {
                return Err(Error::new("value-error", "string-split separator must not be empty"));
            }
            s.split(separator).map(Value::string).collect()
        }
    };
    Ok(Value::vector(parts))
}

fn string_join(args: &[Value]) -> Result<Value, Error> {
    let (separator, coll) = match args {
        [coll] => ("", coll),
        [separator, coll] => (as_str("string-join", separator)?, coll),
        _ => unreachable!(),
    };
    let mut out = String::new();
    for (i, item) in items("string-join", coll)?.enumerate() {
        if i > 0 {
            out.push_str(separator);
        }
        out.push_str(&display(&item));
    }
    Ok(Value::string(&out))
}

fn string_index_of(args: &[Value]) -> Result<Value, Error> {
    let haystack = as_str("string-index-of", &args[0])?;
    let needle = as_str("string-index-of", &args[1])?;
    Ok(match haystack.find(needle) {
        Some(byte) => Value::Int(haystack[..byte].chars().count() as i64),
        None => Value::Nil,
    })
}

fn string_repeat(args: &[Value]) -> Result<Value, Error> {
    let s = as_str("string-repeat", &args[0])?;
    match &args[1] {
        Value::Int(n) if *n >= 0 => Ok(Value::string(&s.repeat(*n as usize))),
        other => Err(expects("string-repeat", "a non-negative count", other)),
    }
}

fn string_to_number(args: &[Value]) -> Result<Value, Error> {
    let s = as_str("string->number", &args[0])?.trim();
    Ok(match parse_number(s) {
        Some(Ok(v)) => v,
        _ => Value::Nil,
    })
}

pub fn join_display(args: &[Value], readable: bool) -> String {
    let mut text = String::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            text.push(' ');
        }
        if readable {
            let _ = write!(text, "{arg}");
        } else {
            let _ = write!(text, "{}", Shown(arg));
        }
    }
    text
}
