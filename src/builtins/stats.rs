use std::cmp::Ordering;

use super::collections::{expects, items, like};
use super::functional::compare_values;
use super::math::binary;
use super::native;
use crate::bytecode::Intrinsic;
use crate::error::Error;
use crate::value::Value;
use crate::vm::{Arity, Globals};

pub fn install(globals: &mut Globals) {
    native(globals, "sum", Arity::Exact(1), sum);
    native(globals, "mean", Arity::Exact(1), mean);
    native(globals, "median", Arity::Exact(1), median);
    native(globals, "variance", Arity::Exact(1), |args| Ok(variance(&args[0])?.map(Value::Float).unwrap_or(Value::Nil)));
    native(globals, "std", Arity::Exact(1), |args| Ok(variance(&args[0])?.map(|v| Value::Float(v.sqrt())).unwrap_or(Value::Nil)));
    native(globals, "cumsum", Arity::Exact(1), cumsum);
    native(globals, "quantile", Arity::Exact(2), quantile);
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

fn sum(args: &[Value]) -> Result<Value, Error> {
    numbers("sum", &args[0])?.iter().try_fold(Value::Int(0), |acc, x| binary(Intrinsic::Add, &acc, x))
}

fn mean(args: &[Value]) -> Result<Value, Error> {
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

fn cumsum(args: &[Value]) -> Result<Value, Error> {
    let mut running: Option<Value> = None;
    let mut out = Vec::new();
    for x in items("cumsum", &args[0])? {
        match x {
            Value::Nil => out.push(Value::Nil),
            Value::Int(_) | Value::Float(_) => {
                let next = match &running {
                    None => x,
                    Some(acc) => binary(Intrinsic::Add, acc, &x)?,
                };
                out.push(next.clone());
                running = Some(next);
            }
            other => return Err(expects("cumsum", "numbers", &other)),
        }
    }
    Ok(like(&args[0], out))
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
