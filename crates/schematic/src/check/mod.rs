mod annotations;
mod builtins;
pub mod types;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::compiler::Compiler;
use crate::interpreter::Interpreter;
use crate::list::List;
use crate::patterns;
use crate::reader::{self, Span, Spans, read_all_spanned};
use crate::symbol::Symbol;
use crate::value::Value;
use crate::vm::{Arity, Globals, Native};
use types::{Signature, Type, array, element, fits, is_callable, join, like_first, list, map, rest_of, set, union, vector};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub column: usize,
    pub span: Span,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

pub fn check_source(src: &str, dir: Option<&Path>, load_path: &[PathBuf]) -> Vec<Diagnostic> {
    let mut interp = Interpreter::new();
    let globals = interp.globals_mut();
    globals.load_path = load_path.to_vec();
    let mut checker = Checker::new(globals);
    checker.check_text(src.to_string(), dir);
    checker.finish()
}

pub fn check_file(path: &Path, load_path: &[PathBuf]) -> Vec<Diagnostic> {
    match std::fs::read_to_string(path) {
        Ok(src) => check_source(&src, path.parent().filter(|p| !p.as_os_str().is_empty()).or(Some(Path::new("."))), load_path),
        Err(e) => vec![Diagnostic { line: 1, column: 1, span: Span::default(), message: format!("cannot read {}: {e}", path.display()) }],
    }
}

struct Source {
    text: String,
    spans: Spans,
}

#[derive(Default)]
struct Params {
    positional: Vec<(String, Type)>,
    rest: Option<Type>,
    bindings: Vec<(Symbol, Type)>,
}

type Part = (Value, Span);

const SPECIALS: &[&str] = &[
    "quote", "quasiquote", "unquote", "unquote-splicing", "define-syntax", "ns", "if", "define", "set!", "begin", "let", "let*", "match", "and",
    "or", "cond", "when", "unless", "loop", "recur", "try", "call/ec", "lambda",
];

fn is_symbol(value: &Value, name: &str) -> bool {
    matches!(value, Value::Symbol(s) if &*s.root().name() == name)
}

fn is_keyword(value: &Value, name: &str) -> bool {
    matches!(value, Value::Keyword(k) if &*k.name() == name)
}

fn split(name: Symbol) -> Option<(Symbol, Symbol)> {
    let text = name.name();
    let at = text.find('/')?;
    if at == 0 || at + 1 >= text.len() {
        return None;
    }
    Some((Symbol::intern(&text[..at]), Symbol::intern(&text[at + 1..])))
}

fn quoted_type(form: &Value) -> Type {
    match form {
        Value::Symbol(_) => Type::Symbol,
        Value::List(_) => list(Type::Any),
        Value::Vector(_) => vector(Type::Any),
        Value::Map(_) => map(Type::Any, Type::Any),
        Value::Set(_) => set(Type::Any),
        other => literal_type(other).unwrap_or(Type::Any),
    }
}

fn literal_type(form: &Value) -> Option<Type> {
    Some(match form {
        Value::Nil => Type::Nil,
        Value::Bool(_) => Type::Bool,
        Value::Int(_) => Type::Int,
        Value::Float(_) => Type::Float,
        Value::Str(_) => Type::Str,
        Value::Keyword(_) => Type::Keyword,
        Value::Date(_) => Type::Date,
        Value::Array(_) => array(Type::Any),
        Value::Table(_) => Type::Table,
        Value::Plot(_) => Type::Plot,
        _ => return None,
    })
}

fn native_signature(native: &Native) -> Signature {
    let table = builtins::lookup(native.name);
    let (required, max) = match native.arity {
        Arity::Exact(n) => (n, Some(n)),
        Arity::AtLeast(n) => (n, None),
        Arity::Between(lo, hi) => (lo, Some(hi)),
    };
    let count = max.unwrap_or(required);
    let expected = |i: usize| table.and_then(|b| b.expected(i)).cloned().unwrap_or(Type::Any);
    let params = (0..count).map(|i| (String::new(), expected(i))).collect();
    let rest = max.is_none().then(|| expected(count));
    Signature {
        name: native.name.to_string(),
        params,
        required,
        rest,
        ret: table.map_or(Type::Any, |b| b.ret.clone()),
        broadcast: table.is_some_and(|b| b.broadcast),
    }
}

fn concrete(t: &Type) -> bool {
    !matches!(t, Type::Any | Type::Union(_) | Type::Labeled(..))
}

fn numeric_result(name: &str, args: &[Type]) -> Type {
    if !args.iter().all(concrete) {
        return Type::Any;
    }
    if args.iter().any(|t| matches!(t, Type::Array(_))) {
        return array(Type::Any);
    }
    let dates = args.iter().filter(|t| **t == Type::Date).count();
    match name {
        "/" => Type::Float,
        "-" if dates == 2 => Type::Int,
        "+" | "-" | "inc" | "dec" | "abs" if dates == 1 => Type::Date,
        "max" | "min" if args.iter().all(|t| matches!(t, Type::Int | Type::Float)) => union(args.iter().cloned()),
        _ if args.iter().all(|t| *t == Type::Int) => Type::Int,
        _ if args.iter().all(|t| matches!(t, Type::Int | Type::Float)) => Type::Float,
        _ => Type::Any,
    }
}

