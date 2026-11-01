use std::fmt::Write;
use std::rc::Rc;

use super::collections::{expects, items, owned, take};
use super::{native, native_owned};
use crate::array::{self, Array};
use crate::error::Error;
use crate::reader::parse_number;
use crate::symbol::Symbol;
use crate::value::{Shown, Value};
use crate::vm::{Arity, Globals};

pub fn install(globals: &mut Globals) {
    native_owned(globals, "str", Arity::AtLeast(0), str_fn);
    native(globals, "format", Arity::AtLeast(1), |args| format_values(&args[0], &args[1..]));
    native(globals, "substring", Arity::Between(2, 3), substring);
    native(globals, "string-length", Arity::Exact(1), |args| {
        each("string-length", &args[0], &|s| Ok(Value::Int(s.chars().count() as i64)))
    });
    native(globals, "string-split", Arity::Between(1, 2), string_split);
    native(globals, "string-join", Arity::Between(1, 2), string_join);
    native(globals, "string-upcase", Arity::Exact(1), |args| {
        each("string-upcase", &args[0], &|s| Ok(Value::string(&s.to_uppercase())))
    });
    native(globals, "string-downcase", Arity::Exact(1), |args| {
        each("string-downcase", &args[0], &|s| Ok(Value::string(&s.to_lowercase())))
    });
    native(globals, "string-trim", Arity::Exact(1), |args| each("string-trim", &args[0], &|s| Ok(Value::string(s.trim()))));
    native(globals, "string-contains?", Arity::Exact(2), |args| check("string-contains?", args, |s, p| s.contains(p)));
    native(globals, "string-starts-with?", Arity::Exact(2), |args| {
        check("string-starts-with?", args, |s, p| s.starts_with(p))
    });
    native(globals, "string-ends-with?", Arity::Exact(2), |args| check("string-ends-with?", args, |s, p| s.ends_with(p)));
    native(globals, "string-replace", Arity::Exact(3), |args| {
        let name = "string-replace";
        let (from, to) = (as_str(name, &args[1])?, as_str(name, &args[2])?);
        each(name, &args[0], &|s| Ok(Value::string(&s.replace(from, to))))
    });
    native(globals, "string-index-of", Arity::Exact(2), string_index_of);
    native(globals, "string-repeat", Arity::Exact(2), string_repeat);
    native(globals, "string-pad-left", Arity::Between(2, 3), |args| pad("string-pad-left", args, true));
    native(globals, "string-pad-right", Arity::Between(2, 3), |args| pad("string-pad-right", args, false));
    native(globals, "letter?", Arity::Exact(1), |args| class("letter?", &args[0], char::is_alphabetic));
    native(globals, "digit?", Arity::Exact(1), |args| class("digit?", &args[0], |c| c.is_ascii_digit()));
    native(globals, "whitespace?", Arity::Exact(1), |args| class("whitespace?", &args[0], char::is_whitespace));
    native(globals, "upper-case?", Arity::Exact(1), |args| class("upper-case?", &args[0], char::is_uppercase));
    native(globals, "lower-case?", Arity::Exact(1), |args| class("lower-case?", &args[0], char::is_lowercase));
    native(globals, "char-code", Arity::Exact(1), |args| {
        array::unary(&args[0], &|value| {
            let text = as_str("char-code", value)?;
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Ok(Value::Int(c as i64)),
                _ => Err(expects("char-code", "a single character", value)),
            }
        })
    });
    native(globals, "code-char", Arity::Exact(1), |args| {
        array::unary(&args[0], &|value| match value {
            Value::Int(code) => u32::try_from(*code)
                .ok()
                .and_then(char::from_u32)
                .map(|c| Value::string(c.encode_utf8(&mut [0; 4])))
                .ok_or_else(|| expects("code-char", "a character code", value)),
            other => Err(expects("code-char", "a character code", other)),
        })
    });
    native(globals, "string->number", Arity::Exact(1), string_to_number);
    native(globals, "number->string", Arity::Exact(1), |args| {
        array::unary(&args[0], &|value| match value {
            Value::Int(_) | Value::Float(_) => Ok(Value::string(&value.to_string())),
            other => Err(expects("number->string", "a number", other)),
        })
    });
    native(globals, "name", Arity::Exact(1), |args| match &args[0] {
        Value::Keyword(s) | Value::Symbol(s) => Ok(Value::string(&s.name())),
        Value::Str(_) => Ok(args[0].clone()),
        other => Err(expects("name", "a keyword, symbol, or string", other)),
    });
    native(globals, "keyword", Arity::Exact(1), |args| match &args[0] {
        Value::Str(s) => Ok(Value::keyword(s)),
        Value::Symbol(s) => Ok(Value::Keyword(*s)),
        Value::Keyword(_) => Ok(args[0].clone()),
        other => Err(expects("keyword", "a string, symbol, or keyword", other)),
    });
    native(globals, "symbol", Arity::Exact(1), |args| match &args[0] {
        Value::Str(s) => Ok(Value::Symbol(Symbol::intern(s))),
        Value::Keyword(s) => Ok(Value::Symbol(*s)),
        Value::Symbol(_) => Ok(args[0].clone()),
        other => Err(expects("symbol", "a string, symbol, or keyword", other)),
    });
}

