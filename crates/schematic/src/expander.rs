use std::collections::HashSet;
use std::rc::Rc;

use crate::error::Error;
use crate::list::List;
use crate::macros::Macro;
use crate::patterns;
use crate::symbol::Symbol;
use crate::value::Value;
use crate::vm::Globals;

pub struct Expander<'a> {
    globals: &'a mut Globals,
    scopes: Vec<HashSet<Symbol>>,
}

fn param_names(params: &Value) -> Vec<Symbol> {
    match params {
        Value::Symbol(s) => vec![*s],
        Value::List(l) => l.iter().flat_map(patterns::names).collect(),
        _ => Vec::new(),
    }
}

fn binding_pairs(bindings: &Value) -> Vec<(Option<Value>, Vec<Symbol>, Vec<Value>)> {
    let Value::List(items) = bindings else { return Vec::new() };
    items
        .iter()
        .map(|b| match b {
            Value::List(pair) => match pair.first() {
                Some(binder) if matches!(binder, Value::Symbol(_)) || patterns::is_pattern(binder) => {
                    (Some(binder.clone()), patterns::names(binder), pair.iter().skip(1).cloned().collect())
                }
                _ => (None, Vec::new(), pair.iter().cloned().collect()),
            },
            other => (None, Vec::new(), vec![other.clone()]),
        })
        .collect()
}

impl<'a> Expander<'a> {
    pub fn new(globals: &'a mut Globals) -> Expander<'a> {
        Expander { globals, scopes: Vec::new() }
    }

    fn is_bound(&self, symbol: Symbol) -> bool {
        self.scopes.iter().any(|scope| scope.contains(&symbol))
    }

    fn special(&self, head: &Value) -> Option<Symbol> {
        let Value::Symbol(s) = head else { return None };
        if self.is_bound(*s) {
            return None;
        }
        Some(s.root())
    }

    fn macro_at_head(&self, form: &Value) -> Option<Rc<Macro>> {
        let Value::List(l) = form else { return None };
        self.globals.macro_for(self.special(l.first()?)?)
    }

    fn with_scope<T>(&mut self, names: Vec<Symbol>, body: impl FnOnce(&mut Self) -> T) -> T {
        self.scopes.push(names.into_iter().collect());
        let result = body(self);
        self.scopes.pop();
        result
    }

    pub fn expand(&mut self, form: &Value) -> Result<Value, Error> {
        match form {
            Value::List(l) if !l.is_empty() => self.expand_list(l),
            Value::Vector(v) => Ok(Value::vector(self.expand_all(v.iter())?)),
            Value::Set(s) => Ok(Value::set(self.expand_all(s.iter())?)),
            Value::Map(m) => {
                let mut pairs = Vec::new();
                for (k, v) in m.iter() {
                    pairs.push((self.expand(k)?, self.expand(v)?));
                }
                Ok(Value::map(pairs))
            }
            other => Ok(other.clone()),
        }
    }

    fn expand_all<'v>(&mut self, forms: impl Iterator<Item = &'v Value>) -> Result<Vec<Value>, Error> {
        forms.map(|f| self.expand(f)).collect()
    }

    fn expand_head(&mut self, form: &Value) -> Result<Value, Error> {
        let mut form = form.clone();
        while let Some(m) = self.macro_at_head(&form) {
            form = m.expand(&form)?;
        }
        Ok(form)
    }

    fn flatten_body_form(&mut self, form: &Value, out: &mut Vec<Value>) -> Result<(), Error> {
        let expanded = self.expand_head(form)?;
        if let Value::List(l) = &expanded {
            if l.first().and_then(|h| self.special(h)).is_some_and(|s| &*s.name() == "begin") {
                for inner in l.rest().iter() {
                    self.flatten_body_form(inner, out)?;
                }
                return Ok(());
            }
        }
        out.push(expanded);
        Ok(())
    }

