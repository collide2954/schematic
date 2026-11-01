use std::fmt;
use std::rc::Rc;

use crate::array::Array;
use crate::error::Error;
use crate::symbol::Symbol;
use crate::value::{Shown, Value};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    names: Vec<Symbol>,
    columns: Vec<Rc<Array>>,
}

const PRINT_ROWS: usize = 20;

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("{n} {word}") } else { format!("{n} {word}s") }
}

impl Table {
    pub fn new(pairs: Vec<(Symbol, Rc<Array>)>) -> Result<Table, Error> {
        let mut table = Table::default();
        for (name, column) in pairs {
            table.set_column(name, column)?;
        }
        Ok(table)
    }

    pub fn set_column(&mut self, name: Symbol, column: Rc<Array>) -> Result<(), Error> {
        let existing = self.names.iter().position(|n| *n == name);
        if let Some((i, other)) = self.columns.iter().enumerate().find(|(i, _)| Some(*i) != existing)
            && other.len() != column.len() {
                let message = format!(
                    "table columns must have the same length: :{} has {} rows but :{name} has {}",
                    self.names[i],
                    other.len(),
                    column.len()
                );
                return Err(Error::new("value-error", message));
            }
        match existing {
            Some(i) => self.columns[i] = column,
            None => {
                self.names.push(name);
                self.columns.push(column);
            }
        }
        Ok(())
    }

    pub fn nrows(&self) -> usize {
        self.columns.first().map_or(0, |c| c.len())
    }

    pub fn ncols(&self) -> usize {
        self.names.len()
    }

    pub fn names(&self) -> &[Symbol] {
        &self.names
    }

    pub fn columns(&self) -> impl Iterator<Item = (Symbol, &Rc<Array>)> {
        self.names.iter().copied().zip(self.columns.iter())
    }

    pub fn column(&self, name: Symbol) -> Option<&Rc<Array>> {
        self.names.iter().position(|n| *n == name).map(|i| &self.columns[i])
    }

    pub fn row(&self, index: usize) -> Option<Value> {
        if index >= self.nrows() {
            return None;
        }
        let pairs = self.columns().map(|(name, column)| (Value::Keyword(name), column.get(index).unwrap()));
        Some(Value::map(pairs))
    }

    pub fn rows(&self) -> impl Iterator<Item = Value> + '_ {
        (0..self.nrows()).map(move |i| self.row(i).unwrap())
    }

    pub fn without(&self, name: Symbol) -> Table {
        let pairs = self.columns().filter(|(n, _)| *n != name).map(|(n, c)| (n, c.clone())).collect();
        Table::new(pairs).unwrap()
    }

    pub fn select(&self, names: &[Symbol]) -> Result<Table, Error> {
        let mut pairs = Vec::with_capacity(names.len());
        for &name in names {
            let column = self.column(name).ok_or_else(|| Table::missing(name))?;
            pairs.push((name, column.clone()));
        }
        Table::new(pairs)
    }

    pub fn missing(name: Symbol) -> Error {
        Error::new("value-error", format!("table has no column :{name}"))
    }

    pub fn rename(&self, from: Symbol, to: Symbol) -> Result<Table, Error> {
        self.column(from).ok_or_else(|| Table::missing(from))?;
        let pairs = self.columns().map(|(n, c)| (if n == from { to } else { n }, c.clone())).collect();
        Table::new(pairs)
    }

    pub fn push_row(&mut self, cells: &[(Symbol, Value)]) -> Result<(), Error> {
        let nrows = self.nrows();
        for (name, _) in cells {
            if self.column(*name).is_none() {
                self.names.push(*name);
                self.columns.push(Rc::new(Array::from_values(std::iter::repeat_n(Value::Nil, nrows))?));
            }
        }
        for (name, column) in self.names.iter().zip(self.columns.iter_mut()) {
            let value = cells.iter().find(|(n, _)| n == name).map_or(Value::Nil, |(_, v)| v.clone());
            Rc::make_mut(column).push_mut(value)?;
        }
        Ok(())
    }

    pub fn stack(parts: &[Table]) -> Result<Table, Error> {
        let mut names: Vec<Symbol> = Vec::new();
        for name in parts.iter().flat_map(|part| part.names()) {
            if !names.contains(name) {
                names.push(*name);
            }
        }
        let mut pairs = Vec::with_capacity(names.len());
        for name in names {
            let values = parts.iter().flat_map(|part| -> Box<dyn Iterator<Item = Value>> {
                match part.column(name) {
                    Some(column) => Box::new(column.iter()),
                    None => Box::new(std::iter::repeat_n(Value::Nil, part.nrows())),
                }
            });
            pairs.push((name, Rc::new(Array::from_values(values)?)));
        }
        Table::new(pairs)
    }

    pub fn take_rows(&self, indices: &[usize]) -> Table {
        let pairs = self
            .columns()
            .map(|(name, column)| {
                let values = indices.iter().map(|&i| column.get(i).unwrap_or(Value::Nil));
                (name, Rc::new(Array::from_values(values).unwrap()))
            })
            .collect();
        Table::new(pairs).unwrap()
    }

    pub fn format(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let nrows = self.nrows();
        let shown = nrows.min(PRINT_ROWS);
        let mut cells: Vec<Vec<String>> = Vec::with_capacity(self.ncols());
        let mut numeric = Vec::with_capacity(self.ncols());
        for (name, column) in self.columns() {
            let mut texts = vec![name.to_string()];
            texts.extend((0..shown).map(|i| Shown(&column.get(i).unwrap()).to_string()));
            cells.push(texts);
            numeric.push(matches!(column.kind(), "int" | "float"));
        }
        let widths: Vec<usize> =
            cells.iter().map(|texts| texts.iter().map(|t| t.chars().count()).max().unwrap_or(0)).collect();
        for line in 0..=shown {
            let mut out = String::new();
            for (c, texts) in cells.iter().enumerate() {
                if c > 0 {
                    out.push_str("  ");
                }
                let text = &texts[line];
                let pad = widths[c] - text.chars().count();
                if numeric[c] && line > 0 {
                    out.push_str(&" ".repeat(pad));
                    out.push_str(text);
                } else {
                    out.push_str(text);
                    out.push_str(&" ".repeat(pad));
                }
            }
            let trimmed = out.trim_end();
            if !trimmed.is_empty() {
                writeln!(f, "{trimmed}")?;
            }
        }
        if nrows > shown {
            writeln!(f, "... {} more rows", nrows - shown)?;
        }
        write!(f, "({}, {})", plural(nrows, "row"), plural(self.ncols(), "column"))
    }
}