pub fn as_str<'a>(name: &str, value: &'a Value) -> Result<&'a str, Error> {
    match value {
        Value::Str(s) => Ok(s),
        other => Err(expects(name, "a string", other)),
    }
}

pub fn display(value: &Value) -> String {
    match value {
        Value::Nil => String::new(),
        other => Shown(other).to_string(),
    }
}

fn each(name: &str, subject: &Value, f: &dyn Fn(&str) -> Result<Value, Error>) -> Result<Value, Error> {
    array::unary(subject, &|value| f(as_str(name, value)?))
}

fn class(name: &str, subject: &Value, test: fn(char) -> bool) -> Result<Value, Error> {
    each(name, subject, &|s| Ok(Value::Bool(!s.is_empty() && s.chars().all(test))))
}

fn check(name: &str, args: &[Value], test: fn(&str, &str) -> bool) -> Result<Value, Error> {
    let part = as_str(name, &args[1])?;
    each(name, &args[0], &|s| Ok(Value::Bool(test(s, part))))
}

fn str_fn(args: &mut [Value]) -> Result<Value, Error> {
    if args.is_empty() {
        return Ok(Value::string(""));
    }
    let mut out = match take(args, 0) {
        Value::Str(s) => owned(s),
        other => display(&other),
    };
    for arg in &args[1..] {
        out.push_str(&display(arg));
    }
    Ok(Value::Str(Rc::new(out)))
}

fn substring(args: &[Value]) -> Result<Value, Error> {
    let bound = |value: Option<&Value>| match value {
        None => Ok(None),
        Some(Value::Int(i)) => Ok(Some(*i)),
        Some(other) => Err(expects("substring", "integer bounds", other)),
    };
    let (start, end) = (bound(args.get(1))?.unwrap_or(0), bound(args.get(2))?);
    each("substring", &args[0], &|s| {
        let len = s.chars().count() as i64;
        let end = end.unwrap_or(len);
        if start < 0 || end < start || end > len {
            let message = format!("substring range {start} to {end} is out of bounds for a string of length {len}");
            return Err(Error::new("index-error", message));
        }
        let piece: String = s.chars().skip(start as usize).take((end - start) as usize).collect();
        Ok(Value::string(&piece))
    })
}

fn string_split(args: &[Value]) -> Result<Value, Error> {
    let separator = match args.get(1) {
        None => None,
        Some(separator) => Some(as_str("string-split", separator)?),
    };
    let split = |s: &str| {
        let parts: Vec<Value> = match separator {
            None => s.split_whitespace().map(Value::string).collect(),
            Some("") => s.chars().map(|c| Value::string(c.encode_utf8(&mut [0; 4]))).collect(),
            Some(separator) => s.split(separator).map(Value::string).collect(),
        };
        Value::vector(parts)
    };
    match &args[0] {
        Value::Array(a) => {
            let parts: Result<Vec<Value>, Error> = a
                .iter()
                .map(|item| match item {
                    Value::Nil => Ok(Value::Nil),
                    other => Ok(split(as_str("string-split", &other)?)),
                })
                .collect();
            Ok(Value::vector(parts?))
        }
        other => Ok(split(as_str("string-split", other)?)),
    }
}

fn string_join(args: &[Value]) -> Result<Value, Error> {
    let (separator, coll) = match args {
        [coll] => ("", coll),
        [separator, coll] => (as_str("string-join", separator)?, coll),
        _ => unreachable!(),
    };
    let mut out = String::new();
    for (i, item) in items("string-join", coll)?.enumerate() {
        if i > 0 {
            out.push_str(separator);
        }
        out.push_str(&display(&item));
    }
    Ok(Value::string(&out))
}

