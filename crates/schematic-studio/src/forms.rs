use std::ops::Range;

use crate::highlight::SPECIALS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Context {
    pub before: Option<char>,
    pub after: Option<char>,
    pub in_string: bool,
    pub in_comment: bool,
}

struct Scan {
    openers: Vec<usize>,
    in_string: bool,
    in_comment: bool,
}

fn scan(text: &str, offset: usize) -> Scan {
    let bytes = text.as_bytes();
    let end = offset.min(bytes.len());
    let mut scan = Scan { openers: Vec::new(), in_string: false, in_comment: false };
    let mut i = 0;
    while i < end {
        match bytes[i] {
            b';' => match text[i..].find('\n') {
                Some(n) if i + n < end => i += n + 1,
                _ => {
                    scan.in_comment = true;
                    i = end;
                }
            },
            b'#' if bytes.get(i + 1) == Some(&b'|') => match text[i + 2..].find("|#") {
                Some(n) if i + 4 + n <= end => i += 4 + n,
                _ => {
                    scan.in_comment = true;
                    i = end;
                }
            },
            b'"' => {
                let mut j = i + 1;
                loop {
                    if j >= end {
                        scan.in_string = true;
                        i = end;
                        break;
                    }
                    match bytes[j] {
                        b'\\' => j += 2,
                        b'"' => {
                            i = j + 1;
                            break;
                        }
                        _ => j += 1,
                    }
                }
            }
            b'(' | b'[' | b'{' => {
                scan.openers.push(i);
                i += 1;
            }
            b')' | b']' | b'}' => {
                scan.openers.pop();
                i += 1;
            }
            _ => i += 1,
        }
    }
    scan
}

pub fn context_at(text: &str, offset: usize) -> Context {
    let offset = offset.min(text.len());
    let scan = scan(text, offset);
    Context {
        before: text[..offset].chars().next_back(),
        after: text[offset..].chars().next(),
        in_string: scan.in_string,
        in_comment: scan.in_comment,
    }
}

fn column_of(text: &str, index: usize) -> usize {
    let line_start = text[..index].rfind('\n').map_or(0, |n| n + 1);
    text[line_start..index].chars().count()
}

fn is_token_end(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';')
}

pub fn indent_for(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    let Some(&open) = scan(text, offset).openers.last() else { return 0 };
    let column = column_of(text, open);
    if text.as_bytes()[open] != b'(' {
        return column + 1;
    }
    let line = text[open + 1..offset].split('\n').next().unwrap_or("");
    let trimmed = line.trim_start();
    let head_len = trimmed.find(is_token_end).unwrap_or(trimmed.len());
    let head = &trimmed[..head_len];
    if head.is_empty() {
        return column + 1;
    }
    if SPECIALS.contains(&head) || head == "ns" {
        return column + 2;
    }
    let after_head = trimmed[head_len..].trim_start();
    if after_head.is_empty() || after_head.starts_with(';') {
        return column + 1;
    }
    column + 1 + line[..line.len() - after_head.len()].chars().count()
}

pub fn reindent(text: &str, lines: Option<Range<usize>>) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let body = line.trim_start();
        let state = scan(&out, out.len());
        let chosen = lines.as_ref().is_none_or(|range| range.contains(&i));
        if !chosen || state.in_string || state.in_comment {
            out.push_str(line);
            continue;
        }
        if body.is_empty() {
            continue;
        }
        let indent = indent_for(&out, out.len());
        out.extend(std::iter::repeat_n(' ', indent));
        out.push_str(body);
    }
    out
}

