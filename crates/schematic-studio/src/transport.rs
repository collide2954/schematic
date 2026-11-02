use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use iced::futures::channel::mpsc as futures_mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::stream;

use crate::claude::{self, Delta, Reader, Request, Turn};

#[derive(Debug, Clone)]
pub enum Packet {
    Delta(Delta),
    Done(Result<Turn, String>),
}

#[derive(Clone, Default)]
pub struct Handle {
    child: Arc<Mutex<Option<Child>>>,
    stopped: Arc<AtomicBool>,
}

impl Handle {
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Ok(mut guard) = self.child.lock()
            && let Some(child) = guard.as_mut()
        {
            let _ = child.kill();
        }
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Handle")
    }
}

fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

pub fn curl_version() -> Option<(u32, u32)> {
    let output = command("curl").arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    version_of(&text)
}

pub fn version_of(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.strip_prefix("curl ")?.split_whitespace().next()?.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

pub fn hides_key() -> bool {
    static HIDES: OnceLock<bool> = OnceLock::new();
    *HIDES.get_or_init(|| curl_version().is_some_and(|version| version >= (8, 3)))
}

pub fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| curl_version().is_some())
}

pub fn start(request: &Request, key: String) -> (Handle, impl Stream<Item = Packet> + Send + 'static) {
    let handle = Handle::default();
    let body = request.body().to_string();
    let shared = handle.clone();
    let stream = stream::channel(64, async move |mut output| {
        let (packets, mut inbox) = futures_mpsc::unbounded();
        std::thread::spawn(move || {
            run(body, key, shared, |packet| {
                let _ = packets.unbounded_send(packet);
            });
        });
        while let Some(packet) = inbox.next().await {
            let _ = output.send(packet).await;
        }
    });
    (handle, stream)
}

fn run(body: String, key: String, handle: Handle, mut emit: impl FnMut(Packet)) {
    let hidden = hides_key();
    let mut curl = command("curl");
    curl.args(claude::curl_arguments(hidden, &key)).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if hidden {
        curl.env("ANTHROPIC_API_KEY", &key);
    }
    let mut child = match curl.spawn() {
        Ok(child) => child,
        Err(e) => return emit(Packet::Done(Err(format!("cannot start curl: {e}")))),
    };
    let stdin = child.stdin.take();
    let Some(mut stdout) = child.stdout.take() else { return emit(Packet::Done(Err("curl gave no output".to_string()))) };
    let stderr = child.stderr.take();
    if let Ok(mut guard) = handle.child.lock() {
        *guard = Some(child);
    }
    if let Some(mut stdin) = stdin {
        let _ = stdin.write_all(body.as_bytes());
    }
    let mut reader = Reader::default();
    let mut raw = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match stdout.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                raw.extend_from_slice(&buffer[..n]);
                for delta in reader.push(&buffer[..n]) {
                    emit(Packet::Delta(delta));
                }
            }
        }
    }
    let status = handle.child.lock().ok().and_then(|mut guard| guard.take()).and_then(|mut child| child.wait().ok());
    let mut errors = String::new();
    if let Some(mut stderr) = stderr {
        let _ = stderr.read_to_string(&mut errors);
    }
    if handle.stopped.load(Ordering::SeqCst) {
        return emit(Packet::Done(Err("stopped".to_string())));
    }
    match status {
        Some(status) if status.success() => emit(Packet::Done(reader.finish())),
        _ => {
            let body = String::from_utf8_lossy(&raw);
            let message = if body.trim().is_empty() { errors.trim().to_string() } else { claude::error_message(&body) };
            emit(Packet::Done(Err(if message.is_empty() { "curl failed".to_string() } else { message })));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_versions_are_read_from_the_banner() {
        assert_eq!(version_of("curl 8.21.0 (Windows) libcurl/8.21.0 Schannel\nRelease-Date: 2026"), Some((8, 21)));
        assert_eq!(version_of("curl 7.81.0 (x86_64-pc-linux-gnu)"), Some((7, 81)));
        assert_eq!(version_of("wget 1.0"), None);
        assert!(Some((8, 3)) >= Some((8, 3)) && Some((7, 81)) < Some((8, 3)));
    }

    #[test]
    fn stopped_handles_report_stopped() {
        let handle = Handle::default();
        handle.stop();
        assert!(handle.stopped.load(Ordering::SeqCst));
        let mut packets = Vec::new();
        run("{}".into(), "k".into(), handle, |packet| packets.push(packet));
        assert!(matches!(packets.last(), Some(Packet::Done(Err(message))) if message == "stopped" || message.starts_with("cannot start curl")), "{packets:?}");
    }
}
