#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Any,
    Nil,
    Bool,
    Int,
    Float,
    Str,
    Keyword,
    Symbol,
    Date,
    Table,
    Plot,
    List(Box<Type>),
    Vector(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Set(Box<Type>),
    Array(Box<Type>),
    Fn(Box<Signature>),
    Union(Vec<Type>),
    Labeled(&'static str, Box<Type>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Signature {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub required: usize,
    pub rest: Option<Type>,
    pub ret: Type,
    pub broadcast: bool,
}

impl Signature {
    pub fn accepts(&self, count: usize) -> bool {
        count >= self.required && (count <= self.params.len() || self.rest.is_some())
    }

    pub fn expected(&self, index: usize) -> Option<&Type> {
        self.params.get(index).map(|(_, t)| t).or(self.rest.as_ref())
    }

    pub fn arity(&self) -> String {
        let plural = |n: usize| if n == 1 { "1 argument".to_string() } else { format!("{n} arguments") };
        if self.rest.is_some() {
            format!("at least {}", plural(self.required))
        } else if self.required == self.params.len() {
            plural(self.required)
        } else {
            format!("{} to {} arguments", self.required, self.params.len())
        }
    }
}

pub fn any_function() -> Type {
    Type::Fn(Box::new(Signature { name: String::new(), params: Vec::new(), required: 0, rest: Some(Type::Any), ret: Type::Any, broadcast: false }))
}

pub fn vector(elem: Type) -> Type {
    Type::Vector(Box::new(elem))
}

pub fn list(elem: Type) -> Type {
    Type::List(Box::new(elem))
}

pub fn array(elem: Type) -> Type {
    Type::Array(Box::new(elem))
}

pub fn set(elem: Type) -> Type {
    Type::Set(Box::new(elem))
}

pub fn map(key: Type, value: Type) -> Type {
    Type::Map(Box::new(key), Box::new(value))
}

fn members(t: Type, out: &mut Vec<Type>) {
    match t {
        Type::Union(items) => items.into_iter().for_each(|item| members(item, out)),
        Type::Labeled(_, inner) => members(*inner, out),
        other => {
            if !out.contains(&other) {
                out.push(other);
            }
        }
    }
}

pub fn union(items: impl IntoIterator<Item = Type>) -> Type {
    let mut out = Vec::new();
    for item in items {
        members(item, &mut out);
    }
    if out.contains(&Type::Any) {
        return Type::Any;
    }
    match out.len() {
        0 => Type::Any,
        1 => out.pop().unwrap(),
        _ => Type::Union(out),
    }
}

pub fn join(a: Type, b: Type) -> Type {
    if a == b { a } else { union([a, b]) }
}

pub fn fits(actual: &Type, expected: &Type) -> bool {
    use Type::*;
    match (actual, expected) {
        (Labeled(_, a), _) => fits(a, expected),
        (_, Labeled(_, e)) => fits(actual, e),
        (Any, _) | (_, Any) => true,
        (Union(items), _) => items.iter().any(|a| fits(a, expected)),
        (_, Union(items)) => items.iter().any(|e| fits(actual, e)),
        (List(a), List(e)) | (Vector(a), Vector(e)) | (Set(a), Set(e)) | (Array(a), Array(e)) => fits(a, e),
        (Map(ak, av), Map(ek, ev)) => fits(ak, ek) && fits(av, ev),
        (Fn(_), Fn(_)) => true,
        _ => std::mem::discriminant(actual) == std::mem::discriminant(expected),
    }
}

pub fn is_callable(t: &Type) -> bool {
    match t {
        Type::Any | Type::Keyword | Type::Map(..) | Type::Vector(_) | Type::Set(_) | Type::Array(_) | Type::Table | Type::Fn(_) => true,
        Type::Union(items) => items.iter().any(is_callable),
        Type::Labeled(_, inner) => is_callable(inner),
        _ => false,
    }
}

pub fn element(t: &Type) -> Type {
    match t {
        Type::Vector(e) | Type::List(e) | Type::Array(e) | Type::Set(e) => union([(**e).clone(), Type::Nil]),
        Type::Str => union([Type::Str, Type::Nil]),
        Type::Nil => Type::Nil,
        Type::Table => union([map(Type::Keyword, Type::Any), Type::Nil]),
        Type::Labeled(_, inner) => element(inner),
        _ => Type::Any,
    }
}

pub fn rest_of(t: &Type) -> Type {
    match t {
        Type::Vector(_) | Type::List(_) | Type::Array(_) | Type::Str => t.clone(),
        _ => Type::Any,
    }
}

pub fn like_first(args: &[Type]) -> Type {
    match args.first() {
        Some(Type::Vector(_)) => vector(Type::Any),
        Some(Type::List(_)) => list(Type::Any),
        Some(Type::Map(..)) => map(Type::Any, Type::Any),
        Some(Type::Set(_)) => set(Type::Any),
        Some(Type::Array(_)) => array(Type::Any),
        Some(Type::Table) => Type::Table,
        _ => Type::Any,
    }
}

fn words(items: Vec<String>) -> String {
    match items.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} or {b}"),
        many => {
            let (last, rest) = many.split_last().unwrap();
            format!("{}, or {last}", rest.join(", "))
        }
    }
}

