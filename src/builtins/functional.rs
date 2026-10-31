use std::cmp::Ordering;
use std::rc::Rc;

use super::collections::{assoc, expects, get, items, seq_items};
use super::{native, native_vm};
use crate::error::Error;
use crate::map::Map;
use crate::value::Value;
use crate::vector::Vector;
use crate::vm::{Arity, Globals, Vm};

pub fn install(globals: &mut Globals) {
    native_vm(globals, "apply", Arity::AtLeast(2), apply);
    native_vm(globals, "map", Arity::AtLeast(2), map);
    native_vm(globals, "filter", Arity::Exact(2), |vm, args| filter(vm, args, true));
    native_vm(globals, "remove", Arity::Exact(2), |vm, args| filter(vm, args, false));
    native_vm(globals, "reduce", Arity::Between(2, 3), reduce);
    native_vm(globals, "for-each", Arity::Exact(2), for_each);
    native_vm(globals, "some", Arity::Exact(2), some);
    native_vm(globals, "every?", Arity::Exact(2), every);
    native_vm(globals, "sort", Arity::Between(1, 2), sort);
    native_vm(globals, "sort-by", Arity::Between(2, 3), sort_by);
    native(globals, "compare", Arity::Exact(2), |args| {
        Ok(Value::Int(compare_values("compare", &args[0], &args[1])? as i64))
    });
    native_vm(globals, "update", Arity::AtLeast(3), update);
    native(globals, "get-in", Arity::Between(2, 3), get_in);
    native(globals, "assoc-in", Arity::Exact(3), assoc_in);
    native_vm(globals, "update-in", Arity::AtLeast(3), update_in);
    native_vm(globals, "take-while", Arity::Exact(2), |vm, args| take_drop_while(vm, args, true));
    native_vm(globals, "drop-while", Arity::Exact(2), |vm, args| take_drop_while(vm, args, false));
    native_vm(globals, "group-by", Arity::Exact(2), group_by);
}

pub fn compare_values(name: &str, a: &Value, b: &Value) -> Result<Ordering, Error> {
    use Value::*;
    let as_float = |v: &Value| match v {
        Int(i) => *i as f64,
        Float(x) => *x,
        _ => unreachable!(),
    };
    let ordering = match (a, b) {
        (Nil, Nil) => Some(Ordering::Equal),
        (Nil, _) => Some(Ordering::Less),
        (_, Nil) => Some(Ordering::Greater),
        (Bool(x), Bool(y)) => Some(x.cmp(y)),
        (Int(x), Int(y)) => Some(x.cmp(y)),
        (Int(_) | Float(_), Int(_) | Float(_)) => Some(as_float(a).partial_cmp(&as_float(b)).unwrap_or(Ordering::Equal)),
        (Str(x), Str(y)) => Some(x.cmp(y)),
        (Keyword(x), Keyword(y)) | (Symbol(x), Symbol(y)) => Some(x.name().cmp(&y.name())),
        (Vector(_), Vector(_)) | (List(_), List(_)) => {
            let (mut xs, mut ys) = (seq_items(a).unwrap(), seq_items(b).unwrap());
            loop {
                match (xs.next(), ys.next()) {
                    (None, None) => break Some(Ordering::Equal),
                    (None, Some(_)) => break Some(Ordering::Less),
                    (Some(_), None) => break Some(Ordering::Greater),
                    (Some(x), Some(y)) => match compare_values(name, &x, &y)? {
                        Ordering::Equal => continue,
                        other => break Some(other),
                    },
                }
            }
        }
        _ => None,
    };
    ordering.ok_or_else(|| Error::type_error(format!("{name} cannot compare {a} and {b}")))
}

fn merge_sort<T>(items: Vec<T>, keep_left: &mut impl FnMut(&T, &T) -> Result<bool, Error>) -> Result<Vec<T>, Error> {
    if items.len() <= 1 {
        return Ok(items);
    }
    let mut right = items;
    let left: Vec<T> = right.drain(..right.len() / 2).collect();
    let left = merge_sort(left, keep_left)?;
    let right = merge_sort(right, keep_left)?;
    let mut out = Vec::with_capacity(left.len() + right.len());
    let mut left = left.into_iter().peekable();
    let mut right = right.into_iter().peekable();
    while let (Some(a), Some(b)) = (left.peek(), right.peek()) {
        if keep_left(a, b)? {
            out.push(left.next().unwrap());
        } else {
            out.push(right.next().unwrap());
        }
    }
    out.extend(left);
    out.extend(right);
    Ok(out)
}

fn before(verdict: Value) -> bool {
    match verdict {
        Value::Int(n) => n < 0,
        Value::Float(x) => x < 0.0,
        other => other.is_truthy(),
    }
}

fn sort(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let (comparator, coll) = match args.len() {
        2 => (Some(&args[0]), &args[1]),
        _ => (None, &args[0]),
    };
    let unsorted: Vec<Value> = items("sort", coll)?.collect();
    let sorted = match comparator {
        None => merge_sort(unsorted, &mut |a, b| Ok(compare_values("sort", a, b)? != Ordering::Greater))?,
        Some(f) => merge_sort(unsorted, &mut |a, b| Ok(!before(vm.call_value(f, &[b.clone(), a.clone()])?)))?,
    };
    Ok(like(coll, sorted))
}

