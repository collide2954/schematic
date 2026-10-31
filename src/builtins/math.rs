use std::cmp::Ordering;

use super::native;
use crate::error::Error;
use crate::value::Value;
use crate::vm::{Arity, Globals};

pub fn install(globals: &mut Globals) {
    native(globals, "+", Arity::AtLeast(0), add);
    native(globals, "-", Arity::AtLeast(1), sub);
    native(globals, "*", Arity::AtLeast(0), mul);
    native(globals, "/", Arity::AtLeast(1), div);
    native(globals, "quot", Arity::Exact(2), quot);
    native(globals, "rem", Arity::Exact(2), rem);
    native(globals, "mod", Arity::Exact(2), modulo);
    native(globals, "<", Arity::AtLeast(1), |args| compare("<", args, |o| o == Ordering::Less));
    native(globals, ">", Arity::AtLeast(1), |args| compare(">", args, |o| o == Ordering::Greater));
    native(globals, "<=", Arity::AtLeast(1), |args| compare("<=", args, |o| o != Ordering::Greater));
    native(globals, ">=", Arity::AtLeast(1), |args| compare(">=", args, |o| o != Ordering::Less));
    native(globals, "=", Arity::AtLeast(1), equal);
    native(globals, "not", Arity::Exact(1), |args| Ok(Value::Bool(!args[0].is_truthy())));
}

fn overflow() -> Error {
    Error::new("arithmetic", "integer overflow")
}

fn division_by_zero() -> Error {
    Error::new("arithmetic", "division by zero")
}

fn not_a_number(name: &str, value: &Value) -> Error {
    Error::type_error(format!("{name} expects numbers, got {value}"))
}

fn as_float(name: &str, value: &Value) -> Result<f64, Error> {
    match value {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(x) => Ok(*x),
        other => Err(not_a_number(name, other)),
    }
}

fn arith(
    name: &str,
    a: &Value,
    b: &Value,
    int_op: fn(i64, i64) -> Option<i64>,
    float_op: fn(f64, f64) -> f64,
) -> Result<Value, Error> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => int_op(*x, *y).map(Value::Int).ok_or_else(overflow),
        (Value::Int(x), Value::Float(y)) => Ok(Value::Float(float_op(*x as f64, *y))),
        (Value::Float(x), Value::Int(y)) => Ok(Value::Float(float_op(*x, *y as f64))),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(float_op(*x, *y))),
        (Value::Int(_) | Value::Float(_), other) | (other, _) => Err(not_a_number(name, other)),
    }
}

fn add(args: &[Value]) -> Result<Value, Error> {
    args.iter().try_fold(Value::Int(0), |acc, x| arith("+", &acc, x, i64::checked_add, |a, b| a + b))
}

fn mul(args: &[Value]) -> Result<Value, Error> {
    args.iter().try_fold(Value::Int(1), |acc, x| arith("*", &acc, x, i64::checked_mul, |a, b| a * b))
}

fn sub(args: &[Value]) -> Result<Value, Error> {
    if args.len() == 1 {
        return arith("-", &Value::Int(0), &args[0], i64::checked_sub, |a, b| a - b);
    }
    let (first, rest) = args.split_first().unwrap();
    as_float("-", first)?;
    rest.iter().try_fold(first.clone(), |acc, x| arith("-", &acc, x, i64::checked_sub, |a, b| a - b))
}

fn div(args: &[Value]) -> Result<Value, Error> {
    if args.len() == 1 {
        return Ok(Value::Float(1.0 / as_float("/", &args[0])?));
    }
    let mut acc = as_float("/", &args[0])?;
    for x in &args[1..] {
        acc /= as_float("/", x)?;
    }
    Ok(Value::Float(acc))
}

fn int_division(
    name: &str,
    args: &[Value],
    int_op: fn(i64, i64) -> Option<i64>,
    float_op: fn(f64, f64) -> f64,
) -> Result<Value, Error> {
    match (&args[0], &args[1]) {
        (Value::Int(_), Value::Int(0)) => Err(division_by_zero()),
        (Value::Int(a), Value::Int(b)) => int_op(*a, *b).map(Value::Int).ok_or_else(overflow),
        (a, b) => Ok(Value::Float(float_op(as_float(name, a)?, as_float(name, b)?))),
    }
}

fn quot(args: &[Value]) -> Result<Value, Error> {
    int_division("quot", args, i64::checked_div, |a, b| (a / b).trunc())
}

fn rem(args: &[Value]) -> Result<Value, Error> {
    int_division("rem", args, i64::checked_rem, |a, b| a % b)
}

fn modulo(args: &[Value]) -> Result<Value, Error> {
    int_division(
        "mod",
        args,
        |a, b| a.checked_rem(b).map(|r| if r != 0 && (r < 0) != (b < 0) { r + b } else { r }),
        |a, b| a - b * (a / b).floor(),
    )
}

fn compare(name: &str, args: &[Value], accept: fn(Ordering) -> bool) -> Result<Value, Error> {
    for pair in args.windows(2) {
        let ordering = match (&pair[0], &pair[1]) {
            (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
            (a, b) => as_float(name, a)?.partial_cmp(&as_float(name, b)?),
        };
        if !ordering.is_some_and(accept) {
            return Ok(Value::Bool(false));
        }
    }
    if args.len() == 1 {
        as_float(name, &args[0])?;
    }
    Ok(Value::Bool(true))
}

fn equal(args: &[Value]) -> Result<Value, Error> {
    Ok(Value::Bool(args.windows(2).all(|pair| pair[0] == pair[1])))
}
