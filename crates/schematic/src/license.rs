use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    One { license: String, exception: Option<String> },
    Any(Vec<Expression>),
    All(Vec<Expression>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Open,
    Close,
    Or,
    And,
    With,
    Id(String),
}

fn tokens(text: &str) -> Option<Vec<Token>> {
    let mut out = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Vec<Token>| {
        if word.is_empty() {
            return;
        }
        out.push(match word.to_ascii_uppercase().as_str() {
            "OR" => Token::Or,
            "AND" => Token::And,
            "WITH" => Token::With,
            _ => Token::Id(std::mem::take(word)),
        });
        word.clear();
    };
    for c in text.chars() {
        match c {
            '(' | ')' => {
                flush(&mut word, &mut out);
                out.push(if c == '(' { Token::Open } else { Token::Close });
            }
            c if c.is_whitespace() => flush(&mut word, &mut out),
            c if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '+') => word.push(c),
            _ => return None,
        }
    }
    flush(&mut word, &mut out);
    Some(out)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn take(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn expression(&mut self) -> Option<Expression> {
        let mut parts = vec![self.conjunction()?];
        while self.peek() == Some(&Token::Or) {
            self.at += 1;
            parts.push(self.conjunction()?);
        }
        Some(if parts.len() == 1 { parts.pop()? } else { Expression::Any(parts) })
    }

    fn conjunction(&mut self) -> Option<Expression> {
        let mut parts = vec![self.primary()?];
        while self.peek() == Some(&Token::And) {
            self.at += 1;
            parts.push(self.primary()?);
        }
        Some(if parts.len() == 1 { parts.pop()? } else { Expression::All(parts) })
    }

    fn primary(&mut self) -> Option<Expression> {
        match self.take()? {
            Token::Open => {
                let inner = self.expression()?;
                (self.take()? == Token::Close).then_some(inner)
            }
            Token::Id(license) => {
                let exception = if self.peek() == Some(&Token::With) {
                    self.at += 1;
                    match self.take()? {
                        Token::Id(exception) => Some(exception),
                        _ => return None,
                    }
                } else {
                    None
                };
                Some(Expression::One { license, exception })
            }
            _ => None,
        }
    }
}

pub fn parse(text: &str) -> Option<Expression> {
    let mut parser = Parser { tokens: tokens(text)?, at: 0 };
    let expression = parser.expression()?;
    (parser.at == parser.tokens.len()).then_some(expression)
}

impl Expression {
    pub fn permitted_by(&self, allowed: &[String]) -> bool {
        match self {
            Expression::One { license, exception } => allowed.iter().any(|entry| {
                let entry = entry.trim();
                entry.eq_ignore_ascii_case(license)
                    || exception.as_ref().is_some_and(|exception| entry.eq_ignore_ascii_case(&format!("{license} WITH {exception}")))
            }),
            Expression::Any(parts) => parts.iter().any(|part| part.permitted_by(allowed)),
            Expression::All(parts) => parts.iter().all(|part| part.permitted_by(allowed)),
        }
    }

    pub fn licenses(&self) -> Vec<String> {
        match self {
            Expression::One { license, .. } => vec![license.clone()],
            Expression::Any(parts) | Expression::All(parts) => parts.iter().flat_map(Expression::licenses).collect(),
        }
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn part(expression: &Expression, f: &mut fmt::Formatter<'_>, nested: bool) -> fmt::Result {
            match expression {
                Expression::One { license, exception: None } => write!(f, "{license}"),
                Expression::One { license, exception: Some(exception) } => write!(f, "{license} WITH {exception}"),
                Expression::Any(parts) | Expression::All(parts) => {
                    let word = if matches!(expression, Expression::Any(_)) { " OR " } else { " AND " };
                    if nested {
                        write!(f, "(")?;
                    }
                    for (i, inner) in parts.iter().enumerate() {
                        if i > 0 {
                            write!(f, "{word}")?;
                        }
                        part(inner, f, true)?;
                    }
                    if nested {
                        write!(f, ")")?;
                    }
                    Ok(())
                }
            }
        }
        part(self, f, false)
    }
}

pub fn permits(expression: &str, allowed: &[String]) -> bool {
    parse(expression).is_some_and(|e| e.permitted_by(allowed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allow(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn spdx_expressions_are_read_and_checked_against_allowed_lists() {
        assert!(permits("MIT", &allow(&["mit"])));
        assert!(permits("MIT OR Apache-2.0", &allow(&["Apache-2.0"])));
        assert!(!permits("MIT AND BSD-3-Clause", &allow(&["MIT"])));
        assert!(permits("MIT AND BSD-3-Clause", &allow(&["MIT", "BSD-3-Clause"])));
        assert!(permits("(MIT OR GPL-3.0-only) AND Apache-2.0", &allow(&["GPL-3.0-only", "Apache-2.0"])));
        assert!(!permits("(MIT OR GPL-3.0-only) AND Apache-2.0", &allow(&["MIT"])));
        assert!(permits("MIT OR GPL-3.0-only AND Apache-2.0", &allow(&["MIT"])), "AND binds tighter than OR");
        assert!(permits("GPL-2.0-only WITH Classpath-exception-2.0", &allow(&["GPL-2.0-only WITH Classpath-exception-2.0"])));
        assert!(permits("GPL-2.0-only WITH Classpath-exception-2.0", &allow(&["GPL-2.0-only"])), "an exception only adds permissions");
        assert!(permits("mit or apache-2.0", &allow(&["MIT"])), "operators and names are case-insensitive");
        assert!(!permits("Proprietary", &allow(&["MIT"])));
        assert!(!permits("MIT", &[]));
        assert!(!permits("MIT OR", &allow(&["MIT"])), "unreadable expressions permit nothing");
        for broken in ["", "MIT OR", "(MIT", "MIT)", "MIT WITH", "AND MIT", "MIT Apache-2.0", "MIT; Apache"] {
            assert_eq!(parse(broken), None, "{broken:?}");
        }
        let expression = parse("MIT AND (Apache-2.0 OR BSD-2-Clause)").unwrap();
        assert_eq!(expression.to_string(), "MIT AND (Apache-2.0 OR BSD-2-Clause)");
        assert_eq!(expression.licenses(), ["MIT", "Apache-2.0", "BSD-2-Clause"]);
        assert_eq!(parse("( MIT )").unwrap().to_string(), "MIT");
        assert_eq!(parse("GPL-2.0-only WITH Classpath-exception-2.0").unwrap().to_string(), "GPL-2.0-only WITH Classpath-exception-2.0");
    }
}
