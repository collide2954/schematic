use std::ops::Range;

use iced::advanced::text::highlighter::{Format, Highlighter};
use iced::{Font, Theme};

use crate::{fonts, theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Comment,
    Str,
    Number,
    Keyword,
    Special,
    Constant,
    Bracket(u8),
    Match,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineState {
    pub depth: usize,
    pub block_comment: bool,
    pub in_string: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    pub pair: Option<[(usize, usize); 2]>,
}

const SPECIALS: &[&str] = &[
    "define", "lambda", "let", "let*", "if", "cond", "else", "when", "unless", "begin", "set!", "and", "or", "loop",
    "recur", "try", "catch", "finally", "call/ec", "quote", "quasiquote", "unquote", "define-syntax", "syntax-rules",
];

fn is_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';' | ',')
}

fn looks_numeric(atom: &str) -> bool {
    let digits = atom.trim_start_matches(['+', '-']);
    digits.starts_with(|c: char| c.is_ascii_digit()) && !digits.is_empty()
}

fn bracket(state: &mut LineState, open: bool) -> Kind {
    if open {
        let kind = Kind::Bracket((state.depth % 3) as u8);
        state.depth += 1;
        kind
    } else {
        state.depth = state.depth.saturating_sub(1);
        Kind::Bracket((state.depth % 3) as u8)
    }
}

pub fn tokens(line: &str, state: &mut LineState) -> Vec<(Range<usize>, Kind)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];
        if state.block_comment {
            let end = rest.find("|#").map_or(line.len(), |n| i + n + 2);
            state.block_comment = end == line.len() && !rest.ends_with("|#");
            out.push((i..end, Kind::Comment));
            i = end;
            continue;
        }
        if state.in_string {
            let end = string_end(rest).map_or(line.len(), |n| i + n);
            state.in_string = end == line.len() && string_end(rest).is_none();
            out.push((i..end, Kind::Str));
            i = end;
            continue;
        }
        let c = rest.chars().next().unwrap();
        if c == ';' {
            out.push((i..line.len(), Kind::Comment));
            break;
        }
        if rest.starts_with("#|") {
            state.block_comment = true;
            continue;
        }
        if c == '"' {
            state.in_string = true;
            let start = i;
            i += 1;
            let end = string_end(&line[i..]).map_or(line.len(), |n| i + n);
            state.in_string = end == line.len() && string_end(&line[i..]).is_none();
            out.push((start..end, Kind::Str));
            i = end;
            continue;
        }
        if rest.starts_with("#[") || rest.starts_with("#{") {
            out.push((i..i + 2, bracket(state, true)));
            i += 2;
            continue;
        }
        if matches!(c, '(' | '[' | '{') {
            out.push((i..i + 1, bracket(state, true)));
            i += 1;
            continue;
        }
        if matches!(c, ')' | ']' | '}') {
            out.push((i..i + 1, bracket(state, false)));
            i += 1;
            continue;
        }
        if c.is_whitespace() || c == ',' || c == '\'' || c == '`' || c == '~' {
            i += c.len_utf8();
            continue;
        }
        let len = rest.find(is_delimiter).unwrap_or(rest.len());
        let atom = &rest[..len];
        let kind = if atom.starts_with(':') {
            Some(Kind::Keyword)
        } else if looks_numeric(atom) {
            Some(Kind::Number)
        } else if matches!(atom, "true" | "false" | "nil") {
            Some(Kind::Constant)
        } else if SPECIALS.contains(&atom) {
            Some(Kind::Special)
        } else {
            None
        };
        if let Some(kind) = kind {
            out.push((i..i + len, kind));
        }
        i += len.max(1);
    }
    out
}

fn string_end(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

pub struct Schematic {
    states: Vec<LineState>,
    current: usize,
    marks: Marks,
}

impl Highlighter for Schematic {
    type Settings = Marks;
    type Highlight = Kind;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Kind)>;

    fn new(settings: &Marks) -> Self {
        Schematic { states: vec![LineState::default()], current: 0, marks: settings.clone() }
    }

    fn update(&mut self, settings: &Marks) {
        self.marks = settings.clone();
        self.states.truncate(1);
        self.current = 0;
    }

    fn change_line(&mut self, line: usize) {
        self.current = line.min(self.states.len().saturating_sub(1));
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let mut state = self.states.get(self.current).copied().unwrap_or_default();
        let mut out = tokens(line, &mut state);
        for &(mark_line, column) in self.marks.pair.iter().flatten() {
            if mark_line == self.current {
                for (range, kind) in &mut out {
                    if matches!(kind, Kind::Bracket(_)) && range.contains(&column) {
                        *kind = Kind::Match;
                    }
                }
            }
        }
        self.current += 1;
        if self.states.len() <= self.current {
            self.states.push(state);
        } else {
            self.states[self.current] = state;
        }
        out.into_iter()
    }

    fn current_line(&self) -> usize {
        self.current
    }
}