    fn define_names(&self, form: &Value) -> Vec<Symbol> {
        let Value::List(l) = form else { return Vec::new() };
        if !l.first().and_then(|h| self.special(h)).is_some_and(|s| &*s.name() == "define") {
            return Vec::new();
        }
        match l.iter().nth(1) {
            Some(Value::Symbol(name)) => vec![*name],
            Some(Value::List(signature)) => match signature.first() {
                Some(Value::Symbol(name)) => vec![*name],
                _ => Vec::new(),
            },
            Some(pattern) if patterns::is_pattern(pattern) => patterns::names(pattern),
            _ => Vec::new(),
        }
    }

    fn expand_clause(&mut self, clause: &Value) -> Result<Value, Error> {
        let Value::List(l) = clause else { return self.expand(clause) };
        let parts: Vec<Value> = l.iter().cloned().collect();
        let Some(pattern) = parts.first() else { return Ok(clause.clone()) };
        let guarded = matches!(parts.get(1), Some(Value::Keyword(k)) if &*k.name() == "when");
        let body_at = if guarded { 3 } else { 1 };
        let expanded = self.with_scope(patterns::names(pattern), |e| {
            let mut out = Vec::new();
            if guarded {
                out.push(parts[1].clone());
                if let Some(guard) = parts.get(2) {
                    out.push(e.expand(guard)?);
                }
            }
            out.extend(e.expand_body(parts.get(body_at..).unwrap_or(&[]))?);
            Ok::<_, Error>(out)
        })?;
        Ok(Value::list(std::iter::once(pattern.clone()).chain(expanded)))
    }

    fn expand_body(&mut self, forms: &[Value]) -> Result<Vec<Value>, Error> {
        let mut flat = Vec::new();
        for form in forms {
            self.flatten_body_form(form, &mut flat)?;
        }
        let defined: Vec<Symbol> = flat.iter().flat_map(|f| self.define_names(f)).collect();
        if let Some(scope) = self.scopes.last_mut() {
            scope.extend(defined);
        }
        self.expand_all(flat.iter())
    }

    fn rebuild(head: &Value, items: Vec<Value>) -> Value {
        Value::list(std::iter::once(head.clone()).chain(items))
    }

    fn expand_list(&mut self, list: &List) -> Result<Value, Error> {
        let head = list.first().unwrap();
        let Some(sym) = self.special(head) else {
            return Ok(Value::list(self.expand_all(list.iter())?));
        };
        if let Some(m) = self.globals.macro_for(sym) {
            let expanded = m.expand(&Value::List(list.clone()))?;
            return self.expand(&expanded);
        }
        let items: Vec<Value> = list.iter().skip(1).cloned().collect();
        let expanded = match &*sym.name() {
            "quote" => return Ok(Value::List(list.clone())),
            "quasiquote" => {
                let Some(inner) = items.first() else { return Ok(Value::List(list.clone())) };
                vec![self.expand_quasi(inner, 1)?]
            }
            "define-syntax" => {
                if let (Some(Value::Symbol(name)), Some(spec)) = (items.first(), items.get(1)) {
                    self.globals.define_macro(name.root(), Rc::new(Macro::parse(spec)?));
                }
                return Ok(Value::List(list.clone()));
            }
            "lambda" => {
                let Some(params) = items.first() else { return Ok(Value::List(list.clone())) };
                let body = self.with_scope(param_names(params), |e| e.expand_body(&items[1..]))?;
                std::iter::once(params.clone()).chain(body).collect()
            }
            "define" => match items.first() {
                Some(Value::List(signature)) => {
                    let params = Value::List(signature.rest());
                    let body = self.with_scope(param_names(&params), |e| e.expand_body(&items[1..]))?;
                    std::iter::once(items[0].clone()).chain(body).collect()
                }
                _ => self.expand_all(items.iter())?,
            },
            "let" | "let*" | "loop" => self.expand_let(&sym.name(), &items)?,
            "match" => {
                let mut out = Vec::new();
                let mut rest = items.iter();
                if let Some(subject) = rest.next() {
                    out.push(self.expand(subject)?);
                }
                for clause in rest {
                    out.push(self.expand_clause(clause)?);
                }
                out
            }
            "try" => {
                let mut out = Vec::new();
                for item in &items {
                    let clause = match item {
                        Value::List(l) => l.first().and_then(|h| self.special(h)).map(|s| s.name()),
                        _ => None,
                    };
                    match (clause.as_deref(), item) {
                        (Some("catch"), Value::List(l)) => {
                            let parts: Vec<Value> = l.iter().cloned().collect();
                            let var = match parts.get(1) {
                                Some(Value::Symbol(s)) => vec![*s],
                                _ => Vec::new(),
                            };
                            let body = self.with_scope(var, |e| e.expand_body(parts.get(2..).unwrap_or(&[])))?;
                            out.push(Value::list(parts.iter().take(2).cloned().chain(body)));
                        }
                        (Some("finally"), Value::List(l)) => {
                            let parts: Vec<Value> = l.iter().cloned().collect();
                            let body = self.expand_all(parts[1..].iter())?;
                            out.push(Value::list(parts.iter().take(1).cloned().chain(body)));
                        }
                        _ => out.push(self.expand(item)?),
                    }
                }
                out
            }
            _ => self.expand_all(items.iter())?,
        };
        Ok(Self::rebuild(head, expanded))
    }

