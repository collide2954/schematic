use schematic::json::Json;

use crate::sse;

pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
pub const API_VERSION: &str = "2026-10-21";
pub const BETAS: &str = "server-side-fallback-2026-07-01";
pub const MAX_TOKENS: u32 = 16000;

pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
}

pub const MODELS: [ModelInfo; 3] = [
    ModelInfo { id: "claude-opus-5-5", name: "Claude Opus 5.5" },
    ModelInfo { id: "claude-sonnet-5-5", name: "Claude Sonnet 5.5" },
    ModelInfo { id: "claude-haiku-5-5", name: "Claude Haiku 5.5" },
];

pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(String),
    ToolUse { id: String, name: String, input: Json },
    ToolResult { id: String, content: String, error: bool },
    Raw(Json),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Content>,
}

impl Message {
    pub fn user(text: &str) -> Message {
        Message { role: Role::User, content: vec![Content::Text(text.to_string())] }
    }

    pub fn json(&self) -> Json {
        let role = match self.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        Json::object([("role", Json::text(role)), ("content", Json::Array(self.content.iter().map(content_json).collect()))])
    }
}

fn content_json(content: &Content) -> Json {
    match content {
        Content::Text(text) => Json::object([("type", Json::text("text")), ("text", Json::text(text.as_str()))]),
        Content::ToolUse { id, name, input } => Json::object([
            ("type", Json::text("tool_use")),
            ("id", Json::text(id.as_str())),
            ("name", Json::text(name.as_str())),
            ("input", input.clone()),
        ]),
        Content::ToolResult { id, content, error } => {
            let mut json = Json::object([
                ("type", Json::text("tool_result")),
                ("tool_use_id", Json::text(id.as_str())),
                ("content", Json::text(content.as_str())),
            ]);
            if *error {
                json.set("is_error", Json::Bool(true));
            }
            json
        }
        Content::Raw(json) => json.clone(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Json,
    pub eager: bool,
}

impl Tool {
    fn json(&self) -> Json {
        let mut json = Json::object([
            ("name", Json::text(self.name)),
            ("description", Json::text(self.description)),
            ("input_schema", self.schema.clone()),
            ("strict", Json::Bool(true)),
        ]);
        if self.eager {
            json.set("eager_input_streaming", Json::Bool(true));
        }
        json
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub model: String,
    pub effort: String,
    pub max_tokens: u32,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
}

impl Request {
    pub fn body(&self) -> Json {
        let mut body = Json::object([
            ("model", Json::text(self.model.as_str())),
            ("max_tokens", Json::from(self.max_tokens)),
            ("stream", Json::Bool(true)),
            ("fallbacks", Json::text("default")),
            ("output_config", Json::object([("effort", Json::text(self.effort.as_str()))])),
            (
                "system",
                Json::Array(vec![Json::object([
                    ("type", Json::text("text")),
                    ("text", Json::text(self.system.as_str())),
                    ("cache_control", Json::object([("type", Json::text("ephemeral"))])),
                ])]),
            ),
        ]);
        if !self.tools.is_empty() {
            body.set("tools", Json::Array(self.tools.iter().map(Tool::json).collect()));
        }
        body.set("messages", Json::Array(self.messages.iter().map(Message::json).collect()));
        body
    }
}

pub fn curl_arguments(key_in_environment: bool, key: &str) -> Vec<String> {
    let mut arguments: Vec<String> = [
        "-sS", "-N", "--no-buffer", "--fail-with-body", "--max-time", "900",
        "-X", "POST", API_URL,
        "-H", "content-type: application/json",
        "-H", &format!("anthropic-version: {API_VERSION}"),
        "-H", &format!("anthropic-beta: {BETAS}"),
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    if key_in_environment {
        arguments.extend(["--variable", "%ANTHROPIC_API_KEY", "--expand-header", "x-api-key: {{ANTHROPIC_API_KEY}}"].map(str::to_string));
    } else {
        arguments.extend(["-H".to_string(), format!("x-api-key: {key}")]);
    }
    arguments.extend(["--data-binary", "@-"].map(str::to_string));
    arguments
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Turn {
    pub model: String,
    pub content: Vec<Content>,
    pub stop: String,
    pub stop_detail: String,
    pub usage: Usage,
}

impl Turn {
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                Content::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn tool_uses(&self) -> Vec<(&str, &str, &Json)> {
        self.content
            .iter()
            .filter_map(|c| match c {
                Content::ToolUse { id, name, input } => Some((id.as_str(), name.as_str(), input)),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Delta {
    Text(String),
    ToolStart(String),
    Error(String),
}

enum Block {
    Text(String),
    Tool { id: String, name: String, json: String },
    Raw(Json),
}

#[derive(Default)]
pub struct Reader {
    turn: Turn,
    blocks: Vec<Block>,
    decoder: sse::Decoder,
}

impl Reader {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Delta> {
        let events = self.decoder.push(chunk);
        events.iter().filter_map(|event| self.event(event)).collect()
    }

    pub fn finish(mut self) -> Result<Turn, String> {
        if let Some(event) = self.decoder.finish()
            && let Some(Delta::Error(message)) = self.event(&event)
        {
            return Err(message);
        }
        if self.turn.stop.is_empty() && self.blocks.is_empty() && self.turn.content.is_empty() {
            return Err("the reply ended before it began".to_string());
        }
        for block in self.blocks.drain(..) {
            self.turn.content.push(finish_block(block));
        }
        Ok(self.turn)
    }

    fn event(&mut self, event: &sse::Event) -> Option<Delta> {
        let data = Json::parse(&event.data).ok()?;
        let kind = data.get("type").and_then(Json::as_str).unwrap_or(&event.name);
        match kind {
            "message_start" => {
                let message = data.get("message")?;
                self.turn.model = message.get("model").and_then(Json::as_str).unwrap_or("").to_string();
                if let Some(usage) = message.get("usage") {
                    self.turn.usage.input = count(usage, "input_tokens");
                    self.turn.usage.cache_read = count(usage, "cache_read_input_tokens");
                    self.turn.usage.cache_write = count(usage, "cache_creation_input_tokens");
                }
                None
            }
            "content_block_start" => {
                let block = data.get("content_block")?;
                match block.get("type").and_then(Json::as_str) {
                    Some("text") => {
                        self.blocks.push(Block::Text(block.get("text").and_then(Json::as_str).unwrap_or("").to_string()));
                        None
                    }
                    Some("tool_use") => {
                        let name = block.get("name").and_then(Json::as_str).unwrap_or("").to_string();
                        let id = block.get("id").and_then(Json::as_str).unwrap_or("").to_string();
                        self.blocks.push(Block::Tool { id, name: name.clone(), json: String::new() });
                        Some(Delta::ToolStart(name))
                    }
                    _ => {
                        self.blocks.push(Block::Raw(block.clone()));
                        None
                    }
                }
            }
            "content_block_delta" => {
                let delta = data.get("delta")?;
                let block = self.blocks.last_mut()?;
                match (delta.get("type").and_then(Json::as_str)?, block) {
                    ("text_delta", Block::Text(text)) => {
                        let piece = delta.get("text").and_then(Json::as_str)?.to_string();
                        text.push_str(&piece);
                        Some(Delta::Text(piece))
                    }
                    ("input_json_delta", Block::Tool { json, .. }) => {
                        json.push_str(delta.get("partial_json").and_then(Json::as_str).unwrap_or(""));
                        None
                    }
                    ("thinking_delta", Block::Raw(raw)) => {
                        let piece = delta.get("thinking").and_then(Json::as_str).unwrap_or("");
                        let mut text = raw.get("thinking").and_then(Json::as_str).unwrap_or("").to_string();
                        text.push_str(piece);
                        raw.set("thinking", Json::text(text));
                        None
                    }
                    ("signature_delta", Block::Raw(raw)) => {
                        raw.set("signature", Json::text(delta.get("signature").and_then(Json::as_str).unwrap_or("")));
                        None
                    }
                    _ => None,
                }
            }
            "content_block_stop" => {
                let block = self.blocks.pop()?;
                self.turn.content.push(finish_block(block));
                None
            }
            "message_delta" => {
                if let Some(delta) = data.get("delta") {
                    if let Some(stop) = delta.get("stop_reason").and_then(Json::as_str) {
                        self.turn.stop = stop.to_string();
                    }
                    if let Some(category) = delta.path(&["stop_details", "category"]).and_then(Json::as_str) {
                        self.turn.stop_detail = category.to_string();
                    }
                }
                if let Some(usage) = data.get("usage") {
                    self.turn.usage.output = count(usage, "output_tokens");
                }
                None
            }
            "error" => {
                let message = data.path(&["error", "message"]).and_then(Json::as_str).unwrap_or("the API returned an error");
                Some(Delta::Error(message.to_string()))
            }
            _ => None,
        }
    }
}

fn count(usage: &Json, key: &str) -> u64 {
    usage.get(key).and_then(Json::as_f64).unwrap_or(0.0) as u64
}

fn finish_block(block: Block) -> Content {
    match block {
        Block::Text(text) => Content::Text(text),
        Block::Tool { id, name, json } => {
            let input = if json.trim().is_empty() { Json::Object(Vec::new()) } else { Json::parse(&json).unwrap_or(Json::Object(Vec::new())) };
            Content::ToolUse { id, name, input }
        }
        Block::Raw(raw) => Content::Raw(raw),
    }
}

pub fn error_message(body: &str) -> String {
    match Json::parse(body.trim()) {
        Ok(json) => json.path(&["error", "message"]).and_then(Json::as_str).unwrap_or(body).to_string(),
        Err(_) => body.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(lines: &[&str]) -> Vec<u8> {
        lines.iter().map(|data| format!("event: x\ndata: {data}\n\n")).collect::<String>().into_bytes()
    }

    #[test]
    fn builds_the_request_body() {
        let request = Request {
            model: "claude-opus-5-5".into(),
            effort: "medium".into(),
            max_tokens: MAX_TOKENS,
            system: "You know Schematic.".into(),
            messages: vec![
                Message::user("hi"),
                Message { role: Role::Assistant, content: vec![Content::ToolUse { id: "t1".into(), name: "evaluate".into(), input: Json::object([("code", Json::text("(+ 1 2)"))]) }] },
                Message { role: Role::User, content: vec![Content::ToolResult { id: "t1".into(), content: "3".into(), error: false }] },
            ],
            tools: vec![Tool { name: "evaluate", description: "Run code", schema: Json::object([("type", Json::text("object"))]), eager: true }],
        };
        let body = request.body();
        assert_eq!(body.get("model").and_then(Json::as_str), Some("claude-opus-5-5"));
        assert_eq!(body.get("stream"), Some(&Json::Bool(true)));
        assert_eq!(body.get("fallbacks").and_then(Json::as_str), Some("default"));
        assert_eq!(body.path(&["output_config", "effort"]).and_then(Json::as_str), Some("medium"));
        assert_eq!(body.path(&["system"]).and_then(Json::as_array).unwrap()[0].path(&["cache_control", "type"]).and_then(Json::as_str), Some("ephemeral"));
        let tool = &body.get("tools").and_then(Json::as_array).unwrap()[0];
        assert_eq!((tool.get("eager_input_streaming"), tool.get("strict")), (Some(&Json::Bool(true)), Some(&Json::Bool(true))));
        let messages = body.get("messages").and_then(Json::as_array).unwrap();
        assert_eq!(messages[1].to_string(), r#"{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"evaluate","input":{"code":"(+ 1 2)"}}]}"#);
        assert_eq!(messages[2].to_string(), r#"{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"3"}]}"#);
        assert!(body.get("thinking").is_none());
        let hidden = curl_arguments(true, "sk-secret");
        assert!(hidden.contains(&"--expand-header".to_string()) && !hidden.iter().any(|a| a.contains("sk-secret")));
        let plain = curl_arguments(false, "sk-secret");
        assert!(plain.contains(&"x-api-key: sk-secret".to_string()) && !plain.contains(&"--variable".to_string()));
    }

    #[test]
    fn reads_text_and_tool_calls_from_a_stream() {
        let bytes = stream(&[
            r#"{"type":"message_start","message":{"model":"claude-opus-5-5","usage":{"input_tokens":12,"cache_read_input_tokens":4000}}}"#,
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"abc"}}"#,
            r#"{"type":"content_block_stop","index":0}"#,
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Let me "}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"check."}}"#,
            r#"{"type":"content_block_stop","index":1}"#,
            r#"{"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"evaluate","input":{}}}"#,
            r#"{"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"code\": \"(mean"}}"#,
            r#"{"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":" xs)\"}"}}"#,
            r#"{"type":"content_block_stop","index":2}"#,
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":30}}"#,
            r#"{"type":"message_stop"}"#,
        ]);
        let mut reader = Reader::default();
        let mut deltas = Vec::new();
        for chunk in bytes.chunks(37) {
            deltas.extend(reader.push(chunk));
        }
        assert_eq!(deltas, vec![Delta::Text("Let me ".into()), Delta::Text("check.".into()), Delta::ToolStart("evaluate".into())]);
        let turn = reader.finish().unwrap();
        assert_eq!(turn.text(), "Let me check.");
        assert_eq!(turn.tool_uses(), vec![("toolu_1", "evaluate", &Json::object([("code", Json::text("(mean xs)"))]))]);
        assert_eq!(turn.content[0], Content::Raw(Json::object([("type", Json::text("thinking")), ("thinking", Json::text("")), ("signature", Json::text("abc"))])));
        assert_eq!((turn.stop.as_str(), turn.usage.input, turn.usage.cache_read, turn.usage.output), ("tool_use", 12, 4000, 30));
        assert_eq!(turn.model, "claude-opus-5-5");
    }

    #[test]
    fn reports_refusals_and_errors() {
        let bytes = stream(&[
            r#"{"type":"message_start","message":{"model":"m","usage":{"input_tokens":1}}}"#,
            r#"{"type":"message_delta","delta":{"stop_reason":"refusal","stop_details":{"type":"refusal","category":"cyber"}},"usage":{"output_tokens":0}}"#,
        ]);
        let mut reader = Reader::default();
        reader.push(&bytes);
        let turn = reader.finish().unwrap();
        assert_eq!((turn.stop.as_str(), turn.stop_detail.as_str()), ("refusal", "cyber"));
        let mut reader = Reader::default();
        let deltas = reader.push(&stream(&[r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#]));
        assert_eq!(deltas, vec![Delta::Error("Overloaded".into())]);
        assert_eq!(Reader::default().finish().unwrap_err(), "the reply ended before it began");
        assert_eq!(error_message(r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#), "invalid x-api-key");
        assert_eq!(error_message("gateway timeout\n"), "gateway timeout");
    }
}
