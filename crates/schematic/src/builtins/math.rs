use std::cmp::Ordering;

use super::{native, native_intrinsic};
use crate::array::{broadcast, fast_binary_arrays, is_array, unary};
use crate::bytecode::Intrinsic;
use crate::error::Error;
use crate::value::Value;
use crate::vm::{Arity, Globals};

pub fn mask_and(a: &Value, b: &Value) -> Result<Value, Error> {
    broadcast(a, b, &|x, y| Ok(Value::Bool(x.is_truthy() && y.is_truthy())))
}

fn mask_or(a: &Value, b: &Value) -> Result<Value, Error> {
    broadcast(a, b, &|x, y| Ok(Value::Bool(x.is_truthy() || y.is_truthy())))
}

fn lt(args: &[Value]) -> Result<Value, Error> {
    compare("<", args, |o| o == Ordering::Less)
}

fn gt(args: &[Value]) -> Result<Value, Error> {
    compare(">", args, |o| o == Ordering::Greater)
}

fn le(args: &[Value]) -> Result<Value, Error> {
    compare("<=", args, |o| o != Ordering::Greater)
}

fn ge(args: &[Value]) -> Result<Value, Error> {
    compare(">=", args, |o| o != Ordering::Less)
}

pub fn not(args: &[Value]) -> Result<Value, Error> {
    unary(&args[0], &|x| Ok(Value::Bool(!x.is_truthy())))
}

pub fn binary(kind: Intrinsic, a: &Value, b: &Value) -> Result<Value, Error> {
    let args = [a.clone(), b.clone()];
    match kind {
        Intrinsic::Add => add(&args),
        Intrinsic::Sub => sub(&args),
        Intrinsic::Mul => mul(&args),
        Intrinsic::Div => div_pair(a, b),
        Intrinsic::Lt => lt(&args),
        Intrinsic::Le => le(&args),
        Intrinsic::Gt => gt(&args),
        Intrinsic::Ge => ge(&args),
        Intrinsic::NumEq => equal(&args),
        Intrinsic::Not => not(&args[..1]),
    }
}

pub fn install(globals: &mut Globals) {
    native_intrinsic(globals, "+", Arity::AtLeast(0), add, Intrinsic::Add);
    native_intrinsic(globals, "-", Arity::AtLeast(1), sub, Intrinsic::Sub);
    native_intrinsic(globals, "*", Arity::AtLeast(0), mul, Intrinsic::Mul);
    native_intrinsic(globals, "/", Arity::AtLeast(1), div, Intrinsic::Div);
    native(globals, "quot", Arity::Exact(2), |args| quot(&args[0], &args[1]));
    native(globals, "rem", Arity::Exact(2), |args| rem(&args[0], &args[1]));
    native(globals, "mod", Arity::Exact(2), |args| modulo(&args[0], &args[1]));
    native(globals, "mask-and", Arity::Exact(2), |args| mask_and(&args[0], &args[1]));
    native(globals, "mask-or", Arity::Exact(2), |args| mask_or(&args[0], &args[1]));
    native_intrinsic(globals, "<", Arity::AtLeast(1), lt, Intrinsic::Lt);
    native_intrinsic(globals, ">", Arity::AtLeast(1), gt, Intrinsic::Gt);
    native_intrinsic(globals, "<=", Arity::AtLeast(1), le, Intrinsic::Le);
    native_intrinsic(globals, ">=", Arity::AtLeast(1), ge, Intrinsic::Ge);
    native_intrinsic(globals, "=", Arity::AtLeast(1), equal, Intrinsic::NumEq);
    native_intrinsic(globals, "not", Arity::Exact(1), not, Intrinsic::Not);
    native(globals, "max", Arity::AtLeast(1), |args| extremum("max", args, Ordering::Greater));
    native(globals, "min", Arity::AtLeast(1), |args| extremum("min", args, Ordering::Less));
    native(globals, "abs", Arity::Exact(1), |args| unary(&args[0], &abs));
    native(globals, "inc", Arity::Exact(1), |args| arith("inc", &args[0], &Value::Int(1), i64::checked_add, |a, b| a + b));
    native(globals, "dec", Arity::Exact(1), |args| arith("dec", &args[0], &Value::Int(1), i64::checked_sub, |a, b| a - b));
    native(globals, "even?", Arity::Exact(1), |args| unary(&args[0], &|x| parity("even?", x, 0)));
    native(globals, "odd?", Arity::Exact(1), |args| unary(&args[0], &|x| parity("odd?", x, 1)));
    native(globals, "zero?", Arity::Exact(1), |args| unary(&args[0], &|x| Ok(Value::Bool(as_float("zero?", x)? == 0.0))));
    native(globals, "pos?", Arity::Exact(1), |args| unary(&args[0], &|x| Ok(Value::Bool(as_float("pos?", x)? > 0.0))));
    native(globals, "neg?", Arity::Exact(1), |args| unary(&args[0], &|x| Ok(Value::Bool(as_float("neg?", x)? < 0.0))));
    native(globals, "floor", Arity::Exact(1), |args| unary(&args[0], &|x| to_integer("floor", x, f64::floor)));
    native(globals, "ceil", Arity::Exact(1), |args| unary(&args[0], &|x| to_integer("ceil", x, f64::ceil)));
    native(globals, "round", Arity::Exact(1), |args| unary(&args[0], &|x| to_integer("round", x, f64::round_ties_even)));
    native(globals, "truncate", Arity::Exact(1), |args| unary(&args[0], &|x| to_integer("truncate", x, f64::trunc)));
    native(globals, "int", Arity::Exact(1), |args| unary(&args[0], &|x| to_integer("int", x, f64::trunc)));
    native(globals, "float", Arity::Exact(1), |args| unary(&args[0], &|x| Ok(Value::Float(as_float("float", x)?))));
    native(globals, "sqrt", Arity::Exact(1), |args| unary(&args[0], &|x| Ok(Value::Float(as_float("sqrt", x)?.sqrt()))));
    native(globals, "exp", Arity::Exact(1), |args| unary(&args[0], &|x| Ok(Value::Float(as_float("exp", x)?.exp()))));
    native(globals, "log", Arity::Between(1, 2), log);
    native(globals, "pow", Arity::Exact(2), |args| pow(&args[0], &args[1]));
}

