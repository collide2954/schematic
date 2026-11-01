use std::sync::OnceLock;

use schematic::docs;

use crate::highlight::{self, Kind};
use crate::worker::Binding;

pub const LIMIT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateKind {
    Binding,
    Keyword,
    Local,
    Special,
    Builtin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub name: String,
    pub kind: CandidateKind,
    pub detail: String,
}

pub fn is_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';' | ',' | '\'' | '`' | '~' | '@')
}

pub fn word_before(text: &str, offset: usize) -> &str {
    let offset = offset.min(text.len());
    if !text.is_char_boundary(offset) {
        return "";
    }
    let before = &text[..offset];
    let start = before.rfind(is_delimiter).map_or(0, |i| i + before[i..].chars().next().map_or(1, char::len_utf8));
    &before[start..]
}

pub fn builtins() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| schematic::Interpreter::new().global_names())
}

pub fn locals(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (range, kind) in highlight::segments(text) {
        if !matches!(kind, None | Some(Kind::Keyword)) {
            continue;
        }
        for word in text[range].split(is_delimiter).filter(|w| !w.is_empty()) {
            if word.starts_with(|c: char| c.is_ascii_digit() || c == '#') || out.iter().any(|o| o == word) {
                continue;
            }
            out.push(word.to_string());
        }
    }
    out
}

pub fn signature_of(name: &str) -> String {
    let Some(entry) = docs::entry(name) else { return String::new() };
    let mut signatures = entry.uses.iter().flat_map(|u| u.signatures.iter());
    let own = signatures.clone().find(|s| {
        let s = s.trim_start_matches('(');
        s == name || s.starts_with(&format!("{name} ")) || s.starts_with(&format!("{name})"))
    });
    own.or_else(|| signatures.next()).cloned().unwrap_or_default()
}

fn clip(detail: String) -> String {
    if detail.chars().count() <= 44 {
        return detail;
    }
    let short: String = detail.chars().take(43).collect();
    format!("{}…", short.trim_end())
}

pub fn candidates(prefix: &str, bindings: &[Binding], locals: &[String]) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    let mut offer = |name: &str, kind: CandidateKind, detail: String| {
        if name.starts_with(prefix) && name != prefix && !out.iter().any(|c| c.name == name) {
            out.push(Candidate { name: name.to_string(), kind, detail: clip(detail) });
        }
    };
    for binding in bindings {
        let detail = if binding.summary.is_empty() { binding.kind.clone() } else { format!("{}, {}", binding.kind, binding.summary) };
        offer(&binding.name, CandidateKind::Binding, detail);
    }
    if prefix.starts_with(':') {
        for binding in bindings {
            for column in &binding.columns {
                offer(&format!(":{column}"), CandidateKind::Keyword, format!("column of {}", binding.name));
            }
        }
    }
    for local in locals {
        offer(local, CandidateKind::Local, "in this file".to_string());
    }
    if !prefix.is_empty() && !prefix.starts_with(':') {
        for special in highlight::SPECIALS {
            offer(special, CandidateKind::Special, "special form".to_string());
        }
        for name in builtins() {
            offer(name, CandidateKind::Builtin, signature_of(name));
        }
    }
    out.sort_by(|a, b| a.name.chars().count().cmp(&b.name.chars().count()).then_with(|| a.name.cmp(&b.name)));
    out.truncate(LIMIT);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_before_the_cursor_stop_at_delimiters() {
        assert_eq!(word_before("(ba", 3), "ba");
        assert_eq!(word_before("x :ci", 5), ":ci");
        assert_eq!(word_before("(x ", 3), "");
        assert_eq!(word_before("a\nbc", 4), "bc");
        assert_eq!(word_before("é", 1), "");
        assert_eq!(word_before("ab", 9), "ab");
    }

    #[test]
    fn candidates_rank_short_prefix_matches_and_know_where_they_come_from() {
        let t = Binding { name: "t".into(), kind: "table".into(), summary: "2 rows".into(), columns: vec!["city".into(), "count".into()] };
        let bars = Binding { name: "bars".into(), kind: "plot".into(), summary: String::new(), columns: Vec::new() };
        let locals = locals("(define banner 1) ; bake\n\"bagel\" :batch bar-chart #[1]");
        assert_eq!(locals, ["banner", ":batch", "bar-chart"]);
        let found = candidates("ba", &[t.clone(), bars.clone()], &locals);
        let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names[..2], ["bars", "banner"]);
        assert!(names.contains(&"bar-chart"), "{names:?}");
        assert_eq!((found[0].kind, found[0].detail.as_str()), (CandidateKind::Binding, "plot"));
        assert_eq!(found.iter().find(|c| c.name == "bar-chart").map(|c| c.kind), Some(CandidateKind::Local));
        assert!(found.len() <= LIMIT);
        let keywords = candidates(":c", std::slice::from_ref(&t), &locals);
        assert_eq!(keywords.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), [":city", ":count"]);
        assert_eq!((keywords[0].kind, keywords[0].detail.as_str()), (CandidateKind::Keyword, "column of t"));
        let broad = candidates("", &[t, bars], &locals);
        assert_eq!(broad.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["t", "bars", ":batch", "banner", "bar-chart"]);
        assert!(candidates("", &[], &[]).is_empty());
        assert!(candidates("zzz", &[], &[]).is_empty());
        let inc = candidates("in", &[], &[]);
        assert!(inc.iter().any(|c| c.name == "inc" && c.kind == CandidateKind::Builtin && c.detail == "(inc x)"), "{inc:?}");
        assert_eq!(signature_of("weekday"), "(weekday d)");
        assert_eq!(signature_of("let"), "(let ((n v) ...) body...)");
        assert_eq!(signature_of("today"), "(today)");
        assert_eq!(signature_of("nope"), "");
        assert_eq!(clip("x".repeat(50)), format!("{}…", "x".repeat(43)));
        let define = candidates("def", &[], &[]);
        assert!(define.iter().any(|c| c.name == "define" && c.kind == CandidateKind::Special));
        assert!(define.iter().all(|c| c.name != "def"));
    }
}
