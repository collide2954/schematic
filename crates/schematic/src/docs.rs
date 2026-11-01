use std::collections::BTreeSet;
use std::sync::OnceLock;

pub const GUIDE: &str = include_str!("../../../docs/guide.md");
pub const REFERENCE: &str = include_str!("../../../docs/reference.md");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading(u8, String),
    Paragraph(String),
    Code(String),
    Quote(String),
    List(Vec<String>),
    Table(Vec<Vec<String>>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    Bold(String),
    Italic(String),
    Link(String, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    pub section: String,
    pub signatures: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub uses: Vec<Use>,
}

fn cells(line: &str) -> Vec<String> {
    let line = line.trim().trim_start_matches('|').trim_end_matches('|');
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cell.push('|');
                chars.next();
            }
            '|' => cells.push(std::mem::take(&mut cell).trim().to_string()),
            _ => cell.push(c),
        }
    }
    cells.push(cell.trim().to_string());
    cells
}

fn is_separator(row: &[String]) -> bool {
    row.iter().all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':'))
}

fn list_item(line: &str) -> Option<&str> {
    if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        return Some(item);
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0).then(|| line[digits..].strip_prefix(". ")).flatten()
}

fn flush(paragraph: &mut Vec<&str>, out: &mut Vec<Block>) {
    if !paragraph.is_empty() {
        out.push(Block::Paragraph(paragraph.join(" ")));
        paragraph.clear();
    }
}

pub fn blocks(markdown: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut lines = markdown.lines().map(str::trim_end).peekable();
    while let Some(line) = lines.next() {
        if line.starts_with("```") {
            flush(&mut paragraph, &mut out);
            let mut code = Vec::new();
            for line in lines.by_ref() {
                if line.starts_with("```") {
                    break;
                }
                code.push(line);
            }
            out.push(Block::Code(code.join("\n")));
        } else if let Some(rest) = line.strip_prefix('#') {
            flush(&mut paragraph, &mut out);
            let level = 1 + rest.chars().take_while(|c| *c == '#').count();
            out.push(Block::Heading(level as u8, rest.trim_start_matches('#').trim().to_string()));
        } else if line.starts_with('|') {
            flush(&mut paragraph, &mut out);
            let mut rows = vec![cells(line)];
            while lines.peek().is_some_and(|next| next.starts_with('|')) {
                rows.push(cells(lines.next().unwrap_or_default()));
            }
            rows.retain(|row| !is_separator(row));
            out.push(Block::Table(rows));
        } else if let Some(item) = list_item(line) {
            flush(&mut paragraph, &mut out);
            let mut items = vec![item.to_string()];
            while let Some(&next) = lines.peek() {
                if let Some(item) = list_item(next) {
                    items.push(item.to_string());
                } else if next.starts_with("  ") && !next.trim().is_empty() {
                    let last = items.last_mut().expect("an item");
                    last.push(' ');
                    last.push_str(next.trim());
                } else {
                    break;
                }
                lines.next();
            }
            out.push(Block::List(items));
        } else if let Some(quote) = line.strip_prefix('>') {
            flush(&mut paragraph, &mut out);
            let mut text = vec![quote.trim()];
            while let Some(more) = lines.peek().and_then(|next| next.strip_prefix('>')) {
                text.push(more.trim());
                lines.next();
            }
            out.push(Block::Quote(text.join(" ")));
        } else if line.trim().is_empty() {
            flush(&mut paragraph, &mut out);
        } else {
            paragraph.push(line.trim());
        }
    }
    flush(&mut paragraph, &mut out);
    out
}

