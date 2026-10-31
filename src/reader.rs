use std::fmt;

use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct ReadError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for ReadError {}

pub fn read_str(src: &str) -> Result<Value, ReadError> {
    let mut reader = Reader::new(src);
    match reader.read()? {
        Some(v) => Ok(v),
        None => Err(reader.error("unexpected end of input")),
    }
}

pub fn read_all(src: &str) -> Result<Vec<Value>, ReadError> {
    let mut reader = Reader::new(src);
    let mut out = Vec::new();
    while let Some(v) = reader.read()? {
        out.push(v);
    }
    Ok(out)
}

pub struct Reader<'a> {
    src: &'a str,
    pos: usize,
    line: usize,
    col: usize,
}

fn is_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, ',' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';')
}

impl<'a> Reader<'a> {
    pub fn new(src: &'a str) -> Reader<'a> {
        Reader { src, pos: 0, line: 1, col: 1 }
    }

    fn error(&self, message: impl Into<String>) -> ReadError {
        ReadError { message: message.into(), line: self.line, col: self.col }
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn peek_second(&self) -> Option<char> {
        let mut chars = self.src[self.pos..].chars();
        chars.next();
        chars.next()
    }

    fn skip_trivia(&mut self) -> Result<(), ReadError> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() || c == ',' => {
                    self.bump();
                }
                Some(';') => while !matches!(self.bump(), None | Some('\n')) {},
                Some('#') if self.peek_second() == Some('|') => self.skip_block_comment()?,
                Some('#') if self.peek_second() == Some('_') => {
                    self.bump();
                    self.bump();
                    if self.read()?.is_none() {
                        return Err(self.error("expected a form after `#_`"));
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), ReadError> {
        let unclosed = self.error("unclosed `#|`");
        self.bump();
        self.bump();
        let mut depth = 1;
        while depth > 0 {
            match self.bump() {
                None => return Err(unclosed),
                Some('#') if self.peek() == Some('|') => {
                    self.bump();
                    depth += 1;
                }
                Some('|') if self.peek() == Some('#') => {
                    self.bump();
                    depth -= 1;
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    pub fn read(&mut self) -> Result<Option<Value>, ReadError> {
        self.skip_trivia()?;
        let Some(c) = self.peek() else {
            return Ok(None);
        };
        let value = match c {
            '"' => self.read_string()?,
            '(' => Value::List(self.read_items(')')?.into_iter().collect()),
            '[' => Value::vector(self.read_items(']')?),
            ')' | ']' | '}' => return Err(self.error(format!("unexpected `{c}`"))),
            '\'' => self.read_prefixed("quote")?,
            '`' => self.read_prefixed("quasiquote")?,
            '~' => {
                self.bump();
                if self.peek() == Some('@') {
                    self.bump();
                    self.read_wrapped("unquote-splicing", "~@")?
                } else {
                    self.read_wrapped("unquote", "~")?
                }
            }
            _ => self.read_atom()?,
        };
        Ok(Some(value))
    }

    fn read_prefixed(&mut self, name: &str) -> Result<Value, ReadError> {
        let prefix = self.bump().unwrap().to_string();
        self.read_wrapped(name, &prefix)
    }

    fn read_wrapped(&mut self, name: &str, prefix: &str) -> Result<Value, ReadError> {
        match self.read()? {
            Some(v) => Ok(Value::list([Value::symbol(name), v])),
            None => Err(self.error(format!("expected a form after `{prefix}`"))),
        }
    }

    fn read_items(&mut self, close: char) -> Result<Vec<Value>, ReadError> {
        let open = self.bump().unwrap();
        let unclosed = self.error(format!("unclosed `{open}`"));
        let mut items = Vec::new();
        loop {
            self.skip_trivia()?;
            match self.peek() {
                None => return Err(unclosed),
                Some(c) if c == close => {
                    self.bump();
                    return Ok(items);
                }
                Some(c @ (')' | ']' | '}')) => {
                    return Err(self.error(format!("expected `{close}` but found `{c}`")));
                }
                Some(_) => match self.read()? {
                    Some(v) => items.push(v),
                    None => return Err(unclosed),
                },
            }
        }
    }

    fn read_string(&mut self) -> Result<Value, ReadError> {
        let start = self.error("unterminated string");
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err(start),
                Some('"') => return Ok(Value::Str(out.into())),
                Some('\\') => {
                    let escaped = match self.bump() {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        Some('0') => '\0',
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some(c) => return Err(self.error(format!("unknown escape `\\{c}`"))),
                        None => return Err(start),
                    };
                    out.push(escaped);
                }
                Some(c) => out.push(c),
            }
        }
    }

    fn read_token(&mut self) -> &'a str {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_delimiter(c) {
                break;
            }
            self.bump();
        }
        &self.src[start..self.pos]
    }

    fn read_atom(&mut self) -> Result<Value, ReadError> {
        let token = self.read_token();
        if token.is_empty() {
            let c = self.peek().unwrap_or(' ');
            return Err(self.error(format!("unexpected `{c}`")));
        }
        match token {
            "true" => return Ok(Value::Bool(true)),
            "false" => return Ok(Value::Bool(false)),
            "nil" => return Ok(Value::Nil),
            _ => {}
        }
        if let Some(name) = token.strip_prefix(':') {
            if name.is_empty() {
                return Err(self.error("keyword needs a name"));
            }
            return Ok(Value::keyword(name));
        }
        match parse_number(token) {
            Some(Ok(v)) => Ok(v),
            Some(Err(msg)) => Err(self.error(msg)),
            None => Ok(Value::symbol(token)),
        }
    }
}

fn parse_number(token: &str) -> Option<Result<Value, String>> {
    match token {
        "+inf.0" => return Some(Ok(Value::Float(f64::INFINITY))),
        "-inf.0" => return Some(Ok(Value::Float(f64::NEG_INFINITY))),
        "+nan.0" => return Some(Ok(Value::Float(f64::NAN))),
        _ => {}
    }
    let body = token.strip_prefix(['+', '-']).unwrap_or(token);
    if !body.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let cleaned: String = token.chars().filter(|&c| c != '_').collect();
    if cleaned.contains(['.', 'e', 'E']) {
        Some(
            cleaned
                .parse::<f64>()
                .map(Value::Float)
                .map_err(|_| format!("invalid number `{token}`")),
        )
    } else {
        Some(
            cleaned
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|_| format!("integer out of range `{token}`")),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(src: &str) -> Value {
        read_str(src).unwrap()
    }

    #[test]
    fn reads_integers() {
        assert_eq!(read("42"), Value::Int(42));
        assert_eq!(read("-7"), Value::Int(-7));
        assert_eq!(read("+3"), Value::Int(3));
        assert_eq!(read("1_000_000"), Value::Int(1_000_000));
    }

    #[test]
    fn reads_floats() {
        assert_eq!(read("3.14"), Value::Float(3.14));
        assert_eq!(read("-0.5"), Value::Float(-0.5));
        assert_eq!(read("1e3"), Value::Float(1000.0));
        assert_eq!(read("2.5e-2"), Value::Float(0.025));
        assert_eq!(read("+inf.0"), Value::Float(f64::INFINITY));
        assert_eq!(read("-inf.0"), Value::Float(f64::NEG_INFINITY));
        assert!(matches!(read("+nan.0"), Value::Float(x) if x.is_nan()));
    }

    #[test]
    fn reads_literals() {
        assert_eq!(read("true"), Value::Bool(true));
        assert_eq!(read("false"), Value::Bool(false));
        assert_eq!(read("nil"), Value::Nil);
    }

    #[test]
    fn reads_symbols() {
        for name in ["map", "+", "-", "set!", "string->number", "<=", "...", "x1"] {
            assert_eq!(read(name), Value::symbol(name));
        }
    }

    #[test]
    fn reads_keywords() {
        assert_eq!(read(":name"), Value::keyword("name"));
    }

    #[test]
    fn reads_strings_with_escapes() {
        assert_eq!(read("\"hi\""), Value::string("hi"));
        assert_eq!(read("\"a\\nb\\t\\\"c\\\\\""), Value::string("a\nb\t\"c\\"));
        assert_eq!(read("\"héllo ✓\""), Value::string("héllo ✓"));
    }

    #[test]
    fn skips_whitespace_and_commas() {
        assert_eq!(read("  \n\t, 7 "), Value::Int(7));
    }

    #[test]
    fn integer_overflow_is_an_error() {
        assert!(read_str("99999999999999999999").is_err());
    }

    #[test]
    fn unterminated_string_is_an_error() {
        assert!(read_str("\"open").is_err());
    }

    #[test]
    fn empty_input_is_an_error() {
        assert!(read_str("   ").is_err());
    }

    fn list(items: Vec<Value>) -> Value {
        Value::list(items)
    }

    #[test]
    fn reads_lists() {
        assert_eq!(
            read("(+ 1 2)"),
            list(vec![Value::symbol("+"), Value::Int(1), Value::Int(2)])
        );
        assert_eq!(read("()"), list(vec![]));
        assert_eq!(read("( a )"), list(vec![Value::symbol("a")]));
    }

    #[test]
    fn reads_nested_lists() {
        assert_eq!(
            read("(a (b c))"),
            list(vec![
                Value::symbol("a"),
                list(vec![Value::symbol("b"), Value::symbol("c")])
            ])
        );
    }

    #[test]
    fn reads_quote_forms() {
        assert_eq!(read("'x"), list(vec![Value::symbol("quote"), Value::symbol("x")]));
        assert_eq!(read("`x"), list(vec![Value::symbol("quasiquote"), Value::symbol("x")]));
        assert_eq!(read("~x"), list(vec![Value::symbol("unquote"), Value::symbol("x")]));
        assert_eq!(
            read("~@x"),
            list(vec![Value::symbol("unquote-splicing"), Value::symbol("x")])
        );
        assert_eq!(
            read("'(1 2)"),
            list(vec![Value::symbol("quote"), list(vec![Value::Int(1), Value::Int(2)])])
        );
    }

    #[test]
    fn reads_several_forms() {
        assert_eq!(
            read_all("1 (2) x").unwrap(),
            vec![Value::Int(1), list(vec![Value::Int(2)]), Value::symbol("x")]
        );
        assert_eq!(read_all("").unwrap(), vec![]);
    }

    #[test]
    fn unbalanced_parens_are_errors() {
        assert!(read_str("(1 2").is_err());
        assert!(read_str(")").is_err());
        assert!(read_str("(1 2]").is_err());
    }

    #[test]
    fn errors_carry_position() {
        let err = read_str("(1\n  \"oops").unwrap_err();
        assert_eq!((err.line, err.col), (2, 3));
    }

    #[test]
    fn skips_line_comments() {
        assert_eq!(read("; hello\n42"), Value::Int(42));
        assert_eq!(read("(1 ; two\n 3)"), list(vec![Value::Int(1), Value::Int(3)]));
        assert_eq!(read_all("7 ; trailing").unwrap(), vec![Value::Int(7)]);
    }

    #[test]
    fn skips_datum_comments() {
        assert_eq!(read("#_ 1 2"), Value::Int(2));
        assert_eq!(read("(1 #_(2 3) 4)"), list(vec![Value::Int(1), Value::Int(4)]));
        assert_eq!(read_all("#_1").unwrap(), vec![]);
    }

    #[test]
    fn reads_vectors() {
        assert_eq!(read("[1 2 3]"), Value::vector([Value::Int(1), Value::Int(2), Value::Int(3)]));
        assert_eq!(read("[]"), Value::vector([]));
        assert_eq!(
            read("(f [x [y]])"),
            list(vec![
                Value::symbol("f"),
                Value::vector([Value::symbol("x"), Value::vector([Value::symbol("y")])])
            ])
        );
        assert!(read_str("[1 2").is_err());
        assert!(read_str("[1 2)").is_err());
    }

    #[test]
    fn skips_block_comments() {
        assert_eq!(read("#| note |# 5"), Value::Int(5));
        assert_eq!(read("#| outer #| inner |# still |# 5"), Value::Int(5));
        assert!(read_str("#| open").is_err());
    }
}
