use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::array::Array;
use crate::compiler::syntax_error;
use crate::error::Error;
use crate::list::List;
use crate::map::Map;
use crate::symbol::Symbol;
use crate::value::Value;
use crate::vector::Vector;
use crate::vm::{Arity, Native, NativeFn};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Bind,
    Match,
}

type Bindings = Vec<(Symbol, Value)>;

fn fresh(hint: &str) -> Value {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    Value::Symbol(Symbol::intern(&format!(" {hint}{n}")))
}

fn sym(name: &str) -> Value {
    Value::symbol(name)
}

fn list(items: impl IntoIterator<Item = Value>) -> Value {
    Value::list(items)
}

fn is_symbol(value: &Value, name: &str) -> bool {
    matches!(value, Value::Symbol(s) if &*s.name() == name)
}

fn is_keyword(value: &Value, name: &str) -> bool {
    matches!(value, Value::Keyword(k) if &*k.name() == name)
}

pub fn is_pattern(value: &Value) -> bool {
    matches!(value, Value::Vector(_) | Value::Map(_))
}

pub fn annotated(value: &Value) -> Option<(&Value, &Value)> {
    let Value::Vector(items) = value else { return None };
    let mut items = items.iter();
    match (items.next(), items.next(), items.next(), items.next()) {
        (Some(binder), Some(Value::Symbol(colon)), Some(kind), None) if &*colon.root().name() == ":" => Some((binder, kind)),
        _ => None,
    }
}

pub fn binder(value: &Value) -> &Value {
    annotated(value).map_or(value, |(inner, _)| inner)
}

pub fn without_return_type(body: &List) -> Result<List, Error> {
    if !matches!(body.first(), Some(Value::Symbol(s)) if &*s.root().name() == ":") {
        return Ok(body.clone());
    }
    if body.len() < 3 {
        return Err(syntax_error("define", "expected a type and a body after :"));
    }
    Ok(body.rest().rest())
}

fn quoted(form: &Value) -> Option<&Value> {
    let Value::List(l) = form else { return None };
    (l.len() == 2 && is_symbol(l.first()?, "quote")).then(|| l.iter().nth(1)).flatten()
}

fn native(name: &'static str, arity: Arity, func: fn(&[Value]) -> Result<Value, Error>) -> Value {
    Value::Native(Rc::new(Native { name, arity, func: NativeFn::Pure(func), intrinsic: None }))
}

fn not_sequence(value: &Value) -> Error {
    Error::type_error(format!("cannot destructure {value} as a sequence"))
}

fn not_map(value: &Value) -> Error {
    Error::type_error(format!("cannot destructure {value} as a map"))
}

fn count(value: &Value) -> Result<usize, Error> {
    let Value::Int(n) = value else { unreachable!() };
    Ok(*n as usize)
}

fn nth(args: &[Value]) -> Result<Value, Error> {
    let i = count(&args[1])?;
    Ok(match &args[0] {
        Value::Nil => Value::Nil,
        Value::Vector(v) => v.iter().nth(i).cloned().unwrap_or(Value::Nil),
        Value::List(l) => l.iter().nth(i).cloned().unwrap_or(Value::Nil),
        Value::Array(a) => a.get(i).unwrap_or(Value::Nil),
        Value::Str(s) => s.chars().nth(i).map_or(Value::Nil, |c| Value::string(&c.to_string())),
        other => return Err(not_sequence(other)),
    })
}

fn rest(args: &[Value]) -> Result<Value, Error> {
    let n = count(&args[1])?;
    Ok(match &args[0] {
        Value::Nil => Value::Nil,
        Value::Vector(v) => Value::vector(v.iter().skip(n).cloned()),
        Value::List(l) => Value::list(l.iter().skip(n).cloned()),
        Value::Array(a) => Value::Array(Rc::new(Array::from_values(a.iter().skip(n))?)),
        Value::Str(s) => Value::string(&s.chars().skip(n).collect::<String>()),
        other => return Err(not_sequence(other)),
    })
}

fn lookup(args: &[Value]) -> Result<Value, Error> {
    let fallback = || args[2].clone();
    Ok(match &args[0] {
        Value::Nil => fallback(),
        Value::Map(m) => m.get(&args[1]).cloned().unwrap_or_else(fallback),
        Value::Vector(v) => match &args[1] {
            Value::Int(i) if *i >= 0 => v.iter().nth(*i as usize).cloned().unwrap_or_else(fallback),
            _ => fallback(),
        },
        other => return Err(not_map(other)),
    })
}