fn pick(name: &str, a: &Value, b: &Value, wanted: Ordering) -> Result<Value, Error> {
    if is_array(a) || is_array(b) {
        return broadcast(a, b, &|x, y| pick(name, x, y, wanted));
    }
    let ordering = match (b, a) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Date(x), Value::Date(y)) => x.cmp(y),
        (x, y) => as_float(name, x)?.partial_cmp(&as_float(name, y)?).unwrap_or(Ordering::Equal),
    };
    Ok(if ordering == wanted { b.clone() } else { a.clone() })
}

fn extremum(name: &str, args: &[Value], wanted: Ordering) -> Result<Value, Error> {
    if let [Value::Array(a)] = args {
        let mut best: Option<Value> = None;
        for x in a.iter().filter(|x| !matches!(x, Value::Nil)) {
            best = Some(match best {
                None => x,
                Some(current) => pick(name, &current, &x, wanted)?,
            });
        }
        return Ok(best.unwrap_or(Value::Nil));
    }
    if !is_array(&args[0]) && !matches!(args[0], Value::Date(_)) {
        as_float(name, &args[0])?;
    }
    args[1..].iter().try_fold(args[0].clone(), |best, candidate| pick(name, &best, candidate, wanted))
}

fn abs(value: &Value) -> Result<Value, Error> {
    match value {
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
    match args {
        [x] => unary(x, &|x| Ok(Value::Float(as_float("log", x)?.ln()))),
        [x, base] => broadcast(x, base, &|x, base| {
            Ok(Value::Float(as_float("log", x)?.ln() / as_float("log", base)?.ln()))
        }),
        _ => unreachable!(),
    }
}

fn pow(a: &Value, b: &Value) -> Result<Value, Error> {
    if is_array(a) || is_array(b) {
        return broadcast(a, b, &pow);
    }
    match (a, b) {
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
    if is_array(a) || is_array(b) {
        let kind = match name {
            "+" | "inc" => Some(Intrinsic::Add),
            "-" | "dec" => Some(Intrinsic::Sub),
            "*" => Some(Intrinsic::Mul),
            _ => None,
        };
        if let Some(result) = kind.and_then(|kind| fast_binary_arrays(kind, a, b)) {
            return result;
        }
        return broadcast(a, b, &|x, y| arith(name, x, y, int_op, float_op));
    }
    match (name, a, b) {
        ("+" | "inc", Value::Date(d), Value::Int(n)) | ("+", Value::Int(n), Value::Date(d)) => return shift_date(*d, *n),
        ("-" | "dec", Value::Date(d), Value::Int(n)) => return shift_date(*d, -*n),
        ("-", Value::Date(x), Value::Date(y)) => return Ok(Value::Int(*x as i64 - *y as i64)),
        _ => {}
    }
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => int_op(*x, *y).map(Value::Int).ok_or_else(overflow),
        (Value::Int(x), Value::Float(y)) => Ok(Value::Float(float_op(*x as f64, *y))),
        (Value::Float(x), Value::Int(y)) => Ok(Value::Float(float_op(*x, *y as f64))),
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(float_op(*x, *y))),
        (Value::Int(_) | Value::Float(_), other) | (other, _) => Err(not_a_number(name, other)),
    }
}

