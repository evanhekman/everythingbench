use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct TrialResult {
    pub trial_id: String,
    pub words: Vec<String>,
    pub prompt: String,
    pub raw_response: String,
    pub parsed_answer: String,
    pub correct: bool,
    pub latency_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunResult {
    pub schema_version: u32,
    pub run_number: u32,
    pub model: String,
    pub benchmark: String,
    pub provider: String,
    pub timestamp: DateTime<Utc>,
    pub config: RunConfig,
    pub trials: Vec<TrialResult>,
    pub summary: Summary,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RunConfig {
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Summary {
    pub total: usize,
    pub correct: usize,
    pub accuracy: f64,
}

impl RunResult {
    /// Writes the full result to the canonical location and also overwrites results/latest.json
    pub fn write(&self) -> anyhow::Result<()> {
        let provider = &self.provider;
        let model = &self.model;
        let benchmark = &self.benchmark;

        let base_dir = format!("results/runs/{}/{}/{}", provider, model, benchmark);
        fs::create_dir_all(&base_dir)?;

        let filename = format!("{:04}.json", self.run_number);
        let full_path = Path::new(&base_dir).join(&filename);

        let json = serde_json::to_string_pretty(self)?;
        fs::write(&full_path, &json)?;

        // Always overwrite the convenient latest.json
        let latest_path = Path::new("results/latest.json");
        fs::write(latest_path, &json)?;

        println!("Wrote result to: {}", full_path.display());
        println!("Also wrote: results/latest.json");

        Ok(())
    }
}