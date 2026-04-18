use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc;

pub enum StreamEvent {
    Delta(String),
    Done,
    Error(String),
}

pub async fn stream_anthropic(
    api_key: String,
    model: String,
    messages: Vec<Value>,
    tx: mpsc::Sender<StreamEvent>,
) {
    let result: anyhow::Result<()> = async {
        let client = Client::new();
        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&json!({
                "model": model,
                "max_tokens": 8096,
                "messages": messages,
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

        let mut stream = resp.bytes_stream();
        let mut buf = String::new();

        while let Some(chunk) = stream.next().await {
            buf.push_str(&String::from_utf8_lossy(&chunk?));
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim().to_string();
                buf = buf[pos + 1..].to_string();
                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(v) = serde_json::from_str::<Value>(data) {
                        if v["type"] == "content_block_delta" {
                            if let Some(text) = v["delta"]["text"].as_str() {
                                tx.send(StreamEvent::Delta(text.to_string())).await.ok();
                            }
                        }
                    }
                }
            }
        }

        tx.send(StreamEvent::Done).await.ok();
        Ok(())
    }
    .await;

    if let Err(e) = result {
        tx.send(StreamEvent::Error(e.to_string())).await.ok();
    }
}