pub fn format(kind: &Kind, theme: &Theme) -> Format<Font> {
    let t = theme::tones(theme);
    let (color, font) = match kind {
        Kind::Comment => (t.comment, Some(fonts::MONO_ITALIC)),
        Kind::Str => (t.string, None),
        Kind::Number => (t.number, None),
        Kind::Keyword => (t.keyword, None),
        Kind::Special => (t.special, None),
        Kind::Constant => (t.constant, None),
        Kind::Bracket(depth) => (t.brackets[*depth as usize % 3], None),
        Kind::Match => (t.accent, Some(fonts::MONO_BOLD)),
    };
    Format { color: Some(color), font }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str) -> Vec<(&str, Kind)> {
        let mut state = LineState::default();
        tokens(line, &mut state).into_iter().map(|(r, k)| (&line[r], k)).collect()
    }

    #[test]
    fn classifies_atoms() {
        assert_eq!(
            kinds("(define x 42)"),
            [("(", Kind::Bracket(0)), ("define", Kind::Special), ("42", Kind::Number), (")", Kind::Bracket(0))]
        );
        assert_eq!(
            kinds(":name \"a;b\" nil -1.5 #[1]"),
            [
                (":name", Kind::Keyword),
                ("\"a;b\"", Kind::Str),
                ("nil", Kind::Constant),
                ("-1.5", Kind::Number),
                ("#[", Kind::Bracket(0)),
                ("1", Kind::Number),
                ("]", Kind::Bracket(0))
            ]
        );
    }

    #[test]
    fn plain_symbols_and_quotes_produce_no_token() {
        assert_eq!(kinds("foo bar-baz 'x ~y"), []);
        assert_eq!(kinds("- + inc"), []);
    }

    #[test]
    fn line_comments_run_to_the_end() {
        assert_eq!(kinds("x ; a \"b\""), [("; a \"b\"", Kind::Comment)]);
    }

    #[test]
    fn block_comments_and_strings_carry_across_lines() {
        let mut state = LineState::default();
        let first = tokens("(x #| start", &mut state);
        assert_eq!(first, [(0..1, Kind::Bracket(0)), (3..11, Kind::Comment)]);
        assert!(state.block_comment);
        let second = tokens("still |# y)", &mut state);
        assert_eq!(second, [(0..8, Kind::Comment), (10..11, Kind::Bracket(0))]);
        assert_eq!(state, LineState::default());
        let mut state = LineState::default();
        assert_eq!(tokens("\"open", &mut state), [(0..5, Kind::Str)]);
        assert!(state.in_string);
        assert_eq!(tokens("close\" 1", &mut state), [(0..6, Kind::Str), (7..8, Kind::Number)]);
        assert!(!state.in_string);
    }

    #[test]
    fn bracket_depth_cycles_and_survives_lines() {
        let depths: Vec<Kind> = kinds("(([{").into_iter().map(|(_, k)| k).collect();
        assert_eq!(depths, [Kind::Bracket(0), Kind::Bracket(1), Kind::Bracket(2), Kind::Bracket(0)]);
        let mut state = LineState::default();
        tokens("((", &mut state);
        assert_eq!(state.depth, 2);
        assert_eq!(tokens(")", &mut state), [(0..1, Kind::Bracket(1))]);
        assert_eq!(tokens("))", &mut state), [(0..1, Kind::Bracket(0)), (1..2, Kind::Bracket(0))]);
    }

    #[test]
    fn marked_brackets_highlight_as_a_match() {
        let marks = Marks { pair: Some([(0, 3), (1, 5)]) };
        let mut h = Schematic::new(&marks);
        assert_eq!(h.highlight_line("(a #[1").collect::<Vec<_>>(), [(0..1, Kind::Bracket(0)), (3..5, Kind::Match), (5..6, Kind::Number)]);
        assert_eq!(h.highlight_line("   2])").collect::<Vec<_>>(), [(3..4, Kind::Number), (4..5, Kind::Bracket(1)), (5..6, Kind::Match)]);
        h.update(&Marks::default());
        assert_eq!(h.current_line(), 0);
        assert_eq!(h.highlight_line("(a #[1").collect::<Vec<_>>(), [(0..1, Kind::Bracket(0)), (3..5, Kind::Bracket(1)), (5..6, Kind::Number)]);
    }

    #[test]
    fn the_highlighter_rehighlights_from_a_changed_line() {
        let mut h = Schematic::new(&Marks::default());
        assert_eq!(h.highlight_line("(x #|").count(), 2);
        assert_eq!(h.highlight_line("y").collect::<Vec<_>>(), [(0..1, Kind::Comment)]);
        h.change_line(0);
        assert_eq!(h.current_line(), 0);
        assert_eq!(h.highlight_line("(x)").count(), 2);
        assert_eq!(h.highlight_line("y").collect::<Vec<_>>(), []);
    }
}
