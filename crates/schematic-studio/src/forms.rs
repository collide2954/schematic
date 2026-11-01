use std::ops::Range;

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

pub fn symbol_at(text: &str, offset: usize) -> Option<&str> {
    let offset = offset.min(text.len());
    if !text.is_char_boundary(offset) {
        return None;
    }
    let delimiter = |c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';' | ',' | '\'' | '`' | '~' | '@');
    let start = text[..offset].rfind(delimiter).map_or(0, |i| i + text[i..].chars().next().map_or(1, char::len_utf8));
    let end = text[offset..].find(delimiter).map_or(text.len(), |i| offset + i);
    (start < end).then(|| &text[start..end])
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
