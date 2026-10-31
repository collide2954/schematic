use std::fmt;
use std::rc::Rc;

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