pub fn find_all(text: &str, query: &str, case: bool) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    if case {
        return text.match_indices(query).map(|(i, found)| (i, i + found.len())).collect();
    }
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let hay: Vec<(usize, char)> = text.char_indices().map(|(i, c)| (i, fold(c))).collect();
    let needle: Vec<char> = query.chars().map(fold).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()].iter().zip(&needle).all(|((_, a), b)| a == b) {
            let start = hay[i].0;
            let last = hay[i + needle.len() - 1].0;
            out.push((start, last + text[last..].chars().next().map_or(1, char::len_utf8)));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

pub fn toggle_comment(text: &str, first: usize, last: usize) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let top = lines.len().saturating_sub(1);
    let targets: Vec<usize> = (first.min(top)..=last.min(top)).filter(|&i| !lines[i].trim().is_empty()).collect();
    if targets.is_empty() {
        return text.to_string();
    }
    let lead = |line: &str| line.len() - line.trim_start().len();
    let commented = targets.iter().all(|&i| lines[i].trim_start().starts_with(';'));
    let rewritten: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if !targets.contains(&i) {
                return line.to_string();
            }
            let at = lead(line);
            if commented {
                let body = line[at..].trim_start_matches(';');
                format!("{}{}", &line[..at], body.strip_prefix(' ').unwrap_or(body))
            } else {
                format!("{};; {}", &line[..at], &line[at..])
            }
        })
        .collect();
    rewritten.join("\n")
}

fn finish(forms: &mut Vec<Range<usize>>, start: &mut Option<usize>, end: usize) {
    if let Some(s) = start.take() {
        forms.push(s..end);
    }
}

fn is_prefix(text: &str) -> bool {
    text.bytes().all(|b| matches!(b, b'\'' | b'`' | b'~' | b'@' | b'#' | b'_'))
}

pub fn top_level_forms(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut forms = Vec::new();
    let mut start: Option<usize> = None;
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let top = depth == 0;
        let prefixed = top && start.is_some_and(|s| is_prefix(&text[s..i]));
        match c {
            b';' => {
                if top {
                    finish(&mut forms, &mut start, i);
                }
                i = text[i..].find('\n').map_or(bytes.len(), |n| i + n);
            }
            b'#' if bytes.get(i + 1) == Some(&b'|') => {
                if top {
                    finish(&mut forms, &mut start, i);
                }
                i = text[i + 2..].find("|#").map_or(bytes.len(), |n| i + 4 + n);
            }
            b'"' => {
                if top && !prefixed {
                    finish(&mut forms, &mut start, i);
                }
                if top && start.is_none() {
                    start = Some(i);
                }
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
                if top {
                    finish(&mut forms, &mut start, i);
                }
            }
            b'(' | b'[' | b'{' => {
                if top && !prefixed {
                    finish(&mut forms, &mut start, i);
                }
                if top && start.is_none() {
                    start = Some(i);
                }
                depth += 1;
                i += 1;
            }
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
                if depth == 0 {
                    finish(&mut forms, &mut start, i);
                }
            }
            c if c.is_ascii_whitespace() || c == b',' => {
                if top {
                    finish(&mut forms, &mut start, i);
                }
                i += 1;
            }
            _ => {
                if top && start.is_none() {
                    start = Some(i);
                }
                i += 1;
            }
        }
    }
    finish(&mut forms, &mut start, bytes.len());
    forms
}