fn is_sequence(args: &[Value]) -> Result<Value, Error> {
    let (n, exact) = (count(&args[1])?, args[2].is_truthy());
    let len = match &args[0] {
        Value::Vector(v) => v.len(),
        Value::List(l) => l.len(),
        Value::Array(a) => a.len(),
        Value::Str(s) => s.chars().count(),
        _ => return Ok(Value::Bool(false)),
    };
    Ok(Value::Bool(if exact { len == n } else { len >= n }))
}

fn has_key(args: &[Value]) -> Result<Value, Error> {
    Ok(Value::Bool(matches!(&args[0], Value::Map(m) if m.contains_key(&args[1]))))
}

fn is_map(args: &[Value]) -> Result<Value, Error> {
    Ok(Value::Bool(matches!(args[0], Value::Map(_))))
}

fn same(args: &[Value]) -> Result<Value, Error> {
    Ok(Value::Bool(args[0] == args[1]))
}

fn no_match(args: &[Value]) -> Result<Value, Error> {
    Err(Error::new("match-error", format!("no match clause for {}", args[0])))
}

fn nth_form(source: &Value, i: usize) -> Value {
    list([native("nth", Arity::Exact(2), nth), source.clone(), Value::Int(i as i64)])
}

fn rest_form(source: &Value, n: usize) -> Value {
    list([native("rest", Arity::Exact(2), rest), source.clone(), Value::Int(n as i64)])
}

fn lookup_form(source: &Value, key: Value, default: Value) -> Value {
    list([native("get", Arity::Exact(3), lookup), source.clone(), key, default])
}

struct SeqPattern<'a> {
    positional: Vec<&'a Value>,
    rest: Option<&'a Value>,
    whole: Option<Symbol>,
}

fn parse_seq<'a>(form: &str, items: &'a Vector) -> Result<SeqPattern<'a>, Error> {
    let items: Vec<&Value> = items.iter().collect();
    let mut pattern = SeqPattern { positional: Vec::new(), rest: None, whole: None };
    let mut i = 0;
    while i < items.len() {
        let item = items[i];
        if is_symbol(item, "&") {
            pattern.rest = Some(items.get(i + 1).copied().ok_or_else(|| syntax_error(form, "expected a pattern after &"))?);
            i += 2;
        } else if is_keyword(item, "as") {
            match items.get(i + 1) {
                Some(Value::Symbol(name)) => pattern.whole = Some(*name),
                _ => return Err(syntax_error(form, "expected a name after :as")),
            }
            i += 2;
        } else if pattern.rest.is_some() {
            return Err(syntax_error(form, "only :as may follow the & pattern"));
        } else {
            pattern.positional.push(item);
            i += 1;
        }
    }
    Ok(pattern)
}

struct MapPattern {
    entries: Vec<(Value, Value)>,
    defaults: Bindings,
    whole: Option<Symbol>,
}

impl MapPattern {
    fn default_for(&self, pattern: &Value) -> Value {
        match pattern {
            Value::Symbol(s) => self.defaults.iter().find(|(n, _)| n == s).map_or(Value::Nil, |(_, d)| d.clone()),
            _ => Value::Nil,
        }
    }

    fn has_default(&self, pattern: &Value) -> bool {
        matches!(pattern, Value::Symbol(s) if self.defaults.iter().any(|(n, _)| n == s))
    }
}

fn parse_map(form: &str, map: &Map) -> Result<MapPattern, Error> {
    let mut pattern = MapPattern { entries: Vec::new(), defaults: Vec::new(), whole: None };
    for (key, value) in map.iter() {
        if is_keyword(key, "keys") {
            let Value::Vector(names) = value else {
                return Err(syntax_error(form, "expected a vector of names after :keys"));
            };
            for name in names.iter() {
                match name {
                    Value::Symbol(s) => pattern.entries.push((name.clone(), Value::Keyword(*s))),
                    Value::Keyword(s) => pattern.entries.push((Value::Symbol(*s), name.clone())),
                    other => return Err(syntax_error(form, &format!("{other} is not a name for :keys"))),
                }
            }
        } else if is_keyword(key, "or") {
            let Value::Map(defaults) = value else {
                return Err(syntax_error(form, "expected a map of defaults after :or"));
            };
            for (name, default) in defaults.iter() {
                let Value::Symbol(s) = name else {
                    return Err(syntax_error(form, &format!("{name} is not a name for :or")));
                };
                pattern.defaults.push((*s, default.clone()));
            }
        } else if is_keyword(key, "as") {
            let Value::Symbol(s) = value else {
                return Err(syntax_error(form, "expected a name after :as"));
            };
            pattern.whole = Some(*s);
        } else {
            pattern.entries.push((value.clone(), key.clone()));
        }
    }
    Ok(pattern)
}

