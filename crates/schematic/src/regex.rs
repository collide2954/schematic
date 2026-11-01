const LIMIT: usize = 10_000;

#[derive(Clone, Debug, PartialEq)]
enum Class {
    Range(char, char),
    Digit(bool),
    Word(bool),
    Space(bool),
    Set(Vec<Class>, bool),
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Class {
    fn matches(&self, c: char) -> bool {
        match self {
            Class::Range(lo, hi) => *lo <= c && c <= *hi,
            Class::Digit(negated) => c.is_ascii_digit() != *negated,
            Class::Word(negated) => is_word(c) != *negated,
            Class::Space(negated) => c.is_whitespace() != *negated,
            Class::Set(items, negated) => items.iter().any(|item| item.matches(c)) != *negated,
        }
    }
}

enum Node {
    Empty,
    Char(char),
    Any,
    Class(Class),
    Start,
    End,
    Boundary(bool),
    Group(Box<Node>, Option<usize>),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat(Box<Node>, usize, Option<usize>, bool),
}

enum Escaped {
    Char(char),
    Class(Class),
    Boundary(bool),
}

struct Parser<'a> {
    chars: &'a [char],
    pos: usize,
    groups: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_alt(&mut self) -> Result<Node, String> {
        let mut alternatives = vec![self.parse_concat()?];
        while self.eat('|') {
            alternatives.push(self.parse_concat()?);
        }
        Ok(if alternatives.len() == 1 { alternatives.pop().unwrap() } else { Node::Alt(alternatives) })
    }