fn sort_by(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let (key_fn, comparator, coll) = match args.len() {
        3 => (&args[0], Some(&args[1]), &args[2]),
        _ => (&args[0], None, &args[1]),
    };
    let mut keyed = Vec::new();
    for x in items("sort-by", coll)? {
        keyed.push((vm.call_value(key_fn, std::slice::from_ref(&x))?, x));
    }
    let sorted = match comparator {
        None => merge_sort(keyed, &mut |a, b| Ok(compare_values("sort-by", &a.0, &b.0)? != Ordering::Greater))?,
        Some(f) => merge_sort(keyed, &mut |a, b| Ok(!before(vm.call_value(f, &[b.0.clone(), a.0.clone()])?)))?,
    };
    Ok(like(coll, sorted.into_iter().map(|(_, x)| x).collect()))
}

fn take_drop_while(vm: &mut Vm, args: Vec<Value>, take: bool) -> Result<Value, Error> {
    let name = if take { "take-while" } else { "drop-while" };
    let mut out = Vec::new();
    let mut dropping = true;
    for x in items(name, &args[1])? {
        let holds = dropping && vm.call_value(&args[0], std::slice::from_ref(&x))?.is_truthy();
        if take {
            if !holds {
                break;
            }
            out.push(x);
        } else {
            dropping = holds;
            if !holds {
                out.push(x);
            }
        }
    }
    Ok(like(&args[1], out))
}

fn group_by(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let mut groups = Map::new();
    for x in items("group-by", &args[1])? {
        let key = vm.call_value(&args[0], std::slice::from_ref(&x))?;
        let group = match groups.get(&key) {
            Some(Value::Vector(v)) => v.push(x),
            _ => Vector::new().push(x),
        };
        groups.insert_mut(key, Value::Vector(Rc::new(group)));
    }
    Ok(Value::Map(Rc::new(groups)))
}

fn update(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let current = get(&args[..2])?;
    let mut call_args = vec![current];
    call_args.extend(args[3..].iter().cloned());
    let updated = vm.call_value(&args[2], &call_args)?;
    assoc(&[args[0].clone(), args[1].clone(), updated])
}

fn path(name: &str, value: &Value) -> Result<Vec<Value>, Error> {
    Ok(items(name, value)?.collect())
}

fn get_in(args: &[Value]) -> Result<Value, Error> {
    let mut current = args[0].clone();
    for key in path("get-in", &args[1])? {
        current = get(&[current, key])?;
    }
    match (current, args.get(2)) {
        (Value::Nil, Some(default)) => Ok(default.clone()),
        (found, _) => Ok(found),
    }
}

fn assoc_path(coll: Value, keys: &[Value], value: Value) -> Result<Value, Error> {
    match keys {
        [] => Err(Error::syntax("assoc-in expects at least one key")),
        [key] => assoc(&[coll, key.clone(), value]),
        [key, rest @ ..] => {
            let inner = get(&[coll.clone(), key.clone()])?;
            let updated = assoc_path(inner, rest, value)?;
            assoc(&[coll, key.clone(), updated])
        }
    }
}

fn assoc_in(args: &[Value]) -> Result<Value, Error> {
    assoc_path(args[0].clone(), &path("assoc-in", &args[1])?, args[2].clone())
}

fn update_in(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let keys = path("update-in", &args[1])?;
    let current = get_in(&args[..2])?;
    let mut call_args = vec![current];
    call_args.extend(args[3..].iter().cloned());
    let updated = vm.call_value(&args[2], &call_args)?;
    assoc_path(args[0].clone(), &keys, updated)
}

pub fn like(template: &Value, out: Vec<Value>) -> Value {
    match template {
        Value::List(_) => Value::list(out),
        _ => Value::vector(out),
    }
}

fn apply(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let (f, rest) = args.split_first().unwrap();
    let (last, middle) = rest.split_last().unwrap();
    let spread = seq_items(last).ok_or_else(|| expects("apply", "a collection as its last argument", last))?;
    let all: Vec<Value> = middle.iter().cloned().chain(spread).collect();
    vm.call_value(f, &all)
}

fn map(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let f = &args[0];
    let colls = &args[1..];
    let mut iters = colls.iter().map(|c| items("map", c)).collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::new();
    loop {
        let mut call_args = Vec::with_capacity(iters.len());
        for iter in iters.iter_mut() {
            match iter.next() {
                Some(v) => call_args.push(v),
                None => return Ok(like(&colls[0], out)),
            }
        }
        out.push(vm.call_value(f, &call_args)?);
    }
}

fn filter(vm: &mut Vm, args: Vec<Value>, keep: bool) -> Result<Value, Error> {
    let name = if keep { "filter" } else { "remove" };
    let mut out = Vec::new();
    for x in items(name, &args[1])? {
        if vm.call_value(&args[0], std::slice::from_ref(&x))?.is_truthy() == keep {
            out.push(x);
        }
    }
    Ok(like(&args[1], out))
}

fn reduce(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let f = &args[0];
    let (init, coll) = match args.len() {
        3 => (Some(args[1].clone()), &args[2]),
        _ => (None, &args[1]),
    };
    let mut iter = items("reduce", coll)?;
    let mut acc = match init.or_else(|| iter.next()) {
        Some(v) => v,
        None => return vm.call_value(f, &[]),
    };
    for x in iter {
        acc = vm.call_values(f, vec![acc, x])?;
    }
    Ok(acc)
}

fn for_each(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    for x in items("for-each", &args[1])? {
        vm.call_value(&args[0], &[x])?;
    }
    Ok(Value::Nil)
}

fn some(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    for x in items("some", &args[1])? {
        let result = vm.call_value(&args[0], &[x])?;
        if result.is_truthy() {
            return Ok(result);
        }
    }
    Ok(Value::Nil)
}

fn every(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    for x in items("every?", &args[1])? {
        if !vm.call_value(&args[0], &[x])?.is_truthy() {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}