fn bind_sub(form: &str, sub: &Value, init: Value, mode: Mode, out: &mut Bindings) -> Result<(), Error> {
    match binder(sub) {
        Value::Symbol(s) if &*s.name() == "_" => Ok(()),
        Value::Symbol(s) => {
            out.push((*s, init));
            Ok(())
        }
        Value::Vector(_) | Value::Map(_) => {
            let temp = fresh("p");
            let Value::Symbol(t) = &temp else { unreachable!() };
            out.push((*t, init));
            destructure(form, sub, &temp, mode, out)
        }
        other => destructure(form, other, &init, mode, out),
    }
}

fn destructure(form: &str, pattern: &Value, source: &Value, mode: Mode, out: &mut Bindings) -> Result<(), Error> {
    match binder(pattern) {
        Value::Symbol(s) => {
            if &*s.name() != "_" {
                out.push((*s, source.clone()));
            }
        }
        Value::Vector(items) => {
            let seq = parse_seq(form, items)?;
            for (i, sub) in seq.positional.iter().enumerate() {
                bind_sub(form, sub, nth_form(source, i), mode, out)?;
            }
            if let Some(sub) = seq.rest {
                bind_sub(form, sub, rest_form(source, seq.positional.len()), mode, out)?;
            }
            if let Some(whole) = seq.whole {
                out.push((whole, source.clone()));
            }
        }
        Value::Map(map) => {
            let entries = parse_map(form, map)?;
            for (sub, key) in &entries.entries {
                bind_sub(form, sub, lookup_form(source, key.clone(), entries.default_for(sub)), mode, out)?;
            }
            if let Some(whole) = entries.whole {
                out.push((whole, source.clone()));
            }
        }
        other if mode == Mode::Match && (quoted(other).is_some() || !matches!(other, Value::List(_))) => {}
        other => return Err(syntax_error(form, &format!("cannot bind to {other}"))),
    }
    Ok(())
}

fn tests(form: &str, pattern: &Value, source: &Value, out: &mut Vec<Value>) -> Result<(), Error> {
    match binder(pattern) {
        Value::Symbol(_) => {}
        Value::Vector(items) => {
            let seq = parse_seq(form, items)?;
            let exact = Value::Bool(seq.rest.is_none());
            out.push(list([native("seq?", Arity::Exact(3), is_sequence), source.clone(), Value::Int(seq.positional.len() as i64), exact]));
            for (i, sub) in seq.positional.iter().enumerate() {
                tests(form, sub, &nth_form(source, i), out)?;
            }
            if let Some(sub) = seq.rest {
                tests(form, sub, &rest_form(source, seq.positional.len()), out)?;
            }
        }
        Value::Map(map) => {
            let entries = parse_map(form, map)?;
            out.push(list([native("map?", Arity::Exact(1), is_map), source.clone()]));
            for (sub, key) in &entries.entries {
                if !entries.has_default(sub) {
                    out.push(list([native("has?", Arity::Exact(2), has_key), source.clone(), key.clone()]));
                }
                tests(form, sub, &lookup_form(source, key.clone(), entries.default_for(sub)), out)?;
            }
        }
        Value::List(_) => match quoted(pattern) {
            Some(_) => out.push(list([native("same?", Arity::Exact(2), same), source.clone(), pattern.clone()])),
            None => return Err(syntax_error(form, &format!("cannot match against {pattern}"))),
        },
        literal => out.push(list([native("same?", Arity::Exact(2), same), source.clone(), literal.clone()])),
    }
    Ok(())
}