    fn parse_concat(&mut self) -> Result<Node, String> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            items.push(self.parse_repeat()?);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap(),
            _ => Node::Concat(items),
        })
    }

    fn parse_repeat(&mut self) -> Result<Node, String> {
        let atom = self.parse_atom()?;
        let (min, max) = match self.peek() {
            Some('*') => {
                self.bump();
                (0, None)
            }
            Some('+') => {
                self.bump();
                (1, None)
            }
            Some('?') => {
                self.bump();
                (0, Some(1))
            }
            Some('{') => match self.parse_braces() {
                Some(bounds) => bounds,
                None => return Ok(atom),
            },
            _ => return Ok(atom),
        };
        if matches!(atom, Node::Empty | Node::Start | Node::End | Node::Boundary(_)) {
            return Err("nothing to repeat".to_string());
        }
        let greedy = !self.eat('?');
        if let Some(max) = max
            && max < min {
                return Err(format!("repeat range {min},{max} is out of order"));
            }
        Ok(Node::Repeat(Box::new(atom), min, max, greedy))
    }

    fn number(&mut self) -> Option<usize> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect::<String>().parse().ok()
    }

    fn parse_braces(&mut self) -> Option<(usize, Option<usize>)> {
        let saved = self.pos;
        self.bump();
        let bounds = (|| {
            let min = self.number()?;
            if self.eat('}') {
                return Some((min, Some(min)));
            }
            if !self.eat(',') {
                return None;
            }
            if self.eat('}') {
                return Some((min, None));
            }
            let max = self.number()?;
            self.eat('}').then_some((min, Some(max)))
        })();
        if bounds.is_none() {
            self.pos = saved;
        }
        bounds
    }

    fn parse_atom(&mut self) -> Result<Node, String> {
        match self.bump() {
            None => Err("unexpected end of pattern".to_string()),
            Some('(') => {
                let capture = if self.eat('?') {
                    match self.bump() {
                        Some(':') => None,
                        Some('<') | Some('P') => return Err("named groups are not supported".to_string()),
                        Some(other) => return Err(format!("unknown group kind (?{other}")),
                        None => return Err("missing )".to_string()),
                    }
                } else {
                    self.groups += 1;
                    Some(self.groups)
                };
                let inner = self.parse_alt()?;
                if !self.eat(')') {
                    return Err("missing )".to_string());
                }
                Ok(Node::Group(Box::new(inner), capture))
            }
            Some(')') => Err("unmatched )".to_string()),
            Some('[') => Ok(Node::Class(self.parse_class()?)),
            Some('.') => Ok(Node::Any),
            Some('^') => Ok(Node::Start),
            Some('$') => Ok(Node::End),
            Some('*' | '+' | '?') => Err("nothing to repeat".to_string()),
            Some('\\') => Ok(match self.parse_escape(false)? {
                Escaped::Char(c) => Node::Char(c),
                Escaped::Class(class) => Node::Class(class),
                Escaped::Boundary(word) => Node::Boundary(word),
            }),
            Some(c) => Ok(Node::Char(c)),
        }
    }

    fn parse_escape(&mut self, in_class: bool) -> Result<Escaped, String> {
        Ok(match self.bump() {
            None => return Err("a pattern cannot end with \\".to_string()),
            Some('d') => Escaped::Class(Class::Digit(false)),
            Some('D') => Escaped::Class(Class::Digit(true)),
            Some('w') => Escaped::Class(Class::Word(false)),
            Some('W') => Escaped::Class(Class::Word(true)),
            Some('s') => Escaped::Class(Class::Space(false)),
            Some('S') => Escaped::Class(Class::Space(true)),
            Some('b') if !in_class => Escaped::Boundary(true),
            Some('B') if !in_class => Escaped::Boundary(false),
            Some('b') => Escaped::Char('\u{8}'),
            Some('n') => Escaped::Char('\n'),
            Some('t') => Escaped::Char('\t'),
            Some('r') => Escaped::Char('\r'),
            Some('f') => Escaped::Char('\u{c}'),
            Some('v') => Escaped::Char('\u{b}'),
            Some('0') => Escaped::Char('\0'),
            Some(c) if c.is_ascii_digit() => return Err("backreferences are not supported".to_string()),
            Some(c) if c.is_ascii_alphanumeric() => return Err(format!("unknown escape \\{c}")),
            Some(c) => Escaped::Char(c),
        })
    }

    fn parse_class(&mut self) -> Result<Class, String> {
        let negated = self.eat('^');
        let mut items = Vec::new();
        let mut first = true;
        loop {
            let c = self.bump().ok_or_else(|| "missing ]".to_string())?;
            if c == ']' && !first {
                break;
            }
            first = false;
            let lo = match c {
                '\\' => match self.parse_escape(true)? {
                    Escaped::Char(c) => c,
                    Escaped::Class(class) => {
                        items.push(class);
                        continue;
                    }
                    Escaped::Boundary(_) => unreachable!(),
                },
                c => c,
            };
            if self.peek() == Some('-') && self.chars.get(self.pos + 1).is_some_and(|&next| next != ']') {
                self.bump();
                let hi = match self.bump().ok_or_else(|| "missing ]".to_string())? {
                    '\\' => match self.parse_escape(true)? {
                        Escaped::Char(c) => c,
                        _ => return Err("a class escape cannot end a range".to_string()),
                    },
                    c => c,
                };
                if hi < lo {
                    return Err(format!("range {lo}-{hi} is out of order"));
                }
                items.push(Class::Range(lo, hi));
            } else {
                items.push(Class::Range(lo, lo));
            }
        }
        Ok(Class::Set(items, negated))
    }
}

#[derive(Clone, Debug)]
enum Inst {
    Char(char),
    Any,
    Class(Class),
    Split(usize, usize),
    Jmp(usize),
    Save(usize),
    Start,
    End,
    Boundary(bool),
    Match,
}

struct Compiler {
    insts: Vec<Inst>,
}

impl Compiler {
    fn emit(&mut self, inst: Inst) -> usize {
        self.insts.push(inst);
        self.insts.len() - 1
    }

