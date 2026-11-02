use super::collections::expects;
use super::native;
use super::strings::as_str;
use crate::date;
use crate::error::Error;
use crate::json::Json;
use crate::value::Value;
use crate::vm::{Arity, Globals};

pub fn install(globals: &mut Globals) {
    native(globals, "parse-json", Arity::Between(1, 2), |args| {
        let text = as_str("parse-json", &args[0])?;
        parse("parse-json", text, args.get(1))
    });
    native(globals, "to-json", Arity::Between(1, 2), |args| serialize("to-json", &args[0], args.get(1)));
    native(globals, "read-json", Arity::Between(1, 2), |args| {
        let path = as_str("read-json", &args[0])?;
        let text = std::fs::read_to_string(path).map_err(|e| Error::new("io-error", format!("cannot read {path}: {e}")))?;
        parse("read-json", &text, args.get(1))
    });
    native(globals, "write-json", Arity::Between(2, 3), |args| {
        let path = as_str("write-json", &args[1])?;
        let Value::Str(text) = serialize("write-json", &args[0], args.get(2))? else { unreachable!() };
        std::fs::write(path, text.as_bytes()).map_err(|e| Error::new("io-error", format!("cannot write {path}: {e}")))?;
        Ok(Value::Nil)
    });
}

fn option<'a>(name: &str, options: Option<&'a Value>, key: &str) -> Result<Option<&'a Value>, Error> {
    let Some(options) = options else { return Ok(None) };
    let Value::Map(map) = options else { return Err(expects(name, "an options map", options)) };
    Ok(map.get(&Value::keyword(key)))
}

fn parse(name: &str, text: &str, options: Option<&Value>) -> Result<Value, Error> {
    let keywords = match option(name, options, "keys")? {
        None => true,
        Some(Value::Keyword(k)) if &*k.name() == "keywords" => true,
        Some(Value::Keyword(k)) if &*k.name() == "strings" => false,
        Some(other) => return Err(expects(name, ":keywords or :strings for :keys", other)),
    };
    let json = Json::parse(text).map_err(|message| Error::new("value-error", format!("{name}: {message}")))?;
    Ok(to_value(&json, keywords))
}

fn serialize(name: &str, value: &Value, options: Option<&Value>) -> Result<Value, Error> {
    let pretty = match option(name, options, "pretty")? {
        None => false,
        Some(Value::Bool(flag)) => *flag,
        Some(other) => return Err(expects(name, "true or false for :pretty", other)),
    };
    let json = to_json(name, value)?;
    Ok(Value::string(&if pretty { json.pretty() } else { json.to_string() }))
}

pub fn to_value(json: &Json, keywords: bool) -> Value {
    match json {
        Json::Null => Value::Nil,
        Json::Bool(flag) => Value::Bool(*flag),
        Json::Number(n) if n.fract() == 0.0 && n.abs() < 9007199254740992.0 => Value::Int(*n as i64),
        Json::Number(n) => Value::Float(*n),
        Json::String(text) => Value::string(text),
        Json::Array(items) => Value::vector(items.iter().map(|item| to_value(item, keywords))),
        Json::Object(fields) => Value::map(fields.iter().map(|(key, value)| {
            let key = if keywords { Value::keyword(key) } else { Value::string(key) };
            (key, to_value(value, keywords))
        })),
    }
}

pub fn to_json(name: &str, value: &Value) -> Result<Json, Error> {
    Ok(match value {
        Value::Nil => Json::Null,
        Value::Bool(flag) => Json::Bool(*flag),
        Value::Int(i) => Json::Number(*i as f64),
        Value::Float(x) => Json::Number(*x),
        Value::Str(s) => Json::String(s.to_string()),
        Value::Keyword(k) | Value::Symbol(k) => Json::String(k.name().to_string()),
        Value::Date(d) => Json::String(date::iso(*d)),
        Value::List(items) => Json::Array(items.iter().map(|item| to_json(name, item)).collect::<Result<_, _>>()?),
        Value::Vector(items) => Json::Array(items.iter().map(|item| to_json(name, item)).collect::<Result<_, _>>()?),
        Value::Set(items) => Json::Array(items.iter().map(|item| to_json(name, item)).collect::<Result<_, _>>()?),
        Value::Array(items) => Json::Array(items.iter().map(|item| to_json(name, &item)).collect::<Result<_, _>>()?),
        Value::Map(map) => Json::Object(
            map.iter()
                .map(|(key, value)| {
                    let key = match key {
                        Value::Keyword(k) | Value::Symbol(k) => k.name().to_string(),
                        Value::Str(s) => s.to_string(),
                        other => other.to_string(),
                    };
                    Ok((key, to_json(name, value)?))
                })
                .collect::<Result<_, Error>>()?,
        ),
        Value::Table(table) => Json::Array(table.rows().map(|row| to_json(name, &row)).collect::<Result<_, _>>()?),
        other => return Err(Error::type_error(format!("{name} cannot encode a {}", other.type_name()))),
    })
}