fn of(word: &str, elem: &Type) -> String {
    match elem.plural() {
        Some(plural) => format!("{word} of {plural}"),
        None => word.to_string(),
    }
}

impl Type {
    fn plural(&self) -> Option<&'static str> {
        Some(match self {
            Type::Bool => "booleans",
            Type::Int => "integers",
            Type::Float => "floats",
            Type::Str => "strings",
            Type::Keyword => "keywords",
            Type::Symbol => "symbols",
            Type::Date => "dates",
            Type::Table => "tables",
            Type::Plot => "plots",
            Type::List(_) => "lists",
            Type::Vector(_) => "vectors",
            Type::Map(..) => "maps",
            Type::Set(_) => "sets",
            Type::Array(_) => "arrays",
            Type::Fn(_) => "functions",
            _ => return None,
        })
    }

    pub fn describe(&self) -> String {
        match self {
            Type::Any => "a value".to_string(),
            Type::Nil => "nil".to_string(),
            Type::Bool => "a boolean".to_string(),
            Type::Int => "an integer".to_string(),
            Type::Float => "a float".to_string(),
            Type::Str => "a string".to_string(),
            Type::Keyword => "a keyword".to_string(),
            Type::Symbol => "a symbol".to_string(),
            Type::Date => "a date".to_string(),
            Type::Table => "a table".to_string(),
            Type::Plot => "a plot".to_string(),
            Type::List(e) => of("a list", e),
            Type::Vector(e) => of("a vector", e),
            Type::Set(e) => of("a set", e),
            Type::Array(e) => of("an array", e),
            Type::Map(..) => "a map".to_string(),
            Type::Fn(_) => "a function".to_string(),
            Type::Labeled(label, _) => label.to_string(),
            Type::Union(items) => {
                let numeric = items.contains(&Type::Int) && items.contains(&Type::Float);
                let mut out: Vec<String> = Vec::new();
                for item in items {
                    let word = if numeric && matches!(item, Type::Int | Type::Float) { "a number".to_string() } else { item.describe() };
                    if !out.contains(&word) {
                        out.push(word);
                    }
                }
                words(out)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unions_flatten_and_any_absorbs() {
        assert_eq!(union([Type::Int, Type::Int]), Type::Int);
        assert_eq!(union([Type::Int, union([Type::Str, Type::Nil])]), Type::Union(vec![Type::Int, Type::Str, Type::Nil]));
        assert_eq!(join(Type::Int, Type::Any), Type::Any);
        assert_eq!(join(Type::Labeled("x", Box::new(Type::Int)), Type::Float), Type::Union(vec![Type::Int, Type::Float]));
    }

    #[test]
    fn fitting_is_possible_not_certain() {
        let maybe = union([Type::Int, Type::Nil]);
        assert!(fits(&maybe, &Type::Int));
        assert!(!fits(&Type::Nil, &Type::Int));
        assert!(fits(&Type::Any, &Type::Str) && fits(&Type::Str, &Type::Any));
        assert!(fits(&vector(Type::Int), &vector(Type::Any)));
        assert!(!fits(&vector(Type::Str), &vector(Type::Int)));
        assert!(!fits(&Type::Str, &union([Type::Int, Type::Float])));
        assert!(fits(&any_function(), &any_function()));
    }

    #[test]
    fn descriptions_read_like_prose() {
        assert_eq!(Type::Int.describe(), "an integer");
        assert_eq!(vector(Type::Int).describe(), "a vector of integers");
        assert_eq!(vector(Type::Any).describe(), "a vector");
        assert_eq!(union([Type::Int, Type::Nil]).describe(), "an integer or nil");
        assert_eq!(union([Type::Int, Type::Float, Type::Str]).describe(), "a number or a string");
        assert_eq!(union([Type::Int, Type::Str, Type::Nil]).describe(), "an integer, a string, or nil");
        assert_eq!(Type::Labeled("a collection", Box::new(Type::Any)).describe(), "a collection");
    }

    #[test]
    fn arities_print_like_the_runtime() {
        let sig = |params: usize, required: usize, rest: bool| Signature {
            name: String::new(),
            params: (0..params).map(|_| (String::new(), Type::Any)).collect(),
            required,
            rest: rest.then_some(Type::Any),
            ret: Type::Any,
            broadcast: false,
        };
        assert_eq!(sig(1, 1, false).arity(), "1 argument");
        assert_eq!(sig(2, 2, false).arity(), "2 arguments");
        assert_eq!(sig(1, 1, true).arity(), "at least 1 argument");
        assert_eq!(sig(3, 1, false).arity(), "1 to 3 arguments");
        assert!(sig(3, 1, false).accepts(2) && !sig(3, 1, false).accepts(4) && sig(1, 1, true).accepts(9));
    }
}
