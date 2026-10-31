use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use crate::error::Error;
use crate::list::List;
use crate::map::Map;
use crate::set::Set;
use crate::symbol::Symbol;
use crate::vector::Vector;
use crate::vm::{Closure, Native};

#[derive(Clone, Debug)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Symbol(Symbol),
    Keyword(Symbol),
    Str(Rc<str>),
    List(List),
    Vector(Rc<Vector>),
    Map(Rc<Map>),
    Set(Rc<Set>),
    Closure(Rc<Closure>),
    Native(Rc<Native>),
    Cell(Rc<RefCell<Value>>),
    Signal(Rc<Error>),
    Escape(u64),
}

impl Value {
    pub fn list(items: impl IntoIterator<Item = Value>) -> Value {
        Value::List(items.into_iter().collect())
    }

    pub fn vector(items: impl IntoIterator<Item = Value>) -> Value {
        Value::Vector(Rc::new(items.into_iter().collect()))
    }

    pub fn map(pairs: impl IntoIterator<Item = (Value, Value)>) -> Value {
        Value::Map(Rc::new(pairs.into_iter().collect()))
    }

    pub fn set(items: impl IntoIterator<Item = Value>) -> Value {
        Value::Set(Rc::new(items.into_iter().collect()))
    }

    pub fn symbol(name: &str) -> Value {
        Value::Symbol(Symbol::intern(name))
    }

    pub fn keyword(name: &str) -> Value {
        Value::Keyword(Symbol::intern(name))
    }

    pub fn string(s: &str) -> Value {
        Value::Str(Rc::from(s))
    }

    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "boolean",
            Value::Int(_) => "integer",
            Value::Float(_) => "float",
            Value::Symbol(_) => "symbol",
            Value::Keyword(_) => "keyword",
            Value::Str(_) => "string",
            Value::List(_) => "list",
            Value::Vector(_) => "vector",
            Value::Map(_) => "map",
            Value::Set(_) => "set",
            Value::Closure(_) | Value::Native(_) | Value::Escape(_) => "function",
            Value::Cell(_) => "cell",
            Value::Signal(_) => "signal",
        }
    }
}

fn float_as_int(x: f64) -> Option<i64> {
    if x.fract() == 0.0 && x >= -9.223372036854776e18 && x < 9.223372036854776e18 {
        Some(x as i64)
    } else {
        None
    }
}

fn int_eq_float(i: i64, x: f64) -> bool {
    float_as_int(x) == Some(i)
}

fn mix(mut h: u64) -> u64 {
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58476d1ce4e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d049bb133111eb);
    h ^ (h >> 31)
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    mix(h)
}

fn hash_ordered<'a>(tag: u64, items: impl Iterator<Item = &'a Value>) -> u64 {
    items.fold(mix(tag), |h, item| mix(h ^ item.hash_code()))
}

const TAG_NIL: u64 = 0x01;
const TAG_BOOL: u64 = 0x02;
const TAG_INT: u64 = 0x03 << 56;
const TAG_FLOAT: u64 = 0x04 << 56;
const TAG_SYMBOL: u64 = 0x05 << 56;
const TAG_KEYWORD: u64 = 0x06 << 56;
const TAG_LIST: u64 = 0x07;
const TAG_VECTOR: u64 = 0x08;
const TAG_MAP: u64 = 0x09;
const TAG_SET: u64 = 0x0a;
const TAG_POINTER: u64 = 0x0b << 56;