pub fn inlines(text: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    let take = |out: &mut Vec<Inline>, plain: &mut String, inline: Inline| {
        if !plain.is_empty() {
            out.push(Inline::Text(std::mem::take(plain)));
        }
        out.push(inline);
    };
    while !rest.is_empty() {
        if rest.starts_with('`') {
            let ticks = rest.chars().take_while(|c| *c == '`').count();
            let fence = &rest[..ticks];
            if let Some(end) = rest[ticks..].find(fence) {
                let mut inner = &rest[ticks..ticks + end];
                if ticks > 1 {
                    inner = inner.strip_prefix(' ').unwrap_or(inner);
                    inner = inner.strip_suffix(' ').unwrap_or(inner);
                }
                take(&mut out, &mut plain, Inline::Code(inner.to_string()));
                rest = &rest[ticks + end + ticks..];
                continue;
            }
        } else if let Some(after) = rest.strip_prefix("**") {
            if let Some(end) = after.find("**") {
                take(&mut out, &mut plain, Inline::Bold(after[..end].to_string()));
                rest = &after[end + 2..];
                continue;
            }
        } else if rest.starts_with('[') {
            if let Some((close, end)) = rest.find("](").and_then(|close| rest[close..].find(')').map(|end| (close, close + end))) {
                take(&mut out, &mut plain, Inline::Link(rest[1..close].to_string(), rest[close + 2..end].to_string()));
                rest = &rest[end + 1..];
                continue;
            }
        } else if let Some(after) = rest.strip_prefix('*').filter(|after| !after.starts_with(' '))
            && let Some(end) = after.find('*') {
                take(&mut out, &mut plain, Inline::Italic(after[..end].to_string()));
                rest = &after[end + 1..];
                continue;
            }
        let c = rest.chars().next().expect("a character");
        plain.push(c);
        rest = &rest[c.len_utf8()..];
    }
    if !plain.is_empty() {
        out.push(Inline::Text(plain));
    }
    out
}

pub fn plain(markdown: &str) -> String {
    inlines(markdown)
        .into_iter()
        .map(|inline| match inline {
            Inline::Text(t) | Inline::Code(t) | Inline::Bold(t) | Inline::Italic(t) | Inline::Link(t, _) => t,
        })
        .collect()
}

fn head(signature: &str) -> Option<String> {
    let signature = signature.trim();
    let inner = signature.strip_prefix('(').unwrap_or(signature);
    let head: String = inner.chars().take_while(|c| !c.is_whitespace() && *c != ')').collect();
    let mut chars = head.chars();
    let first = chars.next()?;
    let second = chars.next();
    let symbolic = first.is_alphabetic() || "+-*/<>=!?_".contains(first);
    let numeric = matches!(first, '+' | '-') && second.is_some_and(|c| c.is_ascii_digit() || c == 'i' || c == 'n');
    let placeholder = head.chars().count() == 1 && first.is_alphabetic();
    (symbolic && !numeric && !placeholder).then_some(head)
}

fn index(markdown: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut section = String::new();
    for block in blocks(markdown) {
        match block {
            Block::Heading(2, title) => section = title,
            Block::Table(rows) if section != "Syntax" => {
                for row in rows.iter().skip(1) {
                    let [forms, description, ..] = row.as_slice() else { continue };
                    let signatures: Vec<String> =
                        inlines(forms).into_iter().filter_map(|i| if let Inline::Code(c) = i { Some(c) } else { None }).collect();
                    let names: BTreeSet<String> = signatures.iter().filter_map(|s| head(s)).collect();
                    for name in names {
                        let found = Use { section: section.clone(), signatures: signatures.clone(), description: plain(description) };
                        match entries.iter_mut().find(|e| e.name == name) {
                            Some(entry) => entry.uses.push(found),
                            None => entries.push(Entry { name, uses: vec![found] }),
                        }
                    }
                }
            }
            _ => {}
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

pub fn entries() -> &'static [Entry] {
    static INDEX: OnceLock<Vec<Entry>> = OnceLock::new();
    INDEX.get_or_init(|| index(REFERENCE))
}

pub fn entry(name: &str) -> Option<&'static Entry> {
    entries().iter().find(|e| e.name == name)
}

pub fn search(query: &str) -> Vec<&'static Entry> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return entries().iter().collect();
    }
    let by_name = entries().iter().filter(|e| e.name.to_lowercase().contains(&query));
    let by_text = entries().iter().filter(|e| {
        !e.name.to_lowercase().contains(&query)
            && e.uses.iter().any(|u| u.description.to_lowercase().contains(&query) || u.signatures.iter().any(|s| s.contains(&query)))
    });
    by_name.chain(by_text).collect()
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

