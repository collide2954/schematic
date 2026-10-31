use super::collections::{expects, items, seq_items};
use super::native_vm;
use crate::error::Error;
use crate::value::Value;
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
        acc = vm.call_value(f, &[acc, x])?;
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
