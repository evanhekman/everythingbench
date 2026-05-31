use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const GAME_DIR: &str = "games/bullshit-dict";
const DATA_DIR: &str = "games/bullshit-dict/data";

#[derive(Debug, Clone)]
pub struct Trial {
    pub id: String,
    pub words: Vec<String>,
    pub has_fake: bool,
}

pub struct BullshitDict {
    pub trials: Vec<Trial>,
}

impl BullshitDict {
    pub fn load() -> Result<Self> {
        let answers_path = Path::new(DATA_DIR).join("answers.txt");
        let answers_content = fs::read_to_string(&answers_path)
            .with_context(|| format!("Failed to read {}", answers_path.display()))?;

        let mut has_fake_map: HashMap<String, bool> = HashMap::new();

        for line in answers_content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Format: "words0 yes" or "words0 no"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() != 2 {
                continue;
            }
            let trial_id = parts[0].to_string();
            let has_fake = parts[1].eq_ignore_ascii_case("yes");
            has_fake_map.insert(trial_id, has_fake);
        }

        let mut trials = Vec::new();

        for i in 0..10 {
            let trial_id = format!("words{}", i);
            let file_path = Path::new(DATA_DIR).join(format!("{}.txt", trial_id));
            let content = fs::read_to_string(&file_path)
                .with_context(|| format!("Failed to read {}", file_path.display()))?;

            let words: Vec<String> = content
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();

            let has_fake = *has_fake_map
                .get(&trial_id)
                .unwrap_or(&false);

            trials.push(Trial {
                id: trial_id,
                words,
                has_fake,
            });
        }

        Ok(Self { trials })
    }

    pub fn get_prompt(&self) -> Result<String> {
        let prompt_path = Path::new(GAME_DIR).join("prompt.txt");
        fs::read_to_string(&prompt_path)
            .with_context(|| format!("Failed to read {}", prompt_path.display()))
    }
}