use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_PROVIDER: &str = "grok";

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

/// Per-seat LLM call stats (Seven Wonders).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmSeatStats {
    pub decisions: u32,
    pub total_latency_ms: u64,
    pub api_errors: u32,
    pub parse_failures: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SevenWondersPlayerResult {
    pub seat: usize,
    pub spec: String,
    pub score: crate::games::seven_wonders::scoring::ScoreBreakdown,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_stats: Option<LlmSeatStats>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SevenWondersRunConfig {
    pub player_count: u8,
    pub player_specs: Vec<String>,
    pub max_rounds: Option<u32>,
    pub rounds_played: u32,
    pub game_complete: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SevenWondersSummary {
    pub winner_seats: Vec<usize>,
    pub winning_score: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SevenWondersRunResult {
    pub schema_version: u32,
    pub run_number: u32,
    pub model: String,
    pub benchmark: String,
    pub provider: String,
    pub timestamp: DateTime<Utc>,
    pub config: SevenWondersRunConfig,
    pub players: Vec<SevenWondersPlayerResult>,
    pub summary: SevenWondersSummary,
}

pub fn raw_dir(provider: &str, model: &str, benchmark: &str) -> String {
    format!("results/raw/{provider}/{model}/{benchmark}")
}

pub fn published_dir(provider: &str, model: &str, benchmark: &str) -> String {
    format!("web/data/runs/{provider}/{model}/{benchmark}")
}

pub fn latest_run_in_dir(dir: &str) -> anyhow::Result<Option<(u32, PathBuf)>> {
    if !Path::new(dir).exists() {
        return Ok(None);
    }

    let mut max_num = 0u32;
    let mut latest_file: Option<PathBuf> = None;

    for entry in fs::read_dir(dir)? {
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

    Ok(latest_file.map(|path| (max_num, path)))
}

pub fn next_run_number(raw_dir: &str) -> anyhow::Result<u32> {
    fs::create_dir_all(raw_dir)?;
    Ok(latest_run_in_dir(raw_dir)?.map(|(n, _)| n + 1).unwrap_or(1))
}

pub fn write_raw_json(
    provider: &str,
    model: &str,
    benchmark: &str,
    run_number: u32,
    value: &impl Serialize,
) -> anyhow::Result<PathBuf> {
    let dir = raw_dir(provider, model, benchmark);
    fs::create_dir_all(&dir)?;

    let filename = format!("{:04}.json", run_number);
    let raw_path = Path::new(&dir).join(&filename);
    let json = serde_json::to_string_pretty(value)?;
    fs::write(&raw_path, &json)?;

    fs::write("results/latest.json", &json)?;

    println!("Wrote raw result to: {}", raw_path.display());
    println!("Updated: results/latest.json");
    Ok(raw_path)
}

pub fn publish_json(
    provider: &str,
    model: &str,
    benchmark: &str,
    run_number: u32,
    value: &impl Serialize,
) -> anyhow::Result<()> {
    let dir = published_dir(provider, model, benchmark);
    fs::create_dir_all(&dir)?;

    let filename = format!("{:04}.json", run_number);
    let published_path = Path::new(&dir).join(&filename);
    let json = serde_json::to_string_pretty(value)?;
    fs::write(&published_path, &json)?;

    println!("Published run to site: {}", published_path.display());
    Ok(())
}

pub trait PersistableRun: Serialize {
    fn provider(&self) -> &str;
    fn model(&self) -> &str;
    fn benchmark(&self) -> &str;
    fn run_number(&self) -> u32;

    fn write_raw(&self) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        write_raw_json(
            self.provider(),
            self.model(),
            self.benchmark(),
            self.run_number(),
            self,
        )?;
        Ok(())
    }

    fn publish_to_site(&self) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        publish_json(
            self.provider(),
            self.model(),
            self.benchmark(),
            self.run_number(),
            self,
        )
    }
}

impl PersistableRun for RunResult {
    fn provider(&self) -> &str {
        &self.provider
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn benchmark(&self) -> &str {
        &self.benchmark
    }
    fn run_number(&self) -> u32 {
        self.run_number
    }
}

impl PersistableRun for SevenWondersRunResult {
    fn provider(&self) -> &str {
        &self.provider
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn benchmark(&self) -> &str {
        &self.benchmark
    }
    fn run_number(&self) -> u32 {
        self.run_number
    }
}

pub const SEVEN_WONDERS_BENCHMARK: &str = "seven-wonders";

/// Model key for results paths when no LLM seat is in the lineup (auto/human only).
pub const LOCAL_RUN_MODEL: &str = "local";

pub fn first_llm_model(player_specs: &[String]) -> Option<String> {
    player_specs
        .iter()
        .find(|spec| crate::config::is_known_model(spec))
        .cloned()
}

pub fn seven_wonders_run_model(player_specs: &[String]) -> String {
    first_llm_model(player_specs).unwrap_or_else(|| LOCAL_RUN_MODEL.to_string())
}

pub fn build_seven_wonders_run_result(
    model: String,
    player_specs: &[String],
    max_rounds: Option<u32>,
    rounds_played: u32,
    game_complete: bool,
    player_count: u8,
    scores: &[crate::games::seven_wonders::scoring::ScoreBreakdown],
    llm_stats: &[Option<LlmSeatStats>],
) -> anyhow::Result<SevenWondersRunResult> {
    let raw_dir = raw_dir(DEFAULT_PROVIDER, &model, SEVEN_WONDERS_BENCHMARK);
    let run_number = next_run_number(&raw_dir)?;

    let mut players = Vec::with_capacity(player_specs.len());
    for (seat, spec) in player_specs.iter().enumerate() {
        let score = scores
            .get(seat)
            .cloned()
            .unwrap_or_default();
        players.push(SevenWondersPlayerResult {
            seat,
            spec: spec.clone(),
            score,
            llm_stats: llm_stats.get(seat).and_then(|s| s.clone()),
        });
    }

    let winning_score = scores.iter().map(|s| s.total).max().unwrap_or(0);
    let winner_seats: Vec<usize> = scores
        .iter()
        .enumerate()
        .filter(|(_, s)| s.total == winning_score)
        .map(|(i, _)| i)
        .collect();

    Ok(SevenWondersRunResult {
        schema_version: 1,
        run_number,
        model,
        benchmark: SEVEN_WONDERS_BENCHMARK.to_string(),
        provider: DEFAULT_PROVIDER.to_string(),
        timestamp: Utc::now(),
        config: SevenWondersRunConfig {
            player_count,
            player_specs: player_specs.to_vec(),
            max_rounds,
            rounds_played,
            game_complete,
        },
        players,
        summary: SevenWondersSummary {
            winner_seats,
            winning_score,
        },
    })
}

/// Copies the latest raw run for a given model + benchmark into the website data directory.
pub fn publish_latest_raw(model: &str, benchmark: &str) -> anyhow::Result<()> {
    let dir = raw_dir(DEFAULT_PROVIDER, model, benchmark);
    let (run_number, source) = latest_run_in_dir(&dir)?.ok_or_else(|| {
        anyhow::anyhow!(
            "No raw runs found for {}/{} at {}",
            model,
            benchmark,
            dir
        )
    })?;

    let dest_dir = published_dir(DEFAULT_PROVIDER, model, benchmark);
    fs::create_dir_all(&dest_dir)?;

    let filename = format!("{:04}.json", run_number);
    let dest = Path::new(&dest_dir).join(&filename);

    fs::copy(&source, &dest)?;

    println!("Published raw run {} → {}", source.display(), dest.display());

    Ok(())
}