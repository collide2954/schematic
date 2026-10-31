use std::fmt;
use std::rc::Rc;

use crate::symbol::Symbol;

#[derive(Clone, Debug)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Symbol(Symbol),
    Keyword(Symbol),
    Str(Rc<str>),
}

impl Value {
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
}

fn int_eq_float(i: i64, x: f64) -> bool {
    x.fract() == 0.0 && x >= -9.223372036854776e18 && x < 9.223372036854776e18 && x as i64 == i
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

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => f.write_str("nil"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write_float(f, *x),
            Value::Symbol(s) => write!(f, "{s}"),
            Value::Keyword(s) => write!(f, ":{s}"),
            Value::Str(s) => write_string(f, s),
        }
    }
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
    fn only_nil_and_false_are_falsy() {
        assert!(!Value::Nil.is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        assert!(Value::Bool(true).is_truthy());
        assert!(Value::Int(0).is_truthy());
        assert!(Value::string("").is_truthy());
    }
}
