use schematic::json::Json;

use crate::claude::{Content, Message, Request, Role, Tool, Turn, Usage, MAX_TOKENS};

pub const ROUNDS: usize = 24;
pub const ATTACHMENT_LIMIT: usize = 60_000;
pub const RESULT_LIMIT: usize = 20_000;

pub fn system() -> String {
    format!("{}\n\n# The Schematic reference\n\n{}", include_str!("../prompt.md").trim_end(), schematic::docs::REFERENCE)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub label: String,
    pub text: String,
}

impl Attachment {
    pub fn new(label: impl Into<String>, text: impl Into<String>) -> Attachment {
        Attachment { label: label.into(), text: text.into() }
    }
}

pub fn compose(question: &str, attachments: &[Attachment]) -> String {
    let mut out = String::new();
    for attachment in attachments {
        out.push_str(&format!("[{}]\n", attachment.label));
        if attachment.text.len() > ATTACHMENT_LIMIT {
            let mut end = ATTACHMENT_LIMIT;
            while !attachment.text.is_char_boundary(end) {
                end -= 1;
            }
            out.push_str(&attachment.text[..end]);
            out.push_str("\n[cut here; read-file returns the rest]\n");
        } else {
            out.push_str(&attachment.text);
            if !attachment.text.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push_str("[end]\n\n");
    }
    out.push_str(question);
    out
}

pub fn clip(text: String) -> String {
    if text.len() <= RESULT_LIMIT {
        return text;
    }
    let mut end = RESULT_LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[cut here]", &text[..end])
}

fn schema(fields: &[(&str, &str)]) -> Json {
    Json::object([
        ("type", Json::text("object")),
        (
            "properties",
            Json::Object(
                fields
                    .iter()
                    .map(|(name, description)| (name.to_string(), Json::object([("type", Json::text("string")), ("description", Json::text(*description))])))
                    .collect(),
            ),
        ),
        ("required", Json::Array(fields.iter().map(|(name, _)| Json::text(*name)).collect())),
        ("additionalProperties", Json::Bool(false)),
    ])
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "evaluate",
            description: "Run Schematic code in the person's live session. Returns what it prints, then the value of each form, or the error.",
            schema: schema(&[("code", "Schematic source to run")]),
            eager: true,
        },
        Tool {
            name: "describe",
            description: "Summarise a table or array bound to a name: columns with types, counts, missing values, and ranges.",
            schema: schema(&[("name", "The binding to describe")]),
            eager: false,
        },
        Tool {
            name: "read-file",
            description: "Return the text of a file in the project folder.",
            schema: schema(&[("path", "Path relative to the project folder")]),
            eager: false,
        },
        Tool {
            name: "list-files",
            description: "List the files in the project folder, one relative path per line.",
            schema: schema(&[]),
            eager: false,
        },
        Tool {
            name: "propose-edit",
            description: "Propose a change to the open file. The person sees it as a diff and applies it. `find` must match the file text exactly once; an empty `find` appends to the end.",
            schema: schema(&[("find", "Exact text to replace"), ("replace", "Replacement text"), ("why", "One sentence on what the change does")]),
            eager: true,
        },
    ]
}

#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub id: String,
    pub name: String,
    pub input: Json,
}

impl Call {
    pub fn field(&self, name: &str) -> &str {
        self.input.get(name).and_then(Json::as_str).unwrap_or("")
    }

    pub fn summary(&self) -> String {
        match self.name.as_str() {
            "evaluate" => format!("ran {}", first_line(self.field("code"))),
            "describe" => format!("described {}", self.field("name")),
            "read-file" => format!("read {}", self.field("path")),
            "list-files" => "listed the project files".to_string(),
            "propose-edit" => format!("proposed an edit: {}", self.field("why")),
            other => other.to_string(),
        }
    }
}

fn first_line(code: &str) -> String {
    let mut lines = code.trim().lines();
    let first = lines.next().unwrap_or("").to_string();
    if lines.next().is_some() { format!("{first} …") } else { first }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Reply,
    Calls(Vec<Call>),
    Refused(String),
    Cut,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub model: String,
    pub effort: String,
    pub system: String,
    pub messages: Vec<Message>,
    pub usage: Usage,
    pub rounds: usize,
}

