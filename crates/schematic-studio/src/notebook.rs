use std::path::{Path, PathBuf};

use schematic::Value;
use schematic::docs::{self, Block, Inline};
use schematic::plot::Plot;
use schematic::reader::read_str;

pub const MARKER: &str = ";; %%";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Code,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellText {
    pub kind: Kind,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    Printed(String),
    Result(String),
    Error(String),
    Chart(Box<Plot>),
    Picture { description: String, svg: String },
}

impl Output {
    pub fn description(&self) -> String {
        match self {
            Output::Chart(plot) => plot.describe(),
            Output::Picture { description, .. } => description.clone(),
            Output::Printed(text) | Output::Result(text) | Output::Error(text) => text.clone(),
        }
    }

    fn svg(&self) -> Option<String> {
        match self {
            Output::Chart(plot) => Some(plot.svg()),
            Output::Picture { svg, .. } => Some(svg.clone()),
            _ => None,
        }
    }
}

fn marker_kind(line: &str) -> Option<Kind> {
    let rest = line.strip_prefix(MARKER)?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(match rest.trim() {
        "text" | "md" | "markdown" => Kind::Text,
        _ => Kind::Code,
    })
}

pub fn is_notebook(text: &str) -> bool {
    text.lines().any(|line| marker_kind(line.trim_end()).is_some())
}

fn trimmed(lines: &[String]) -> String {
    let start = lines.iter().position(|l| !l.trim().is_empty());
    let end = lines.iter().rposition(|l| !l.trim().is_empty());
    match (start, end) {
        (Some(start), Some(end)) => lines[start..=end].join("\n"),
        _ => String::new(),
    }
}

fn uncomment(line: &str) -> String {
    line.strip_prefix(";; ").or_else(|| line.strip_prefix(";;")).unwrap_or(line).to_string()
}

fn flush(kind: Option<Kind>, body: &mut Vec<String>, cells: &mut Vec<CellText>) {
    let source = trimmed(body);
    body.clear();
    match kind {
        None if source.is_empty() => {}
        None => cells.push(CellText { kind: Kind::Code, source }),
        Some(kind) => cells.push(CellText { kind, source }),
    }
}

pub fn parse(text: &str) -> Vec<CellText> {
    let mut cells = Vec::new();
    let mut kind = None;
    let mut body = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        match marker_kind(line) {
            Some(next) => {
                flush(kind, &mut body, &mut cells);
                kind = Some(next);
            }
            None => body.push(if kind == Some(Kind::Text) { uncomment(line) } else { line.to_string() }),
        }
    }
    flush(kind, &mut body, &mut cells);
    cells
}

pub fn render(cells: &[CellText]) -> String {
    let mut out = String::new();
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(MARKER);
        match cell.kind {
            Kind::Code => {
                out.push('\n');
                if !cell.source.is_empty() {
                    out.push_str(&cell.source);
                    out.push('\n');
                }
            }
            Kind::Text => {
                out.push_str(" text\n");
                for line in cell.source.lines() {
                    if line.is_empty() {
                        out.push_str(";;\n");
                    } else {
                        out.push_str(";; ");
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            }
        }
    }
    out
}

pub fn fresh() -> Vec<CellText> {
    vec![CellText { kind: Kind::Text, source: "# Untitled".to_string() }, CellText { kind: Kind::Code, source: String::new() }]
}

pub fn sidecar(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".out");
    path.with_file_name(name)
}

fn keyword(name: &str) -> Value {
    Value::keyword(name)
}

pub fn render_outputs(outputs: &[Vec<Output>]) -> String {
    let mut out = String::from("[");
    for (i, cell) in outputs.iter().enumerate() {
        if i > 0 {
            out.push_str("\n ");
        }
        out.push('[');
        for (j, output) in cell.iter().enumerate() {
            if j > 0 {
                out.push_str("\n  ");
            }
            let kind = match output {
                Output::Printed(_) => "output",
                Output::Result(_) => "result",
                Output::Error(_) => "error",
                Output::Chart(_) | Output::Picture { .. } => "chart",
            };
            let mut pairs = vec![(keyword("kind"), keyword(kind)), (keyword("text"), Value::string(&output.description()))];
            if let Some(svg) = output.svg() {
                pairs.push((keyword("svg"), Value::string(&svg)));
            }
            out.push_str(&Value::map(pairs).to_string());
        }
        out.push(']');
    }
    out.push_str("]\n");
    out
}

