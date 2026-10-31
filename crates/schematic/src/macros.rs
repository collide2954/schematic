use std::collections::HashMap;

use crate::error::Error;
use crate::symbol::Symbol;
use crate::value::Value;

pub struct Macro {
    literals: Vec<Symbol>,
    rules: Vec<(Vec<Value>, Value)>,
    ellipsis: Symbol,
    underscore: Symbol,
}

#[derive(Clone)]
enum Binding {
    One(Value),
    Seq(Vec<Binding>),
}

type Bindings = HashMap<Symbol, Binding>;
type Renames = HashMap<Symbol, Symbol>;

fn syntax(message: String) -> Error {
    Error::syntax(message)
}

fn elements(value: &Value) -> Option<Vec<Value>> {
    match value {
        Value::List(l) => Some(l.iter().cloned().collect()),
        Value::Vector(v) => Some(v.iter().cloned().collect()),
        _ => None,
    }
}

fn rebuild(template: &Value, items: Vec<Value>) -> Value {
    match template {
        Value::List(_) => Value::list(items),
        _ => Value::vector(items),
    }
}

pub fn strip_aliases(value: &Value) -> Value {
    match value {
        Value::Symbol(s) => Value::Symbol(s.root()),
        Value::List(l) => Value::list(l.iter().map(strip_aliases)),
        Value::Vector(v) => Value::vector(v.iter().map(strip_aliases)),
        Value::Map(m) => Value::map(m.iter().map(|(k, v)| (strip_aliases(k), strip_aliases(v)))),
        Value::Set(s) => Value::set(s.iter().map(strip_aliases)),
        other => other.clone(),
    }
}