    fn compile(&mut self, node: &Node) -> Result<(), String> {
        if self.insts.len() > LIMIT {
            return Err("pattern is too large".to_string());
        }
        match node {
            Node::Empty => {}
            Node::Char(c) => {
                self.emit(Inst::Char(*c));
            }
            Node::Any => {
                self.emit(Inst::Any);
            }
            Node::Class(class) => {
                self.emit(Inst::Class(class.clone()));
            }
            Node::Start => {
                self.emit(Inst::Start);
            }
            Node::End => {
                self.emit(Inst::End);
            }
            Node::Boundary(word) => {
                self.emit(Inst::Boundary(*word));
            }
            Node::Group(inner, Some(index)) => {
                self.emit(Inst::Save(2 * index));
                self.compile(inner)?;
                self.emit(Inst::Save(2 * index + 1));
            }
            Node::Group(inner, None) => self.compile(inner)?,
            Node::Concat(items) => {
                for item in items {
                    self.compile(item)?;
                }
            }
            Node::Alt(items) => {
                let mut jumps = Vec::new();
                for (k, item) in items.iter().enumerate() {
                    if k + 1 < items.len() {
                        let split = self.emit(Inst::Split(0, 0));
                        self.compile(item)?;
                        jumps.push(self.emit(Inst::Jmp(0)));
                        let next = self.insts.len();
                        self.insts[split] = Inst::Split(split + 1, next);
                    } else {
                        self.compile(item)?;
                    }
                }
                let end = self.insts.len();
                for jump in jumps {
                    self.insts[jump] = Inst::Jmp(end);
                }
            }
            Node::Repeat(inner, min, max, greedy) => {
                for _ in 0..*min {
                    self.compile(inner)?;
                }
                let branch = |split: usize, exit: usize| if *greedy { Inst::Split(split + 1, exit) } else { Inst::Split(exit, split + 1) };
                match max {
                    None => {
                        let split = self.emit(Inst::Split(0, 0));
                        self.compile(inner)?;
                        self.emit(Inst::Jmp(split));
                        let exit = self.insts.len();
                        self.insts[split] = branch(split, exit);
                    }
                    Some(max) => {
                        let mut splits = Vec::new();
                        for _ in *min..*max {
                            splits.push(self.emit(Inst::Split(0, 0)));
                            self.compile(inner)?;
                        }
                        let exit = self.insts.len();
                        for split in splits {
                            self.insts[split] = branch(split, exit);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Regex {
    insts: Vec<Inst>,
    pub groups: usize,
    icase: bool,
    whole: bool,
}

#[derive(Clone)]
struct Thread {
    pc: usize,
    caps: Vec<Option<usize>>,
}

pub type Spans = Vec<Option<(usize, usize)>>;

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

impl Regex {
    pub fn new(pattern: &str, whole: bool) -> Result<Regex, String> {
        let (icase, body) = match pattern.strip_prefix("(?i)") {
            Some(rest) => (true, rest),
            None => (false, pattern),
        };
        let chars: Vec<char> = body.chars().collect();
        let mut parser = Parser { chars: &chars, pos: 0, groups: 0 };
        let mut node = parser.parse_alt()?;
        if parser.pos < chars.len() {
            return Err("unmatched )".to_string());
        }
        if whole {
            node = Node::Concat(vec![node, Node::End]);
        }
        let mut compiler = Compiler { insts: vec![Inst::Save(0)] };
        compiler.compile(&node)?;
        compiler.emit(Inst::Save(1));
        compiler.emit(Inst::Match);
        Ok(Regex { insts: compiler.insts, groups: parser.groups, icase, whole })
    }

    fn same(&self, pattern: char, actual: char) -> bool {
        pattern == actual || (self.icase && fold(pattern) == fold(actual))
    }

    fn in_class(&self, class: &Class, c: char) -> bool {
        if class.matches(c) {
            return true;
        }
        if !self.icase {
            return false;
        }
        c.to_lowercase().chain(c.to_uppercase()).any(|variant| class.matches(variant))
    }

    fn add_thread(&self, list: &mut Vec<Thread>, pc: usize, caps: Vec<Option<usize>>, pos: usize, text: &[char], visited: &mut [usize], stamp: usize) {
        if visited[pc] == stamp {
            return;
        }
        visited[pc] = stamp;
        match &self.insts[pc] {
            Inst::Jmp(target) => self.add_thread(list, *target, caps, pos, text, visited, stamp),
            Inst::Split(first, second) => {
                self.add_thread(list, *first, caps.clone(), pos, text, visited, stamp);
                self.add_thread(list, *second, caps, pos, text, visited, stamp);
            }
            Inst::Save(slot) => {
                let mut caps = caps;
                caps[*slot] = Some(pos);
                self.add_thread(list, pc + 1, caps, pos, text, visited, stamp);
            }
            Inst::Start => {
                if pos == 0 {
                    self.add_thread(list, pc + 1, caps, pos, text, visited, stamp);
                }
            }
            Inst::End => {
                if pos == text.len() {
                    self.add_thread(list, pc + 1, caps, pos, text, visited, stamp);
                }
            }
            Inst::Boundary(word) => {
                let before = pos > 0 && is_word(text[pos - 1]);
                let after = pos < text.len() && is_word(text[pos]);
                if (before != after) == *word {
                    self.add_thread(list, pc + 1, caps, pos, text, visited, stamp);
                }
            }
            _ => list.push(Thread { pc, caps }),
        }
    }

    fn run(&self, text: &[char], start: usize) -> Option<Vec<Option<usize>>> {
        let slots = 2 * (self.groups + 1);
        let mut visited = vec![0usize; self.insts.len()];
        let mut stamp = 1;
        let mut current = Vec::new();
        let mut next = Vec::new();
        let mut matched: Option<Vec<Option<usize>>> = None;
        self.add_thread(&mut current, 0, vec![None; slots], start, text, &mut visited, stamp);
        let mut pos = start;
        loop {
            let c = text.get(pos).copied();
            stamp += 1;
            next.clear();
            for i in 0..current.len() {
                let Thread { pc, caps } = current[i].clone();
                match &self.insts[pc] {
                    Inst::Match => {
                        matched = Some(caps);
                        break;
                    }
                    Inst::Char(expected) => {
                        if c.is_some_and(|c| self.same(*expected, c)) {
                            self.add_thread(&mut next, pc + 1, caps, pos + 1, text, &mut visited, stamp);
                        }
                    }
                    Inst::Any => {
                        if c.is_some_and(|c| c != '\n') {
                            self.add_thread(&mut next, pc + 1, caps, pos + 1, text, &mut visited, stamp);
                        }
                    }
                    Inst::Class(class) => {
                        if c.is_some_and(|c| self.in_class(class, c)) {
                            self.add_thread(&mut next, pc + 1, caps, pos + 1, text, &mut visited, stamp);
                        }
                    }
                    _ => {}
                }
            }
            if c.is_none() {
                break;
            }
            if matched.is_none() && !self.whole {
                self.add_thread(&mut next, 0, vec![None; slots], pos + 1, text, &mut visited, stamp);
            }
            if next.is_empty() && (matched.is_some() || self.whole) {
                break;
            }
            std::mem::swap(&mut current, &mut next);
            pos += 1;
        }
        matched
    }

    pub fn find(&self, text: &[char], start: usize) -> Option<Spans> {
        if start > text.len() {
            return None;
        }
        let caps = self.run(text, start)?;
        Some((0..=self.groups).map(|i| caps[2 * i].zip(caps[2 * i + 1])).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(pattern: &str, text: &str) -> Option<Vec<Option<String>>> {
        let chars: Vec<char> = text.chars().collect();
        let regex = Regex::new(pattern, false).unwrap();
        regex.find(&chars, 0).map(|spans| spans.iter().map(|s| s.map(|(a, b)| chars[a..b].iter().collect())).collect())
    }

    fn first(pattern: &str, text: &str) -> Option<String> {
        find(pattern, text).map(|spans| spans[0].clone().unwrap())
    }

    fn whole(pattern: &str, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        Regex::new(pattern, true).unwrap().find(&chars, 0).is_some()
    }

    #[test]
    fn literals_classes_and_quantifiers_match() {
        assert_eq!(first("b", "abc"), Some("b".into()));
        assert_eq!(first("\\d+", "ab 42 x"), Some("42".into()));
        assert_eq!(first("[a-c]+", "xxabcabd"), Some("abcab".into()));
        assert_eq!(first("[^a-c ]+", "ab xyz"), Some("xyz".into()));
        assert_eq!(first("a.c", "abc"), Some("abc".into()));
        assert_eq!(first("a.c", "a\nc"), None);
        assert_eq!(first("colou?r", "color colour"), Some("color".into()));
        assert_eq!(first("a{2}", "aaaa"), Some("aa".into()));
        assert_eq!(first("a{2,}", "aaaa"), Some("aaaa".into()));
        assert_eq!(first("a{2,3}?", "aaaa"), Some("aa".into()));
        assert_eq!(first("a{x", "a{x"), Some("a{x".into()));
        assert_eq!(first("\\s\\S", "a b"), Some(" b".into()));
        assert_eq!(first("[\\d.]+", "v1.25x"), Some("1.25".into()));
        assert_eq!(first("[]a]+", "]a]"), Some("]a]".into()));
    }

    #[test]
    fn alternation_prefers_the_left_branch_and_groups_capture() {
        assert_eq!(first("a|ab", "ab"), Some("a".into()));
        assert_eq!(first("ab|a", "ab"), Some("ab".into()));
        assert_eq!(find("(\\w+)@(\\w+)", "mail ada@host now"), Some(vec![Some("ada@host".into()), Some("ada".into()), Some("host".into())]));
        assert_eq!(find("a(x)?b", "ab"), Some(vec![Some("ab".into()), None]));
        assert_eq!(find("(?:a|b)+(c)", "abbac"), Some(vec![Some("abbac".into()), Some("c".into())]));
        assert_eq!(first("(a*)*b", "aaab"), Some("aaab".into()));
        assert_eq!(first("(a|b)*c", "ababc"), Some("ababc".into()));
    }

    #[test]
    fn anchors_boundaries_and_case_folding() {
        assert_eq!(first("^ab", "abab"), Some("ab".into()));
        assert_eq!(first("^b", "ab"), None);
        assert_eq!(first("b$", "ab"), Some("b".into()));
        assert_eq!(first("\\bcat\\b", "concat cat"), Some("cat".into()));
        assert_eq!(first("\\Bcat", "concat cat"), Some("cat".into()));
        assert_eq!(first("(?i)hello", "Say HeLLo"), Some("HeLLo".into()));
        assert_eq!(first("(?i)[a-c]+", "xBAc"), Some("BAc".into()));
        assert!(whole("\\d+", "123") && !whole("\\d+", "123x") && whole("a|ab", "ab"));
        assert_eq!(first("x*", "abc"), Some(String::new()));
    }

    #[test]
    fn pattern_errors_are_explained() {
        let error = |pattern: &str| Regex::new(pattern, false).unwrap_err();
        assert_eq!(error("("), "missing )");
        assert_eq!(error(")"), "unmatched )");
        assert_eq!(error("a)"), "unmatched )");
        assert_eq!(error("*a"), "nothing to repeat");
        assert_eq!(error("a{3,2}"), "repeat range 3,2 is out of order");
        assert_eq!(error("[z-a]"), "range z-a is out of order");
        assert_eq!(error("[ab"), "missing ]");
        assert_eq!(error("\\1"), "backreferences are not supported");
        assert_eq!(error("\\q"), "unknown escape \\q");
        assert_eq!(error("(?<n>a)"), "named groups are not supported");
        assert_eq!(error("a\\"), "a pattern cannot end with \\");
    }

    #[test]
    fn pathological_patterns_stay_linear() {
        let text: String = "a".repeat(40);
        assert_eq!(first("(a*)*b", &text), None);
        assert_eq!(first("(a|aa)+$", &text).as_deref(), Some(text.as_str()));
    }
}