fn string_index_of(args: &[Value]) -> Result<Value, Error> {
    let needle = as_str("string-index-of", &args[1])?;
    each("string-index-of", &args[0], &|haystack| {
        Ok(match haystack.find(needle) {
            Some(byte) => Value::Int(haystack[..byte].chars().count() as i64),
            None => Value::Nil,
        })
    })
}

fn string_repeat(args: &[Value]) -> Result<Value, Error> {
    let count = match &args[1] {
        Value::Int(n) if *n >= 0 => *n as usize,
        other => return Err(expects("string-repeat", "a non-negative count", other)),
    };
    each("string-repeat", &args[0], &|s| Ok(Value::string(&s.repeat(count))))
}

fn pad(name: &str, args: &[Value], left: bool) -> Result<Value, Error> {
    let width = match &args[1] {
        Value::Int(n) if *n >= 0 => *n as usize,
        other => return Err(expects(name, "a non-negative width", other)),
    };
    let fill = match args.get(2) {
        None => ' ',
        Some(value) => {
            let mut chars = as_str(name, value)?.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => c,
                _ => return Err(expects(name, "a single padding character", value)),
            }
        }
    };
    each(name, &args[0], &|s| {
        let padding: String = std::iter::repeat_n(fill, width.saturating_sub(s.chars().count())).collect();
        Ok(Value::string(&if left { padding + s } else { s.to_string() + &padding }))
    })
}

fn string_to_number(args: &[Value]) -> Result<Value, Error> {
    each("string->number", &args[0], &|s| {
        Ok(match parse_number(s.trim()) {
            Some(Ok(v)) => v,
            _ => Value::Nil,
        })
    })
}

pub fn join_display(args: &[Value], readable: bool) -> String {
    let mut text = String::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            text.push(' ');
        }
        if readable {
            let _ = write!(text, "{arg}");
        } else {
            let _ = write!(text, "{}", Shown(arg));
        }
    }
    text
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Right,
    Center,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Display,
    Readable,
    Exp,
    Percent,
}

struct Spec {
    fill: char,
    align: Option<Align>,
    zero: bool,
    width: usize,
    grouping: bool,
    precision: Option<usize>,
    kind: Kind,
}

enum Piece {
    Text(String),
    Hole(usize, Spec),
}

fn parse_spec(spec: &str) -> Option<Spec> {
    let chars: Vec<char> = spec.chars().collect();
    let align_of = |c: char| match c {
        '<' => Some(Align::Left),
        '>' => Some(Align::Right),
        '^' => Some(Align::Center),
        _ => None,
    };
    let mut out = Spec { fill: ' ', align: None, zero: false, width: 0, grouping: false, precision: None, kind: Kind::Display };
    let mut i = 0;
    if chars.len() >= 2 && align_of(chars[1]).is_some() {
        out.fill = chars[0];
        out.align = align_of(chars[1]);
        i = 2;
    } else if let Some(align) = chars.first().copied().and_then(align_of) {
        out.align = Some(align);
        i = 1;
    }
    if chars.get(i) == Some(&'0') {
        out.zero = true;
        i += 1;
    }
    let digits = |i: &mut usize| {
        let start = *i;
        while chars.get(*i).is_some_and(|c| c.is_ascii_digit()) {
            *i += 1;
        }
        chars[start..*i].iter().collect::<String>().parse::<usize>().ok()
    };
    if let Some(width) = digits(&mut i) {
        out.width = width;
    }
    if chars.get(i) == Some(&',') {
        out.grouping = true;
        i += 1;
    }
    if chars.get(i) == Some(&'.') {
        i += 1;
        out.precision = Some(digits(&mut i)?);
    }
    out.kind = match chars.get(i) {
        None => Kind::Display,
        Some('?') => Kind::Readable,
        Some('e') => Kind::Exp,
        Some('%') => Kind::Percent,
        Some(_) => return None,
    };
    if out.kind != Kind::Display {
        i += 1;
    }
    (i == chars.len()).then_some(out)
}

