use std::fmt;

const DEPTH: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn parse(text: &str) -> Result<Json, String> {
        let mut parser = Parser { bytes: text.as_bytes(), text, at: 0 };
        let value = parser.value(0)?;
        parser.skip_space();
        if parser.at < parser.bytes.len() {
            return Err(parser.unexpected());
        }
        Ok(value)
    }

    pub fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
        Json::Object(fields.into_iter().map(|(key, value)| (key.to_string(), value)).collect())
    }

    pub fn text(text: impl Into<String>) -> Json {
        Json::String(text.into())
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields.iter().find(|(name, _)| name == key).map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn path(&self, keys: &[&str]) -> Option<&Json> {
        keys.iter().try_fold(self, |value, key| value.get(key))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(number) => Some(*number),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    pub fn set(&mut self, key: &str, value: Json) {
        if let Json::Object(fields) = self {
            match fields.iter_mut().find(|(name, _)| name == key) {
                Some(field) => field.1 = value,
                None => fields.push((key.to_string(), value)),
            }
        }
    }

    pub fn pretty(&self) -> String {
        let mut out = String::new();
        write_json(self, &mut out, Some(0));
        out
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        write_json(self, &mut out, None);
        f.write_str(&out)
    }
}

impl From<&str> for Json {
    fn from(text: &str) -> Json {
        Json::String(text.to_string())
    }
}

impl From<String> for Json {
    fn from(text: String) -> Json {
        Json::String(text)
    }
}

impl From<bool> for Json {
    fn from(flag: bool) -> Json {
        Json::Bool(flag)
    }
}

impl From<f64> for Json {
    fn from(number: f64) -> Json {
        Json::Number(number)
    }
}

impl From<i64> for Json {
    fn from(number: i64) -> Json {
        Json::Number(number as f64)
    }
}

impl From<u32> for Json {
    fn from(number: u32) -> Json {
        Json::Number(number as f64)
    }
}

impl From<Vec<Json>> for Json {
    fn from(items: Vec<Json>) -> Json {
        Json::Array(items)
    }
}

pub fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

pub fn write_number(number: f64, out: &mut String) {
    if !number.is_finite() {
        out.push_str("null");
    } else if number.fract() == 0.0 && number.abs() < 1e15 {
        out.push_str(&format!("{}", number as i64));
    } else {
        out.push_str(&format!("{number}"));
    }
}

fn write_json(value: &Json, out: &mut String, indent: Option<usize>) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Json::Number(number) => write_number(*number, out),
        Json::String(text) => write_string(text, out),
        Json::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, indent.map(|n| n + 1));
                write_json(item, out, indent.map(|n| n + 1));
            }
            newline(out, indent);
            out.push(']');
        }
        Json::Object(fields) => {
            if fields.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (key, item)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, indent.map(|n| n + 1));
                write_string(key, out);
                out.push(':');
                if indent.is_some() {
                    out.push(' ');
                }
                write_json(item, out, indent.map(|n| n + 1));
            }
            newline(out, indent);
            out.push('}');
        }
    }
}

