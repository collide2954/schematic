use std::collections::HashMap;
use std::sync::OnceLock;

use super::types::{Type, any_function, array, list, map, set, union, vector};

pub struct Builtin {
    pub params: Vec<Type>,
    pub rest: Option<Type>,
    pub ret: Type,
    pub broadcast: bool,
}

impl Builtin {
    pub fn expected(&self, index: usize) -> Option<&Type> {
        self.params.get(index).or(self.rest.as_ref())
    }
}

static TABLE: OnceLock<HashMap<&'static str, Builtin>> = OnceLock::new();

pub fn lookup(name: &str) -> Option<&'static Builtin> {
    TABLE.get_or_init(build).get(name)
}

fn labeled(label: &'static str, types: impl IntoIterator<Item = Type>) -> Type {
    Type::Labeled(label, Box::new(union(types)))
}

fn any() -> Type {
    Type::Any
}

fn number() -> Type {
    labeled("a number", [Type::Int, Type::Float, array(Type::Any)])
}

fn numeric() -> Type {
    labeled("a number", [Type::Int, Type::Float, Type::Date, array(Type::Any)])
}

fn text() -> Type {
    labeled("a string", [Type::Str, array(Type::Any)])
}

fn date() -> Type {
    labeled("a date", [Type::Date, array(Type::Any)])
}

fn sequence() -> Type {
    labeled("a sequence", [list(any()), vector(any()), array(any()), Type::Str, Type::Table, Type::Nil])
}

fn collection() -> Type {
    labeled("a collection", [list(any()), vector(any()), map(any(), any()), set(any()), array(any()), Type::Str, Type::Table, Type::Nil])
}

fn numbers() -> Type {
    labeled("a collection of numbers", [array(any()), vector(any()), list(any()), Type::Nil])
}

fn mapping() -> Type {
    labeled("a map", [map(any(), any()), Type::Table, Type::Nil])
}

fn callable() -> Type {
    labeled("a function", [any_function(), Type::Keyword, map(any(), any()), vector(any()), set(any()), array(any()), Type::Table])
}

fn sequences() -> Type {
    union([list(any()), vector(any()), array(any())])
}

fn maybe(t: Type) -> Type {
    union([t, Type::Nil])
}