fn parse_template(template: &str) -> Result<Vec<Piece>, Error> {
    let unmatched = |brace: char| Error::new("value-error", format!("format found an unmatched {brace} in {template:?}"));
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut next = 0;
    let mut chars = template.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '{' | '}' if chars.peek().is_some_and(|&(_, n)| n == c) => {
                chars.next();
                text.push(c);
            }
            '}' => return Err(unmatched('}')),
            '{' => {
                let end = template[i..].find('}').ok_or_else(|| unmatched('{'))? + i;
                let inner = &template[i + 1..end];
                let (index, spec) = inner.split_once(':').unwrap_or((inner, ""));
                let index = if index.is_empty() {
                    next += 1;
                    Some(next - 1)
                } else {
                    index.parse().ok()
                };
                let (index, spec) = index.zip(parse_spec(spec)).ok_or_else(|| {
                    Error::new("value-error", format!("format does not understand \"{{{inner}}}\""))
                })?;
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(Piece::Hole(index, spec));
                while chars.peek().is_some_and(|&(j, _)| j <= end) {
                    chars.next();
                }
            }
            other => text.push(other),
        }
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    Ok(pieces)
}

fn group_thousands(body: &str) -> String {
    let (sign, rest) = body.strip_prefix('-').map_or(("", body), |rest| ("-", rest));
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    let (digits, tail) = rest.split_at(end);
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{sign}{grouped}{tail}")
}

fn number(x: f64, int: Option<i64>, spec: &Spec) -> String {
    if !x.is_finite() {
        return Value::Float(x).to_string();
    }
    let body = match (spec.kind, spec.precision, int) {
        (Kind::Exp, Some(p), _) => format!("{x:.p$e}"),
        (Kind::Exp, None, _) => format!("{x:e}"),
        (Kind::Percent, p, _) => format!("{:.*}%", p.unwrap_or(0), x * 100.0),
        (_, None, Some(i)) => i.to_string(),
        (_, None, None) => Value::Float(x).to_string(),
        (_, Some(p), _) => format!("{x:.p$}"),
    };
    if spec.grouping { group_thousands(&body) } else { body }
}

fn pad_to(body: String, spec: &Spec, numeric: bool) -> String {
    let gap = spec.width.saturating_sub(body.chars().count());
    if gap == 0 {
        return body;
    }
    if spec.zero && spec.align.is_none() && numeric {
        let (sign, digits) = body.strip_prefix('-').map_or(("", body.as_str()), |digits| ("-", digits));
        return format!("{sign}{}{digits}", "0".repeat(gap));
    }
    let fill = |n: usize| std::iter::repeat_n(spec.fill, n).collect::<String>();
    match spec.align.unwrap_or(if numeric { Align::Right } else { Align::Left }) {
        Align::Left => body + &fill(gap),
        Align::Right => fill(gap) + &body,
        Align::Center => fill(gap / 2) + &body + &fill(gap - gap / 2),
    }
}

fn render(value: &Value, spec: &Spec) -> String {
    let numeric = matches!(value, Value::Int(_) | Value::Float(_));
    let body = match value {
        Value::Int(i) => number(*i as f64, Some(*i), spec),
        Value::Float(x) => number(*x, None, spec),
        other => {
            let shown = if spec.kind == Kind::Readable { other.to_string() } else { display(other) };
            match spec.precision {
                Some(p) => shown.chars().take(p).collect(),
                None => shown,
            }
        }
    };
    pad_to(body, spec, numeric)
}

fn fill_template(pieces: &[Piece], arg: &dyn Fn(usize) -> Value) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(text) => out.push_str(text),
            Piece::Hole(index, spec) => out.push_str(&render(&arg(*index), spec)),
        }
    }
    out
}

pub fn format_values(template: &Value, args: &[Value]) -> Result<Value, Error> {
    let template = as_str("format", template).map_err(|_| expects("format", "a string template", template))?;
    let pieces = parse_template(template)?;
    let needed = pieces.iter().filter_map(|p| if let Piece::Hole(i, _) = p { Some(i + 1) } else { None }).max().unwrap_or(0);
    if args.len() < needed {
        let message = format!("format expects {needed} values for {template:?}, got {}", args.len());
        return Err(Error::new("value-error", message));
    }
    let lengths: Vec<usize> = args.iter().filter_map(|a| if let Value::Array(x) = a { Some(x.len()) } else { None }).collect();
    let Some(&len) = lengths.first() else {
        return Ok(Value::string(&fill_template(&pieces, &|i| args[i].clone())));
    };
    if let Some(&other) = lengths.iter().find(|&&l| l != len) {
        return Err(Error::new("value-error", format!("array lengths differ: {len} and {other}")));
    }
    let mut out = Array::new();
    for row in 0..len {
        out.push_mut(Value::string(&fill_template(&pieces, &|i| array::element(&args[i], row))))?;
    }
    Ok(Value::Array(Rc::new(out)))
}