impl Session {
    pub fn new(model: &str, effort: &str, system: String) -> Session {
        Session { model: model.to_string(), effort: effort.to_string(), system, messages: Vec::new(), usage: Usage::default(), rounds: 0 }
    }

    pub fn ask(&mut self, text: String) -> Request {
        self.rounds = 0;
        self.drop_pending();
        self.messages.push(Message { role: Role::User, content: vec![Content::Text(text)] });
        self.request()
    }

    pub fn received(&mut self, turn: &Turn) -> Step {
        self.usage.input += turn.usage.input;
        self.usage.output += turn.usage.output;
        self.usage.cache_read += turn.usage.cache_read;
        self.usage.cache_write += turn.usage.cache_write;
        if !turn.content.is_empty() {
            self.messages.push(Message { role: Role::Assistant, content: turn.content.clone() });
        }
        match turn.stop.as_str() {
            "refusal" => Step::Refused(turn.stop_detail.clone()),
            "max_tokens" => Step::Cut,
            "tool_use" => {
                self.rounds += 1;
                let calls: Vec<Call> = turn.tool_uses().into_iter().map(|(id, name, input)| Call { id: id.into(), name: name.into(), input: input.clone() }).collect();
                if calls.is_empty() || self.rounds > ROUNDS { Step::Reply } else { Step::Calls(calls) }
            }
            _ => Step::Reply,
        }
    }

    pub fn answer(&mut self, results: Vec<(String, Result<String, String>)>) -> Request {
        let content = results
            .into_iter()
            .map(|(id, result)| match result {
                Ok(text) => Content::ToolResult { id, content: text, error: false },
                Err(text) => Content::ToolResult { id, content: text, error: true },
            })
            .collect();
        self.messages.push(Message { role: Role::User, content });
        self.request()
    }

    pub fn drop_pending(&mut self) {
        if let Some(last) = self.messages.last()
            && last.role == Role::Assistant
            && last.content.iter().any(|c| matches!(c, Content::ToolUse { .. }))
        {
            self.messages.pop();
        }
    }

    fn request(&self) -> Request {
        Request { model: self.model.clone(), effort: self.effort.clone(), max_tokens: MAX_TOKENS, system: self.system.clone(), messages: self.messages.clone(), tools: tools() }
    }
}

pub const SUGGEST_TOKENS: u32 = 200;

pub fn suggestion_request(system: &str, before: &str, after: &str) -> Request {
    let text = format!(
        "Complete the Schematic code at the cursor, marked <cursor>. Reply with only the text to insert at the cursor: no explanation, no code fence, nothing that is already there. Reply with nothing when there is nothing useful to add.\n\n{before}<cursor>{after}"
    );
    Request {
        model: "claude-haiku-5-5".to_string(),
        effort: "low".to_string(),
        max_tokens: SUGGEST_TOKENS,
        system: system.to_string(),
        messages: vec![Message::user(&text)],
        tools: Vec::new(),
    }
}

