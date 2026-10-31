use std::fmt;
use std::rc::Rc;

use crate::bytecode::Intrinsic;
use crate::error::Error;
use crate::value::Value;

#[derive(Clone, Debug)]
pub enum Data {
    Float(Vec<f64>),
    Int(Vec<i64>),
    Bool(Vec<bool>),
    Str(Vec<Rc<String>>),
}

#[derive(Clone, Debug, Default)]
pub struct Array {
    data: Data,
    missing: Option<Vec<bool>>,
}

impl Default for Data {
    fn default() -> Data {
        Data::Float(Vec::new())
    }
}

const PRINT_LIMIT: usize = 20;

fn mismatch(a: &Value, b: &Value) -> Error {
    Error::type_error(format!("array elements must all be numbers, all booleans, or all strings, got {a} and {b}"))
}

impl Array {
    pub fn new() -> Array {
        Array::default()
    }

    pub fn from_values(items: impl IntoIterator<Item = Value>) -> Result<Array, Error> {
        let mut out = Array::new();
        for item in items {
            out.push_mut(item)?;
        }
        Ok(out)
    }

    pub fn len(&self) -> usize {
        match &self.data {
            Data::Float(v) => v.len(),
            Data::Int(v) => v.len(),
            Data::Bool(v) => v.len(),
            Data::Str(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn kind(&self) -> &'static str {
        match &self.data {
            Data::Float(_) => "float",
            Data::Int(_) => "int",
            Data::Bool(_) => "bool",
            Data::Str(_) => "string",
        }
    }

    pub fn data(&self) -> &Data {
        &self.data
    }

    pub fn missing_mask(&self) -> Option<&[bool]> {
        self.missing.as_deref()
    }

    pub fn missing_count(&self) -> usize {
        self.missing.as_ref().map_or(0, |m| m.iter().filter(|&&b| b).count())
    }

    pub fn is_missing(&self, index: usize) -> bool {
        self.missing.as_ref().is_some_and(|m| m[index])
    }

    pub fn get(&self, index: usize) -> Option<Value> {
        if index >= self.len() {
            return None;
        }
        if self.is_missing(index) {
            return Some(Value::Nil);
        }
        Some(match &self.data {
            Data::Float(v) => Value::Float(v[index]),
            Data::Int(v) => Value::Int(v[index]),
            Data::Bool(v) => Value::Bool(v[index]),
            Data::Str(v) => Value::Str(v[index].clone()),
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = Value> + '_ {
        (0..self.len()).map(move |i| self.get(i).unwrap())
    }

    fn representative(&self) -> Value {
        (0..self.len()).find(|&i| !self.is_missing(i)).and_then(|i| self.get(i)).unwrap_or(Value::Nil)
    }

    fn retype_for(&mut self, value: &Value) {
        let n = self.len();
        self.data = match value {
            Value::Int(_) => Data::Int(vec![0; n]),
            Value::Bool(_) => Data::Bool(vec![false; n]),
            Value::Str(_) => Data::Str(vec![Rc::new(String::new()); n]),
            _ => Data::Float(vec![0.0; n]),
        };
    }

    fn promote_to_float(&mut self) {
        if let Data::Int(v) = &self.data {
            self.data = Data::Float(v.iter().map(|&i| i as f64).collect());
        }
    }

    fn push_missing(&mut self) {
        let len = self.len();
        match &mut self.data {
            Data::Float(v) => v.push(0.0),
            Data::Int(v) => v.push(0),
            Data::Bool(v) => v.push(false),
            Data::Str(v) => v.push(Rc::new(String::new())),
        }
        self.missing.get_or_insert_with(|| vec![false; len]).push(true);
    }

    pub fn push_mut(&mut self, value: Value) -> Result<(), Error> {
        if matches!(value, Value::Nil) {
            self.push_missing();
            return Ok(());
        }
        if self.len() == self.missing_count() {
            self.retype_for(&value);
        }
        if matches!((&self.data, &value), (Data::Int(_), Value::Float(_))) {
            self.promote_to_float();
        }
        match (&mut self.data, &value) {
            (Data::Int(v), Value::Int(i)) => v.push(*i),
            (Data::Float(v), Value::Int(i)) => v.push(*i as f64),
            (Data::Float(v), Value::Float(x)) => v.push(*x),
            (Data::Bool(v), Value::Bool(b)) => v.push(*b),
            (Data::Str(v), Value::Str(s)) => v.push(s.clone()),
            _ => return Err(mismatch(&self.representative(), &value)),
        }
        if let Some(m) = &mut self.missing {
            m.push(false);
        }
        Ok(())
    }

    pub fn format(&self, f: &mut fmt::Formatter<'_>, readable: bool) -> fmt::Result {
        f.write_str("#[")?;
        let len = self.len();
        for i in 0..len.min(PRINT_LIMIT) {
            if i > 0 {
                f.write_str(" ")?;
            }
            self.get(i).unwrap().format(f, readable)?;
        }
        if len > PRINT_LIMIT {
            write!(f, " ... {len} items")?;
        }
        f.write_str("]")
    }
}

pub fn is_array(value: &Value) -> bool {
    matches!(value, Value::Array(_))
}

enum Side<'a> {
    Floats(&'a [f64]),
    Ints(&'a [i64]),
    Float(f64),
    Int(i64),
}

impl Side<'_> {
    fn len(&self) -> Option<usize> {
        match self {
            Side::Floats(v) => Some(v.len()),
            Side::Ints(v) => Some(v.len()),
            _ => None,
        }
    }

    fn is_float(&self) -> bool {
        matches!(self, Side::Floats(_) | Side::Float(_))
    }

    #[inline]
    fn float_at(&self, i: usize) -> f64 {
        match self {
            Side::Floats(v) => v[i],
            Side::Ints(v) => v[i] as f64,
            Side::Float(x) => *x,
            Side::Int(x) => *x as f64,
        }
    }

    #[inline]
    fn int_at(&self, i: usize) -> i64 {
        match self {
            Side::Ints(v) => v[i],
            Side::Int(x) => *x,
            _ => 0,
        }
    }
}

fn side<'a>(value: &'a Value) -> Option<(Side<'a>, Option<&'a [bool]>)> {
    match value {
        Value::Int(i) => Some((Side::Int(*i), None)),
        Value::Float(x) => Some((Side::Float(*x), None)),
        Value::Array(a) => match &a.data {
            Data::Float(v) => Some((Side::Floats(v), a.missing.as_deref())),
            Data::Int(v) => Some((Side::Ints(v), a.missing.as_deref())),
            _ => None,
        },
        _ => None,
    }
}

pub fn fast_binary_arrays(kind: Intrinsic, a: &Value, b: &Value) -> Option<Result<Value, Error>> {
    let (x, xm) = side(a)?;
    let (y, ym) = side(b)?;
    let len = match (x.len(), y.len()) {
        (Some(p), Some(q)) if p != q => {
            return Some(Err(Error::new("value-error", format!("array lengths differ: {p} and {q}"))));
        }
        (Some(n), _) | (None, Some(n)) => n,
        (None, None) => return None,
    };
    let missing = match (xm, ym) {
        (None, None) => None,
        (Some(m), None) | (None, Some(m)) => Some(m.to_vec()),
        (Some(p), Some(q)) => Some(p.iter().zip(q).map(|(a, b)| *a || *b).collect()),
    };
    let floats = x.is_float() || y.is_float() || kind == Intrinsic::Div;
    let data = match kind {
        Intrinsic::Add | Intrinsic::Sub | Intrinsic::Mul | Intrinsic::Div => {
            if floats {
                let op: fn(f64, f64) -> f64 = match kind {
                    Intrinsic::Add => |p, q| p + q,
                    Intrinsic::Sub => |p, q| p - q,
                    Intrinsic::Mul => |p, q| p * q,
                    _ => |p, q| p / q,
                };
                Data::Float((0..len).map(|i| op(x.float_at(i), y.float_at(i))).collect())
            } else {
                let op: fn(i64, i64) -> Option<i64> = match kind {
                    Intrinsic::Add => i64::checked_add,
                    Intrinsic::Sub => i64::checked_sub,
                    _ => i64::checked_mul,
                };
                let mut out = Vec::with_capacity(len);
                for i in 0..len {
                    if missing.as_ref().is_some_and(|m| m[i]) {
                        out.push(0);
                        continue;
                    }
                    match op(x.int_at(i), y.int_at(i)) {
                        Some(v) => out.push(v),
                        None => return Some(Err(Error::new("arithmetic", "integer overflow"))),
                    }
                }
                Data::Int(out)
            }
        }
        Intrinsic::Not => return None,
        _ => {
            let accept: fn(std::cmp::Ordering) -> bool = match kind {
                Intrinsic::Lt => |o| o == std::cmp::Ordering::Less,
                Intrinsic::Le => |o| o != std::cmp::Ordering::Greater,
                Intrinsic::Gt => |o| o == std::cmp::Ordering::Greater,
                Intrinsic::Ge => |o| o != std::cmp::Ordering::Less,
                _ => |o| o == std::cmp::Ordering::Equal,
            };
            let flags = (0..len)
                .map(|i| {
                    let ordering = if floats {
                        x.float_at(i).partial_cmp(&y.float_at(i))
                    } else {
                        Some(x.int_at(i).cmp(&y.int_at(i)))
                    };
                    ordering.is_some_and(accept)
                })
                .collect();
            Data::Bool(flags)
        }
    };
    Some(Ok(Value::Array(Rc::new(Array { data, missing }))))
}

pub fn element(value: &Value, index: usize) -> Value {
    match value {
        Value::Array(a) => a.get(index).unwrap_or(Value::Nil),
        other => other.clone(),
    }
}

pub fn broadcast(a: &Value, b: &Value, f: &dyn Fn(&Value, &Value) -> Result<Value, Error>) -> Result<Value, Error> {
    let len_of = |v: &Value| match v {
        Value::Array(a) => Some(a.len()),
        _ => None,
    };
    let len = match (len_of(a), len_of(b)) {
        (Some(x), Some(y)) if x != y => {
            return Err(Error::new("value-error", format!("array lengths differ: {x} and {y}")));
        }
        (Some(x), _) | (None, Some(x)) => x,
        (None, None) => return f(a, b),
    };
    let mut out = Array::new();
    for i in 0..len {
        let (x, y) = (element(a, i), element(b, i));
        if matches!(x, Value::Nil) || matches!(y, Value::Nil) {
            out.push_mut(Value::Nil)?;
        } else {
            out.push_mut(f(&x, &y)?)?;
        }
    }
    Ok(Value::Array(Rc::new(out)))
}

pub fn map_array(a: &Array, f: &dyn Fn(&Value) -> Result<Value, Error>) -> Result<Value, Error> {
    let mut out = Array::new();
    for x in a.iter() {
        if matches!(x, Value::Nil) {
            out.push_mut(Value::Nil)?;
        } else {
            out.push_mut(f(&x)?)?;
        }
    }
    Ok(Value::Array(Rc::new(out)))
}

pub fn unary(value: &Value, f: &dyn Fn(&Value) -> Result<Value, Error>) -> Result<Value, Error> {
    match value {
        Value::Array(a) => map_array(a, f),
        other => f(other),
    }
}

impl PartialEq for Array {
    fn eq(&self, other: &Array) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl FromIterator<Value> for Result<Array, Error> {
    fn from_iter<I: IntoIterator<Item = Value>>(iter: I) -> Result<Array, Error> {
        Array::from_values(iter)
    }
}