fn newline(out: &mut String, indent: Option<usize>) {
    if let Some(level) = indent {
        out.push('\n');
        for _ in 0..level {
            out.push_str("  ");
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn skip_space(&mut self) {
        while self.at < self.bytes.len() && matches!(self.bytes[self.at], b' ' | b'\t' | b'\n' | b'\r') {
            self.at += 1;
        }
    }

    fn unexpected(&self) -> String {
        match self.text[self.at..].chars().next() {
            Some(c) => format!("unexpected {c:?} at byte {}", self.at),
            None => "unexpected end of JSON".to_string(),
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.at < self.bytes.len() && self.bytes[self.at] == byte {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.eat(byte) { Ok(()) } else { Err(self.unexpected()) }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > DEPTH {
            return Err("JSON nested too deeply".to_string());
        }
        self.skip_space();
        match self.bytes.get(self.at) {
            None => Err(self.unexpected()),
            Some(b'{') => {
                self.at += 1;
                let mut fields = Vec::new();
                self.skip_space();
                if self.eat(b'}') {
                    return Ok(Json::Object(fields));
                }
                loop {
                    self.skip_space();
                    self.expect(b'"')?;
                    let key = self.string()?;
                    self.skip_space();
                    self.expect(b':')?;
                    let value = self.value(depth + 1)?;
                    fields.push((key, value));
                    self.skip_space();
                    if self.eat(b',') {
                        continue;
                    }
                    self.expect(b'}')?;
                    return Ok(Json::Object(fields));
                }
            }
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                self.skip_space();
                if self.eat(b']') {
                    return Ok(Json::Array(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.skip_space();
                    if self.eat(b',') {
                        continue;
                    }
                    self.expect(b']')?;
                    return Ok(Json::Array(items));
                }
            }
            Some(b'"') => {
                self.at += 1;
                Ok(Json::String(self.string()?))
            }
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'n') => self.word("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.unexpected()),
        }
    }

    fn word(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.unexpected())
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.at;
        self.eat(b'-');
        if !self.eat(b'0') && !self.digits() {
            return Err(self.unexpected());
        }
        if self.eat(b'.') && !self.digits() {
            return Err(self.unexpected());
        }
        if self.at < self.bytes.len() && matches!(self.bytes[self.at], b'e' | b'E') {
            self.at += 1;
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if !self.digits() {
                return Err(self.unexpected());
            }
        }
        let text = &self.text[start..self.at];
        text.parse::<f64>().map(Json::Number).map_err(|_| format!("bad number {text}"))
    }

    fn digits(&mut self) -> bool {
        let start = self.at;
        while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_digit() {
            self.at += 1;
        }
        self.at > start
    }

    fn string(&mut self) -> Result<String, String> {
        let mut out = String::new();
        loop {
            let rest = &self.text[self.at..];
            let Some(c) = rest.chars().next() else { return Err(self.unexpected()) };
            self.at += c.len_utf8();
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let Some(escape) = self.text[self.at..].chars().next() else { return Err(self.unexpected()) };
                    self.at += escape.len_utf8();
                    match escape {
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        '/' => out.push('/'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'u' => {
                            let mut code = self.hex4()?;
                            if (0xD800..0xDC00).contains(&code) {
                                if self.bytes[self.at..].starts_with(b"\\u") {
                                    self.at += 2;
                                    let low = self.hex4()?;
                                    if (0xDC00..0xE000).contains(&low) {
                                        code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                                    } else {
                                        return Err("bad surrogate pair in JSON string".to_string());
                                    }
                                } else {
                                    return Err("lone surrogate in JSON string".to_string());
                                }
                            }
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(format!("bad escape \\{escape} in JSON string")),
                    }
                }
                c if (c as u32) < 0x20 => return Err("control character in JSON string".to_string()),
                c => out.push(c),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self.text.get(self.at..self.at + 4).ok_or_else(|| self.unexpected())?;
        let code = u32::from_str_radix(digits, 16).map_err(|_| format!("bad \\u escape {digits}"))?;
        self.at += 4;
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_kind_of_value() {
        let text = r#" {"a": [1, -2.5, 3e2, true, false, null], "b": {"c": "d\n\u00e9\ud83d\ude00"}, "e": {}, "f": []} "#;
        let json = Json::parse(text).unwrap();
        assert_eq!(json.path(&["a"]).unwrap().as_array().unwrap().len(), 6);
        assert_eq!(json.path(&["a"]).unwrap().as_array().unwrap()[2], Json::Number(300.0));
        assert_eq!(json.path(&["b", "c"]).unwrap().as_str(), Some("d\né😀"));
        assert_eq!(json.get("e"), Some(&Json::Object(Vec::new())));
        assert_eq!(json.get("f"), Some(&Json::Array(Vec::new())));
        assert_eq!(json.get("missing"), None);
    }

    #[test]
    fn rejects_malformed_text() {
        for bad in ["", "{", "[1,]", "{\"a\" 1}", "tru", "01", "1.", "-", "\"\\x\"", "\"\n\"", "[1] 2", "\"\\ud800\""] {
            assert!(Json::parse(bad).is_err(), "{bad:?} should fail");
        }
        let deep = "[".repeat(300) + &"]".repeat(300);
        assert_eq!(Json::parse(&deep).unwrap_err(), "JSON nested too deeply");
    }

    #[test]
    fn writes_compact_and_pretty_text() {
        let json = Json::object([
            ("name", Json::text("tab\there")),
            ("n", Json::from(3i64)),
            ("x", Json::from(2.5)),
            ("big", Json::from(1e21)),
            ("none", Json::Null),
            ("list", Json::from(vec![Json::from(true), Json::object([("k", Json::text("v"))])])),
            ("empty", Json::Array(Vec::new())),
        ]);
        assert_eq!(json.to_string(), r#"{"name":"tab\there","n":3,"x":2.5,"big":1000000000000000000000,"none":null,"list":[true,{"k":"v"}],"empty":[]}"#);
        assert_eq!(json.pretty(), "{\n  \"name\": \"tab\\there\",\n  \"n\": 3,\n  \"x\": 2.5,\n  \"big\": 1000000000000000000000,\n  \"none\": null,\n  \"list\": [\n    true,\n    {\n      \"k\": \"v\"\n    }\n  ],\n  \"empty\": []\n}");
        assert_eq!(Json::parse(&json.to_string()).unwrap(), json);
        assert_eq!(Json::Number(f64::NAN).to_string(), "null");
        assert_eq!(Json::text("\u{1}\"\\").to_string(), r#""\u0001\"\\""#);
    }

    #[test]
    fn sets_and_replaces_fields() {
        let mut json = Json::object([("a", Json::from(1i64))]);
        json.set("b", Json::from(2i64));
        json.set("a", Json::from(3i64));
        assert_eq!(json.to_string(), r#"{"a":3,"b":2}"#);
    }
}