pub fn parse_outputs(text: &str) -> Vec<Vec<Output>> {
    let Ok(Value::Vector(cells)) = read_str(text) else { return Vec::new() };
    cells
        .iter()
        .map(|cell| {
            let Value::Vector(items) = cell else { return Vec::new() };
            items
                .iter()
                .filter_map(|item| {
                    let Value::Map(map) = item else { return None };
                    let string = |key: &str| match map.get(&keyword(key)) {
                        Some(Value::Str(s)) => s.to_string(),
                        _ => String::new(),
                    };
                    let Some(Value::Keyword(kind)) = map.get(&keyword("kind")) else { return None };
                    let text = string("text");
                    Some(match &*kind.name() {
                        "output" => Output::Printed(text),
                        "result" => Output::Result(text),
                        "error" => Output::Error(text),
                        "chart" => Output::Picture { description: text, svg: string("svg") },
                        _ => return None,
                    })
                })
                .collect()
        })
        .collect()
}

pub fn to_markdown(cells: &[CellText], outputs: &[Vec<Output>]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (i, cell) in cells.iter().enumerate() {
        match cell.kind {
            Kind::Text => parts.push(cell.source.clone()),
            Kind::Code => {
                parts.push(format!("```scheme\n{}\n```", cell.source));
                for output in outputs.get(i).into_iter().flatten() {
                    parts.push(match output {
                        Output::Printed(text) | Output::Result(text) => format!("```\n{}\n```", text.trim_end()),
                        Output::Error(text) => format!("```\nerror: {text}\n```"),
                        Output::Chart(_) | Output::Picture { .. } => format!("*{}*", output.description()),
                    });
                }
            }
        }
    }
    let mut out = parts.join("\n\n");
    out.push('\n');
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn inline_html(text: &str) -> String {
    docs::inlines(text)
        .into_iter()
        .map(|inline| match inline {
            Inline::Text(t) => escape(&t),
            Inline::Code(t) => format!("<code>{}</code>", escape(&t)),
            Inline::Bold(t) => format!("<strong>{}</strong>", escape(&t)),
            Inline::Italic(t) => format!("<em>{}</em>", escape(&t)),
            Inline::Link(label, url) => format!("<a href=\"{}\">{}</a>", escape(&url), escape(&label)),
        })
        .collect()
}

fn block_html(block: Block) -> String {
    match block {
        Block::Heading(level, title) => format!("<h{level}>{}</h{level}>", inline_html(&title)),
        Block::Paragraph(text) => format!("<p>{}</p>", inline_html(&text)),
        Block::Code(source) => format!("<pre class=\"code\">{}</pre>", escape(&source)),
        Block::Quote(text) => format!("<blockquote><p>{}</p></blockquote>", inline_html(&text)),
        Block::List(items) => format!("<ul>{}</ul>", items.iter().map(|item| format!("<li>{}</li>", inline_html(item))).collect::<String>()),
        Block::Table(rows) => {
            let mut out = String::from("<table>");
            for (i, row) in rows.iter().enumerate() {
                let tag = if i == 0 { "th" } else { "td" };
                out.push_str("<tr>");
                for cell in row {
                    out.push_str(&format!("<{tag}>{}</{tag}>", inline_html(cell)));
                }
                out.push_str("</tr>");
            }
            out.push_str("</table>");
            out
        }
    }
}

const STYLE: &str = "body{margin:0;background:#fff;color:#1a2230;font:15px/1.5 system-ui,sans-serif}\
main{max-width:52rem;margin:0 auto;padding:2rem 1.25rem}\
.cell{margin:0 0 1.25rem}.cell>*+*{margin-top:.4rem}\
pre{margin:0;padding:.6rem .8rem;border-radius:6px;font:13px/1.45 ui-monospace,Menlo,Consolas,monospace;white-space:pre-wrap}\
.code{background:#f3f5f8}.result{background:#f8fafb;border-left:3px solid #0f7b86}.output{color:#444c5c}.error{color:#b42318;background:#fdf2f0}\
figure{margin:.5rem 0}svg{max-width:100%;height:auto}\
h1,h2,h3{line-height:1.25;margin:1.2em 0 .4em}p{margin:.4em 0}\
code{font:.92em ui-monospace,Menlo,Consolas,monospace;background:#f3f5f8;padding:.1em .3em;border-radius:4px}pre code{background:none;padding:0}\
blockquote{margin:0;padding:.2rem 1rem;border-left:3px solid #d8dde6;color:#555}\
table{border-collapse:collapse}td,th{border:1px solid #d8dde6;padding:.25rem .6rem;text-align:left}";

pub fn to_html(title: &str, cells: &[CellText], outputs: &[Vec<Output>]) -> String {
    let mut out = format!("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main>\n", escape(title));
    for (i, cell) in cells.iter().enumerate() {
        out.push_str("<section class=\"cell\">\n");
        match cell.kind {
            Kind::Text => {
                for block in docs::blocks(&cell.source) {
                    out.push_str(&block_html(block));
                    out.push('\n');
                }
            }
            Kind::Code => {
                out.push_str(&format!("<pre class=\"code\">{}</pre>\n", escape(&cell.source)));
                for output in outputs.get(i).into_iter().flatten() {
                    out.push_str(&match output {
                        Output::Printed(text) => format!("<pre class=\"output\">{}</pre>\n", escape(text.trim_end())),
                        Output::Result(text) => format!("<pre class=\"result\">{}</pre>\n", escape(text)),
                        Output::Error(text) => format!("<pre class=\"error\">{}</pre>\n", escape(text)),
                        Output::Chart(_) | Output::Picture { .. } => format!("<figure>{}</figure>\n", output.svg().unwrap_or_default()),
                    });
                }
            }
        }
        out.push_str("</section>\n");
    }
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart() -> Output {
        let value = schematic::Interpreter::new().eval_str("(bar-chart [\"a\" \"b\"] [1 2] {:title \"T\"})").unwrap();
        let Value::Plot(plot) = value else { panic!("{value}") };
        Output::Chart(Box::new((*plot).clone()))
    }

    #[test]
    fn cells_round_trip_through_marked_source() {
        let source = "(define x 1)\n\n;; %% text\n;; # Sales\n;;\n;; Totals by *city*.\n;; %%\n(define t (table {:a [1 2]}))\n\n(count t)\n\n;; %% md\n;; Done.\n;; %%\n";
        assert!(is_notebook(source) && !is_notebook("(define x 1)\n; %% not a marker\n;; %%% nope\n"));
        let cells = parse(source);
        let shown: Vec<(Kind, &str)> = cells.iter().map(|c| (c.kind, c.source.as_str())).collect();
        assert_eq!(
            shown,
            [
                (Kind::Code, "(define x 1)"),
                (Kind::Text, "# Sales\n\nTotals by *city*."),
                (Kind::Code, "(define t (table {:a [1 2]}))\n\n(count t)"),
                (Kind::Text, "Done."),
                (Kind::Code, ""),
            ]
        );
        let text = render(&cells);
        assert!(text.starts_with(";; %%\n(define x 1)\n\n;; %% text\n;; # Sales\n;;\n;; Totals by *city*.\n\n;; %%\n"), "{text}");
        assert_eq!(parse(&text), cells);
        assert_eq!(parse(""), Vec::new());
        assert_eq!(fresh().len(), 2);
        assert_eq!(render(&fresh()), ";; %% text\n;; # Untitled\n\n;; %%\n");
        assert_eq!(sidecar(Path::new("notes/report.scm")), PathBuf::from("notes/report.scm.out"));
    }

    #[test]
    fn outputs_round_trip_through_the_sidecar() {
        let outputs = vec![
            vec![Output::Printed("hello\n".into()), Output::Result("3".into()), Output::Error("boom: \"x\"".into())],
            Vec::new(),
            vec![chart(), Output::Result("line one\nline two".into())],
        ];
        let text = render_outputs(&outputs);
        assert!(text.starts_with("[[{:kind :output, :text \"hello\\n\"}\n  {:kind :result, :text \"3\"}"), "{text}");
        let back = parse_outputs(&text);
        assert_eq!(back[0], outputs[0]);
        assert!(back[1].is_empty());
        match &back[2][0] {
            Output::Picture { description, svg } => {
                assert_eq!(description, "bar chart with 2 bars");
                assert!(svg.starts_with("<svg"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(back[2][1], outputs[2][1]);
        assert_eq!(render_outputs(&back), text, "pictures render the same as live charts");
        assert!(parse_outputs("nonsense").is_empty());
    }

    #[test]
    fn notebooks_export_to_markdown_and_html() {
        let cells = vec![
            CellText { kind: Kind::Text, source: "# Report\n\nSee `count` & *more*.".into() },
            CellText { kind: Kind::Code, source: "(+ 1 2)".into() },
            CellText { kind: Kind::Code, source: "(bar-chart [\"a\"] [1])".into() },
        ];
        let outputs = vec![Vec::new(), vec![Output::Printed("hi\n".into()), Output::Result("3".into())], vec![chart(), Output::Error("<bad>".into())]];
        let markdown = to_markdown(&cells, &outputs);
        assert_eq!(markdown, "# Report\n\nSee `count` & *more*.\n\n```scheme\n(+ 1 2)\n```\n\n```\nhi\n```\n\n```\n3\n```\n\n```scheme\n(bar-chart [\"a\"] [1])\n```\n\n*bar chart with 2 bars*\n\n```\nerror: <bad>\n```\n");
        let html = to_html("Report", &cells, &outputs);
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<h1>Report</h1>") && html.contains("<p>See <code>count</code> &amp; <em>more</em>.</p>"), "{html}");
        assert!(html.contains("<pre class=\"code\">(+ 1 2)</pre>\n<pre class=\"output\">hi</pre>\n<pre class=\"result\">3</pre>"), "{html}");
        assert!(html.contains("<figure><svg") && html.contains("<pre class=\"error\">&lt;bad&gt;</pre>"), "{html}");
    }
}