impl Value {
    pub fn hash_code(&self) -> u64 {
        match self {
            Value::Nil => mix(TAG_NIL),
            Value::Bool(b) => mix(TAG_BOOL + *b as u64),
            Value::Int(i) => mix(*i as u64 ^ TAG_INT),
            Value::Float(x) => match float_as_int(*x) {
                Some(i) => mix(i as u64 ^ TAG_INT),
                None if x.is_nan() => mix(TAG_FLOAT),
                None => mix(x.to_bits() ^ TAG_FLOAT),
            },
            Value::Symbol(s) => mix(s.id() as u64 ^ TAG_SYMBOL),
            Value::Keyword(s) => mix(s.id() as u64 ^ TAG_KEYWORD),
            Value::Str(s) => hash_bytes(s.as_bytes()),
            Value::List(l) => hash_ordered(TAG_LIST, l.iter()),
            Value::Vector(v) => hash_ordered(TAG_VECTOR, v.iter()),
            Value::Map(m) => {
                let sum = m
                    .iter()
                    .fold(0u64, |acc, (k, v)| acc.wrapping_add(mix(k.hash_code() ^ mix(v.hash_code()))));
                mix(TAG_MAP ^ sum)
            }
            Value::Set(s) => {
                let sum = s.iter().fold(0u64, |acc, item| acc.wrapping_add(mix(item.hash_code())));
                mix(TAG_SET ^ sum)
            }
            Value::Closure(c) => mix(Rc::as_ptr(c) as usize as u64 ^ TAG_POINTER),
            Value::Native(n) => mix(Rc::as_ptr(n) as usize as u64 ^ TAG_POINTER),
            Value::Cell(c) => mix(Rc::as_ptr(c) as usize as u64 ^ TAG_POINTER),
            Value::Signal(s) => mix(Rc::as_ptr(s) as usize as u64 ^ TAG_POINTER),
            Value::Escape(token) => mix(*token ^ TAG_POINTER),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        use Value::*;
        match (self, other) {
            (Nil, Nil) => true,
            (Bool(a), Bool(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Float(a), Float(b)) => a == b,
            (Int(a), Float(b)) | (Float(b), Int(a)) => int_eq_float(*a, *b),
            (Symbol(a), Symbol(b)) => a == b,
            (Keyword(a), Keyword(b)) => a == b,
            (Str(a), Str(b)) => a == b,
            (List(a), List(b)) => a == b,
            (Vector(a), Vector(b)) => Rc::ptr_eq(a, b) || a == b,
            (Map(a), Map(b)) => Rc::ptr_eq(a, b) || a == b,
            (Set(a), Set(b)) => Rc::ptr_eq(a, b) || a == b,
            (Closure(a), Closure(b)) => Rc::ptr_eq(a, b),
            (Native(a), Native(b)) => Rc::ptr_eq(a, b),
            (Cell(a), Cell(b)) => Rc::ptr_eq(a, b),
            (Signal(a), Signal(b)) => Rc::ptr_eq(a, b),
            (Escape(a), Escape(b)) => a == b,
            _ => false,
        }
    }
}

fn write_float(f: &mut fmt::Formatter<'_>, x: f64) -> fmt::Result {
    if x.is_nan() {
        f.write_str("+nan.0")
    } else if x == f64::INFINITY {
        f.write_str("+inf.0")
    } else if x == f64::NEG_INFINITY {
        f.write_str("-inf.0")
    } else {
        write!(f, "{x:?}")
    }
}

fn write_string(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    f.write_str("\"")?;
    for c in s.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\t' => f.write_str("\\t")?,
            '\r' => f.write_str("\\r")?,
            c => write!(f, "{c}")?,
        }
    }
    f.write_str("\"")
}

pub struct Shown<'a>(pub &'a Value);

impl fmt::Display for Shown<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.format(f, false)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.format(f, true)
    }
}

impl Value {
    fn format(&self, f: &mut fmt::Formatter<'_>, readable: bool) -> fmt::Result {
        match self {
            Value::Nil => f.write_str("nil"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write_float(f, *x),
            Value::Symbol(s) => write!(f, "{s}"),
            Value::Keyword(s) => write!(f, ":{s}"),
            Value::Str(s) if readable => write_string(f, s),
            Value::Str(s) => f.write_str(s),
            Value::List(l) => write_seq(f, "(", l.iter(), ")", readable),
            Value::Vector(v) => write_seq(f, "[", v.iter(), "]", readable),
            Value::Map(m) => {
                f.write_str("{")?;
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    k.format(f, readable)?;
                    f.write_str(" ")?;
                    v.format(f, readable)?;
                }
                f.write_str("}")
            }
            Value::Set(s) => write_seq(f, "#{", s.iter(), "}", readable),
            Value::Closure(c) => match c.proto.name {
                Some(name) => write!(f, "#<procedure {name}>"),
                None => f.write_str("#<procedure>"),
            },
            Value::Native(n) => write!(f, "#<builtin {}>", n.name),
            Value::Cell(_) => f.write_str("#<cell>"),
            Value::Signal(_) => f.write_str("#<signal>"),
            Value::Escape(_) => f.write_str("#<escape>"),
        }
    }
}

