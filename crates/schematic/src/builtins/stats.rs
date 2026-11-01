use std::cmp::Ordering;

use super::collections::{count_arg, expects, items, like};
use super::functional::{compare_values, merge_sort};
use super::math::binary;
use super::{native, native_vm};
use crate::array::{Array, Data};
use crate::bytecode::Intrinsic;
use crate::error::Error;
use crate::value::Value;
use crate::vm::{Arity, Globals, Vm};

pub fn install(globals: &mut Globals) {
    native(globals, "sum", Arity::Exact(1), sum);
    native(globals, "mean", Arity::Exact(1), mean);
    native(globals, "median", Arity::Exact(1), median);
    native(globals, "variance", Arity::Exact(1), |args| Ok(variance(&args[0])?.map(Value::Float).unwrap_or(Value::Nil)));
    native(globals, "std", Arity::Exact(1), |args| Ok(variance(&args[0])?.map(|v| Value::Float(v.sqrt())).unwrap_or(Value::Nil)));
    native(globals, "quantile", Arity::Exact(2), quantile);
    native(globals, "running-sum", Arity::Exact(1), |args| running("running-sum", args, |a, b| binary(Intrinsic::Add, a, b)));
    native(globals, "running-max", Arity::Exact(1), |args| running("running-max", args, |a, b| pick("running-max", a, b, Ordering::Greater)));
    native(globals, "running-min", Arity::Exact(1), |args| running("running-min", args, |a, b| pick("running-min", a, b, Ordering::Less)));
    native(globals, "rank", Arity::Exact(1), rank);
    native(globals, "lag", Arity::Between(1, 2), |args| shift("lag", args, true));
    native(globals, "lead", Arity::Between(1, 2), |args| shift("lead", args, false));
    native(globals, "diff", Arity::Exact(1), diff);
    native_vm(globals, "rolling", Arity::Exact(3), rolling);
}

fn numbers(name: &str, value: &Value) -> Result<Vec<Value>, Error> {
    let mut out = Vec::new();
    for x in items(name, value)? {
        match x {
            Value::Nil => {}
            Value::Int(_) | Value::Float(_) => out.push(x),
            other => return Err(expects(name, "numbers", &other)),
        }
    }
    Ok(out)
}

fn as_f64(value: &Value) -> f64 {
    match value {
        Value::Int(i) => *i as f64,
        Value::Float(x) => *x,
        _ => f64::NAN,
    }
}

fn floats(name: &str, value: &Value) -> Result<Vec<f64>, Error> {
    Ok(numbers(name, value)?.iter().map(as_f64).collect())
}

fn typed_sum(array: &Array) -> Option<(Value, usize)> {
    let present = |i: usize| !array.missing_mask().is_some_and(|m| m[i]);
    match array.data() {
        Data::Float(v) => {
            let mut total = 0.0;
            let mut count = 0;
            for (i, x) in v.iter().enumerate() {
                if present(i) {
                    total += x;
                    count += 1;
                }
            }
            Some((if count == 0 { Value::Int(0) } else { Value::Float(total) }, count))
        }
        Data::Int(v) => {
            let mut total: i64 = 0;
            let mut count = 0;
            for (i, x) in v.iter().enumerate() {
                if present(i) {
                    total = total.checked_add(*x)?;
                    count += 1;
                }
            }
            Some((Value::Int(total), count))
        }
        _ => None,
    }
}

fn sum(args: &[Value]) -> Result<Value, Error> {
    if let Value::Array(a) = &args[0]
        && let Some((total, _)) = typed_sum(a) {
            return Ok(total);
        }
    numbers("sum", &args[0])?.iter().try_fold(Value::Int(0), |acc, x| binary(Intrinsic::Add, &acc, x))
}

fn mean(args: &[Value]) -> Result<Value, Error> {
    if let Value::Array(a) = &args[0]
        && let Some((total, count)) = typed_sum(a) {
            return Ok(if count == 0 { Value::Nil } else { Value::Float(as_f64(&total) / count as f64) });
        }
    let xs = floats("mean", &args[0])?;
    if xs.is_empty() {
        return Ok(Value::Nil);
    }
    Ok(Value::Float(xs.iter().sum::<f64>() / xs.len() as f64))
}

fn median(args: &[Value]) -> Result<Value, Error> {
    let mut xs = numbers("median", &args[0])?;
    xs.sort_by(|a, b| compare_values("median", a, b).unwrap_or(Ordering::Equal));
    let n = xs.len();
    Ok(match n {
        0 => Value::Nil,
        _ if n % 2 == 1 => xs[n / 2].clone(),
        _ => Value::Float((as_f64(&xs[n / 2 - 1]) + as_f64(&xs[n / 2])) / 2.0),
    })
}