pub fn describe(name: &str) -> Option<String> {
    let entry = entry(name)?;
    let mut out = String::new();
    let mut section = "";
    for (i, found) in entry.uses.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if found.section != section {
            out.push_str(&format!("{name} ({})\n", found.section));
            section = &found.section;
        }
        out.push_str(&format!("  {}\n", found.signatures.join(", ")));
        for line in wrap(&found.description, 76) {
            out.push_str(&format!("  {line}\n"));
        }
    }
    Some(out.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_splits_into_blocks() {
        let md = "# Title\n\nOne line\nand two.\n\n| A | B |\n|---|---|\n| `x`, `y` | says \\| pipe |\n\n```scheme\n(+ 1 2)\n```\n\n- first\n- second\n  more\n\n> quoted\n> twice\n";
        assert_eq!(
            blocks(md),
            vec![
                Block::Heading(1, "Title".into()),
                Block::Paragraph("One line and two.".into()),
                Block::Table(vec![vec!["A".into(), "B".into()], vec!["`x`, `y`".into(), "says | pipe".into()]]),
                Block::Code("(+ 1 2)".into()),
                Block::List(vec!["first".into(), "second more".into()]),
                Block::Quote("quoted twice".into()),
            ]
        );
        assert_eq!(blocks("### Deep\n1. one\n2. two"), vec![Block::Heading(3, "Deep".into()), Block::List(vec!["one".into(), "two".into()])]);
    }

    #[test]
    fn inline_markup_splits_into_runs() {
        use Inline::*;
        assert_eq!(
            inlines("a `b` **c** [d](e) ``f `g` `` *h*"),
            vec![
                Text("a ".into()),
                Code("b".into()),
                Text(" ".into()),
                Bold("c".into()),
                Text(" ".into()),
                Link("d".into(), "e".into()),
                Text(" ".into()),
                Code("f `g`".into()),
                Text(" ".into()),
                Italic("h".into()),
            ]
        );
        assert_eq!(inlines("2 * 3 * 4"), vec![Text("2 * 3 * 4".into())]);
        assert_eq!(plain("use `inc` on *it*"), "use inc on it");
    }

    #[test]
    fn the_reference_indexes_every_documented_name() {
        let chart = entry("bar-chart").unwrap();
        assert_eq!(chart.uses.len(), 1);
        assert_eq!(chart.uses[0].section, "Charts");
        assert_eq!(chart.uses[0].signatures, ["(bar-chart data x y [opts])"]);
        let sections: Vec<&str> = entry("where").unwrap().uses.iter().map(|u| u.section.as_str()).collect();
        assert_eq!(sections, ["Arrays", "Tables"]);
        assert_eq!(entry("let").unwrap().uses.len(), 2);
        assert_eq!(entry("define").unwrap().uses.len(), 5);
        assert_eq!(entry("define").unwrap().uses.iter().filter(|u| u.section == "Types").count(), 2);
        assert!(entry("[name").is_none() && entry("int").is_some());
        assert!(entry("+").is_some() && entry("-").is_some() && entry("call/ec").is_some() && entry("string->number").is_some());
        assert!(entry("a").is_none() && entry(":name").is_none() && entry("42").is_none() && entry("+inf.0").is_none());
        let names: Vec<&str> = search("chart").iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names[..4], ["area-chart", "bar-chart", "line-chart", "pie-chart"]);
        assert!(names.contains(&"to-svg"));
        assert_eq!(search("").len(), entries().len());
        let inc = describe("inc").unwrap();
        assert!(inc.starts_with("inc (Numbers)\n  (abs x), (inc x), (dec x)\n  absolute value, plus one, minus one"), "{inc}");
        assert_eq!(describe("nope"), None);
        let format = describe("format").unwrap();
        assert!(format.lines().all(|l| l.chars().count() <= 80), "{format}");
    }
}