fn write_seq<'a>(
    f: &mut fmt::Formatter<'_>,
    open: &str,
    items: impl Iterator<Item = &'a Value>,
    close: &str,
    readable: bool,
) -> fmt::Result {
    f.write_str(open)?;
    for (i, item) in items.enumerate() {
        if i > 0 {
            f.write_str(" ")?;
        }
        item.format(f, readable)?;
    }
    f.write_str(close)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(v: Value) -> String {
        v.to_string()
    }

    #[test]
    fn prints_nil_and_booleans() {
        assert_eq!(shown(Value::Nil), "nil");
        assert_eq!(shown(Value::Bool(true)), "true");
        assert_eq!(shown(Value::Bool(false)), "false");
    }

    #[test]
    fn prints_integers() {
        assert_eq!(shown(Value::Int(42)), "42");
        assert_eq!(shown(Value::Int(-7)), "-7");
    }

    #[test]
    fn prints_floats_distinctly_from_integers() {
        assert_eq!(shown(Value::Float(1.0)), "1.0");
        assert_eq!(shown(Value::Float(3.25)), "3.25");
        assert_eq!(shown(Value::Float(-0.5)), "-0.5");
    }

    #[test]
    fn prints_special_floats() {
        assert_eq!(shown(Value::Float(f64::INFINITY)), "+inf.0");
        assert_eq!(shown(Value::Float(f64::NEG_INFINITY)), "-inf.0");
        assert_eq!(shown(Value::Float(f64::NAN)), "+nan.0");
    }

    #[test]
    fn prints_symbols_and_keywords() {
        assert_eq!(shown(Value::symbol("map")), "map");
        assert_eq!(shown(Value::keyword("name")), ":name");
    }

    #[test]
    fn prints_strings_quoted_with_escapes() {
        assert_eq!(shown(Value::string("hi")), "\"hi\"");
        assert_eq!(shown(Value::string("a\"b\\c\n")), "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn numeric_equality_crosses_int_and_float() {
        assert_eq!(Value::Int(1), Value::Float(1.0));
        assert_ne!(Value::Int(1), Value::Float(1.5));
        assert_ne!(Value::Int(1), Value::string("1"));
    }

    #[test]
    fn equal_values_hash_alike() {
        assert_eq!(Value::Int(1).hash_code(), Value::Float(1.0).hash_code());
        assert_eq!(Value::string("ab").hash_code(), Value::string("ab").hash_code());
        assert_eq!(Value::keyword("k").hash_code(), Value::keyword("k").hash_code());
        assert_eq!(
            Value::list([Value::Int(1), Value::Int(2)]).hash_code(),
            Value::list([Value::Int(1), Value::Int(2)]).hash_code()
        );
        assert_eq!(Value::Float(0.0).hash_code(), Value::Float(-0.0).hash_code());
    }

    #[test]
    fn distinct_values_usually_hash_differently() {
        let hashes = [
            Value::Nil,
            Value::Bool(false),
            Value::Int(0),
            Value::Int(1),
            Value::Float(0.5),
            Value::symbol("a"),
            Value::keyword("a"),
            Value::string("a"),
            Value::list([Value::Int(1)]),
            Value::vector([Value::Int(1)]),
        ]
        .iter()
        .map(Value::hash_code)
        .collect::<std::collections::HashSet<u64>>();
        assert_eq!(hashes.len(), 10);
    }

    #[test]
    fn only_nil_and_false_are_falsy() {
        assert!(!Value::Nil.is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        assert!(Value::Bool(true).is_truthy());
        assert!(Value::Int(0).is_truthy());
        assert!(Value::string("").is_truthy());
    }
}