pub fn bracket_pairs(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut pairs = Vec::new();
    let mut open = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b';' => i = text[i..].find('\n').map_or(bytes.len(), |n| i + n),
            b'#' if bytes.get(i + 1) == Some(&b'|') => {
                i = text[i + 2..].find("|#").map_or(bytes.len(), |n| i + 4 + n);
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            b'(' | b'[' | b'{' => {
                open.push(i);
                i += 1;
            }
            b')' | b']' | b'}' => {
                if let Some(start) = open.pop() {
                    pairs.push((start, i));
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    pairs
}

pub fn matching_bracket(text: &str, offset: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let opens = offset < bytes.len() && matches!(bytes[offset], b'(' | b'[' | b'{');
    let closes = offset > 0 && matches!(bytes[offset - 1], b')' | b']' | b'}');
    if !opens && !closes {
        return None;
    }
    bracket_pairs(text).into_iter().find(|&(start, end)| (opens && start == offset) || (closes && end + 1 == offset))
}

pub fn line_offset(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line_start = before.rfind('\n').map_or(0, |n| n + 1);
    (before.matches('\n').count(), before.len() - line_start)
}

pub fn form_at(text: &str, offset: usize) -> Option<Range<usize>> {
    let forms = top_level_forms(text);
    forms
        .iter()
        .find(|r| r.start <= offset && offset <= r.end)
        .or_else(|| forms.iter().find(|r| r.start >= offset))
        .or_else(|| forms.last())
        .cloned()
}

pub fn next_form(text: &str, offset: usize) -> Option<Range<usize>> {
    top_level_forms(text).into_iter().find(|r| r.start >= offset)
}

pub fn offset_of(text: &str, line: usize, column: usize) -> usize {
    let mut offset = 0;
    for (index, current) in text.split('\n').enumerate() {
        if index == line {
            let within: usize = current.chars().take(column).map(char::len_utf8).sum();
            return offset + within;
        }
        offset += current.len() + 1;
    }
    text.len()
}

pub fn symbol_range_at(text: &str, offset: usize) -> Option<Range<usize>> {
    let offset = offset.min(text.len());
    if !text.is_char_boundary(offset) {
        return None;
    }
    let delimiter = |c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';' | ',' | '\'' | '`' | '~' | '@');
    let start = text[..offset].rfind(delimiter).map_or(0, |i| i + text[i..].chars().next().map_or(1, char::len_utf8));
    let end = text[offset..].find(delimiter).map_or(text.len(), |i| offset + i);
    (start < end).then_some(start..end)
}

pub fn symbol_at(text: &str, offset: usize) -> Option<&str> {
    symbol_range_at(text, offset).map(|range| &text[range])
}

pub fn enclosing(text: &str, start: usize, end: usize) -> Option<Range<usize>> {
    if start == end
        && let Some(symbol) = symbol_range_at(text, start)
        && (symbol.start < start || symbol.end > end) {
            return Some(symbol);
        }
    let bytes = text.as_bytes();
    let mut best: Option<Range<usize>> = None;
    for (open, close) in bracket_pairs(text) {
        let mut from = open;
        while from > 0 && matches!(bytes[from - 1], b'\'' | b'`' | b'~' | b'@' | b'#') {
            from -= 1;
        }
        let range = from..close + 1;
        let around = range.start <= start && end <= range.end && (range.start < start || range.end > end);
        if around && best.as_ref().is_none_or(|b| range.len() < b.len()) {
            best = Some(range);
        }
    }
    best
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutlineKind {
    Namespace,
    Function,
    Value,
    Macro,
}

impl OutlineKind {
    pub fn label(self) -> &'static str {
        match self {
            OutlineKind::Namespace => "ns",
            OutlineKind::Function => "fn",
            OutlineKind::Value => "value",
            OutlineKind::Macro => "macro",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineItem {
    pub line: usize,
    pub offset: usize,
    pub label: String,
    pub kind: OutlineKind,
}

pub fn outline(text: &str) -> Vec<OutlineItem> {
    use schematic::Value;
    let mut out = Vec::new();
    for range in top_level_forms(text) {
        let Ok(Value::List(form)) = schematic::reader::read_str(&text[range.clone()]) else { continue };
        let items: Vec<&Value> = form.iter().collect();
        let (Some(Value::Symbol(head)), Some(target)) = (items.first(), items.get(1)) else { continue };
        let (label, kind) = match (&*head.name(), target) {
            ("ns", Value::Symbol(name)) => (name.to_string(), OutlineKind::Namespace),
            ("define-syntax", Value::Symbol(name)) => (name.to_string(), OutlineKind::Macro),
            ("define", Value::List(signature)) => match signature.first() {
                Some(Value::Symbol(name)) => (name.to_string(), OutlineKind::Function),
                _ => continue,
            },
            ("define", Value::Symbol(name)) => {
                let lambda = matches!(items.get(2), Some(Value::List(value)) if matches!(value.first(), Some(Value::Symbol(s)) if &*s.name() == "lambda"));
                (name.to_string(), if lambda { OutlineKind::Function } else { OutlineKind::Value })
            }
            ("define", pattern) if schematic::patterns::is_pattern(pattern) => {
                let names: Vec<String> = schematic::patterns::names(pattern).iter().map(ToString::to_string).collect();
                (names.join(" "), OutlineKind::Value)
            }
            _ => continue,
        };
        let (line, _) = position_of(text, range.start);
        out.push(OutlineItem { line, offset: range.start, label, kind });
    }
    out
}

pub fn position_of(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let column = before.rsplit('\n').next().unwrap_or("").chars().count();
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "(define a 1)\n\n(+ a\n   2) ; sum\n\"s;(\" #| (x) |# [1 2]";

    #[test]
    fn finds_top_level_form_ranges() {
        let spans = top_level_forms(SRC);
        let texts: Vec<&str> = spans.iter().map(|r| &SRC[r.clone()]).collect();
        assert_eq!(texts, ["(define a 1)", "(+ a\n   2)", "\"s;(\"", "[1 2]"]);
        assert_eq!(spans, [0..12, 14..24, 31..36, 47..52]);
    }

    #[test]
    fn atoms_and_prefixed_literals_are_forms_too() {
        let src = "x :k 42 #[1 2] #{3} 'y";
        let texts: Vec<&str> = top_level_forms(src).iter().map(|r| &src[r.clone()]).collect();
        assert_eq!(texts, ["x", ":k", "42", "#[1 2]", "#{3}", "'y"]);
    }

    #[test]
    fn unclosed_forms_run_to_the_end() {
        assert_eq!(top_level_forms("(define (f x)\n  (+ x"), vec![0..20; 1]);
        assert_eq!(top_level_forms("   \n; only a comment\n"), []);
        assert_eq!(top_level_forms("\"open string"), vec![0..12; 1]);
    }

    #[test]
    fn comments_inside_forms_do_not_split_them() {
        let src = "(+ a ; one\n   #| two |# b)\n'(1 2)";
        let texts: Vec<&str> = top_level_forms(src).iter().map(|r| &src[r.clone()]).collect();
        assert_eq!(texts, ["(+ a ; one\n   #| two |# b)", "'(1 2)"]);
    }

    #[test]
    fn form_at_prefers_the_form_under_the_cursor_then_the_next_one() {
        assert_eq!(form_at(SRC, 3), Some(0..12));
        assert_eq!(form_at(SRC, 12), Some(0..12));
        assert_eq!(form_at(SRC, 13), Some(14..24));
        assert_eq!(form_at(SRC, 40), Some(47..52));
        assert_eq!(form_at(SRC, 52), Some(47..52));
        assert_eq!(form_at("(a)  ", 5), Some(0..3));
        assert_eq!(form_at("", 0), None);
    }

    #[test]
    fn next_form_starts_after_an_offset() {
        assert_eq!(next_form(SRC, 12), Some(14..24));
        assert_eq!(next_form(SRC, 14), Some(14..24));
        assert_eq!(next_form("(a)", 3), None);
    }

    #[test]
    fn matching_bracket_pairs_the_bracket_at_or_before_the_cursor() {
        let src = "(a [b] \"(\" ; )\n c)";
        let end = src.len() - 1;
        assert_eq!(matching_bracket(src, 0), Some((0, end)));
        assert_eq!(matching_bracket(src, src.len()), Some((0, end)));
        assert_eq!(matching_bracket(src, 3), Some((3, 5)));
        assert_eq!(matching_bracket(src, 6), Some((3, 5)));
        assert_eq!(matching_bracket(src, 8), None);
        assert_eq!(matching_bracket(src, 1), None);
        assert_eq!(matching_bracket("#[1 2]", 1), Some((1, 5)));
        assert_eq!(matching_bracket("(a", 0), None);
        assert_eq!(matching_bracket("", 0), None);
    }

    #[test]
    fn line_offsets_count_bytes_within_the_line() {
        let text = "ab\ncdé\nf";
        assert_eq!(line_offset(text, 0), (0, 0));
        assert_eq!(line_offset(text, 7), (1, 4));
        assert_eq!(line_offset(text, 8), (2, 0));
        assert_eq!(line_offset(text, 99), (2, 1));
    }

    #[test]
    fn the_symbol_at_an_offset_surrounds_or_precedes_it() {
        assert_eq!(symbol_at("(bar-chart t)", 3), Some("bar-chart"));
        assert_eq!(symbol_at("(bar-chart t)", 10), Some("bar-chart"));
        assert_eq!(symbol_at("(bar-chart t)", 11), Some("t"));
        assert_eq!(symbol_at("(bar-chart t)", 0), None);
        assert_eq!(symbol_at("(inc x)", 4), Some("inc"));
        assert_eq!(symbol_at("'é x", 3), Some("é"));
        assert_eq!(symbol_at("'é x", 2), None);
        assert_eq!(symbol_at("", 0), None);
        assert_eq!(symbol_at("ab", 9), Some("ab"));
    }

    #[test]
    fn indentation_follows_the_innermost_open_form() {
        assert_eq!(indent_for("x", 1), 0);
        assert_eq!(indent_for("(define (f x)", 13), 2);
        assert_eq!(indent_for("(when c\n", 8), 2);
        assert_eq!(indent_for("(+ 1", 4), 3);
        assert_eq!(indent_for("(foo", 4), 1);
        assert_eq!(indent_for("(foo ; (\n", 9), 1);
        assert_eq!(indent_for("[1 2", 4), 1);
        assert_eq!(indent_for("{:a 1", 5), 1);
        assert_eq!(indent_for("(let ((x 1))\n  (bar", 19), 3);
        assert_eq!(indent_for("(f (g 1)\n", 9), 3);
        assert_eq!(indent_for("(define x \"(\"\n", 14), 2);
        assert_eq!(indent_for("(f a\n   b)", 4), 3);
        assert_eq!(indent_for("((make-adder 1)\n", 16), 1);
        assert_eq!(indent_for("(ns app\n", 8), 2);
        assert_eq!(indent_for("(f \"é\" 1\n", 10), 3);
    }

    #[test]
    fn reindenting_rewrites_leading_whitespace_only() {
        let messy = "(define (f x)\n(let ((y 1))\n      (+ x\ny)))\n\n   \n(define s \"a\n   b\")\n  x";
        let tidy = "(define (f x)\n  (let ((y 1))\n    (+ x\n       y)))\n\n\n(define s \"a\n   b\")\nx";
        assert_eq!(reindent(messy, None), tidy);
        assert_eq!(reindent(messy, Some(2..3)), "(define (f x)\n(let ((y 1))\n  (+ x\ny)))\n\n   \n(define s \"a\n   b\")\n  x");
        assert_eq!(reindent("#| a\n   b |#\n  (f)", None), "#| a\n   b |#\n(f)");
    }

    #[test]
    fn comment_toggling_adds_and_removes_semicolons() {
        assert_eq!(toggle_comment("(a)\n  (b)\n\n(c)", 0, 1), ";; (a)\n  ;; (b)\n\n(c)");
        assert_eq!(toggle_comment(";; (a)\n  ;; (b)\n\n(c)", 0, 2), "(a)\n  (b)\n\n(c)");
        assert_eq!(toggle_comment("; x\n;y", 0, 1), "x\ny");
        assert_eq!(toggle_comment("  (a)\n(b)", 0, 1), "  ;; (a)\n;; (b)");
        assert_eq!(toggle_comment("", 0, 0), "");
        assert_eq!(toggle_comment("(a)", 5, 9), ";; (a)");
    }

    #[test]
    fn expanding_the_selection_climbs_the_forms() {
        let src = "(a (b c) '(d))";
        assert_eq!(enclosing(src, 4, 4), Some(4..5));
        assert_eq!(enclosing(src, 4, 5), Some(3..8));
        assert_eq!(enclosing(src, 3, 8), Some(0..14));
        assert_eq!(enclosing(src, 0, 14), None);
        assert_eq!(enclosing(src, 11, 11), Some(11..12));
        assert_eq!(enclosing(src, 11, 12), Some(9..13));
        assert_eq!(enclosing(src, 9, 13), Some(0..14));
        assert_eq!(enclosing("x y", 1, 1), Some(0..1));
        assert_eq!(enclosing("x #[1 2]", 4, 5), Some(2..8));
        assert_eq!(enclosing("", 0, 0), None);
    }

    #[test]
    fn outlines_list_the_top_level_definitions() {
        let src = "(ns app)\n(define x 1)\n(define (f a) a)\n(define-syntax m (syntax-rules () ((_) 1)))\n(define [p q] [1 2])\n(f 1)\n(define double (lambda (x) x))\n(define";
        let found = outline(src);
        let items: Vec<(usize, usize, &str, OutlineKind)> = found.iter().map(|i| (i.line, i.offset, i.label.as_str(), i.kind)).collect();
        assert_eq!(
            items,
            [
                (0, 0, "app", OutlineKind::Namespace),
                (1, 9, "x", OutlineKind::Value),
                (2, 22, "f", OutlineKind::Function),
                (3, 39, "m", OutlineKind::Macro),
                (4, 83, "p q", OutlineKind::Value),
                (6, 110, "double", OutlineKind::Function),
            ]
        );
        assert!(outline("").is_empty());
    }

    #[test]
    fn finding_ignores_case_unless_asked() {
        assert_eq!(find_all("aAa", "a", false), [(0, 1), (1, 2), (2, 3)]);
        assert_eq!(find_all("aAa", "a", true), [(0, 1), (2, 3)]);
        assert_eq!(find_all("ÉcolE école", "école", false), [(0, 6), (7, 13)]);
        assert_eq!(find_all("abc", "", true), []);
        assert_eq!(find_all("aaa", "aa", true), [(0, 2)]);
        assert_eq!(find_all("aaa", "aa", false), [(0, 2)]);
    }

    #[test]
    fn the_context_knows_strings_comments_and_neighbours() {
        assert_eq!(context_at("(a)", 1), Context { before: Some('('), after: Some('a'), in_string: false, in_comment: false });
        assert_eq!(context_at("\"ab", 2), Context { before: Some('a'), after: Some('b'), in_string: true, in_comment: false });
        assert_eq!(context_at("\"a\\\"b\" c", 7), Context { before: Some(' '), after: Some('c'), in_string: false, in_comment: false });
        assert!(context_at("x ; note", 5).in_comment);
        assert!(!context_at("x ; note\ny", 9).in_comment);
        assert!(context_at("#| a", 4).in_comment);
        assert_eq!(context_at("", 0), Context::default());
        assert_eq!(context_at("é", 2).before, Some('é'));
    }

    #[test]
    fn positions_convert_to_byte_offsets_and_back() {
        let text = "ab\ncdé\nf";
        assert_eq!(offset_of(text, 1, 2), 5);
        assert_eq!(offset_of(text, 1, 3), 7);
        assert_eq!(offset_of(text, 2, 0), 8);
        assert_eq!(offset_of(text, 9, 9), text.len());
        assert_eq!(position_of(text, 7), (1, 3));
        assert_eq!(position_of(text, 8), (2, 0));
        assert_eq!(position_of(text, 0), (0, 0));
    }
}