pub fn clean_suggestion(text: &str) -> String {
    let mut text = text.trim_end().to_string();
    if let Some(inner) = text.strip_prefix("```") {
        let inner = inner.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
        text = inner.trim_end_matches("```").trim_end().to_string();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_context_with_labels_and_limits() {
        let text = compose("why nil?", &[Attachment::new("file report.scm", "(define x 1)"), Attachment::new("console", "error: + expects a number, got nil\n")]);
        assert_eq!(text, "[file report.scm]\n(define x 1)\n[end]\n\n[console]\nerror: + expects a number, got nil\n[end]\n\nwhy nil?");
        let long = "é".repeat(ATTACHMENT_LIMIT);
        let cut = compose("q", &[Attachment::new("file big.scm", long)]);
        assert!(cut.contains("\n[cut here; read-file returns the rest]\n[end]"), "{}", &cut[cut.len() - 120..]);
        assert!(cut.len() < ATTACHMENT_LIMIT + 200);
        assert_eq!(compose("plain", &[]), "plain");
        assert!(clip("x".repeat(RESULT_LIMIT + 5)).ends_with("\n[cut here]"));
        assert!(system().contains("## Tables") && system().starts_with("You are the assistant inside Schematic Studio"));
    }

    #[test]
    fn tools_have_strict_schemas_and_summaries() {
        let tools = tools();
        assert_eq!(tools.iter().map(|t| t.name).collect::<Vec<_>>(), ["evaluate", "describe", "read-file", "list-files", "propose-edit"]);
        for tool in &tools {
            assert_eq!(tool.schema.get("additionalProperties"), Some(&Json::Bool(false)), "{}", tool.name);
            assert!(tool.schema.get("required").is_some());
        }
        assert!(tools[0].eager && tools[4].eager && !tools[1].eager);
        let call = Call { id: "t".into(), name: "evaluate".into(), input: Json::object([("code", Json::text("(mean xs)\n(sum xs)"))]) };
        assert_eq!(call.summary(), "ran (mean xs) …");
        assert_eq!(Call { id: "t".into(), name: "list-files".into(), input: Json::Object(Vec::new()) }.summary(), "listed the project files");
    }

    #[test]
    fn sessions_loop_through_tool_calls() {
        let mut session = Session::new("claude-opus-5-5", "medium", "system".into());
        let request = session.ask("mean of xs?".into());
        assert_eq!(request.messages.len(), 1);
        let turn = Turn {
            model: "claude-opus-5-5".into(),
            content: vec![Content::Text("Checking.".into()), Content::ToolUse { id: "t1".into(), name: "evaluate".into(), input: Json::object([("code", Json::text("(mean xs)"))]) }],
            stop: "tool_use".into(),
            stop_detail: String::new(),
            usage: Usage { input: 10, output: 5, cache_read: 100, cache_write: 0 },
        };
        assert_eq!(session.received(&turn), Step::Calls(vec![Call { id: "t1".into(), name: "evaluate".into(), input: Json::object([("code", Json::text("(mean xs)"))]) }]));
        let request = session.answer(vec![("t1".into(), Ok("2.5".into()))]);
        assert_eq!(request.messages.len(), 3);
        assert_eq!(request.messages[2].content, vec![Content::ToolResult { id: "t1".into(), content: "2.5".into(), error: false }]);
        let reply = Turn { content: vec![Content::Text("2.5".into())], stop: "end_turn".into(), usage: Usage { input: 20, output: 2, cache_read: 110, cache_write: 0 }, ..Turn::default() };
        assert_eq!(session.received(&reply), Step::Reply);
        assert_eq!((session.usage.input, session.usage.output, session.usage.cache_read), (30, 7, 210));
        assert_eq!(session.messages.len(), 4);
        let refused = Turn { stop: "refusal".into(), stop_detail: "cyber".into(), ..Turn::default() };
        assert_eq!(session.received(&refused), Step::Refused("cyber".into()));
        assert_eq!(session.received(&Turn { stop: "max_tokens".into(), ..Turn::default() }), Step::Cut);
        assert_eq!(session.messages.len(), 4);
    }

    #[test]
    fn sessions_stop_looping_after_the_round_limit_and_drop_pending_calls() {
        let mut session = Session::new("m", "low", String::new());
        session.ask("go".into());
        let call = Turn { content: vec![Content::ToolUse { id: "t".into(), name: "list-files".into(), input: Json::Object(Vec::new()) }], stop: "tool_use".into(), ..Turn::default() };
        for _ in 0..ROUNDS {
            assert!(matches!(session.received(&call), Step::Calls(_)));
            session.answer(vec![("t".into(), Err("no project".into()))]);
        }
        assert_eq!(session.received(&call), Step::Reply);
        session.ask("again".into());
        assert!(!session.messages.iter().rev().nth(1).unwrap().content.iter().any(|c| matches!(c, Content::ToolUse { .. })));
    }

    #[test]
    fn suggestions_are_short_haiku_requests_and_fences_are_stripped() {
        let request = suggestion_request("sys", "(define (f x)\n  ", ")");
        assert_eq!((request.model.as_str(), request.effort.as_str(), request.max_tokens), ("claude-haiku-5-5", "low", SUGGEST_TOKENS));
        assert!(request.tools.is_empty());
        assert!(matches!(&request.messages[0].content[0], Content::Text(t) if t.contains("(define (f x)\n  <cursor>)")));
        assert_eq!(clean_suggestion("```scheme\n(+ x 1)\n```\n"), "(+ x 1)");
        assert_eq!(clean_suggestion("(+ x 1)  "), "(+ x 1)");
    }
}
