use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub enum StreamEvent {
    Delta(String),
    ApiHistory(Vec<Value>),
    Done,
    Error(String),
}

struct ToolCall {
    id: String,
    name: String,
    input_json: String,
}

pub async fn stream_anthropic(
    api_key: String,
    model: String,
    messages: Vec<Value>,
    tx: mpsc::Sender<StreamEvent>,
) {
    if let Err(e) = agentic_loop(api_key, model, messages, &tx).await {
        tx.send(StreamEvent::Error(e.to_string())).await.ok();
    }
}

async fn agentic_loop(
    api_key: String,
    model: String,
    initial_messages: Vec<Value>,
    tx: &mpsc::Sender<StreamEvent>,
) -> anyhow::Result<()> {
    let client = Client::new();
    let mut all_messages = initial_messages;
    let mut new_msgs: Vec<Value> = Vec::new();

    loop {
        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&json!({
                "model": model,
                "max_tokens": 8096,
                "messages": all_messages,
                "tools": tool_defs(),
                "stream": true,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tx.send(StreamEvent::Error(format!("{}: {}", status, body))).await.ok();
            return Ok(());
        }

        let (text, calls) = collect_stream(resp, tx).await?;

        if calls.is_empty() {
            new_msgs.push(json!({"role": "assistant", "content": text}));
            break;
        }

        // Build assistant message with full content array (text + tool_use blocks)
        let mut content: Vec<Value> = Vec::new();
        if !text.is_empty() {
            content.push(json!({"type": "text", "text": text}));
        }
        for tc in &calls {
            let input: Value = serde_json::from_str(&tc.input_json).unwrap_or(Value::Null);
            content.push(json!({
                "type": "tool_use",
                "id": tc.id,
                "name": tc.name,
                "input": input,
            }));
        }
        let assistant_msg = json!({"role": "assistant", "content": content});
        all_messages.push(assistant_msg.clone());
        new_msgs.push(assistant_msg);

        // Execute tools, build tool_result user message
        let mut results: Vec<Value> = Vec::new();
        for tc in &calls {
            let input: Value = serde_json::from_str(&tc.input_json).unwrap_or(Value::Null);
            let output = run_tool(&tc.name, &input, tx).await;
            results.push(json!({
                "type": "tool_result",
                "tool_use_id": tc.id,
                "content": output,
            }));
        }
        let results_msg = json!({"role": "user", "content": results});
        all_messages.push(results_msg.clone());
        new_msgs.push(results_msg);
    }

    tx.send(StreamEvent::ApiHistory(new_msgs)).await.ok();
    tx.send(StreamEvent::Done).await.ok();
    Ok(())
}

async fn collect_stream(
    resp: reqwest::Response,
    tx: &mpsc::Sender<StreamEvent>,
) -> anyhow::Result<(String, Vec<ToolCall>)> {
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut text = String::new();
    let mut calls: Vec<ToolCall> = Vec::new();
    let mut cur_tool: Option<usize> = None;

    while let Some(chunk) = stream.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk?));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf = buf[pos + 1..].to_string();

            let data = match line.strip_prefix("data: ") {
                Some(d) => d.to_string(),
                None => continue,
            };
            let v: Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            match v["type"].as_str().unwrap_or("") {
                "content_block_start" => {
                    let block = &v["content_block"];
                    if block["type"] == "tool_use" {
                        let id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        tx.send(StreamEvent::Delta(format!("\n→ **{}**", name))).await.ok();
                        cur_tool = Some(calls.len());
                        calls.push(ToolCall { id, name, input_json: String::new() });
                    } else {
                        cur_tool = None;
                    }
                }
                "content_block_delta" => {
                    let delta = &v["delta"];
                    match delta["type"].as_str().unwrap_or("") {
                        "text_delta" => {
                            if let Some(t) = delta["text"].as_str() {
                                text.push_str(t);
                                tx.send(StreamEvent::Delta(t.to_string())).await.ok();
                            }
                        }
                        "input_json_delta" => {
                            if let Some(i) = cur_tool {
                                if let Some(p) = delta["partial_json"].as_str() {
                                    calls[i].input_json.push_str(p);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                "content_block_stop" => {
                    if let Some(i) = cur_tool {
                        let input: Value =
                            serde_json::from_str(&calls[i].input_json).unwrap_or(Value::Null);
                        let hint = tool_hint(&calls[i].name, &input);
                        tx.send(StreamEvent::Delta(format!(" `{}`\n", hint))).await.ok();
                    }
                    cur_tool = None;
                }
                _ => {}
            }
        }
    }

    Ok((text, calls))
}

async fn run_tool(name: &str, input: &Value, tx: &mpsc::Sender<StreamEvent>) -> String {
    match name {
        "read_file" => {
            let path = input["path"].as_str().unwrap_or("");
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    tx.send(StreamEvent::Delta(format!(
                        "← _read {} bytes_\n",
                        content.len()
                    )))
                    .await
                    .ok();
                    content
                }
                Err(e) => {
                    let msg = format!("error: {}", e);
                    tx.send(StreamEvent::Delta(format!("← _{}_\n", msg))).await.ok();
                    msg
                }
            }
        }
        "write_file" => {
            let path = input["path"].as_str().unwrap_or("");
            let content = input["content"].as_str().unwrap_or("");
            match std::fs::write(path, content) {
                Ok(_) => {
                    tx.send(StreamEvent::Delta(format!(
                        "← _wrote {} bytes_\n",
                        content.len()
                    )))
                    .await
                    .ok();
                    "ok".to_string()
                }
                Err(e) => {
                    let msg = format!("error: {}", e);
                    tx.send(StreamEvent::Delta(format!("← _{}_\n", msg))).await.ok();
                    msg
                }
            }
        }
        _ => format!("unknown tool: {}", name),
    }
}

fn tool_hint(name: &str, input: &Value) -> String {
    match name {
        "read_file" | "write_file" => input["path"].as_str().unwrap_or("?").to_string(),
        _ => String::new(),
    }
}

fn tool_defs() -> Vec<Value> {
    vec![
        json!({
            "name": "read_file",
            "description": "Read the contents of a file at the given path. Returns the file content as text.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to read"
                    }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "write_file",
            "description": "Write content to a file, creating it if it doesn't exist and overwriting if it does.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }
        }),
    ]
}
