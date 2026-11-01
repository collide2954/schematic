use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::collections::expects;
use super::strings::{as_str, display};
use super::{native, native_vm};
use crate::array::Array;
use crate::error::Error;
use crate::regex::{Regex, Spans};
use crate::value::Value;
use crate::vm::{Arity, Globals, Vm};

pub fn install(globals: &mut Globals) {
    native(globals, "re-find", Arity::Exact(2), |args| {
        over("re-find", &args[0], &args[1], false, &|regex, text| Ok(match_value(regex, text, regex.find(text, 0))))
    });
    native(globals, "re-matches", Arity::Exact(2), |args| {
        over("re-matches", &args[0], &args[1], true, &|regex, text| Ok(match_value(regex, text, regex.find(text, 0))))
    });
    native(globals, "re-seq", Arity::Exact(2), |args| {
        over("re-seq", &args[0], &args[1], false, &|regex, text| {
            Ok(Value::vector(all_matches(regex, text).into_iter().map(|spans| match_value(regex, text, Some(spans)))))
        })
    });
    native(globals, "re-contains?", Arity::Exact(2), |args| {
        over("re-contains?", &args[0], &args[1], false, &|regex, text| Ok(Value::Bool(regex.find(text, 0).is_some())))
    });
    native(globals, "re-split", Arity::Exact(2), |args| over("re-split", &args[0], &args[1], false, &|regex, text| Ok(split(regex, text))));
    native_vm(globals, "re-replace", Arity::Exact(3), replace);
}

thread_local! {
    static CACHE: RefCell<HashMap<(String, bool), Rc<Regex>>> = RefCell::new(HashMap::new());
}

fn compile(name: &str, pattern: &Value, whole: bool) -> Result<Rc<Regex>, Error> {
    let pattern = as_str(name, pattern).map_err(|_| expects(name, "a pattern string", pattern))?;
    let key = (pattern.to_string(), whole);
    if let Some(found) = CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return Ok(found);
    }
    let regex = Rc::new(Regex::new(pattern, whole).map_err(|message| Error::new("value-error", format!("regex {pattern:?}: {message}")))?);
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(key, regex.clone());
    });
    Ok(regex)
}

fn over(name: &str, subject: &Value, pattern: &Value, whole: bool, f: &dyn Fn(&Regex, &[char]) -> Result<Value, Error>) -> Result<Value, Error> {
    let regex = compile(name, pattern, whole)?;
    let one = |value: &Value| -> Result<Value, Error> {
        let text: Vec<char> = as_str(name, value)?.chars().collect();
        f(&regex, &text)
    };
    broadcast(subject, &one)
}

pub fn broadcast(subject: &Value, one: &dyn Fn(&Value) -> Result<Value, Error>) -> Result<Value, Error> {
    let Value::Array(items) = subject else { return one(subject) };
    let results = items.iter().map(|item| if item == Value::Nil { Ok(Value::Nil) } else { one(&item) }).collect::<Result<Vec<_>, _>>()?;
    Ok(match Array::from_values(results.clone()) {
        Ok(array) => Value::Array(Rc::new(array)),
        Err(_) => Value::vector(results),
    })
}

fn piece(text: &[char], span: (usize, usize)) -> Value {
    Value::string(&text[span.0..span.1].iter().collect::<String>())
}

fn match_value(regex: &Regex, text: &[char], found: Option<Spans>) -> Value {
    match found {
        None => Value::Nil,
        Some(spans) if regex.groups == 0 => piece(text, spans[0].expect("a whole match")),
        Some(spans) => Value::vector(spans.iter().map(|span| span.map_or(Value::Nil, |span| piece(text, span)))),
    }
}

fn all_matches(regex: &Regex, text: &[char]) -> Vec<Spans> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(spans) = regex.find(text, pos) {
        let (start, end) = spans[0].expect("a whole match");
        pos = if end == start { end + 1 } else { end };
        out.push(spans);
        if pos > text.len() {
            break;
        }
    }
    out
}

fn split(regex: &Regex, text: &[char]) -> Value {
    let mut pieces = Vec::new();
    let mut last = 0;
    let mut pos = 0;
    while pos <= text.len() {
        let Some(spans) = regex.find(text, pos) else { break };
        let (start, end) = spans[0].expect("a whole match");
        if end == start {
            if start < text.len() && start != last {
                pieces.push(piece(text, (last, start)));
                last = start;
            }
            pos = start + 1;
        } else {
            pieces.push(piece(text, (last, start)));
            last = end;
            pos = end;
        }
    }
    pieces.push(piece(text, (last, text.len())));
    Value::vector(pieces)
}

fn expand(template: &[char], text: &[char], spans: &Spans, out: &mut String) {
    let mut i = 0;
    while i < template.len() {
        let c = template[i];
        if c != '$' {
            out.push(c);
            i += 1;
            continue;
        }
        if template.get(i + 1) == Some(&'$') {
            out.push('$');
            i += 2;
            continue;
        }
        let digits = template[i + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
        let index = template[i + 1..i + 1 + digits].iter().collect::<String>().parse::<usize>().ok();
        match index {
            Some(index) if digits > 0 && index < spans.len() => {
                if let Some(span) = spans[index] {
                    out.extend(&text[span.0..span.1]);
                }
                i += 1 + digits;
            }
            _ => {
                out.push('$');
                i += 1;
            }
        }
    }
}

fn replace(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let regex = compile("re-replace", &args[1], false)?;
    let template: Option<Vec<char>> = match &args[2] {
        Value::Str(s) => Some(s.chars().collect()),
        _ => None,
    };
    let mut one = |value: &Value| -> Result<Value, Error> {
        let text: Vec<char> = as_str("re-replace", value)?.chars().collect();
        let mut out = String::new();
        let mut last = 0;
        for spans in all_matches(&regex, &text) {
            let (start, end) = spans[0].expect("a whole match");
            out.extend(&text[last..start]);
            match &template {
                Some(template) => expand(template, &text, &spans, &mut out),
                None => {
                    let replacement = vm.call_value(&args[2], &[match_value(&regex, &text, Some(spans))])?;
                    out.push_str(&display(&replacement));
                }
            }
            last = end;
        }
        out.extend(&text[last..]);
        Ok(Value::string(&out))
    };
    match &args[0] {
        Value::Array(items) => {
            let mut results = Vec::with_capacity(items.len());
            for item in items.iter() {
                results.push(if item == Value::Nil { Value::Nil } else { one(&item)? });
            }
            Ok(match Array::from_values(results.clone()) {
                Ok(array) => Value::Array(Rc::new(array)),
                Err(_) => Value::vector(results),
            })
        }
        other => one(other),
    }
}
