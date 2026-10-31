use std::rc::Rc;

use super::tables::{append, column_from, column_name, distinct_rows, push_rows};
use super::{native, native_owned};
use crate::array::Array;
use crate::error::Error;
use crate::list::List;
use crate::map::Map;
use crate::set::Set;
use crate::value::Value;
use crate::vector::Vector;
use crate::vm::{Arity, Globals};

pub fn install(globals: &mut Globals) {
    native(globals, "first", Arity::Exact(1), first);
    native(globals, "rest", Arity::Exact(1), rest);
    native(globals, "cons", Arity::Exact(2), cons);
    native(globals, "empty?", Arity::Exact(1), is_empty);
    native(globals, "count", Arity::Exact(1), count);
    native(globals, "last", Arity::Exact(1), last);
    native(globals, "nth", Arity::Between(2, 3), nth);
    native_owned(globals, "conj", Arity::AtLeast(1), conj_owned);
    native(globals, "list", Arity::AtLeast(0), |args| Ok(Value::list(args.iter().cloned())));
    native(globals, "vector", Arity::AtLeast(0), |args| Ok(Value::vector(args.iter().cloned())));
    native(globals, "vec", Arity::Exact(1), |args| Ok(Value::vector(items("vec", &args[0])?)));
    native(globals, "hash-map", Arity::AtLeast(0), hash_map);
    native(globals, "hash-set", Arity::AtLeast(0), |args| Ok(Value::set(args.iter().cloned())));
    native(globals, "set", Arity::Exact(1), |args| Ok(Value::set(items("set", &args[0])?)));
    native(globals, "range", Arity::Between(1, 3), range);
    native(globals, "reverse", Arity::Exact(1), reverse);
    native(globals, "get", Arity::Between(2, 3), get);
    native(globals, "find", Arity::Exact(2), find);
    native_owned(globals, "assoc", Arity::AtLeast(1), assoc_owned);
    native_owned(globals, "dissoc", Arity::AtLeast(1), dissoc);
    native(globals, "keys", Arity::Exact(1), |args| map_part("keys", &args[0], |m| m.keys().cloned().collect()));
    native(globals, "vals", Arity::Exact(1), |args| map_part("vals", &args[0], |m| m.values().cloned().collect()));
    native(globals, "contains?", Arity::Exact(2), contains);
    native(globals, "merge", Arity::AtLeast(0), merge);
    native_owned(globals, "disj", Arity::AtLeast(1), disj);
    native(globals, "select-keys", Arity::Exact(2), select_keys);
    native(globals, "zipmap", Arity::Exact(2), zipmap);
    native(globals, "subvec", Arity::Between(2, 3), subvec);
    native(globals, "take", Arity::Exact(2), |args| take_drop("take", args, true));
    native(globals, "drop", Arity::Exact(2), |args| take_drop("drop", args, false));
    native(globals, "concat", Arity::AtLeast(0), concat);
    native_owned(globals, "into", Arity::Exact(2), into);
    native(globals, "distinct", Arity::Between(1, 2), distinct);
    native(globals, "frequencies", Arity::Exact(1), frequencies);
    native(globals, "partition", Arity::Between(2, 3), partition);
    native(globals, "interleave", Arity::AtLeast(0), interleave);
    native(globals, "flatten", Arity::Exact(1), flatten);
    native(globals, "second", Arity::Exact(1), |args| Ok(items("second", &args[0])?.nth(1).unwrap_or(Value::Nil)));
    native(globals, "nil?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Nil))));
    native(globals, "boolean?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Bool(_)))));
    native(globals, "integer?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Int(_)))));
    native(globals, "float?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Float(_)))));
    native(globals, "number?", Arity::Exact(1), |args| {
        Ok(Value::Bool(matches!(args[0], Value::Int(_) | Value::Float(_))))
    });
    native(globals, "string?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Str(_)))));
    native(globals, "symbol?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Symbol(_)))));
    native(globals, "keyword?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Keyword(_)))));
    native(globals, "list?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::List(_)))));
    native(globals, "vector?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Vector(_)))));
    native(globals, "map?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Map(_)))));
    native(globals, "set?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Set(_)))));
    native(globals, "procedure?", Arity::Exact(1), |args| {
        Ok(Value::Bool(matches!(args[0], Value::Closure(_) | Value::Native(_) | Value::Escape(_))))
    });
    native(globals, "coll?", Arity::Exact(1), |args| {
        Ok(Value::Bool(matches!(
            args[0],
            Value::List(_) | Value::Vector(_) | Value::Map(_) | Value::Set(_) | Value::Array(_) | Value::Table(_)
        )))
    });
    native(globals, "array", Arity::Exact(1), |args| Value::array(items("array", &args[0])?));
    native(globals, "array?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Array(_)))));
    native(globals, "array-type", Arity::Exact(1), |args| match &args[0] {
        Value::Array(a) => Ok(Value::keyword(a.kind())),
        other => Err(expects("array-type", "an array", other)),
    });
    native(globals, "count-missing", Arity::Exact(1), |args| match &args[0] {
        Value::Array(a) => Ok(Value::Int(a.missing_count() as i64)),
        other => Ok(Value::Int(items("count-missing", other)?.filter(|v| matches!(v, Value::Nil)).count() as i64)),
    });
    native(globals, "equal?", Arity::Exact(2), |args| Ok(Value::Bool(args[0] == args[1])));
    native(globals, "drop-missing", Arity::Exact(1), drop_missing);
    native(globals, "fill-missing", Arity::Exact(2), fill_missing);
    native(globals, "missing?", Arity::Exact(1), |args| Ok(Value::Bool(matches!(args[0], Value::Nil))));
}

pub fn expects(name: &str, what: &str, got: &Value) -> Error {
    Error::type_error(format!("{name} expects {what}, got {got}"))
}

pub fn seq_items(value: &Value) -> Option<Box<dyn Iterator<Item = Value> + '_>> {
    Some(match value {
        Value::Nil => Box::new(std::iter::empty()),
        Value::List(l) => Box::new(l.iter().cloned()),
        Value::Vector(v) => Box::new(v.iter().cloned()),
        Value::Map(m) => Box::new(m.iter().map(|(k, v)| Value::vector([k.clone(), v.clone()]))),
        Value::Set(s) => Box::new(s.iter().cloned()),
        Value::Array(a) => Box::new(a.iter()),
        Value::Table(t) => Box::new(t.rows()),
        Value::Str(s) => Box::new(s.chars().map(|c| Value::string(c.encode_utf8(&mut [0; 4])))),
        _ => return None,
    })
}

pub fn items<'a>(name: &str, value: &'a Value) -> Result<Box<dyn Iterator<Item = Value> + 'a>, Error> {
    seq_items(value).ok_or_else(|| expects(name, "a collection", value))
}

pub fn out_of_bounds(index: i64, kind: &str, len: usize) -> Error {
    let article = if kind.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
    Error::new("index-error", format!("index {index} is out of bounds for {article} {kind} of length {len}"))
}

fn first(args: &[Value]) -> Result<Value, Error> {
    Ok(items("first", &args[0])?.next().unwrap_or(Value::Nil))
}

fn rest(args: &[Value]) -> Result<Value, Error> {
    match &args[0] {
        Value::List(l) => Ok(Value::List(l.rest())),
        other => Ok(Value::List(items("rest", other)?.skip(1).collect())),
    }
}

fn cons(args: &[Value]) -> Result<Value, Error> {
    let tail: List = match &args[1] {
        Value::List(l) => l.clone(),
        other => items("cons", other)?.collect(),
    };
    Ok(Value::List(tail.cons(args[0].clone())))
}

fn is_empty(args: &[Value]) -> Result<Value, Error> {
    let empty = match &args[0] {
        Value::Nil => true,
        Value::List(l) => l.is_empty(),
        Value::Vector(v) => v.is_empty(),
        Value::Map(m) => m.is_empty(),
        Value::Set(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Table(t) => t.nrows() == 0,
        Value::Str(s) => s.is_empty(),
        other => return Err(expects("empty?", "a collection", other)),
    };
    Ok(Value::Bool(empty))
}

fn count(args: &[Value]) -> Result<Value, Error> {
    let n = match &args[0] {
        Value::Nil => 0,
        Value::List(l) => l.len(),
        Value::Vector(v) => v.len(),
        Value::Map(m) => m.len(),
        Value::Set(s) => s.len(),
        Value::Array(a) => a.len(),
        Value::Table(t) => t.nrows(),
        Value::Str(s) => s.chars().count(),
        other => return Err(expects("count", "a collection", other)),
    };
    Ok(Value::Int(n as i64))
}

fn last(args: &[Value]) -> Result<Value, Error> {
    match &args[0] {
        Value::Vector(v) => Ok(v.last().cloned().unwrap_or(Value::Nil)),
        other => Ok(items("last", other)?.last().unwrap_or(Value::Nil)),
    }
}

fn nth(args: &[Value]) -> Result<Value, Error> {
    let Value::Int(index) = &args[1] else {
        return Err(expects("nth", "an integer index", &args[1]));
    };
    let position = usize::try_from(*index).ok();
    let (found, kind, len) = match &args[0] {
        Value::Vector(v) => (position.and_then(|i| v.get(i)).cloned(), "vector", v.len()),
        Value::Array(a) => (position.and_then(|i| a.get(i)), "array", a.len()),
        Value::List(l) => (position.and_then(|i| l.iter().nth(i)).cloned(), "list", l.len()),
        Value::Str(s) => (
            position.and_then(|i| s.chars().nth(i)).map(|c| Value::string(c.encode_utf8(&mut [0; 4]))),
            "string",
            s.chars().count(),
        ),
        other => return Err(expects("nth", "a list, vector, array, or string", other)),
    };
    match (found, args.get(2)) {
        (Some(v), _) => Ok(v),
        (None, Some(default)) => Ok(default.clone()),
        (None, None) => Err(out_of_bounds(*index, kind, len)),
    }
}

pub fn owned<T: Clone>(shared: Rc<T>) -> T {
    Rc::try_unwrap(shared).unwrap_or_else(|rc| (*rc).clone())
}

pub fn take(args: &mut [Value], index: usize) -> Value {
    std::mem::take(&mut args[index])
}

fn conj_owned(args: &mut [Value]) -> Result<Value, Error> {
    let target = take(args, 0);
    let args = args[1..].iter_mut().map(std::mem::take);
    match target {
        Value::Nil => Ok(Value::List(args.fold(List::new(), List::cons))),
        Value::List(l) => Ok(Value::List(args.fold(l, List::cons))),
        Value::Vector(v) => {
            let mut out = owned(v);
            args.for_each(|x| out.push_mut(x));
            Ok(Value::Vector(Rc::new(out)))
        }
        Value::Set(s) => {
            let mut out = owned(s);
            args.for_each(|x| out.insert_mut(x));
            Ok(Value::Set(Rc::new(out)))
        }
        Value::Array(a) => {
            let mut out: Array = owned(a);
            for x in args {
                out.push_mut(x)?;
            }
            Ok(Value::Array(Rc::new(out)))
        }
        Value::Table(t) => push_rows(owned(t), args),
        Value::Map(m) => {
            let mut out = owned(m);
            for addition in args {
                match addition {
                    Value::Vector(pair) if pair.len() == 2 => {
                        out.insert_mut(pair.get(0).unwrap().clone(), pair.get(1).unwrap().clone());
                    }
                    Value::Map(other) => other.iter().for_each(|(k, v)| out.insert_mut(k.clone(), v.clone())),
                    other => return Err(expects("conj onto a map", "a [key value] pair or a map", &other)),
                }
            }
            Ok(Value::Map(Rc::new(out)))
        }
        other => Err(expects("conj", "a collection", &other)),
    }
}

fn hash_map(args: &[Value]) -> Result<Value, Error> {
    if args.len() % 2 != 0 {
        let message = format!("hash-map expects an even number of arguments, got {}", args.len());
        return Err(Error::new("arity-error", message));
    }
    Ok(Value::map(args.chunks(2).map(|pair| (pair[0].clone(), pair[1].clone()))))
}

fn range(args: &[Value]) -> Result<Value, Error> {
    let (start, end, step) = match args {
        [end] => (Value::Int(0), end.clone(), Value::Int(1)),
        [start, end] => (start.clone(), end.clone(), Value::Int(1)),
        [start, end, step] => (start.clone(), end.clone(), step.clone()),
        _ => unreachable!(),
    };
    for v in [&start, &end, &step] {
        if !matches!(v, Value::Int(_) | Value::Float(_)) {
            return Err(expects("range", "numbers", v));
        }
    }
    if let (Value::Int(start), Value::Int(end), Value::Int(step)) = (&start, &end, &step) {
        if *step == 0 {
            return Err(Error::new("arithmetic", "range step must not be zero"));
        }
        let mut out = Vector::new();
        let mut i = *start;
        while (*step > 0 && i < *end) || (*step < 0 && i > *end) {
            out.push_mut(Value::Int(i));
            i += step;
        }
        return Ok(Value::Vector(Rc::new(out)));
    }
    let as_float = |v: &Value| match v {
        Value::Int(i) => *i as f64,
        Value::Float(x) => *x,
        _ => unreachable!(),
    };
    let (start, end, step) = (as_float(&start), as_float(&end), as_float(&step));
    if step == 0.0 {
        return Err(Error::new("arithmetic", "range step must not be zero"));
    }
    let n = ((end - start) / step).ceil().max(0.0) as usize;
    Ok(Value::vector((0..n).map(|k| Value::Float(start + k as f64 * step))))
}

fn reverse(args: &[Value]) -> Result<Value, Error> {
    match &args[0] {
        Value::Vector(v) => Ok(Value::vector((0..v.len()).rev().map(|i| v.get(i).unwrap().clone()))),
        Value::Array(a) => Ok(like(&args[0], a.iter().collect::<Vec<_>>().into_iter().rev().collect())),
        other => Ok(Value::List(items("reverse", other)?.fold(List::new(), List::cons))),
    }
}

pub fn like(template: &Value, out: Vec<Value>) -> Value {
    match template {
        Value::List(_) => Value::list(out),
        Value::Array(_) => match Array::from_values(out.iter().cloned()) {
            Ok(a) => Value::Array(Rc::new(a)),
            Err(_) => Value::vector(out),
        },
        _ => Value::vector(out),
    }
}

fn indexed_len(name: &str, coll: &Value) -> Result<usize, Error> {
    match coll {
        Value::Array(a) => Ok(a.len()),
        Value::Vector(v) => Ok(v.len()),
        other => Err(expects(name, "an array or vector", other)),
    }
}

fn element_at(coll: &Value, index: usize) -> Value {
    match coll {
        Value::Array(a) => a.get(index).unwrap_or(Value::Nil),
        Value::Vector(v) => v.get(index).cloned().unwrap_or(Value::Nil),
        _ => Value::Nil,
    }
}

pub fn select_mask(coll: &Value, mask: &Value) -> Result<Value, Error> {
    let flags: Vec<Value> = items("where", mask)?.collect();
    let values: Vec<Value> = items("where", coll)?.collect();
    if flags.len() != values.len() {
        let message = format!("mask length {} does not match {} length {}", flags.len(), coll.type_name(), values.len());
        return Err(Error::new("value-error", message));
    }
    let out = values.into_iter().zip(flags).filter(|(_, flag)| flag.is_truthy()).map(|(v, _)| v).collect();
    Ok(like(coll, out))
}

pub fn gather(coll: &Value, key: &Value) -> Result<Option<Value>, Error> {
    if !matches!(coll, Value::Array(_) | Value::Vector(_)) {
        return Ok(None);
    }
    let indices: Vec<Value> = match key {
        Value::Array(a) if a.kind() == "bool" => return select_mask(coll, key).map(Some),
        Value::Array(a) => a.iter().collect(),
        Value::Vector(v) => v.iter().cloned().collect(),
        _ => return Ok(None),
    };
    let len = indexed_len("index", coll)?;
    let mut out = Vec::with_capacity(indices.len());
    for index in indices {
        match index {
            Value::Nil => out.push(Value::Nil),
            Value::Int(i) => match usize::try_from(i).ok().filter(|&i| i < len) {
                Some(i) => out.push(element_at(coll, i)),
                None => return Err(out_of_bounds(i, coll.type_name(), len)),
            },
            other => return Err(expects("index", "integers", &other)),
        }
    }
    Ok(Some(like(coll, out)))
}

fn drop_missing(args: &[Value]) -> Result<Value, Error> {
    let kept = items("drop-missing", &args[0])?.filter(|v| !matches!(v, Value::Nil)).collect();
    Ok(like(&args[0], kept))
}

fn fill_missing(args: &[Value]) -> Result<Value, Error> {
    let filled = items("fill-missing", &args[0])?
        .map(|v| if matches!(v, Value::Nil) { args[1].clone() } else { v })
        .collect();
    Ok(like(&args[0], filled))
}

fn count_arg(name: &str, value: &Value) -> Result<usize, Error> {
    match value {
        Value::Int(n) if *n >= 0 => Ok(*n as usize),
        other => Err(expects(name, "a non-negative count", other)),
    }
}

fn take_drop(name: &str, args: &[Value], take: bool) -> Result<Value, Error> {
    let n = count_arg(name, &args[0])?;
    let iter = items(name, &args[1])?;
    let out: Vec<Value> = if take { iter.take(n).collect() } else { iter.skip(n).collect() };
    Ok(like(&args[1], out))
}

fn concat(args: &[Value]) -> Result<Value, Error> {
    if let Some(Value::Table(t)) = args.first() {
        return append(t, &args[1..]);
    }
    let mut out = Vec::new();
    for arg in args {
        out.extend(items("concat", arg)?);
    }
    Ok(like(args.first().unwrap_or(&Value::Nil), out))
}

fn into(args: &mut [Value]) -> Result<Value, Error> {
    let source = take(args, 1);
    let mut conj_args = vec![take(args, 0)];
    conj_args.extend(items("into", &source)?);
    conj_owned(&mut conj_args)
}

fn distinct(args: &[Value]) -> Result<Value, Error> {
    if let Value::Table(t) = &args[0] {
        return distinct_rows(t, args.get(1));
    }
    if args.len() == 2 {
        return Err(expects("distinct", "a table when given column names", &args[0]));
    }
    let mut seen = Set::new();
    let mut out = Vec::new();
    for x in items("distinct", &args[0])? {
        if !seen.contains(&x) {
            seen.insert_mut(x.clone());
            out.push(x);
        }
    }
    Ok(like(&args[0], out))
}

fn frequencies(args: &[Value]) -> Result<Value, Error> {
    let mut counts = Map::new();
    for x in items("frequencies", &args[0])? {
        let n = match counts.get(&x) {
            Some(Value::Int(n)) => n + 1,
            _ => 1,
        };
        counts.insert_mut(x, Value::Int(n));
    }
    Ok(Value::Map(Rc::new(counts)))
}

fn partition(args: &[Value]) -> Result<Value, Error> {
    let n = count_arg("partition", &args[0])?;
    let (step, coll) = match args.len() {
        3 => (count_arg("partition", &args[1])?, &args[2]),
        _ => (n, &args[1]),
    };
    if n == 0 || step == 0 {
        return Err(Error::new("arithmetic", "partition size and step must be positive"));
    }
    let all: Vec<Value> = items("partition", coll)?.collect();
    let mut out = Vec::new();
    let mut start = 0;
    while start + n <= all.len() {
        out.push(Value::vector(all[start..start + n].iter().cloned()));
        start += step;
    }
    Ok(like(coll, out))
}

fn interleave(args: &[Value]) -> Result<Value, Error> {
    let mut iters = args.iter().map(|c| items("interleave", c)).collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::new();
    if iters.is_empty() {
        return Ok(Value::vector(out));
    }
    loop {
        let mut round = Vec::with_capacity(iters.len());
        for iter in iters.iter_mut() {
            match iter.next() {
                Some(v) => round.push(v),
                None => return Ok(Value::vector(out)),
            }
        }
        out.extend(round);
    }
}

fn flatten_into(value: Value, out: &mut Vec<Value>) {
    match value {
        Value::Vector(_) | Value::List(_) => seq_items(&value).unwrap().for_each(|v| flatten_into(v, out)),
        other => out.push(other),
    }
}

fn flatten(args: &[Value]) -> Result<Value, Error> {
    let mut out = Vec::new();
    for x in items("flatten", &args[0])? {
        flatten_into(x, &mut out);
    }
    Ok(Value::vector(out))
}

fn char_at(s: &str, index: usize) -> Option<Value> {
    s.chars().nth(index).map(|c| Value::string(c.encode_utf8(&mut [0; 4])))
}

pub fn get(args: &[Value]) -> Result<Value, Error> {
    if let Some(found) = gather(&args[0], &args[1])? {
        return Ok(found);
    }
    let index = |i: &i64| usize::try_from(*i).ok();
    let found = match (&args[0], &args[1]) {
        (Value::Map(m), key) => m.get(key).cloned(),
        (Value::Set(s), key) => s.contains(key).then(|| key.clone()),
        (Value::Table(t), Value::Keyword(k)) => t.column(k.root()).map(|c| Value::Array(c.clone())),
        (Value::Vector(v), Value::Int(i)) => index(i).and_then(|i| v.get(i)).cloned(),
        (Value::Array(a), Value::Int(i)) => index(i).and_then(|i| a.get(i)),
        (Value::Str(s), Value::Int(i)) => index(i).and_then(|i| char_at(s, i)),
        _ => None,
    };
    Ok(found.or_else(|| args.get(2).cloned()).unwrap_or(Value::Nil))
}

fn find(args: &[Value]) -> Result<Value, Error> {
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Map(m) => Ok(match m.get(&args[1]) {
            Some(v) => Value::vector([args[1].clone(), v.clone()]),
            None => Value::Nil,
        }),
        other => Err(expects("find", "a map", other)),
    }
}

pub fn assoc(args: &[Value]) -> Result<Value, Error> {
    assoc_owned(&mut args.to_vec())
}

fn assoc_owned(args: &mut [Value]) -> Result<Value, Error> {
    if (args.len() - 1) % 2 != 0 {
        let message = format!("assoc expects an even number of key and value arguments, got {}", args.len() - 1);
        return Err(Error::new("arity-error", message));
    }
    let target = take(args, 0);
    let mut pairs = args[1..].iter_mut().map(std::mem::take);
    match target {
        Value::Table(t) => {
            let mut out = owned(t);
            while let (Some(k), Some(v)) = (pairs.next(), pairs.next()) {
                let name = column_name("assoc", &k)?;
                let nrows = if out.ncols() > 0 { Some(out.nrows()) } else { None };
                out.set_column(name, column_from(&v, nrows)?)?;
            }
            Ok(Value::Table(Rc::new(out)))
        }
        Value::Nil | Value::Map(_) => {
            let mut out = match target {
                Value::Map(m) => owned(m),
                _ => Map::new(),
            };
            while let (Some(k), Some(v)) = (pairs.next(), pairs.next()) {
                out.insert_mut(k, v);
            }
            Ok(Value::Map(Rc::new(out)))
        }
        Value::Vector(v) => {
            let mut out = owned(v);
            while let (Some(k), Some(v)) = (pairs.next(), pairs.next()) {
                let Value::Int(i) = k else {
                    return Err(expects("assoc on a vector", "an integer index", &k));
                };
                let len = out.len();
                match usize::try_from(i).ok().filter(|&i| i <= len) {
                    Some(i) if i == len => out.push_mut(v),
                    Some(i) => out.set_mut(i, v),
                    None => return Err(out_of_bounds(i, "vector", len)),
                }
            }
            Ok(Value::Vector(Rc::new(out)))
        }
        other => Err(expects("assoc", "a map, vector, or table", &other)),
    }
}

fn dissoc(args: &mut [Value]) -> Result<Value, Error> {
    match take(args, 0) {
        Value::Nil => Ok(Value::Nil),
        Value::Table(t) => {
            let mut out = (*t).clone();
            for k in &args[1..] {
                out = out.without(column_name("dissoc", k)?);
            }
            Ok(Value::Table(Rc::new(out)))
        }
        Value::Map(m) => {
            let mut out = owned(m);
            args[1..].iter().for_each(|k| {
                out.remove_mut(k);
            });
            Ok(Value::Map(Rc::new(out)))
        }
        other => Err(expects("dissoc", "a map", &other)),
    }
}

fn map_part(name: &str, value: &Value, pick: fn(&Map) -> Vec<Value>) -> Result<Value, Error> {
    match value {
        Value::Nil => Ok(Value::vector([])),
        Value::Map(m) => Ok(Value::vector(pick(m))),
        other => Err(expects(name, "a map", other)),
    }
}

fn contains(args: &[Value]) -> Result<Value, Error> {
    let in_range = |i: &i64, len: usize| usize::try_from(*i).is_ok_and(|i| i < len);
    let found = match (&args[0], &args[1]) {
        (Value::Nil, _) => false,
        (Value::Map(m), key) => m.contains_key(key),
        (Value::Set(s), key) => s.contains(key),
        (Value::Vector(v), Value::Int(i)) => in_range(i, v.len()),
        (Value::Str(s), Value::Int(i)) => in_range(i, s.chars().count()),
        (Value::Vector(_) | Value::Str(_), _) => false,
        (other, _) => return Err(expects("contains?", "a collection", other)),
    };
    Ok(Value::Bool(found))
}

fn merge(args: &[Value]) -> Result<Value, Error> {
    let mut out = Map::new();
    for arg in args {
        match arg {
            Value::Nil => {}
            Value::Map(m) => m.iter().for_each(|(k, v)| out.insert_mut(k.clone(), v.clone())),
            other => return Err(expects("merge", "maps", other)),
        }
    }
    Ok(Value::Map(Rc::new(out)))
}

fn disj(args: &mut [Value]) -> Result<Value, Error> {
    match take(args, 0) {
        Value::Nil => Ok(Value::Nil),
        Value::Set(s) => {
            let mut out = owned(s);
            args[1..].iter().for_each(|x| {
                out.remove_mut(x);
            });
            Ok(Value::Set(Rc::new(out)))
        }
        other => Err(expects("disj", "a set", &other)),
    }
}

fn select_keys(args: &[Value]) -> Result<Value, Error> {
    let source = match &args[0] {
        Value::Nil => Map::new(),
        Value::Map(m) => (**m).clone(),
        other => return Err(expects("select-keys", "a map", other)),
    };
    let mut out = Map::new();
    for key in items("select-keys", &args[1])? {
        if let Some(v) = source.get(&key) {
            out.insert_mut(key, v.clone());
        }
    }
    Ok(Value::Map(Rc::new(out)))
}

fn zipmap(args: &[Value]) -> Result<Value, Error> {
    let keys = items("zipmap", &args[0])?;
    let vals = items("zipmap", &args[1])?;
    Ok(Value::map(keys.zip(vals)))
}

fn subvec(args: &[Value]) -> Result<Value, Error> {
    let Value::Vector(v) = &args[0] else {
        return Err(expects("subvec", "a vector", &args[0]));
    };
    let bound = |value: Option<&Value>, default: i64| match value {
        None => Ok(default),
        Some(Value::Int(i)) => Ok(*i),
        Some(other) => Err(expects("subvec", "integer bounds", other)),
    };
    let start = bound(args.get(1), 0)?;
    let end = bound(args.get(2), v.len() as i64)?;
    if start < 0 || end < start || end > v.len() as i64 {
        let message = format!("subvec range {start} to {end} is out of bounds for a vector of length {}", v.len());
        return Err(Error::new("index-error", message));
    }
    let (start, end) = (start as usize, end as usize);
    Ok(Value::vector(v.iter().skip(start).take(end - start).cloned()))
}
