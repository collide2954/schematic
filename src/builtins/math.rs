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
    native(globals, "max", Arity::AtLeast(1), |args| extremum("max", args, Ordering::Greater));
    native(globals, "min", Arity::AtLeast(1), |args| extremum("min", args, Ordering::Less));
    native(globals, "abs", Arity::Exact(1), abs);
    native(globals, "inc", Arity::Exact(1), |args| arith("inc", &args[0], &Value::Int(1), i64::checked_add, |a, b| a + b));
    native(globals, "dec", Arity::Exact(1), |args| arith("dec", &args[0], &Value::Int(1), i64::checked_sub, |a, b| a - b));
    native(globals, "even?", Arity::Exact(1), |args| parity("even?", &args[0], 0));
    native(globals, "odd?", Arity::Exact(1), |args| parity("odd?", &args[0], 1));
    native(globals, "zero?", Arity::Exact(1), |args| Ok(Value::Bool(as_float("zero?", &args[0])? == 0.0)));
    native(globals, "pos?", Arity::Exact(1), |args| Ok(Value::Bool(as_float("pos?", &args[0])? > 0.0)));
    native(globals, "neg?", Arity::Exact(1), |args| Ok(Value::Bool(as_float("neg?", &args[0])? < 0.0)));
    native(globals, "floor", Arity::Exact(1), |args| to_integer("floor", &args[0], f64::floor));
    native(globals, "ceil", Arity::Exact(1), |args| to_integer("ceil", &args[0], f64::ceil));
    native(globals, "round", Arity::Exact(1), |args| to_integer("round", &args[0], f64::round_ties_even));
    native(globals, "truncate", Arity::Exact(1), |args| to_integer("truncate", &args[0], f64::trunc));
    native(globals, "int", Arity::Exact(1), |args| to_integer("int", &args[0], f64::trunc));
    native(globals, "float", Arity::Exact(1), |args| Ok(Value::Float(as_float("float", &args[0])?)));
    native(globals, "sqrt", Arity::Exact(1), |args| Ok(Value::Float(as_float("sqrt", &args[0])?.sqrt())));
    native(globals, "exp", Arity::Exact(1), |args| Ok(Value::Float(as_float("exp", &args[0])?.exp())));
    native(globals, "log", Arity::Between(1, 2), log);
    native(globals, "pow", Arity::Exact(2), pow);
}

fn extremum(name: &str, args: &[Value], wanted: Ordering) -> Result<Value, Error> {
    let mut best = &args[0];
    as_float(name, best)?;
    for candidate in &args[1..] {
        let ordering = match (candidate, best) {
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (a, b) => as_float(name, a)?.partial_cmp(&as_float(name, b)?).unwrap_or(Ordering::Equal),
        };
        if ordering == wanted {
            best = candidate;
        }
    }
    Ok(best.clone())
}

fn abs(args: &[Value]) -> Result<Value, Error> {
    match &args[0] {
        Value::Int(i) => i.checked_abs().map(Value::Int).ok_or_else(overflow),
        Value::Float(x) => Ok(Value::Float(x.abs())),
        other => Err(not_a_number("abs", other)),
    }
}

fn parity(name: &str, value: &Value, remainder: i64) -> Result<Value, Error> {
    match value {
        Value::Int(i) => Ok(Value::Bool(i.rem_euclid(2) == remainder)),
        other => Err(Error::type_error(format!("{name} expects an integer, got {other}"))),
    }
}

fn float_to_int(x: f64) -> Result<Value, Error> {
    if x.is_finite() && (-9.223372036854776e18..9.223372036854776e18).contains(&x) {
        Ok(Value::Int(x as i64))
    } else {
        Err(Error::new("arithmetic", format!("cannot convert {} to an integer", Value::Float(x))))
    }
}

fn to_integer(name: &str, value: &Value, round: fn(f64) -> f64) -> Result<Value, Error> {
    match value {
        Value::Int(_) => Ok(value.clone()),
        Value::Float(x) => float_to_int(round(*x)),
        other => Err(not_a_number(name, other)),
    }
}

fn log(args: &[Value]) -> Result<Value, Error> {
    let x = as_float("log", &args[0])?.ln();
    match args.get(1) {
        Some(base) => Ok(Value::Float(x / as_float("log", base)?.ln())),
        None => Ok(Value::Float(x)),
    }
}

fn pow(args: &[Value]) -> Result<Value, Error> {
    match (&args[0], &args[1]) {
        (Value::Int(base), Value::Int(exponent)) if *exponent >= 0 => u32::try_from(*exponent)
            .ok()
            .and_then(|e| base.checked_pow(e))
            .map(Value::Int)
            .ok_or_else(overflow),
        (a, b) => Ok(Value::Float(as_float("pow", a)?.powf(as_float("pow", b)?))),
    }
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