    fn expand_quasi(&mut self, form: &Value, depth: usize) -> Result<Value, Error> {
        match form {
            Value::List(l) if !l.is_empty() => {
                let head = l.first().unwrap();
                let tag = self.special(head).map(|s| s.name());
                match tag.as_deref() {
                    Some("unquote") | Some("unquote-splicing") if l.len() == 2 => {
                        let arg = l.iter().nth(1).unwrap();
                        let expanded = if depth == 1 { self.expand(arg)? } else { self.expand_quasi(arg, depth - 1)? };
                        Ok(Value::list([head.clone(), expanded]))
                    }
                    Some("quasiquote") if l.len() == 2 => {
                        let arg = l.iter().nth(1).unwrap();
                        Ok(Value::list([head.clone(), self.expand_quasi(arg, depth + 1)?]))
                    }
                    _ => {
                        let mut out = Vec::new();
                        for item in l.iter() {
                            out.push(self.expand_quasi(item, depth)?);
                        }
                        Ok(Value::list(out))
                    }
                }
            }
            Value::Vector(v) => {
                let mut out = Vec::new();
                for item in v.iter() {
                    out.push(self.expand_quasi(item, depth)?);
                }
                Ok(Value::vector(out))
            }
            Value::Map(m) => {
                let mut pairs = Vec::new();
                for (k, v) in m.iter() {
                    pairs.push((self.expand_quasi(k, depth)?, self.expand_quasi(v, depth)?));
                }
                Ok(Value::map(pairs))
            }
            Value::Set(s) => {
                let mut out = Vec::new();
                for item in s.iter() {
                    out.push(self.expand_quasi(item, depth)?);
                }
                Ok(Value::set(out))
            }
            other => Ok(other.clone()),
        }
    }

    fn expand_let(&mut self, form: &str, items: &[Value]) -> Result<Vec<Value>, Error> {
        let (name, rest) = match (form, items.first()) {
            ("let", Some(Value::Symbol(name))) => (Some(*name), &items[1..]),
            _ => (None, items),
        };
        let Some(bindings) = rest.first() else { return Ok(items.to_vec()) };
        let pairs = binding_pairs(bindings);
        let mut names: Vec<Symbol> = name.into_iter().collect();
        let mut expanded_bindings = Vec::new();
        for (binder, bound, inits) in &pairs {
            let inits = if form == "let*" {
                self.with_scope(names.clone(), |e| e.expand_all(inits.iter()))?
            } else {
                self.expand_all(inits.iter())?
            };
            if let Some(binder) = binder {
                names.extend(bound.iter().copied());
                expanded_bindings.push(Value::list(std::iter::once(binder.clone()).chain(inits)));
            } else {
                expanded_bindings.push(Value::list(inits));
            }
        }
        let body = self.with_scope(names, |e| e.expand_body(&rest[1..]))?;
        let mut out: Vec<Value> = name.map(Value::Symbol).into_iter().collect();
        out.push(Value::list(expanded_bindings));
        out.extend(body);
        Ok(out)
    }
}
