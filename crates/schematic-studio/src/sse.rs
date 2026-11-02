#[derive(Debug, Clone, PartialEq, Default)]
pub struct Event {
    pub name: String,
    pub data: String,
}

#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
    name: String,
    data: Vec<String>,
}

impl Decoder {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Event> {
        self.pending.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            if let Some(event) = self.line(line) {
                events.push(event);
            }
        }
        events
    }

    pub fn finish(&mut self) -> Option<Event> {
        let rest = std::mem::take(&mut self.pending);
        let mut events = Vec::new();
        if !rest.is_empty() {
            let line = String::from_utf8_lossy(&rest);
            if let Some(event) = self.line(line.trim_end_matches(['\n', '\r'])) {
                events.push(event);
            }
        }
        events.into_iter().next().or_else(|| self.dispatch())
    }

    fn line(&mut self, line: &str) -> Option<Event> {
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "event" => self.name = value.to_string(),
            "data" => self.data.push(value.to_string()),
            _ => {}
        }
        None
    }

    fn dispatch(&mut self) -> Option<Event> {
        if self.data.is_empty() {
            self.name.clear();
            return None;
        }
        let event = Event { name: std::mem::take(&mut self.name), data: self.data.join("\n") };
        self.data.clear();
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_events_across_chunks_and_line_endings() {
        let mut decoder = Decoder::default();
        let mut events = decoder.push(b"event: message_start\r\ndata: {\"a\":1}\r\n\r\n: keep-alive\n\nevent: delta\ndata: one\nda");
        events.extend(decoder.push(b"ta: two\n\ndata: tail"));
        events.extend(decoder.finish());
        assert_eq!(
            events,
            vec![
                Event { name: "message_start".into(), data: "{\"a\":1}".into() },
                Event { name: "delta".into(), data: "one\ntwo".into() },
                Event { name: String::new(), data: "tail".into() },
            ]
        );
    }

    #[test]
    fn keeps_multibyte_characters_split_between_chunks() {
        let text = "data: caf\u{e9}\n\n".as_bytes();
        let mut decoder = Decoder::default();
        let mut events = decoder.push(&text[..9]);
        events.extend(decoder.push(&text[9..]));
        assert_eq!(events, vec![Event { name: String::new(), data: "caf\u{e9}".into() }]);
        assert_eq!(decoder.finish(), None);
    }
}
