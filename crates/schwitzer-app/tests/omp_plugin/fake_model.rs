//! A scripted OpenAI-compatible model: each chat completion answers with the next queued reply.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};
use tokio::runtime::Runtime;

/// One answer of the model.
pub enum Reply {
    Text(String),
    Tool { name: String, args: Value },
}

#[derive(Default)]
struct Script {
    replies: VecDeque<Reply>,
    /// The text of every request's messages, in the order the requests came.
    requests: Vec<String>,
    calls: u64,
}

pub struct FakeModel {
    script: Arc<Mutex<Script>>,
    port: u16,
}

type Shared = Arc<Mutex<Script>>;

impl FakeModel {
    /// Serves the model on a free loopback port, on `rt`.
    pub fn start(rt: &Runtime) -> FakeModel {
        let script: Shared = Arc::default();
        let listener = rt
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .expect("bind the fake model");
        let port = listener.local_addr().expect("fake model address").port();
        let router = axum::Router::new()
            .route("/v1/chat/completions", post(chat))
            .route("/v1/models", get(models))
            .with_state(script.clone());
        rt.spawn(async move {
            axum::serve(listener, router).await.ok();
        });
        FakeModel { script, port }
    }

    /// Queues the answer to the next request.
    pub fn push(&self, reply: Reply) {
        self.lock().replies.push_back(reply);
    }

    /// What the model saw so far: the text of each request's messages.
    pub fn requests(&self) -> Vec<String> {
        self.lock().requests.clone()
    }

    /// omp's `models.yml` with this model as `fake/fake-model`.
    pub fn models_yml(&self) -> String {
        format!(
            "providers:\n  fake:\n    baseUrl: http://127.0.0.1:{}/v1\n    auth: none\n    api: openai-completions\n    models:\n      - id: fake-model\n        name: Fake\n        reasoning: false\n",
            self.port
        )
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Script> {
        self.script.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

async fn models() -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        json!({"object": "list", "data": [{"id": "fake-model", "object": "model"}]}).to_string(),
    )
        .into_response()
}

/// The text of a message's `content`: a string, or parts whose `text` fields are joined.
fn content_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|p| p["text"].as_str().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

async fn chat(State(script): State<Shared>, body: String) -> Response {
    let Ok(body) = serde_json::from_str::<Value>(&body) else {
        return (StatusCode::BAD_REQUEST, "not JSON").into_response();
    };
    let (reply, n) = {
        let mut s = script.lock().unwrap_or_else(PoisonError::into_inner);
        let text = body["messages"]
            .as_array()
            .map(|ms| {
                ms.iter()
                    .map(|m| content_text(&m["content"]))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        s.requests.push(text);
        // A stray turn answers "ok" so it never hangs a test.
        let reply = s
            .replies
            .pop_front()
            .unwrap_or_else(|| Reply::Text("ok".into()));
        s.calls += 1;
        (reply, s.calls)
    };
    let (message, finish) = match &reply {
        Reply::Text(text) => (json!({"role": "assistant", "content": text}), "stop"),
        Reply::Tool { name, args } => (
            json!({"role": "assistant", "tool_calls": [{
                "id": format!("call_{n}"),
                "type": "function",
                "function": {"name": name, "arguments": args.to_string()},
            }]}),
            "tool_calls",
        ),
    };
    let usage = json!({"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2});
    if body["stream"] != Value::Bool(true) {
        let answer = json!({
            "id": "x",
            "object": "chat.completion",
            "created": 0,
            "model": "fake-model",
            "choices": [{"index": 0, "message": message, "finish_reason": finish}],
            "usage": usage,
        });
        return (
            [(header::CONTENT_TYPE, "application/json")],
            answer.to_string(),
        )
            .into_response();
    }
    let mut delta = message;
    if let Some(calls) = delta["tool_calls"].as_array_mut() {
        calls[0]["index"] = json!(0);
    }
    let chunk = |extra: Value| {
        let mut c = json!({"id": "x", "object": "chat.completion.chunk", "created": 0, "model": "fake-model"});
        for (k, v) in extra.as_object().expect("chunk fields") {
            c[k] = v.clone();
        }
        format!("data: {c}\n\n")
    };
    let stream = [
        chunk(json!({"choices": [{"index": 0, "delta": delta, "finish_reason": null}]})),
        chunk(json!({"choices": [{"index": 0, "delta": {}, "finish_reason": finish}]})),
        chunk(json!({"choices": [], "usage": usage})),
        "data: [DONE]\n\n".to_string(),
    ]
    .concat();
    ([(header::CONTENT_TYPE, "text/event-stream")], stream).into_response()
}
