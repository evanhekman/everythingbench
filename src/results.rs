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
    /// Writes the run to the raw directory and updates results/latest.json (scratchpad).
    /// This is called on every execution.
    pub fn write_raw(&self) -> anyhow::Result<()> {
        let provider = &self.provider;
        let model = &self.model;
        let benchmark = &self.benchmark;

        let raw_dir = format!("results/raw/{}/{}/{}", provider, model, benchmark);
        fs::create_dir_all(&raw_dir)?;

        let filename = format!("{:04}.json", self.run_number);
        let raw_path = Path::new(&raw_dir).join(&filename);

        let json = serde_json::to_string_pretty(self)?;
        fs::write(&raw_path, &json)?;

        // Always update latest.json as a convenience scratchpad (last raw execution only)
        let latest_path = Path::new("results/latest.json");
        fs::write(latest_path, &json)?;

        println!("Wrote raw result to: {}", raw_path.display());
        println!("Updated: results/latest.json");

        Ok(())
    }

    /// Copies this run into the website data directory so it becomes visible on the site.
    /// The site is self-contained under web/data/runs/
    pub fn publish_to_site(&self) -> anyhow::Result<()> {
        let provider = &self.provider;
        let model = &self.model;
        let benchmark = &self.benchmark;

        let published_dir = format!("web/data/runs/{}/{}/{}", provider, model, benchmark);
        fs::create_dir_all(&published_dir)?;

        let filename = format!("{:04}.json", self.run_number);
        let published_path = Path::new(&published_dir).join(&filename);

        let json = serde_json::to_string_pretty(self)?;
        fs::write(&published_path, &json)?;

        println!("Published run to site: {}", published_path.display());

        Ok(())
    }
}

/// Copies the latest raw run for a given model + benchmark into the website data directory.
pub fn publish_latest_raw(model: &str, benchmark: &str) -> anyhow::Result<()> {
    let provider = "grok"; // hardcoded for now as per our structure
    let raw_dir = format!("results/raw/{}/{}/{}", provider, model, benchmark);

    if !Path::new(&raw_dir).exists() {
        anyhow::bail!(
            "No raw runs found for {}/{} at {}",
            model,
            benchmark,
            raw_dir
        );
    }

    // Find the highest numbered raw run
    let mut max_num = 0u32;
    let mut latest_file: Option<std::path::PathBuf> = None;

    for entry in fs::read_dir(&raw_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if let Some(num_str) = name.strip_suffix(".json") {
            if let Ok(num) = num_str.parse::<u32>() {
                if num > max_num {
                    max_num = num;
                    latest_file = Some(entry.path());
                }
            }
        }
    }

    let source = match latest_file {
        Some(p) => p,
        None => anyhow::bail!("No raw run files found in {}", raw_dir),
    };

    let published_dir = format!("web/data/runs/{}/{}/{}", provider, model, benchmark);
    fs::create_dir_all(&published_dir)?;

    let filename = format!("{:04}.json", max_num);
    let dest = Path::new(&published_dir).join(&filename);

    fs::copy(&source, &dest)?;

    println!("Published raw run {} → {}", source.display(), dest.display());

    Ok(())
}