fn maybe_array(t: &Type) -> bool {
    match t {
        Type::Any | Type::Array(_) => true,
        Type::Union(items) => items.iter().any(maybe_array),
        Type::Labeled(_, inner) => maybe_array(inner),
        _ => false,
    }
}

struct Checker<'a> {
    globals: &'a mut Globals,
    types: HashMap<usize, Type>,
    predeclared: HashMap<(Symbol, Symbol), Type>,
    unknown: HashSet<Symbol>,
    dynamic: bool,
    mutated: HashSet<Symbol>,
    scopes: Vec<Vec<(Symbol, Type)>>,
    sources: Vec<Source>,
    diagnostics: Vec<Diagnostic>,
    pending: Vec<(usize, Diagnostic)>,
    lenient: usize,
}

impl<'a> Checker<'a> {
    fn new(globals: &'a mut Globals) -> Checker<'a> {
        Checker {
            globals,
            types: HashMap::new(),
            predeclared: HashMap::new(),
            unknown: HashSet::new(),
            dynamic: false,
            mutated: HashSet::new(),
            scopes: Vec::new(),
            sources: Vec::new(),
            diagnostics: Vec::new(),
            pending: Vec::new(),
            lenient: 0,
        }
    }

    fn finish(mut self) -> Vec<Diagnostic> {
        for (slot, diagnostic) in std::mem::take(&mut self.pending) {
            let defined = self.types.contains_key(&slot) || self.globals.get(slot).is_some();
            if !defined && !self.dynamic {
                self.diagnostics.push(diagnostic);
            }
        }
        self.diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
        self.diagnostics.dedup();
        self.diagnostics
    }

    fn check_text(&mut self, text: String, dir: Option<&Path>) {
        let (forms, spans) = match read_all_spanned(&text) {
            Ok(read) => read,
            Err(e) => {
                let span = Span { start: e.offset, end: e.offset };
                self.diagnostics.push(Diagnostic { line: e.line, column: e.col, span, message: e.message });
                return;
            }
        };
        self.sources.push(Source { text, spans });
        if let Some(dir) = dir {
            self.globals.load_path.insert(0, dir.to_path_buf());
        }
        self.predeclare(&forms);
        for (form, _) in &forms {
            self.collect_mutations(form);
        }
        for (form, span) in &forms {
            if let Err(e) = Compiler::new(self.globals).compile_toplevel(form) {
                self.report(*span, e.message());
                continue;
            }
            self.infer(form, *span);
        }
        if dir.is_some() {
            self.globals.load_path.remove(0);
        }
        self.sources.pop();
    }

    fn check_nested(&mut self, path: &Path) {
        let Ok(text) = std::fs::read_to_string(path) else {
            self.dynamic = true;
            return;
        };
        let previous = self.globals.current();
        let scopes = std::mem::take(&mut self.scopes);
        let lenient = std::mem::take(&mut self.lenient);
        let keep = (self.diagnostics.len(), self.pending.len());
        let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        self.check_text(text, Some(&dir));
        self.diagnostics.truncate(keep.0);
        self.pending.truncate(keep.1);
        self.scopes = scopes;
        self.lenient = lenient;
        self.globals.set_current(previous);
    }

    fn source(&self) -> &Source {
        self.sources.last().expect("a source being checked")
    }

    fn diagnostic(&self, span: Span, message: String) -> Diagnostic {
        let (line, column) = reader::position(&self.source().text, span.start);
        Diagnostic { line, column, span, message }
    }

    fn report(&mut self, span: Span, message: String) {
        let diagnostic = self.diagnostic(span, message);
        self.diagnostics.push(diagnostic);
    }

    fn item_span(&self, container: &Value, index: usize, item: &Value, fallback: Span) -> Span {
        let spans = &self.source().spans;
        spans.item(container, index).or_else(|| spans.of(item)).unwrap_or(fallback)
    }

    fn parts(&self, form: &Value, at: Span) -> Vec<Part> {
        let items: Vec<Value> = match form {
            Value::List(l) => l.iter().cloned().collect(),
            Value::Vector(v) => v.iter().cloned().collect(),
            _ => return Vec::new(),
        };
        items.into_iter().enumerate().map(|(i, item)| {
            let span = self.item_span(form, i, &item, at);
            (item, span)
        }).collect()
    }

    fn predeclare(&mut self, forms: &[Part]) {
        let mut namespace = self.globals.current();
        for (form, _) in forms {
            self.predeclare_form(form, &mut namespace);
        }
    }

    fn predeclare_form(&mut self, form: &Value, namespace: &mut Symbol) {
        let Value::List(l) = form else { return };
        let items: Vec<&Value> = l.iter().collect();
        let Some(Value::Symbol(head)) = items.first() else { return };
        match &*head.root().name() {
            "ns" => {
                if let Some(Value::Symbol(name)) = items.get(1) {
                    *namespace = name.root();
                }
            }
            "begin" => items[1..].iter().for_each(|item| self.predeclare_form(item, namespace)),
            "define" => match items.get(1) {
                Some(signature @ Value::List(parts)) => {
                    let Some(Value::Symbol(name)) = parts.first() else { return };
                    let params = self.parse_params(signature, 1, None);
                    let ret = return_annotation(&items[2..]).and_then(|kind| annotations::parse(kind).ok()).unwrap_or(Type::Any);
                    let required = params.positional.len();
                    let signature = Signature { name: name.to_string(), params: params.positional, required, rest: params.rest, ret, broadcast: false };
                    self.predeclared.insert((*namespace, name.root()), Type::Fn(Box::new(signature)));
                }
                Some(Value::Symbol(name)) => {
                    if let Some(kind) = value_annotation(&items[2..]).and_then(|kind| annotations::parse(kind).ok()) {
                        self.predeclared.insert((*namespace, name.root()), kind);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn collect_mutations(&mut self, form: &Value) {
        match form {
            Value::List(l) => {
                if is_symbol(l.first().unwrap_or(&Value::Nil), "quote") {
                    return;
                }
                if is_symbol(l.first().unwrap_or(&Value::Nil), "set!")
                    && let Some(Value::Symbol(target)) = l.iter().nth(1) {
                        self.mutated.insert(target.root());
                    }
                l.iter().for_each(|item| self.collect_mutations(item));
            }
            Value::Vector(v) => v.iter().for_each(|item| self.collect_mutations(item)),
            Value::Map(m) => m.iter().for_each(|(k, v)| {
                self.collect_mutations(k);
                self.collect_mutations(v);
            }),
            _ => {}
        }
    }

    fn lookup_local(&self, name: Symbol) -> Option<Type> {
        self.scopes.iter().rev().find_map(|scope| scope.iter().rev().find(|(n, _)| *n == name).map(|(_, t)| t.clone()))
    }

    fn is_local(&self, name: Symbol) -> bool {
        self.scopes.iter().any(|scope| scope.iter().any(|(n, _)| *n == name))
    }

    fn set_local(&mut self, name: Symbol, t: Type) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(entry) = scope.iter_mut().rev().find(|(n, _)| *n == name) {
                entry.1 = t;
                return;
            }
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((name, t));
        }
    }

    fn define(&mut self, name: Symbol, t: Type) {
        if !self.scopes.is_empty() {
            self.set_local(name, t);
            return;
        }
        let slot = self.globals.define_slot(name.root());
        self.globals.set(slot, Value::Nil);
        self.types.insert(slot, t);
    }

    fn parse_type(&mut self, kind: &Value, at: Option<Span>) -> Type {
        match annotations::parse(kind) {
            Ok(t) => t,
            Err(message) => {
                if let Some(span) = at {
                    self.report(span, message);
                }
                Type::Any
            }
        }
    }

    fn parse_params(&mut self, container: &Value, offset: usize, at: Option<Span>) -> Params {
        let mut out = Params::default();
        let items: Vec<&Value> = match container {
            Value::Symbol(s) => {
                out.rest = Some(Type::Any);
                out.bindings.push((*s, list(Type::Any)));
                return out;
            }
            Value::List(l) => l.iter().skip(offset).collect(),
            _ => return out,
        };
        let span_of = |checker: &Checker, index: usize, item: &Value| at.map(|fallback| checker.item_span(container, offset + index, item, fallback));
        let mut i = 0;
        while i < items.len() {
            let item = items[i];
            if is_symbol(item, "&") {
                if let Some(rest) = items.get(i + 1) {
                    let span = span_of(self, i + 1, rest).unwrap_or_default();
                    let bound = self.bind_pattern(rest, list(Type::Any), span, at.is_some());
                    out.bindings.extend(bound);
                }
                out.rest = Some(Type::Any);
                break;
            }
            let span = span_of(self, i, item);
            let (name, t) = match patterns::annotated(item) {
                Some((Value::Symbol(name), kind)) => {
                    let kind_span = span.map(|s| self.item_span(item, 2, kind, s));
                    let t = self.parse_type(kind, kind_span);
                    out.bindings.push((*name, t.clone()));
                    (name.to_string(), t)
                }
                Some((pattern, kind)) => {
                    let kind_span = span.map(|s| self.item_span(item, 2, kind, s));
                    let t = self.parse_type(kind, kind_span);
                    let bound = self.bind_pattern(pattern, t.clone(), span.unwrap_or_default(), at.is_some());
                    out.bindings.extend(bound);
                    (String::new(), t)
                }
                None => match item {
                    Value::Symbol(name) => {
                        out.bindings.push((*name, Type::Any));
                        (name.to_string(), Type::Any)
                    }
                    pattern => {
                        let bound = self.bind_pattern(pattern, Type::Any, span.unwrap_or_default(), at.is_some());
                        out.bindings.extend(bound);
                        (String::new(), Type::Any)
                    }
                },
            };
            out.positional.push((name, t));
            i += 1;
        }
        out
    }

    fn bind_pattern(&mut self, pattern: &Value, t: Type, at: Span, reporting: bool) -> Vec<(Symbol, Type)> {
        let mut out = Vec::new();
        self.bind_into(pattern, t, at, reporting, &mut out);
        out
    }

    fn bind_into(&mut self, pattern: &Value, t: Type, at: Span, reporting: bool, out: &mut Vec<(Symbol, Type)>) {
        if let Some((inner, kind)) = patterns::annotated(pattern) {
            let kind_span = self.item_span(pattern, 2, kind, at);
            let declared = self.parse_type(kind, reporting.then_some(kind_span));
            if reporting && !fits(&t, &declared) {
                let what = match inner {
                    Value::Symbol(name) => name.to_string(),
                    other => other.to_string(),
                };
                self.report(at, format!("{what} is declared {} but given {}", declared.describe(), t.describe()));
            }
            return self.bind_into(inner, declared, at, reporting, out);
        }
        match pattern {
            Value::Symbol(s) => {
                if &*s.name() != "_" {
                    out.push((*s, t));
                }
            }
            Value::Vector(items) => {
                let elem = join(element(&t), Type::Nil);
                let items: Vec<&Value> = items.iter().collect();
                let mut i = 0;
                while i < items.len() {
                    let item = items[i];
                    if is_symbol(item, "&") {
                        if let Some(rest) = items.get(i + 1) {
                            self.bind_into(rest, rest_of(&t), at, reporting, out);
                        }
                        i += 2;
                    } else if is_keyword(item, "as") {
                        if let Some(Value::Symbol(name)) = items.get(i + 1) {
                            out.push((*name, t.clone()));
                        }
                        i += 2;
                    } else {
                        self.bind_into(item, elem.clone(), at, reporting, out);
                        i += 1;
                    }
                }
            }
            Value::Map(entries) => {
                for (key, value) in entries.iter() {
                    if is_keyword(key, "keys") {
                        if let Value::Vector(names) = value {
                            for name in names.iter() {
                                if let Value::Symbol(s) | Value::Keyword(s) = name {
                                    out.push((*s, Type::Any));
                                }
                            }
                        }
                    } else if is_keyword(key, "as") {
                        if let Value::Symbol(s) = value {
                            out.push((*s, t.clone()));
                        }
                    } else if !is_keyword(key, "or") {
                        self.bind_into(value, Type::Any, at, reporting, out);
                    }
                }
            }
            _ => {}
        }
    }

    fn slot_type(&mut self, slot: usize, name: Symbol, at: Span) -> Type {
        if let Some(t) = self.types.get(&slot) {
            return t.clone();
        }
        match self.globals.get(slot) {
            Some(Value::Native(native)) => Type::Fn(Box::new(native_signature(native))),
            Some(Value::Closure(closure)) => {
                let table = builtins::lookup(&self.globals.name(slot).name());
                let required = closure.proto.params;
                let expected = |i: usize| table.and_then(|b| b.expected(i)).cloned().unwrap_or(Type::Any);
                let params = (0..required).map(|i| (String::new(), expected(i))).collect();
                let rest = closure.proto.rest.then(|| expected(required));
                let ret = table.map_or(Type::Any, |b| b.ret.clone());
                Type::Fn(Box::new(Signature { name: name.to_string(), params, required, rest, ret, broadcast: false }))
            }
            Some(_) => Type::Any,
            None => {
                let owner = self.globals.owner(slot);
                if self.unknown.contains(&owner) {
                    return Type::Any;
                }
                if let Some(t) = self.predeclared.get(&(owner, self.globals.name(slot))) {
                    return t.clone();
                }
                let diagnostic = self.diagnostic(at, format!("{name} is not defined"));
                self.pending.push((slot, diagnostic));
                Type::Any
            }
        }
    }

    fn symbol_type(&mut self, symbol: Symbol, at: Span) -> Type {
        if self.mutated.contains(&symbol.root()) {
            return Type::Any;
        }
        if let Some(t) = self.lookup_local(symbol) {
            return t;
        }
        let name = symbol.root();
        match self.globals.find(name) {
            Some(slot) => self.slot_type(slot, name, at),
            None => {
                if let Some((prefix, _)) = split(name)
                    && self.globals.resolve_prefix(prefix).is_none_or(|target| self.unknown.contains(&target)) {
                        return Type::Any;
                    }
                let diagnostic = self.diagnostic(at, format!("{name} is not defined"));
                self.pending.push((usize::MAX, diagnostic));
                Type::Any
            }
        }
    }

    fn infer(&mut self, form: &Value, at: Span) -> Type {
        match form {
            Value::Symbol(s) => self.symbol_type(*s, at),
            Value::List(l) if !l.is_empty() => self.infer_list(form, l, at),
            Value::List(_) => list(Type::Any),
            Value::Vector(items) => {
                let mut elem: Option<Type> = None;
                for (i, item) in items.iter().enumerate() {
                    let span = self.item_span(form, i, item, at);
                    let t = self.infer(item, span);
                    elem = Some(match elem {
                        Some(e) => join(e, t),
                        None => t,
                    });
                }
                vector(elem.unwrap_or(Type::Any))
            }
            Value::Map(entries) => {
                let mut key: Option<Type> = None;
                let mut value: Option<Type> = None;
                for (k, v) in entries.iter() {
                    let kt = self.infer(k, at);
                    let vt = self.infer(v, at);
                    key = Some(key.map_or(kt.clone(), |e| join(e, kt)));
                    value = Some(value.map_or(vt.clone(), |e| join(e, vt)));
                }
                map(key.unwrap_or(Type::Any), value.unwrap_or(Type::Any))
            }
            Value::Set(items) => {
                let mut elem: Option<Type> = None;
                for item in items.iter() {
                    let t = self.infer(item, at);
                    elem = Some(elem.map_or(t.clone(), |e| join(e, t)));
                }
                set(elem.unwrap_or(Type::Any))
            }
            other => literal_type(other).unwrap_or(Type::Any),
        }
    }

    fn infer_list(&mut self, form: &Value, l: &List, at: Span) -> Type {
        let head = l.first().unwrap();
        if let Value::Symbol(s) = head
            && !self.is_local(*s) {
                let name = s.root();
                if let Some(definition) = self.globals.macro_for(name) {
                    return match definition.expand(form) {
                        Ok(expanded) => self.infer(&expanded, at),
                        Err(_) => Type::Any,
                    };
                }
                let text = name.name();
                if SPECIALS.contains(&&*text) {
                    let parts = self.parts(form, at);
                    return self.infer_special(&text, form, &parts, at);
                }
            }
        self.infer_call(form, at)
    }

    fn infer_special(&mut self, name: &str, form: &Value, parts: &[Part], at: Span) -> Type {
        match name {
            "quote" => parts.get(1).map_or(Type::Nil, |(datum, _)| quoted_type(datum)),
            "quasiquote" | "unquote" | "unquote-splicing" | "recur" => {
                for (item, span) in &parts[1..] {
                    if name == "recur" {
                        self.infer(item, *span);
                    }
                }
                Type::Any
            }
            "define-syntax" => Type::Symbol,
            "ns" => self.infer_ns(parts),
            "if" => {
                if let Some((test, span)) = parts.get(1) {
                    self.infer(test, *span);
                }
                let then = parts.get(2).map_or(Type::Nil, |(form, span)| self.infer(form, *span));
                let otherwise = parts.get(3).map_or(Type::Nil, |(form, span)| self.infer(form, *span));
                join(then, otherwise)
            }
            "define" => self.infer_define(form, parts, at),
            "set!" => parts.get(2).map_or(Type::Nil, |(value, span)| self.infer(value, *span)),
            "begin" => {
                let mut t = Type::Nil;
                for (item, span) in &parts[1..] {
                    t = self.infer(item, *span);
                }
                t
            }
            "let" => self.infer_let(parts, false, at),
            "let*" => self.infer_let(parts, true, at),
            "loop" => {
                let Some((bindings, span)) = parts.get(1) else { return Type::Any };
                let bindings = self.parts(bindings, *span);
                self.infer_loop(None, bindings, &parts[2..])
            }
            "match" => self.infer_match(parts),
            "and" | "or" => {
                if parts.len() == 1 {
                    return Type::Bool;
                }
                let mut t: Option<Type> = None;
                for (item, span) in &parts[1..] {
                    let it = self.infer(item, *span);
                    t = Some(t.map_or(it.clone(), |e| join(e, it)));
                }
                t.unwrap_or(Type::Bool)
            }
            "cond" => self.infer_cond(parts),
            "when" | "unless" => {
                if let Some((test, span)) = parts.get(1) {
                    self.infer(test, *span);
                }
                let body = self.check_body(&parts[2.min(parts.len())..]);
                join(body, Type::Nil)
            }
            "try" => self.infer_try(parts),
            "call/ec" => {
                if let Some((f, span)) = parts.get(1) {
                    self.infer(f, *span);
                }
                Type::Any
            }
            "lambda" => {
                let Some((params, _)) = parts.get(1) else { return Type::Any };
                self.infer_function(None, params, 0, &parts[2..], at)
            }
            _ => Type::Any,
        }
    }

    fn infer_define(&mut self, form: &Value, parts: &[Part], at: Span) -> Type {
        let Some((target, target_span)) = parts.get(1) else { return Type::Symbol };
        match target {
            Value::Symbol(name) => {
                let annotated = matches!(parts.get(2), Some((colon, _)) if is_symbol(colon, ":"));
                let declared = annotated.then(|| parts.get(3)).flatten().map(|(kind, span)| self.parse_type(kind, Some(*span)));
                let value_index = if annotated { 4 } else { 2 };
                let (value_t, value_span) = match parts.get(value_index) {
                    Some((value, span)) => (self.infer(value, *span), *span),
                    None => (Type::Nil, at),
                };
                let t = match declared {
                    Some(declared) => {
                        if !fits(&value_t, &declared) {
                            self.report(value_span, format!("{name} is declared {} but given {}", declared.describe(), value_t.describe()));
                        }
                        declared
                    }
                    None => value_t,
                };
                self.define(*name, t);
            }
            Value::List(signature) => {
                if let Some(Value::Symbol(name)) = signature.first() {
                    self.infer_function(Some(*name), target, 1, &parts[2..], at);
                }
            }
            pattern => {
                let value_t = parts.get(2).map_or(Type::Nil, |(value, span)| self.infer(value, *span));
                let bound = self.bind_pattern(pattern, value_t, *target_span, true);
                for (name, t) in bound {
                    self.define(name, t);
                }
            }
        }
        let _ = form;
        Type::Symbol
    }

    fn infer_function(&mut self, name: Option<Symbol>, params: &Value, offset: usize, body: &[Part], at: Span) -> Type {
        let params_span = self.source().spans.of(params).unwrap_or(at);
        let parsed = self.parse_params(params, offset, Some(params_span));
        let (declared, body) = match body {
            [(colon, _), (kind, kind_span), rest @ ..] if is_symbol(colon, ":") => (Some(self.parse_type(kind, Some(*kind_span))), rest),
            _ => (None, body),
        };
        let label = name.map_or_else(|| "this function".to_string(), |n| n.to_string());
        let mut signature = Signature {
            name: label.clone(),
            required: parsed.positional.len(),
            params: parsed.positional,
            rest: parsed.rest,
            ret: declared.clone().unwrap_or(Type::Any),
            broadcast: false,
        };
        if let Some(name) = name {
            self.define(name, Type::Fn(Box::new(signature.clone())));
        }
        self.scopes.push(parsed.bindings);
        let body_t = self.check_body(body);
        self.scopes.pop();
        signature.ret = match declared {
            Some(declared) => {
                if !fits(&body_t, &declared) {
                    let span = body.last().map_or(at, |(_, span)| *span);
                    self.report(span, format!("{label} is declared to return {} but returns {}", declared.describe(), body_t.describe()));
                }
                declared
            }
            None => body_t,
        };
        let t = Type::Fn(Box::new(signature));
        if let Some(name) = name {
            self.define(name, t.clone());
        }
        t
    }

    fn internal_names(&self, form: &Value, out: &mut Vec<(Symbol, Type)>) {
        let Value::List(l) = form else { return };
        let items: Vec<&Value> = l.iter().collect();
        let Some(head) = items.first() else { return };
        if is_symbol(head, "begin") {
            items[1..].iter().for_each(|item| self.internal_names(item, out));
        } else if is_symbol(head, "define") {
            match items.get(1) {
                Some(Value::Symbol(name)) => out.push((*name, Type::Any)),
                Some(Value::List(signature)) => {
                    if let Some(Value::Symbol(name)) = signature.first() {
                        out.push((*name, Type::Any));
                    }
                }
                Some(pattern) => out.extend(patterns::names(pattern).into_iter().map(|n| (n, Type::Any))),
                None => {}
            }
        }
    }

    fn check_body(&mut self, body: &[Part]) -> Type {
        let mut names = Vec::new();
        for (form, _) in body {
            self.internal_names(form, &mut names);
        }
        self.scopes.push(names);
        let mut t = Type::Nil;
        for (form, span) in body {
            t = self.infer(form, *span);
        }
        self.scopes.pop();
        t
    }

    fn infer_let(&mut self, parts: &[Part], sequential: bool, at: Span) -> Type {
        let (name, bindings_at) = match parts.get(1) {
            Some((Value::Symbol(name), _)) if !sequential => (Some(*name), 2),
            _ => (None, 1),
        };
        let Some((bindings, span)) = parts.get(bindings_at) else { return Type::Any };
        let bindings = self.parts(bindings, *span);
        let body = &parts[(bindings_at + 1).min(parts.len())..];
        if let Some(name) = name {
            return self.infer_loop(Some(name), bindings, body);
        }
        let mut scope = Vec::new();
        if sequential {
            self.scopes.push(Vec::new());
        }
        for (binding, binding_span) in bindings {
            let pair = self.parts(&binding, binding_span);
            let (Some((pattern, pattern_span)), Some((init, init_span))) = (pair.first(), pair.get(1)) else { continue };
            let init_t = self.infer(init, *init_span);
            let bound = self.bind_pattern(pattern, init_t, *pattern_span, true);
            if sequential {
                self.scopes.last_mut().unwrap().extend(bound);
            } else {
                scope.extend(bound);
            }
        }
        if !sequential {
            self.scopes.push(scope);
        }
        let t = self.check_body(body);
        self.scopes.pop();
        let _ = at;
        t
    }

    fn infer_loop(&mut self, name: Option<Symbol>, bindings: Vec<Part>, body: &[Part]) -> Type {
        let mut scope = Vec::new();
        let mut params = Vec::new();
        for (binding, binding_span) in bindings {
            let pair = self.parts(&binding, binding_span);
            let (Some((pattern, pattern_span)), Some((init, init_span))) = (pair.first(), pair.get(1)) else { continue };
            let init_t = self.infer(init, *init_span);
            let var_t = match patterns::annotated(pattern) {
                Some((_, kind)) => {
                    let kind_span = self.item_span(pattern, 2, kind, *pattern_span);
                    let declared = self.parse_type(kind, Some(kind_span));
                    if !fits(&init_t, &declared) {
                        let what = patterns::binder(pattern).to_string();
                        self.report(*pattern_span, format!("{what} is declared {} but given {}", declared.describe(), init_t.describe()));
                    }
                    declared
                }
                None => Type::Any,
            };
            let binder = patterns::binder(pattern).clone();
            let label = match &binder {
                Value::Symbol(s) => s.to_string(),
                _ => String::new(),
            };
            scope.extend(self.bind_pattern(&binder, var_t.clone(), *pattern_span, true));
            params.push((label, var_t));
        }
        if let Some(name) = name {
            let required = params.len();
            let signature = Signature { name: name.to_string(), params, required, rest: None, ret: Type::Any, broadcast: false };
            scope.push((name, Type::Fn(Box::new(signature))));
        }
        self.scopes.push(scope);
        let t = self.check_body(body);
        self.scopes.pop();
        t
    }

    fn infer_match(&mut self, parts: &[Part]) -> Type {
        let Some((subject, span)) = parts.get(1) else { return Type::Any };
        let subject_t = self.infer(subject, *span);
        let mut result: Option<Type> = None;
        for (clause, clause_span) in &parts[2..] {
            let items = self.parts(clause, *clause_span);
            let Some((pattern, pattern_span)) = items.first() else { continue };
            let guarded = matches!(items.get(1), Some((k, _)) if is_keyword(k, "when"));
            let body_at = if guarded { 3 } else { 1 };
            let bindings = if is_symbol(pattern, "else") { Vec::new() } else { self.bind_pattern(pattern, subject_t.clone(), *pattern_span, true) };
            self.scopes.push(bindings);
            if guarded && let Some((guard, guard_span)) = items.get(2) {
                self.infer(guard, *guard_span);
            }
            let t = self.check_body(&items[body_at.min(items.len())..]);
            self.scopes.pop();
            result = Some(result.map_or(t.clone(), |r| join(r, t)));
        }
        result.unwrap_or(Type::Any)
    }

    fn infer_cond(&mut self, parts: &[Part]) -> Type {
        let mut result: Option<Type> = None;
        let mut has_else = false;
        for (clause, clause_span) in &parts[1..] {
            let items = self.parts(clause, *clause_span);
            let Some((test, test_span)) = items.first() else { continue };
            let t = if is_symbol(test, "else") {
                has_else = true;
                self.check_body(&items[1..])
            } else {
                let test_t = self.infer(test, *test_span);
                if items.len() == 1 { test_t } else { self.check_body(&items[1..]) }
            };
            result = Some(result.map_or(t.clone(), |r| join(r, t)));
        }
        let result = result.unwrap_or(Type::Nil);
        if has_else { result } else { join(result, Type::Nil) }
    }

    fn infer_try(&mut self, parts: &[Part]) -> Type {
        let mut body = Vec::new();
        let mut catches = Vec::new();
        let mut finally = Vec::new();
        for part in &parts[1..] {
            let clause = match &part.0 {
                Value::List(l) => l.first().map(|h| (is_symbol(h, "catch"), is_symbol(h, "finally"))),
                _ => None,
            };
            match clause {
                Some((true, _)) => catches.push(part.clone()),
                Some((_, true)) => finally.push(part.clone()),
                _ => body.push(part.clone()),
            }
        }
        if !catches.is_empty() {
            self.lenient += 1;
        }
        let mut t = self.check_body(&body);
        if !catches.is_empty() {
            self.lenient -= 1;
        }
        for (clause, span) in catches {
            let items = self.parts(&clause, span);
            let scope = match items.get(1) {
                Some((Value::Symbol(name), _)) => vec![(*name, Type::Any)],
                _ => Vec::new(),
            };
            self.scopes.push(scope);
            let handler = self.check_body(&items[2.min(items.len())..]);
            self.scopes.pop();
            t = join(t, handler);
        }
        for (clause, span) in finally {
            let items = self.parts(&clause, span);
            self.check_body(&items[1..]);
        }
        t
    }

    fn infer_call(&mut self, form: &Value, at: Span) -> Type {
        let parts = self.parts(form, at);
        let (head, head_span) = &parts[0];
        if let Value::Symbol(s) = head
            && !self.is_local(*s)
            && let Some(Value::Native(native)) = self.globals.lookup(s.root())
            && matches!(native.name, "require" | "refer" | "alias" | "load") {
                let name = native.name;
                return self.namespace_call(name, &parts[1..]);
            }
        let callee = self.infer(head, *head_span);
        let mut args = Vec::with_capacity(parts.len() - 1);
        for (arg, span) in &parts[1..] {
            args.push((self.infer(arg, *span), *span));
        }
        let label = match head {
            Value::Symbol(s) => s.to_string(),
            other => other.to_string(),
        };
        self.apply(callee, &label, &args, at)
    }

    fn apply(&mut self, callee: Type, label: &str, args: &[(Type, Span)], at: Span) -> Type {
        match callee {
            Type::Labeled(_, inner) => self.apply(*inner, label, args, at),
            Type::Fn(signature) => {
                if !signature.accepts(args.len()) {
                    self.report(at, format!("{label} expects {}, got {}", signature.arity(), args.len()));
                    return signature.ret.clone();
                }
                if self.lenient == 0 {
                    for (i, (actual, span)) in args.iter().enumerate() {
                        let Some(expected) = signature.expected(i) else { continue };
                        if fits(actual, expected) {
                            continue;
                        }
                        let message = match signature.params.get(i).filter(|(name, _)| !name.is_empty()) {
                            Some((name, _)) => format!("{label} expects {} for {name}, got {}", expected.describe(), actual.describe()),
                            None => format!("{label} expects {}, got {}", expected.describe(), actual.describe()),
                        };
                        self.report(*span, message);
                    }
                }
                let types: Vec<Type> = args.iter().map(|(t, _)| t.clone()).collect();
                self.result_type(&signature, &types)
            }
            Type::Union(ref items) if items.iter().any(is_callable) => Type::Any,
            other if is_callable(&other) => Type::Any,
            other => {
                if self.lenient == 0 {
                    self.report(at, format!("{label} is {}, not a function", other.describe()));
                }
                Type::Any
            }
        }
    }

    fn result_type(&self, signature: &Signature, args: &[Type]) -> Type {
        match signature.name.as_str() {
            name @ ("+" | "-" | "*" | "/" | "quot" | "rem" | "mod" | "max" | "min" | "abs" | "inc" | "dec" | "pow") => numeric_result(name, args),
            "conj" | "assoc" | "dissoc" | "update" | "assoc-in" | "update-in" | "into" | "disj" | "merge" => like_first(args),
            "first" | "second" | "last" => args.first().map_or(Type::Any, element),
            "identity" => args.first().cloned().unwrap_or(Type::Any),
            _ if signature.broadcast && args.iter().any(|t| matches!(t, Type::Array(_))) => array(Type::Any),
            _ if signature.broadcast && args.iter().any(maybe_array) => union([signature.ret.clone(), array(Type::Any)]),
            _ => signature.ret.clone(),
        }
    }

    fn infer_ns(&mut self, parts: &[Part]) -> Type {
        let Some((Value::Symbol(name), _)) = parts.get(1) else { return Type::Nil };
        self.globals.set_current(name.root());
        for (clause, span) in &parts[2..] {
            let items = self.parts(clause, *span);
            if matches!(items.first(), Some((k, _)) if is_keyword(k, "require")) {
                for (spec, spec_span) in &items[1..] {
                    self.require_spec(spec, *spec_span);
                }
            }
        }
        Type::Nil
    }

    fn unquote(form: &Value) -> Value {
        match form {
            Value::List(l) if l.len() == 2 && is_symbol(l.first().unwrap(), "quote") => l.iter().nth(1).cloned().unwrap_or(Value::Nil),
            other => other.clone(),
        }
    }

    fn namespace_call(&mut self, name: &str, args: &[Part]) -> Type {
        match name {
            "require" => {
                for (spec, span) in args {
                    let spec = Self::unquote(spec);
                    self.require_spec(&spec, *span);
                }
            }
            "alias" => {
                if let (Some((alias, _)), Some((target, span))) = (args.first(), args.get(1))
                    && let (Value::Symbol(alias), Value::Symbol(target)) = (Self::unquote(alias), Self::unquote(target)) {
                        self.ensure_loaded(target.root(), *span);
                        let _ = self.globals.alias(alias.root(), target.root());
                    }
            }
            "refer" => {
                if let Some((target, span)) = args.first()
                    && let Value::Symbol(target) = Self::unquote(target) {
                        let target = target.root();
                        self.ensure_loaded(target, *span);
                        match args.get(2).map(|(names, _)| Self::unquote(names)) {
                            Some(Value::Vector(names)) => {
                                let names: Vec<Symbol> = names.iter().filter_map(|n| if let Value::Symbol(s) = n { Some(s.root()) } else { None }).collect();
                                self.refer_names(target, &names, *span);
                            }
                            _ => self.refer_all(target),
                        }
                    }
            }
            "load" => {
                if let Some((Value::Str(path), _)) = args.first() {
                    let path = Path::new(path.as_str());
                    let found = if path.is_file() || path.is_absolute() {
                        Some(path.to_path_buf())
                    } else {
                        self.globals.load_path.iter().map(|dir| dir.join(path)).find(|candidate| candidate.is_file())
                    };
                    match found {
                        Some(path) => self.check_nested(&path),
                        None => self.dynamic = true,
                    }
                } else {
                    self.dynamic = true;
                }
            }
            _ => {}
        }
        Type::Nil
    }

    fn require_spec(&mut self, spec: &Value, at: Span) {
        let (name, options): (Symbol, Vec<Value>) = match spec {
            Value::Symbol(s) => (s.root(), Vec::new()),
            Value::Vector(v) => match v.iter().next() {
                Some(Value::Symbol(s)) => (s.root(), v.iter().skip(1).cloned().collect()),
                _ => return,
            },
            _ => return,
        };
        self.ensure_loaded(name, at);
        let mut options = options.into_iter();
        while let (Some(key), Some(value)) = (options.next(), options.next()) {
            match (&key, &value) {
                (k, Value::Symbol(alias)) if is_keyword(k, "as") => {
                    let _ = self.globals.alias(alias.root(), name);
                }
                (k, all) if is_keyword(k, "refer") && is_keyword(all, "all") => self.refer_all(name),
                (k, Value::Vector(names)) if is_keyword(k, "refer") => {
                    let names: Vec<Symbol> = names.iter().filter_map(|n| if let Value::Symbol(s) = n { Some(s.root()) } else { None }).collect();
                    self.refer_names(name, &names, at);
                }
                _ => {}
            }
        }
    }

    fn refer_all(&mut self, target: Symbol) {
        if self.unknown.contains(&target) {
            self.dynamic = true;
        }
        let _ = self.globals.refer(target, None);
    }

    fn refer_names(&mut self, target: Symbol, names: &[Symbol], at: Span) {
        if self.unknown.contains(&target) {
            let current = self.globals.current();
            self.globals.set_current(target);
            for &name in names {
                self.globals.define_slot(name);
            }
            self.globals.set_current(current);
        }
        if let Err(e) = self.globals.refer(target, Some(names)) {
            self.report(at, e.message());
        }
    }

    fn ensure_loaded(&mut self, name: Symbol, at: Span) {
        if self.globals.loading.contains(&name) {
            self.report(at, format!("circular require of {name}"));
            return;
        }
        if self.globals.has_namespace(name) {
            return;
        }
        let relative = PathBuf::from(format!("{}.scm", name.name().replace('.', "/")));
        let found = self.globals.load_path.iter().map(|dir| dir.join(&relative)).find(|candidate| candidate.is_file());
        if let Some(path) = found {
            self.globals.loading.push(name);
            self.check_nested(&path);
            self.globals.loading.pop();
        }
        if !self.globals.has_namespace(name) {
            self.globals.create(name);
            self.unknown.insert(name);
        }
    }
}

fn return_annotation<'v>(body: &[&'v Value]) -> Option<&'v Value> {
    match body {
        [colon, kind, ..] if is_symbol(colon, ":") => Some(kind),
        _ => None,
    }
}

fn value_annotation<'v>(rest: &[&'v Value]) -> Option<&'v Value> {
    match rest {
        [colon, kind, _] if is_symbol(colon, ":") => Some(kind),
        _ => None,
    }
}
