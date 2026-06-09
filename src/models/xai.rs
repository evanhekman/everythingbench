use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::Instant;

#[derive(Deserialize)]
struct ChatCompletion {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

pub struct XaiClient {
    client: Client,
    api_key: String,
}

impl XaiClient {
    pub fn new() -> Result<Self> {
        let api_key = std::env::var("XAI_API_KEY")
            .context("XAI_API_KEY not found in environment (check .env)")?;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        Ok(Self { client, api_key })
    }

    pub fn complete(&self, model: &str, prompt: &str, system: Option<&str>, max_tokens: Option<u32>) -> Result<(String, u64)> {
        let mut messages = vec![];
        if let Some(sys) = system {
            messages.push((String::from("system"), sys.to_string()));
        }
        messages.push((String::from("user"), prompt.to_string()));
        self.complete_with_messages(model, &messages, max_tokens)
    }

    /// Multi-turn chat: each `(role, content)` pair is sent in order (e.g. system, user, assistant, user, …).
    pub fn complete_with_messages(
        &self,
        model: &str,
        messages: &[(String, String)],
        max_tokens: Option<u32>,
    ) -> Result<(String, u64)> {
        let start = Instant::now();

        let api_messages: Vec<_> = messages
            .iter()
            .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
            .collect();

        let mt = max_tokens.unwrap_or(32);
        let body = serde_json::json!({
            "model": model,
            "messages": api_messages,
            "temperature": 0.0,
            "max_tokens": mt,
        });

        let resp = self.client
            .post("https://api.x.ai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .context("Failed to send request to xAI")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("xAI API error ({}): {}", status, text);
        }

        let completion: ChatCompletion = resp.json()?;
        let content = completion
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        let latency = start.elapsed().as_millis() as u64;

        Ok((content, latency))
    }
}