fn shift_date(days: i32, by: i64) -> Result<Value, Error> {
    i32::try_from(days as i64 + by).map(Value::Date).map_err(|_| Error::new("value-error", "date out of range"))
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
    rest.iter().try_fold(first.clone(), |acc, x| arith("-", &acc, x, i64::checked_sub, |a, b| a - b))
}

fn div_pair(a: &Value, b: &Value) -> Result<Value, Error> {
    if is_array(a) || is_array(b) {
        if let Some(result) = fast_binary_arrays(Intrinsic::Div, a, b) {
            return result;
        }
        return broadcast(a, b, &div_pair);
    }
    Ok(Value::Float(as_float("/", a)? / as_float("/", b)?))
}

fn div(args: &[Value]) -> Result<Value, Error> {
    if args.len() == 1 {
        return div_pair(&Value::Int(1), &args[0]);
    }
    args[1..].iter().try_fold(args[0].clone(), |acc, x| div_pair(&acc, x))
}

fn int_division(
    name: &str,
    a: &Value,
    b: &Value,
    int_op: fn(i64, i64) -> Option<i64>,
    float_op: fn(f64, f64) -> f64,
) -> Result<Value, Error> {
    if is_array(a) || is_array(b) {
        return broadcast(a, b, &|x, y| int_division(name, x, y, int_op, float_op));
    }
    match (a, b) {
        (Value::Int(_), Value::Int(0)) => Err(division_by_zero()),
        (Value::Int(a), Value::Int(b)) => int_op(*a, *b).map(Value::Int).ok_or_else(overflow),
        (a, b) => Ok(Value::Float(float_op(as_float(name, a)?, as_float(name, b)?))),
    }
}

fn quot(a: &Value, b: &Value) -> Result<Value, Error> {
    int_division("quot", a, b, i64::checked_div, |a, b| (a / b).trunc())
}

fn rem(a: &Value, b: &Value) -> Result<Value, Error> {
    int_division("rem", a, b, i64::checked_rem, |a, b| a % b)
}

fn modulo(a: &Value, b: &Value) -> Result<Value, Error> {
    int_division(
        "mod",
        a,
        b,
        |a, b| a.checked_rem(b).map(|r| if r != 0 && (r < 0) != (b < 0) { r + b } else { r }),
        |a, b| a - b * (a / b).floor(),
    )
}

fn chained_masks(
    args: &[Value],
    kind: Intrinsic,
    pair: &dyn Fn(&Value, &Value) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let mut mask: Option<Value> = None;
    for window in args.windows(2) {
        let next = match fast_binary_arrays(kind, &window[0], &window[1]) {
            Some(result) => result?,
            None => broadcast(&window[0], &window[1], pair)?,
        };
        mask = Some(match mask {
            None => next,
            Some(previous) => mask_and(&previous, &next)?,
        });
    }
    Ok(mask.unwrap_or(Value::Bool(true)))
}

fn compare(name: &str, args: &[Value], accept: fn(Ordering) -> bool) -> Result<Value, Error> {
    if args.iter().any(is_array) {
        let kind = match name {
            "<" => Intrinsic::Lt,
            "<=" => Intrinsic::Le,
            ">" => Intrinsic::Gt,
            _ => Intrinsic::Ge,
        };
        return chained_masks(args, kind, &|x, y| compare(name, &[x.clone(), y.clone()], accept));
    }
    for pair in args.windows(2) {
        let ordering = match (&pair[0], &pair[1]) {
            (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
            (Value::Date(a), Value::Date(b)) => Some(a.cmp(b)),
            (a, b) => as_float(name, a)?.partial_cmp(&as_float(name, b)?),
        };
        if !ordering.is_some_and(accept) {
            return Ok(Value::Bool(false));
        }
    }
    if args.len() == 1 && !matches!(args[0], Value::Date(_)) {
        as_float(name, &args[0])?;
    }
    Ok(Value::Bool(true))
}

fn equal(args: &[Value]) -> Result<Value, Error> {
    if args.iter().any(is_array) {
        return chained_masks(args, Intrinsic::NumEq, &|x, y| Ok(Value::Bool(x == y)));
    }
    Ok(Value::Bool(args.windows(2).all(|pair| pair[0] == pair[1])))
}
