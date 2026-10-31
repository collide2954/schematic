use std::rc::Rc;

use super::native;
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
    native(globals, "conj", Arity::AtLeast(1), conj);
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
    native(globals, "assoc", Arity::AtLeast(1), assoc);
    native(globals, "dissoc", Arity::AtLeast(1), dissoc);
    native(globals, "keys", Arity::Exact(1), |args| map_part("keys", &args[0], |m| m.keys().cloned().collect()));
    native(globals, "vals", Arity::Exact(1), |args| map_part("vals", &args[0], |m| m.values().cloned().collect()));
    native(globals, "contains?", Arity::Exact(2), contains);
    native(globals, "merge", Arity::AtLeast(0), merge);
    native(globals, "disj", Arity::AtLeast(1), disj);
    native(globals, "select-keys", Arity::Exact(2), select_keys);
    native(globals, "zipmap", Arity::Exact(2), zipmap);
    native(globals, "subvec", Arity::Between(2, 3), subvec);
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
        Ok(Value::Bool(matches!(args[0], Value::List(_) | Value::Vector(_) | Value::Map(_) | Value::Set(_))))
    });
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
        Value::Str(s) => Box::new(s.chars().map(|c| Value::string(c.encode_utf8(&mut [0; 4])))),
        _ => return None,
    })
}

pub fn items<'a>(name: &str, value: &'a Value) -> Result<Box<dyn Iterator<Item = Value> + 'a>, Error> {
    seq_items(value).ok_or_else(|| expects(name, "a collection", value))
}

fn out_of_bounds(index: i64, kind: &str, len: usize) -> Error {
    Error::new("index-error", format!("index {index} is out of bounds for a {kind} of length {len}"))
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
        Value::List(l) => (position.and_then(|i| l.iter().nth(i)).cloned(), "list", l.len()),
        Value::Str(s) => (
            position.and_then(|i| s.chars().nth(i)).map(|c| Value::string(c.encode_utf8(&mut [0; 4]))),
            "string",
            s.chars().count(),
        ),
        other => return Err(expects("nth", "a list, vector, or string", other)),
    };
    match (found, args.get(2)) {
        (Some(v), _) => Ok(v),
        (None, Some(default)) => Ok(default.clone()),
        (None, None) => Err(out_of_bounds(*index, kind, len)),
    }
}

fn conj(args: &[Value]) -> Result<Value, Error> {
    let (target, additions) = args.split_first().unwrap();
    match target {
        Value::Nil => Ok(Value::List(additions.iter().cloned().fold(List::new(), List::cons))),
        Value::List(l) => Ok(Value::List(additions.iter().cloned().fold(l.clone(), List::cons))),
        Value::Vector(v) => {
            let mut out: Vector = (**v).clone();
            additions.iter().cloned().for_each(|x| out.push_mut(x));
            Ok(Value::Vector(Rc::new(out)))
        }
        Value::Set(s) => {
            let mut out: Set = (**s).clone();
            additions.iter().cloned().for_each(|x| out.insert_mut(x));
            Ok(Value::Set(Rc::new(out)))
        }
        Value::Map(m) => {
            let mut out: Map = (**m).clone();
            for addition in additions {
                match addition {
                    Value::Vector(pair) if pair.len() == 2 => {
                        out.insert_mut(pair.get(0).unwrap().clone(), pair.get(1).unwrap().clone());
                    }
                    Value::Map(other) => other.iter().for_each(|(k, v)| out.insert_mut(k.clone(), v.clone())),
                    other => return Err(expects("conj onto a map", "a [key value] pair or a map", other)),
                }
            }
            Ok(Value::Map(Rc::new(out)))
        }
        other => Err(expects("conj", "a collection", other)),
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
        other => Ok(Value::List(items("reverse", other)?.fold(List::new(), List::cons))),
    }
}

fn char_at(s: &str, index: usize) -> Option<Value> {
    s.chars().nth(index).map(|c| Value::string(c.encode_utf8(&mut [0; 4])))
}

fn get(args: &[Value]) -> Result<Value, Error> {
    let index = |i: &i64| usize::try_from(*i).ok();
    let found = match (&args[0], &args[1]) {
        (Value::Map(m), key) => m.get(key).cloned(),
        (Value::Set(s), key) => s.contains(key).then(|| key.clone()),
        (Value::Vector(v), Value::Int(i)) => index(i).and_then(|i| v.get(i)).cloned(),
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

fn assoc(args: &[Value]) -> Result<Value, Error> {
    let (target, pairs) = args.split_first().unwrap();
    if pairs.len() % 2 != 0 {
        let message = format!("assoc expects an even number of key and value arguments, got {}", pairs.len());
        return Err(Error::new("arity-error", message));
    }
    let pairs = pairs.chunks(2).map(|pair| (&pair[0], &pair[1]));
    match target {
        Value::Nil | Value::Map(_) => {
            let mut out = match target {
                Value::Map(m) => (**m).clone(),
                _ => Map::new(),
            };
            pairs.for_each(|(k, v)| out.insert_mut(k.clone(), v.clone()));
            Ok(Value::Map(Rc::new(out)))
        }
        Value::Vector(v) => {
            let mut out = (**v).clone();
            for (k, v) in pairs {
                let Value::Int(i) = k else {
                    return Err(expects("assoc on a vector", "an integer index", k));
                };
                out = usize::try_from(*i)
                    .ok()
                    .and_then(|i| out.assoc(i, v.clone()))
                    .ok_or_else(|| out_of_bounds(*i, "vector", out.len()))?;
            }
            Ok(Value::Vector(Rc::new(out)))
        }
        other => Err(expects("assoc", "a map or vector", other)),
    }
}

fn dissoc(args: &[Value]) -> Result<Value, Error> {
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Map(m) => {
            let mut out = (**m).clone();
            args[1..].iter().for_each(|k| {
                out.remove_mut(k);
            });
            Ok(Value::Map(Rc::new(out)))
        }
        other => Err(expects("dissoc", "a map", other)),
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

fn disj(args: &[Value]) -> Result<Value, Error> {
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Set(s) => {
            let mut out = (**s).clone();
            args[1..].iter().for_each(|x| {
                out.remove_mut(x);
            });
            Ok(Value::Set(Rc::new(out)))
        }
        other => Err(expects("disj", "a set", other)),
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
