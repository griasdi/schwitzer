//! A real `omp --mode rpc`, driven over its JSON lines protocol.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Once, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(30);

pub struct Omp {
    child: Child,
    stdin: Option<ChildStdin>,
    frames: mpsc::Receiver<Value>,
    /// Frames received that no wait took yet.
    pending: VecDeque<Value>,
    /// The first 300 characters of every frame received.
    log: Vec<String>,
    stderr: PathBuf,
    next_id: u64,
}

/// The first `omp` on `PATH`.
pub fn omp_bin() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join("omp"))
        .find(|p| p.is_file())
        .expect("omp is not on PATH: the omp plugin tests drive a real omp")
}

/// The `HOME` of every omp the tests start: shared, so omp extracts its natives once per version.
pub fn omp_home() -> PathBuf {
    static WARM: Once = Once::new();
    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("omp-home");
    WARM.call_once(|| {
        std::fs::create_dir_all(&home).expect("create the omp home");
        // Outside this repository: omp would read `.omp/mcp.json` (the user's token) from an
        // ancestor of its working directory.
        let scratch = tempfile::tempdir().expect("warm-up dir");
        // With stdin at EOF and no model, omp extracts its natives and exits.
        Command::new(omp_bin())
            .args(["--mode", "rpc"])
            .current_dir(scratch.path())
            .env("HOME", &home)
            .env("PI_CODING_AGENT_DIR", scratch.path().join("agent"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok();
    });
    home
}

impl Omp {
    /// Starts omp in `repo` with its agent dir `agent_dir`, its stderr into `stderr`, and waits
    /// for it to be ready.
    pub fn start(repo: &Path, agent_dir: &Path, stderr: PathBuf) -> Omp {
        let mut cmd = Command::new(omp_bin());
        cmd.args(["--mode", "rpc", "--model", "fake/fake-model"])
            .current_dir(repo)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", omp_home())
            .env("PI_CODING_AGENT_DIR", agent_dir);
        if let Some(tmp) = std::env::var_os("TMPDIR") {
            cmd.env("TMPDIR", tmp);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(File::create(&stderr).expect("omp stderr file"))
            .spawn()
            .expect("start omp");
        let stdout = child.stdout.take().expect("omp stdout");
        let (tx, frames) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let frame = serde_json::from_str(&line).unwrap_or(Value::String(line));
                if tx.send(frame).is_err() {
                    break;
                }
            }
        });
        let mut omp = Omp {
            stdin: child.stdin.take(),
            child,
            frames,
            pending: VecDeque::new(),
            log: Vec::new(),
            stderr,
            next_id: 0,
        };
        omp.wait("ready", |f| f["type"] == "ready");
        omp
    }

    /// Sends `text` as a prompt, as if typed and sent with Enter; returns the prompt's id.
    pub fn prompt(&mut self, text: &str) -> String {
        self.next_id += 1;
        let id = format!("p{}", self.next_id);
        self.send(json!({"id": id, "type": "prompt", "message": text}));
        id
    }

    /// Waits until the prompt `id` completed.
    pub fn finished(&mut self, id: &str) {
        let frame = self.wait(&format!("prompt_result {id}"), |f| {
            f["type"] == "prompt_result" && f["id"] == id
        });
        assert_eq!(frame["status"], "completed", "prompt {id}: {frame}");
    }

    /// Waits for the select dialog titled `title`; returns its id and options.
    pub fn select(&mut self, title: &str) -> (String, Vec<String>) {
        let frame = self.wait(&format!("select {title:?}"), |f| {
            ui(f, "select") && f["title"] == title
        });
        let options = frame["options"]
            .as_array()
            .expect("select options")
            .iter()
            .map(|o| o.as_str().expect("option label").to_string())
            .collect();
        (str_field(&frame, "id"), options)
    }

    /// Answers the dialog `id` with `value`.
    pub fn answer(&mut self, id: &str, value: &str) {
        self.send(json!({"type": "extension_ui_response", "id": id, "value": value}));
    }

    /// Waits for a notice reading exactly `message`.
    pub fn notified(&mut self, message: &str) {
        self.wait(&format!("notice {message:?}"), |f| {
            ui(f, "notify") && f["message"] == message
        });
    }

    /// Waits for the plugin to put text into the editor; returns it.
    pub fn editor_text(&mut self) -> String {
        let frame = self.wait("editor text", |f| ui(f, "set_editor_text"));
        str_field(&frame, "text")
    }

    /// Waits for the end of a call of the tool `tool`.
    pub fn tool_end(&mut self, tool: &str) -> Value {
        self.wait(&format!("end of {tool}"), |f| {
            f["type"] == "tool_execution_end" && f["toolName"] == tool
        })
    }

    fn send(&mut self, frame: Value) {
        let stdin = self.stdin.as_mut().expect("omp stdin");
        writeln!(stdin, "{frame}").expect("write to omp");
        stdin.flush().expect("flush omp stdin");
    }

    /// The first frame, in or after `pending`, for which `pred` holds. Panics on an extension
    /// error, or when none comes within `WAIT`.
    fn wait(&mut self, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
        if let Some(i) = self.pending.iter().position(&pred) {
            return self.pending.remove(i).expect("pending frame");
        }
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Ok(frame) = self.frames.recv_timeout(left) else {
                let start = self.log.len().saturating_sub(60);
                panic!(
                    "omp: no {what} within {} s\nlast frames:\n{}\nomp stderr: {}",
                    WAIT.as_secs(),
                    self.log[start..].join("\n"),
                    self.stderr.display()
                );
            };
            self.log.push(frame.to_string().chars().take(300).collect());
            if frame["type"] == "extension_error" {
                panic!("omp: extension error while waiting for {what}: {frame}");
            }
            if pred(&frame) {
                return frame;
            }
            self.pending.push_back(frame);
        }
    }
}

impl Drop for Omp {
    fn drop(&mut self) {
        // omp drains its input and exits at EOF.
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

/// Whether `frame` is an extension UI request of `method`.
fn ui(frame: &Value, method: &str) -> bool {
    frame["type"] == "extension_ui_request" && frame["method"] == method
}

fn str_field(frame: &Value, field: &str) -> String {
    frame[field]
        .as_str()
        .unwrap_or_else(|| panic!("no {field} in {frame}"))
        .to_string()
}