impl Macro {
    pub fn parse(spec: &Value) -> Result<Macro, Error> {
        let bad = || syntax("define-syntax expects (syntax-rules (literal ...) (pattern template) ...)".into());
        let Value::List(parts) = spec else { return Err(bad()) };
        let mut parts = parts.iter();
        match parts.next() {
            Some(Value::Symbol(s)) if &*s.root().name() == "syntax-rules" => {}
            _ => return Err(bad()),
        }
        let Some(Value::List(literal_list)) = parts.next() else { return Err(bad()) };
        let literals = literal_list
            .iter()
            .map(|v| match v {
                Value::Symbol(s) => Ok(s.root()),
                other => Err(syntax(format!("syntax-rules literal {other} is not a symbol"))),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut rules = Vec::new();
        for rule in parts {
            let items = elements(rule).unwrap_or_default();
            let [pattern, template] = items.as_slice() else {
                return Err(syntax(format!("syntax-rules rule {rule} must be (pattern template)")));
            };
            let Some(pattern) = elements(pattern) else {
                return Err(syntax(format!("syntax-rules pattern {pattern} must be a list")));
            };
            if pattern.is_empty() {
                return Err(syntax("syntax-rules pattern must start with the macro name or _".into()));
            }
            rules.push((pattern[1..].to_vec(), template.clone()));
        }
        Ok(Macro { literals, rules, ellipsis: Symbol::intern("..."), underscore: Symbol::intern("_") })
    }

    pub fn expand(&self, form: &Value) -> Result<Value, Error> {
        let args: Vec<Value> = elements(form).unwrap_or_default().into_iter().skip(1).collect();
        for (pattern, template) in &self.rules {
            let mut bindings = Bindings::new();
            if self.match_seq(pattern, &args, &mut bindings) {
                let mut renames = Renames::new();
                return self.expand_template(template, &bindings, &mut renames);
            }
        }
        Err(syntax(format!("no syntax-rules pattern matches {form}")))
    }

    fn is_ellipsis(&self, value: &Value) -> bool {
        matches!(value, Value::Symbol(s) if s.root() == self.ellipsis)
    }

    fn is_literal(&self, symbol: Symbol) -> bool {
        self.literals.contains(&symbol.root())
    }

    fn match_pattern(&self, pattern: &Value, form: &Value, bindings: &mut Bindings) -> bool {
        match pattern {
            Value::Symbol(s) if s.root() == self.underscore => true,
            Value::Symbol(s) if self.is_literal(*s) => {
                matches!(form, Value::Symbol(f) if f.root() == s.root())
            }
            Value::Symbol(s) => {
                bindings.insert(*s, Binding::One(form.clone()));
                true
            }
            Value::List(_) if !matches!(form, Value::List(_)) => false,
            Value::Vector(_) if !matches!(form, Value::Vector(_)) => false,
            Value::List(_) | Value::Vector(_) => {
                self.match_seq(&elements(pattern).unwrap(), &elements(form).unwrap(), bindings)
            }
            other => other == form,
        }
    }

    fn match_seq(&self, patterns: &[Value], forms: &[Value], bindings: &mut Bindings) -> bool {
        let Some(i) = patterns.iter().position(|p| self.is_ellipsis(p)) else {
            return patterns.len() == forms.len()
                && patterns.iter().zip(forms).all(|(p, f)| self.match_pattern(p, f, bindings));
        };
        if i == 0 {
            return false;
        }
        let (before, repeated, after) = (&patterns[..i - 1], &patterns[i - 1], &patterns[i + 1..]);
        if forms.len() < before.len() + after.len() {
            return false;
        }
        let middle_end = forms.len() - after.len();
        if !before.iter().zip(forms).all(|(p, f)| self.match_pattern(p, f, bindings)) {
            return false;
        }
        if !after.iter().zip(&forms[middle_end..]).all(|(p, f)| self.match_pattern(p, f, bindings)) {
            return false;
        }
        let mut matches = Vec::new();
        for form in &forms[before.len()..middle_end] {
            let mut inner = Bindings::new();
            if !self.match_pattern(repeated, form, &mut inner) {
                return false;
            }
            matches.push(inner);
        }
        for var in self.pattern_vars(repeated) {
            let seq = matches.iter_mut().map(|m| m.remove(&var).unwrap_or(Binding::Seq(Vec::new()))).collect();
            bindings.insert(var, Binding::Seq(seq));
        }
        true
    }

    fn pattern_vars(&self, pattern: &Value) -> Vec<Symbol> {
        let mut vars = Vec::new();
        self.collect_pattern_vars(pattern, &mut vars);
        vars
    }

    fn collect_pattern_vars(&self, pattern: &Value, vars: &mut Vec<Symbol>) {
        match pattern {
            Value::Symbol(s) => {
                let root = s.root();
                if root != self.underscore && root != self.ellipsis && !self.is_literal(*s) {
                    vars.push(*s);
                }
            }
            Value::List(_) | Value::Vector(_) => {
                elements(pattern).unwrap().iter().for_each(|p| self.collect_pattern_vars(p, vars));
            }
            _ => {}
        }
    }

    fn expand_template(&self, template: &Value, bindings: &Bindings, renames: &mut Renames) -> Result<Value, Error> {
        match template {
            Value::Symbol(s) => match bindings.get(s) {
                Some(Binding::One(v)) => Ok(v.clone()),
                Some(Binding::Seq(_)) => {
                    Err(syntax(format!("pattern variable {s} is bound by ... and must be followed by ...")))
                }
                None => Ok(Value::Symbol(*renames.entry(*s).or_insert_with(|| Symbol::fresh_alias(*s)))),
            },
            Value::List(_) | Value::Vector(_) => {
                let items = elements(template).unwrap();
                if items.len() == 2 && self.is_ellipsis(&items[0]) {
                    return self.expand_escaped(&items[1], bindings, renames);
                }
                Ok(rebuild(template, self.expand_seq(&items, bindings, renames)?))
            }
            Value::Map(m) => {
                let mut pairs = Vec::new();
                for (k, v) in m.iter() {
                    pairs.push((self.expand_template(k, bindings, renames)?, self.expand_template(v, bindings, renames)?));
                }
                Ok(Value::map(pairs))
            }
            Value::Set(s) => {
                let mut items = Vec::new();
                for item in s.iter() {
                    items.push(self.expand_template(item, bindings, renames)?);
                }
                Ok(Value::set(items))
            }
            other => Ok(other.clone()),
        }
    }

    fn expand_escaped(&self, template: &Value, bindings: &Bindings, renames: &mut Renames) -> Result<Value, Error> {
        match template {
            Value::Symbol(s) if s.root() == self.ellipsis => Ok(template.clone()),
            Value::List(_) | Value::Vector(_) => {
                let mut items = Vec::new();
                for item in elements(template).unwrap() {
                    items.push(self.expand_escaped(&item, bindings, renames)?);
                }
                Ok(rebuild(template, items))
            }
            other => self.expand_template(other, bindings, renames),
        }
    }

    fn expand_seq(&self, items: &[Value], bindings: &Bindings, renames: &mut Renames) -> Result<Vec<Value>, Error> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < items.len() {
            let mut depth = 0;
            while items.get(i + 1 + depth).is_some_and(|v| self.is_ellipsis(v)) {
                depth += 1;
            }
            if depth == 0 {
                out.push(self.expand_template(&items[i], bindings, renames)?);
            } else {
                self.expand_repeated(&items[i], depth, bindings, renames, &mut out)?;
            }
            i += 1 + depth;
        }
        Ok(out)
    }

    fn expand_repeated(
        &self,
        item: &Value,
        depth: usize,
        bindings: &Bindings,
        renames: &mut Renames,
        out: &mut Vec<Value>,
    ) -> Result<(), Error> {
        let mut vars = Vec::new();
        self.collect_template_vars(item, &mut vars);
        let seqs: Vec<(Symbol, &Vec<Binding>)> = vars
            .iter()
            .filter_map(|v| match bindings.get(v) {
                Some(Binding::Seq(items)) => Some((*v, items)),
                _ => None,
            })
            .collect();
        let Some((_, first)) = seqs.first() else {
            return Err(syntax(format!("... follows {item}, which has no pattern variables bound by ...")));
        };
        let len = first.len();
        if seqs.iter().any(|(_, items)| items.len() != len) {
            return Err(syntax(format!("pattern variables in {item} matched different numbers of forms")));
        }
        for k in 0..len {
            let mut inner = bindings.clone();
            for (var, items) in &seqs {
                inner.insert(*var, items[k].clone());
            }
            if depth == 1 {
                out.push(self.expand_template(item, &inner, renames)?);
            } else {
                self.expand_repeated(item, depth - 1, &inner, renames, out)?;
            }
        }
        Ok(())
    }

    fn collect_template_vars(&self, template: &Value, vars: &mut Vec<Symbol>) {
        match template {
            Value::Symbol(s) => vars.push(*s),
            Value::List(_) | Value::Vector(_) => {
                elements(template).unwrap().iter().for_each(|t| self.collect_template_vars(t, vars));
            }
            _ => {}
        }
    }
}
