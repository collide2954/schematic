use super::types::{Signature, Type, any_function, array, list, map, set, union, vector};
use crate::value::Value;

fn named(name: &str) -> Option<Type> {
    Some(match name {
        "any" => Type::Any,
        "nil" => Type::Nil,
        "bool" | "boolean" => Type::Bool,
        "int" | "integer" => Type::Int,
        "float" => Type::Float,
        "number" => union([Type::Int, Type::Float]),
        "string" => Type::Str,
        "keyword" => Type::Keyword,
        "symbol" => Type::Symbol,
        "date" => Type::Date,
        "table" => Type::Table,
        "plot" => Type::Plot,
        "list" => list(Type::Any),
        "vector" => vector(Type::Any),
        "map" => map(Type::Any, Type::Any),
        "set" => set(Type::Any),
        "array" => array(Type::Any),
        "fn" | "function" => any_function(),
        _ => return None,
    })
}

fn not_a_type(form: &Value) -> String {
    format!("{form} is not a type")
}

pub fn parse(form: &Value) -> Result<Type, String> {
    match form {
        Value::Nil => Ok(Type::Nil),
        Value::Symbol(s) => named(&s.root().name()).ok_or_else(|| format!("unknown type {s}")),
        Value::Vector(items) => match items.iter().collect::<Vec<_>>().as_slice() {
            [] => Ok(vector(Type::Any)),
            [item] => Ok(vector(parse(item)?)),
            _ => Err(format!("{form} is not a type; a vector type is written [element]")),
        },
        Value::Map(entries) => match entries.iter().collect::<Vec<_>>().as_slice() {
            [] => Ok(map(Type::Any, Type::Any)),
            [(key, value)] => Ok(map(parse(key)?, parse(value)?)),
            _ => Err(format!("{form} is not a type; a map type is written {{key value}}")),
        },
        Value::Set(items) => match items.iter().collect::<Vec<_>>().as_slice() {
            [] => Ok(set(Type::Any)),
            [item] => Ok(set(parse(item)?)),
            _ => Err(format!("{form} is not a type; a set type is written #{{element}}")),
        },
        Value::List(items) => {
            let items: Vec<&Value> = items.iter().collect();
            let Some(Value::Symbol(head)) = items.first() else { return Err(not_a_type(form)) };
            let args = &items[1..];
            match &*head.root().name() {
                "or" => {
                    if args.is_empty() {
                        return Err("(or) needs at least one type".to_string());
                    }
                    let types = args.iter().map(|a| parse(a)).collect::<Result<Vec<_>, _>>()?;
                    Ok(union(types))
                }
                "->" => {
                    let Some((ret, params)) = args.split_last() else {
                        return Err("a function type needs a result, as in (-> int string)".to_string());
                    };
                    let params = params.iter().map(|p| Ok((String::new(), parse(p)?))).collect::<Result<Vec<_>, String>>()?;
                    let required = params.len();
                    Ok(Type::Fn(Box::new(Signature { name: String::new(), params, required, rest: None, ret: parse(ret)?, broadcast: false })))
                }
                "array" => match args {
                    [Value::Vector(inner)] if inner.len() == 1 => Ok(array(parse(inner.iter().next().unwrap())?)),
                    _ => Err(format!("{form} is not a type; an array type is written #[element]")),
                },
                "list" => match args {
                    [] => Ok(list(Type::Any)),
                    [item] => Ok(list(parse(item)?)),
                    _ => Err(format!("{form} is not a type; a list type is written (list element)")),
                },
                _ => Err(not_a_type(form)),
            }
        }
        other => Err(not_a_type(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::read_str;

    fn parsed(src: &str) -> Result<Type, String> {
        parse(&read_str(src).unwrap())
    }

    #[test]
    fn names_and_literals_spell_types() {
        assert_eq!(parsed("int"), Ok(Type::Int));
        assert_eq!(parsed("number"), Ok(union([Type::Int, Type::Float])));
        assert_eq!(parsed("[string]"), Ok(vector(Type::Str)));
        assert_eq!(parsed("{keyword int}"), Ok(map(Type::Keyword, Type::Int)));
        assert_eq!(parsed("#{int}"), Ok(set(Type::Int)));
        assert_eq!(parsed("#[float]"), Ok(array(Type::Float)));
        assert_eq!(parsed("(list any)"), Ok(list(Type::Any)));
        assert_eq!(parsed("(or int nil)"), Ok(union([Type::Int, Type::Nil])));
        let Ok(Type::Fn(sig)) = parsed("(-> int string bool)") else { panic!() };
        assert_eq!((sig.params.len(), sig.ret), (2, Type::Bool));
        assert_eq!(parsed("fn"), Ok(any_function()));
    }

    #[test]
    fn mistakes_are_explained() {
        assert_eq!(parsed("whatever"), Err("unknown type whatever".to_string()));
        assert_eq!(parsed("[int string]"), Err("[int string] is not a type; a vector type is written [element]".to_string()));
        assert_eq!(parsed("42"), Err("42 is not a type".to_string()));
        assert_eq!(parsed("(->)"), Err("a function type needs a result, as in (-> int string)".to_string()));
    }
}