fn build() -> HashMap<&'static str, Builtin> {
    let mut table = HashMap::new();
    let mut add = |name: &'static str, params: Vec<Type>, rest: Option<Type>, ret: Type, broadcast: bool| {
        table.insert(name, Builtin { params, rest, ret, broadcast });
    };

    for name in ["+", "*"] {
        add(name, vec![], Some(numeric()), any(), false);
    }
    add("-", vec![numeric()], Some(numeric()), any(), false);
    add("/", vec![numeric()], Some(numeric()), any(), false);
    for name in ["quot", "rem", "mod", "pow"] {
        add(name, vec![number(), number()], None, any(), false);
    }
    for name in ["<", "<=", ">", ">="] {
        add(name, vec![numeric(), numeric()], Some(numeric()), Type::Bool, true);
    }
    add("=", vec![any()], Some(any()), Type::Bool, true);
    add("not", vec![any()], None, Type::Bool, true);
    for name in ["max", "min"] {
        add(name, vec![numeric()], Some(numeric()), any(), false);
    }
    for name in ["abs", "inc", "dec"] {
        add(name, vec![numeric()], None, any(), false);
    }
    for name in ["even?", "odd?", "zero?", "pos?", "neg?"] {
        add(name, vec![number()], None, Type::Bool, true);
    }
    for name in ["floor", "ceil", "round", "truncate"] {
        add(name, vec![number()], None, Type::Int, true);
    }
    add("int", vec![any()], None, Type::Int, true);
    add("float", vec![any()], None, Type::Float, true);
    for name in ["sqrt", "exp"] {
        add(name, vec![number()], None, Type::Float, true);
    }
    add("log", vec![number(), number()], None, Type::Float, true);
    add("mask-and", vec![array(any()), array(any())], None, array(Type::Bool), false);
    add("mask-or", vec![array(any()), array(any())], None, array(Type::Bool), false);

    add("equal?", vec![any(), any()], None, Type::Bool, false);
    add("compare", vec![any(), any()], None, Type::Int, false);
    for name in [
        "nil?", "missing?", "boolean?", "integer?", "float?", "number?", "string?", "symbol?", "keyword?", "procedure?", "list?", "vector?",
        "map?", "set?", "array?", "table?", "coll?", "plot?", "date?",
    ] {
        add(name, vec![any()], None, Type::Bool, false);
    }

    add("list", vec![], Some(any()), list(any()), false);
    add("vector", vec![], Some(any()), vector(any()), false);
    add("hash-map", vec![], Some(any()), map(any(), any()), false);
    add("hash-set", vec![], Some(any()), set(any()), false);
    add("vec", vec![collection()], None, vector(any()), false);
    add("set", vec![collection()], None, set(any()), false);
    add("array", vec![collection()], None, array(any()), false);
    add("range", vec![number(), number(), number()], None, vector(union([Type::Int, Type::Float])), false);
    for name in ["first", "second", "last"] {
        add(name, vec![sequence()], None, any(), false);
    }
    add("rest", vec![sequence()], None, any(), false);
    add("nth", vec![sequence(), Type::Int, any()], None, any(), false);
    add("get", vec![any(), any(), any()], None, any(), false);
    add("count", vec![collection()], None, Type::Int, false);
    add("empty?", vec![collection()], None, Type::Bool, false);
    add("cons", vec![any(), any()], None, list(any()), false);
    add("conj", vec![collection()], Some(any()), any(), false);
    add("assoc", vec![any()], Some(any()), any(), false);
    add("dissoc", vec![any()], Some(any()), any(), false);
    add("disj", vec![any()], Some(any()), any(), false);
    add("update", vec![any(), any(), callable()], Some(any()), any(), false);
    add("get-in", vec![any(), any(), any()], None, any(), false);
    add("assoc-in", vec![any(), any(), any()], None, any(), false);
    add("update-in", vec![any(), any(), callable()], Some(any()), any(), false);
    add("keys", vec![mapping()], None, vector(any()), false);
    add("vals", vec![mapping()], None, vector(any()), false);
    add("find", vec![mapping(), any()], None, any(), false);
    add("contains?", vec![collection(), any()], None, Type::Bool, false);
    add("merge", vec![], Some(mapping()), any(), false);
    add("select-keys", vec![mapping(), any()], None, map(any(), any()), false);
    add("zipmap", vec![collection(), collection()], None, map(any(), any()), false);
    add("take", vec![Type::Int, collection()], None, any(), false);
    add("drop", vec![Type::Int, collection()], None, any(), false);
    add("take-while", vec![callable(), collection()], None, sequences(), false);
    add("drop-while", vec![callable(), collection()], None, sequences(), false);
    add("concat", vec![], Some(any()), any(), false);
    add("into", vec![any(), any()], None, any(), false);
    add("interleave", vec![], Some(collection()), sequences(), false);
    add("flatten", vec![collection()], None, sequences(), false);
    add("distinct", vec![collection()], Some(any()), any(), false);
    add("frequencies", vec![collection()], None, map(any(), Type::Int), false);
    add("partition", vec![any(), any(), any()], None, sequences(), false);
    add("reverse", vec![collection()], None, any(), false);

    add("map", vec![callable()], Some(collection()), sequences(), false);
    add("filter", vec![callable(), collection()], None, sequences(), false);
    add("remove", vec![callable(), collection()], None, sequences(), false);
    add("reduce", vec![callable(), any(), any()], None, any(), false);
    add("for-each", vec![callable(), collection()], None, Type::Nil, false);
    add("some", vec![callable(), collection()], None, any(), false);
    add("every?", vec![callable(), collection()], None, Type::Bool, false);
    add("sort", vec![any(), any()], None, sequences(), false);
    add("sort-by", vec![any(), any(), any()], None, sequences(), false);
    add("group-by", vec![any(), collection()], None, any(), false);
    add("apply", vec![callable()], Some(any()), any(), false);
    add("identity", vec![any()], None, any(), false);
    add("constantly", vec![any()], None, any_function(), false);
    add("complement", vec![callable()], None, any_function(), false);
    add("partial", vec![callable()], Some(any()), any_function(), false);
    add("comp", vec![], Some(callable()), any_function(), false);
    add("juxt", vec![], Some(callable()), any_function(), false);

    add("str", vec![], Some(any()), Type::Str, false);
    add("format", vec![Type::Str], Some(any()), Type::Str, true);
    add("string-length", vec![text()], None, Type::Int, true);
    add("substring", vec![text(), Type::Int, Type::Int], None, Type::Str, true);
    add("string-split", vec![text(), Type::Str], None, vector(Type::Str), true);
    add("string-join", vec![any(), any()], None, Type::Str, false);
    for name in ["string-upcase", "string-downcase", "string-trim"] {
        add(name, vec![text()], None, Type::Str, true);
    }
    for name in ["string-contains?", "string-starts-with?", "string-ends-with?"] {
        add(name, vec![text(), text()], None, Type::Bool, true);
    }
    add("string-replace", vec![text(), Type::Str, Type::Str], None, Type::Str, true);
    add("string-index-of", vec![text(), Type::Str], None, maybe(Type::Int), true);
    add("string-repeat", vec![text(), Type::Int], None, Type::Str, true);
    add("string-pad-left", vec![text(), Type::Int, Type::Str], None, Type::Str, true);
    add("string-pad-right", vec![text(), Type::Int, Type::Str], None, Type::Str, true);
    add("string->number", vec![text()], None, union([Type::Int, Type::Float, Type::Nil]), true);
    for name in ["letter?", "digit?", "whitespace?", "upper-case?", "lower-case?"] {
        add(name, vec![text()], None, Type::Bool, true);
    }
    add("char-code", vec![text()], None, Type::Int, true);
    add("code-char", vec![number()], None, Type::Str, true);
    add("re-find", vec![text(), Type::Str], None, any(), false);
    add("re-matches", vec![text(), Type::Str], None, any(), false);
    add("re-seq", vec![text(), Type::Str], None, vector(any()), false);
    add("re-contains?", vec![text(), Type::Str], None, Type::Bool, true);
    add("re-replace", vec![text(), Type::Str, any()], None, Type::Str, true);
    add("re-split", vec![text(), Type::Str], None, vector(any()), false);
    add("number->string", vec![number()], None, Type::Str, true);
    add("name", vec![any()], None, Type::Str, false);
    add("keyword", vec![any()], None, Type::Keyword, false);
    add("symbol", vec![any()], None, Type::Symbol, false);

    add("date", vec![], Some(any()), Type::Date, true);
    add("date-format", vec![date(), Type::Str], None, Type::Str, true);
    for name in ["year", "month", "day", "weekday", "day-of-year", "quarter", "days-in-month"] {
        add(name, vec![date()], None, Type::Int, true);
    }
    add("date-trunc", vec![date(), Type::Keyword], None, Type::Date, true);
    add("add-months", vec![date(), Type::Int], None, Type::Date, true);
    add("add-years", vec![date(), Type::Int], None, Type::Date, true);
    add("leap-year?", vec![any()], None, Type::Bool, true);
    add("date-range", vec![Type::Date, Type::Date, Type::Keyword], None, array(Type::Date), false);
    add("today", vec![], None, Type::Date, false);

    add("array-type", vec![array(any())], None, Type::Keyword, false);
    add("count-missing", vec![array(any())], None, Type::Int, false);
    add("drop-missing", vec![array(any())], None, array(any()), false);
    add("fill-missing", vec![array(any()), any()], None, array(any()), false);
    add("where", vec![any(), any()], None, any(), false);
    add("sum", vec![numbers()], None, any(), false);
    add("median", vec![numbers()], None, any(), false);
    for name in ["mean", "variance", "std"] {
        add(name, vec![numbers()], None, maybe(Type::Float), false);
    }
    add("quantile", vec![numbers(), number()], None, any(), false);
    for name in ["running-sum", "running-max", "running-min", "rank", "diff"] {
        add(name, vec![any()], None, any(), false);
    }
    add("lag", vec![any(), Type::Int], None, any(), false);
    add("lead", vec![any(), Type::Int], None, any(), false);
    add("rolling", vec![Type::Int, callable(), any()], None, any(), false);

    add("table", vec![], Some(any()), Type::Table, false);
    add("columns", vec![Type::Table], None, vector(Type::Keyword), false);
    add("nrows", vec![Type::Table], None, Type::Int, false);
    add("col", vec![Type::Table, Type::Keyword], None, array(any()), false);
    add("row", vec![Type::Table, Type::Int], None, map(Type::Keyword, any()), false);
    add("rows", vec![Type::Table], None, vector(map(Type::Keyword, any())), false);
    add("select", vec![Type::Table, any()], None, Type::Table, false);
    add("rename", vec![Type::Table, map(any(), any())], None, Type::Table, false);
    add("order-by", vec![Type::Table, any(), Type::Keyword], None, Type::Table, false);
    add("head", vec![Type::Table, Type::Int], None, Type::Table, false);
    add("tail", vec![Type::Table, Type::Int], None, Type::Table, false);
    add("aggregate", vec![Type::Table, any(), any()], None, Type::Table, false);
    add("window", vec![Type::Table, any(), any()], None, Type::Table, false);
    add("pivot", vec![Type::Table, any(), Type::Keyword, Type::Keyword, callable()], None, Type::Table, false);
    add("unpivot", vec![Type::Table, any(), Type::Keyword, Type::Keyword], None, Type::Table, false);
    add("join", vec![Type::Table, Type::Table, any(), Type::Keyword], None, Type::Table, false);
    add("describe", vec![Type::Table], None, Type::Table, false);

    add("read-csv", vec![Type::Str, map(any(), any())], None, Type::Table, false);
    add("write-csv", vec![Type::Table, Type::Str, map(any(), any())], None, any(), false);
    add("parse-csv", vec![Type::Str, map(any(), any())], None, Type::Table, false);
    add("to-csv", vec![Type::Table, map(any(), any())], None, Type::Str, false);
    for name in ["print", "println", "prn"] {
        add(name, vec![], Some(any()), Type::Nil, false);
    }
    add("printf", vec![Type::Str], Some(any()), Type::Nil, false);
    add("newline", vec![], None, Type::Nil, false);

    for name in ["bar-chart", "line-chart", "area-chart", "scatter", "pie-chart", "bars", "lines", "steps", "points", "areas", "lollipops", "pie"] {
        add(name, vec![any(), any(), any(), map(any(), any())], None, Type::Plot, false);
    }
    for name in ["histogram", "density"] {
        add(name, vec![any(), any(), any(), any()], None, Type::Plot, false);
    }
    for name in ["box-plot", "boxes"] {
        add(name, vec![any(), any(), any(), any(), any()], None, Type::Plot, false);
    }
    add("heatmap", vec![any(), any(), any(), any(), any(), any()], None, Type::Plot, false);
    add("plot", vec![map(any(), any())], None, Type::Plot, false);
    add("title", vec![Type::Plot, any(), any()], None, Type::Plot, false);
    add("subtitle", vec![Type::Plot, any()], None, Type::Plot, false);
    add("caption", vec![Type::Plot, any()], None, Type::Plot, false);
    add("hline", vec![Type::Plot, any(), any()], None, Type::Plot, false);
    add("vline", vec![Type::Plot, any(), any()], None, Type::Plot, false);
    add("annotate", vec![Type::Plot, any(), any(), any()], None, Type::Plot, false);
    for name in ["x-axis", "y-axis", "right-axis"] {
        add(name, vec![Type::Plot, map(any(), any())], None, Type::Plot, false);
    }
    add("legend", vec![Type::Plot, any()], None, Type::Plot, false);
    add("palette", vec![Type::Plot, any()], None, Type::Plot, false);
    add("trend", vec![Type::Plot, map(any(), any())], None, Type::Plot, false);
    add("smooth", vec![Type::Plot, Type::Int], None, Type::Plot, false);
    add("figure", vec![any(), map(any(), any())], None, Type::Plot, false);
    add("to-svg", vec![Type::Plot, map(any(), any())], None, Type::Str, false);
    add("save-plot", vec![Type::Plot, Type::Str, map(any(), any())], None, any(), false);
    add("plot-theme!", vec![map(any(), any())], None, Type::Nil, false);
    add("plot-theme", vec![], None, map(any(), any()), false);

    add("doc-string", vec![any()], None, maybe(Type::Str), false);
    add("apropos", vec![Type::Str], None, vector(Type::Str), false);
    add("print-doc", vec![any()], None, Type::Nil, false);
    add("current-ns", vec![], None, Type::Symbol, false);
    table
}