fn variance(value: &Value) -> Result<Option<f64>, Error> {
    let xs = floats("variance", value)?;
    if xs.len() < 2 {
        return Ok(None);
    }
    let mean = xs.iter().sum::<f64>() / xs.len() as f64;
    let squares: f64 = xs.iter().map(|x| (x - mean) * (x - mean)).sum();
    Ok(Some(squares / (xs.len() - 1) as f64))
}

fn pick(name: &str, acc: &Value, x: &Value, wanted: Ordering) -> Result<Value, Error> {
    Ok(if compare_values(name, x, acc)? == wanted { x.clone() } else { acc.clone() })
}

fn running(name: &str, args: &[Value], step: impl Fn(&Value, &Value) -> Result<Value, Error>) -> Result<Value, Error> {
    let mut running: Option<Value> = None;
    let mut out = Vec::new();
    for x in items(name, &args[0])? {
        match x {
            Value::Nil => out.push(Value::Nil),
            Value::Int(_) | Value::Float(_) => {
                let next = match &running {
                    None => x,
                    Some(acc) => step(acc, &x)?,
                };
                out.push(next.clone());
                running = Some(next);
            }
            other => return Err(expects(name, "numbers", &other)),
        }
    }
    Ok(like(&args[0], out))
}

fn rank(args: &[Value]) -> Result<Value, Error> {
    let values: Vec<Value> = items("rank", &args[0])?.collect();
    let present: Vec<usize> = (0..values.len()).filter(|&i| !matches!(values[i], Value::Nil)).collect();
    let order = merge_sort(present, &mut |&a, &b| {
        Ok(compare_values("rank", &values[a], &values[b])? != Ordering::Greater)
    })?;
    let mut ranks = vec![Value::Nil; values.len()];
    for (position, &i) in order.iter().enumerate() {
        let previous = position.checked_sub(1).map(|p| order[p]);
        ranks[i] = match previous {
            Some(p) if values[p] == values[i] => ranks[p].clone(),
            _ => Value::Int(position as i64 + 1),
        };
    }
    Ok(like(&args[0], ranks))
}

fn shift(name: &str, args: &[Value], backward: bool) -> Result<Value, Error> {
    let values: Vec<Value> = items(name, &args[0])?.collect();
    let n = match args.get(1) {
        None => 1,
        Some(v) => count_arg(name, v)?,
    };
    let len = values.len();
    let out = (0..len)
        .map(|i| {
            let source = if backward { i.checked_sub(n) } else { Some(i + n).filter(|&j| j < len) };
            source.map_or(Value::Nil, |j| values[j].clone())
        })
        .collect();
    Ok(like(&args[0], out))
}

fn diff(args: &[Value]) -> Result<Value, Error> {
    let values: Vec<Value> = items("diff", &args[0])?.collect();
    let mut out = Vec::with_capacity(values.len());
    for (i, x) in values.iter().enumerate() {
        let previous = if i == 0 { &Value::Nil } else { &values[i - 1] };
        out.push(match (previous, x) {
            (Value::Nil, _) | (_, Value::Nil) => Value::Nil,
            _ => binary(Intrinsic::Sub, x, previous)?,
        });
    }
    Ok(like(&args[0], out))
}

fn rolling(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let n = match &args[0] {
        Value::Int(n) if *n > 0 => *n as usize,
        other => return Err(expects("rolling", "a positive window size", other)),
    };
    let values: Vec<Value> = items("rolling", &args[2])?.collect();
    let mut out = Vec::with_capacity(values.len());
    for i in 0..values.len() {
        if i + 1 < n {
            out.push(Value::Nil);
            continue;
        }
        let window = like(&args[2], values[i + 1 - n..=i].to_vec());
        out.push(vm.call_value(&args[1], &[window])?);
    }
    Ok(like(&args[2], out))
}

fn quantile(args: &[Value]) -> Result<Value, Error> {
    let q = match &args[1] {
        Value::Int(_) | Value::Float(_) => as_f64(&args[1]),
        other => return Err(expects("quantile", "a fraction between 0 and 1", other)),
    };
    if !(0.0..=1.0).contains(&q) {
        return Err(expects("quantile", "a fraction between 0 and 1", &args[1]));
    }
    let mut xs = floats("quantile", &args[0])?;
    if xs.is_empty() {
        return Ok(Value::Nil);
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let position = q * (xs.len() - 1) as f64;
    let (lo, hi) = (position.floor() as usize, position.ceil() as usize);
    let fraction = position - lo as f64;
    Ok(Value::Float(xs[lo] + (xs[hi] - xs[lo]) * fraction))
}