fn all(tests: Vec<Value>) -> Value {
    let mut tests = tests.into_iter().rev();
    let Some(last) = tests.next() else { return Value::Bool(true) };
    tests.fold(last, |rest, test| list([sym("if"), test, rest, Value::Bool(false)]))
}

fn binding_list(bindings: &Bindings) -> Value {
    list(bindings.iter().map(|(name, init)| list([Value::Symbol(*name), init.clone()])))
}

fn body_form(forms: &[Value]) -> Value {
    match forms {
        [single] => single.clone(),
        many => list([sym("let"), list([])].into_iter().chain(many.iter().cloned())),
    }
}

pub fn names(pattern: &Value) -> Vec<Symbol> {
    fn collect(pattern: &Value, out: &mut Vec<Symbol>) {
        match binder(pattern) {
            Value::Symbol(s) if !matches!(&*s.name(), "&" | "_") => out.push(*s),
            Value::Vector(items) => items.iter().for_each(|item| collect(item, out)),
            Value::Map(map) => {
                for (key, value) in map.iter() {
                    if is_keyword(key, "keys") {
                        if let Value::Vector(items) = value {
                            for item in items.iter() {
                                match item {
                                    Value::Symbol(s) | Value::Keyword(s) => out.push(*s),
                                    _ => {}
                                }
                            }
                        }
                    } else if !is_keyword(key, "or") {
                        collect(value, out);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    collect(pattern, &mut out);
    out
}

pub fn rewrite_bindings(form: &str, args: &List) -> Result<Option<List>, Error> {
    let items: Vec<Value> = args.iter().cloned().collect();
    let prefix = usize::from(form == "let" && matches!(items.first(), Some(Value::Symbol(_))));
    let Some(Value::List(bindings)) = items.get(prefix) else { return Ok(None) };
    let has_pattern = |binding: &Value| matches!(binding, Value::List(pair) if pair.first().is_some_and(is_pattern));
    if !bindings.iter().any(has_pattern) {
        return Ok(None);
    }
    let sequential = form == "let*";
    let mut outer = Vec::new();
    let mut inner = Vec::new();
    for binding in bindings.iter() {
        let pair: Vec<&Value> = match binding {
            Value::List(pair) => pair.iter().collect(),
            _ => Vec::new(),
        };
        match pair.as_slice() {
            [pattern, init] if matches!(binder(pattern), Value::Symbol(_)) => {
                outer.push(list([binder(pattern).clone(), (*init).clone()]));
            }
            [pattern, init] if is_pattern(pattern) => {
                let temp = fresh("p");
                outer.push(list([temp.clone(), (*init).clone()]));
                if sequential {
                    let mut subs = Vec::new();
                    destructure(form, pattern, &temp, Mode::Bind, &mut subs)?;
                    outer.extend(subs.iter().map(|(name, init)| list([Value::Symbol(*name), init.clone()])));
                } else {
                    destructure(form, pattern, &temp, Mode::Bind, &mut inner)?;
                }
            }
            _ => outer.push(binding.clone()),
        }
    }
    let body = &items[prefix + 1..];
    let body: Vec<Value> = if inner.is_empty() {
        body.to_vec()
    } else {
        vec![list([sym("let*"), binding_list(&inner)].into_iter().chain(body.iter().cloned()))]
    };
    let rewritten = items[..prefix].iter().cloned().chain([Value::list(outer)]).chain(body);
    Ok(Some(rewritten.collect()))
}

pub fn rewrite_params(params: &Value, body: &List) -> Result<Option<(Value, List)>, Error> {
    let Value::List(items) = params else { return Ok(None) };
    if !items.iter().any(is_pattern) {
        return Ok(None);
    }
    let mut renamed = Vec::new();
    let mut inner = Vec::new();
    for item in items.iter() {
        if is_pattern(binder(item)) {
            let temp = fresh("p");
            renamed.push(temp.clone());
            destructure("lambda", item, &temp, Mode::Bind, &mut inner)?;
        } else {
            renamed.push(binder(item).clone());
        }
    }
    if inner.is_empty() {
        return Ok(Some((Value::list(renamed), body.clone())));
    }
    let body = list([sym("let*"), binding_list(&inner)].into_iter().chain(body.iter().cloned()));
    Ok(Some((Value::list(renamed), std::iter::once(body).collect())))
}

pub fn rewrite_define(target: &Value, body: &List) -> Result<Value, Error> {
    let value = match body.iter().collect::<Vec<_>>().as_slice() {
        [value] => (*value).clone(),
        _ => return Err(syntax_error("define", "expected a single value")),
    };
    let temp = fresh("p");
    let mut bindings = Vec::new();
    destructure("define", target, &temp, Mode::Bind, &mut bindings)?;
    let is_user = |name: &Symbol| !name.name().starts_with(' ');
    let names: Vec<Value> = bindings.iter().filter(|(n, _)| is_user(n)).map(|(n, _)| Value::Symbol(*n)).collect();
    fn assign(bindings: &[(Symbol, Value)], is_user: &dyn Fn(&Symbol) -> bool, result: Value) -> Vec<Value> {
        let Some(((name, init), rest)) = bindings.split_first() else { return vec![result] };
        if is_user(name) {
            let mut out = vec![list([sym("set!"), Value::Symbol(*name), init.clone()])];
            out.extend(assign(rest, is_user, result));
            out
        } else {
            let inner = assign(rest, is_user, result);
            vec![list([sym("let*"), list([list([Value::Symbol(*name), init.clone()])])].into_iter().chain(inner))]
        }
    }
    let result = list([sym("quote"), Value::vector(names.clone())]);
    let steps = assign(&bindings, &is_user, result);
    let bind = list([sym("let"), list([list([temp, value])])].into_iter().chain(steps));
    let defines = names.iter().map(|name| list([sym("define"), name.clone(), Value::Nil]));
    Ok(list([sym("begin")].into_iter().chain(defines).chain([bind])))
}

fn clause_parts<'a>(form: &str, clause: &'a Value) -> Result<(&'a Value, Option<&'a Value>, Vec<Value>), Error> {
    let Value::List(parts) = clause else {
        return Err(syntax_error(form, &format!("clause {clause} must be a list")));
    };
    let parts: Vec<&Value> = parts.iter().collect();
    let Some(pattern) = parts.first().copied() else {
        return Err(syntax_error(form, "a clause needs a pattern and a body"));
    };
    let (guard, body) = match parts.get(1) {
        Some(keyword) if is_keyword(keyword, "when") => match parts.get(2) {
            Some(guard) => (Some(*guard), &parts[3..]),
            None => return Err(syntax_error(form, "expected a condition after :when")),
        },
        _ => (None, &parts[1..]),
    };
    if body.is_empty() {
        return Err(syntax_error(form, &format!("clause {clause} needs a body")));
    }
    Ok((pattern, guard, body.iter().map(|v| (*v).clone()).collect()))
}

fn chain(form: &str, subject: &Value, clauses: &[&Value]) -> Result<Value, Error> {
    let Some((clause, remaining)) = clauses.split_first() else {
        return Ok(list([native("no-match", Arity::Exact(1), no_match), subject.clone()]));
    };
    let (pattern, guard, body) = clause_parts(form, clause)?;
    let body = body_form(&body);
    if is_symbol(pattern, "else") {
        return Ok(body);
    }
    let mut checks = Vec::new();
    tests(form, pattern, subject, &mut checks)?;
    let mut bindings = Vec::new();
    destructure(form, pattern, subject, Mode::Match, &mut bindings)?;
    let bound = |inner: Value| {
        if bindings.is_empty() { inner } else { list([sym("let*"), binding_list(&bindings), inner]) }
    };
    let test = all(checks);
    let fallthrough = chain(form, subject, remaining)?;
    Ok(match guard {
        None => list([sym("if"), test, bound(body), fallthrough]),
        Some(guard) => {
            let next = fresh("k");
            let call_next = list([next.clone()]);
            let guarded = list([sym("if"), guard.clone(), body, call_next.clone()]);
            let thunk = list([sym("lambda"), list([]), fallthrough]);
            list([sym("let"), list([list([next, thunk])]), list([sym("if"), test, bound(guarded), call_next])])
        }
    })
}

pub fn rewrite_match(args: &List) -> Result<Value, Error> {
    let Some(subject) = args.first() else {
        return Err(syntax_error("match", "expected a value and clauses"));
    };
    let temp = fresh("m");
    let clauses: Vec<&Value> = args.iter().skip(1).collect();
    let body = chain("match", &temp, &clauses)?;
    Ok(list([sym("let"), list([list([temp, subject.clone()])]), body]))
}
