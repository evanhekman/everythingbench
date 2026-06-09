//! GameLog: single canonical plain-text log for transparency + personalized views for agents.
//!
//! - full: always contains every decision block with full private info (for the deciding player at the time).
//! - For decision views supplied to an agent: only completed prior rounds (summaries + age summaries),
//!   plus the *current* open decision block for that player (no same-round actions from others).
//! - Autos and post-round actions are recorded as simple action lines (never private decision blocks).
//! - After each round completes, summaries are committed so future decisions see them.
//! - Always writes the full log to log.txt on disk for easy `cat log.txt` / `tail -f`.

use std::fs;

pub struct GameLog {
    /// The complete unredacted log (with all private decision blocks as they were built).
    /// This is what gets written to log.txt.
    pub full: String,
    /// Safe prefix containing only complete prior rounds (public summaries + age headers/summaries).
    /// Decision views for agents are built from this + the current open round header + open decision.
    committed: String,
    /// Current age (e.g. 1)
    current_age: Option<u8>,
    /// Current round within the age (e.g. 1)
    current_round: Option<u8>,
    /// If a decision is open for a player, which one (0-based).
    current_decision_player: Option<usize>,
    /// Accumulating content inside the current decision (private info + error messages).
    current_decision_text: String,
    /// Action lines collected during the current round (for the round summary).
    current_round_actions: Vec<String>,
}

impl GameLog {
    pub fn new() -> Self {
        Self {
            full: String::new(),
            committed: String::new(),
            current_age: None,
            current_round: None,
            current_decision_player: None,
            current_decision_text: String::new(),
            current_round_actions: Vec::new(),
        }
    }

    /// Call when a new age begins (before its rounds).
    pub fn start_age(&mut self, age: u8) {
        let header = format!("=== AGE {} ===\n\n", age);
        self.full.push_str(&header);
        self.committed.push_str(&header);
        self.current_age = Some(age);
        self.write_to_disk();
    }

    /// Call at the beginning of each round (before players decide).
    pub fn start_round(&mut self, round: u8) {
        self.current_round = Some(round);
        self.current_round_actions.clear();
        let header = format!("--- Round {} ---\n", round);
        self.full.push_str(&header);
        self.write_to_disk();
    }

    /// Begin a decision block for a log-using player (LLM). Private info goes here.
    pub fn begin_player_decision(&mut self, player: usize, private_info: String) {
        self.current_decision_player = Some(player);
        self.current_decision_text = private_info;
    }

    /// Append text inside the open decision (e.g. ERROR messages on retry).
    pub fn append_to_current_decision(&mut self, text: &str) {
        if self.current_decision_player.is_some() {
            self.current_decision_text.push_str(text);
        }
    }

    /// Record action lines for the round summary and close the current decision block in the full log.
    pub fn close_current_decision(&mut self, summary_lines: &[String]) {
        if self.current_decision_player.is_some() {
            let block = format!("--- Your Turn ---\n{}\n", self.current_decision_text);
            self.full.push_str(&block);
            self.current_round_actions.extend(summary_lines.iter().cloned());
            self.current_decision_player = None;
            self.current_decision_text.clear();
            self.write_to_disk();
        }
    }

    /// For autos (and any non-log decider): record action lines for the round summary.
    /// Also appends them to the full log immediately.
    pub fn append_simple_player_action(&mut self, summary_lines: &[String]) {
        for line in summary_lines {
            self.full.push_str(line);
            self.full.push('\n');
            self.current_round_actions.push(line.clone());
        }
        self.write_to_disk();
    }

    /// Add the public round summary, then commit everything so far.
    pub fn add_round_summary(&mut self, round: u8) {
        let mut summary = format!("Round {} Summary:\n", round);
        for line in &self.current_round_actions {
            summary.push_str(line);
            summary.push('\n');
        }
        summary.push('\n');
        self.full.push_str(&summary);
        self.committed.push_str(&summary);
        self.current_round = None;
        self.current_round_actions.clear();
        self.write_to_disk();
    }

    /// Close the current age with an end-of-age summary block.
    pub fn close_age(&mut self, summary_text: &str) {
        if let Some(age) = self.current_age {
            let block = format!("--- Age {} Summary ---\n{}\n\n", age, summary_text);
            self.full.push_str(&block);
            self.committed.push_str(&block);
            self.current_age = None;
            self.write_to_disk();
        }
    }

    /// Build the personalized string to supply as context for a deciding player.
    /// Prior complete rounds + current round header + open decision block (with "(you)" markers).
    pub fn get_decision_view_for(&self, player: usize) -> String {
        let mut view = personalize_for_player(&self.committed, player);
        if let Some(round) = self.current_round {
            view.push_str(&format!("--- Round {} ---\n", round));
        }
        if self.current_decision_player == Some(player) {
            view.push_str("--- Your Turn ---\n");
            view.push_str(&self.current_decision_text);
            if !self.current_decision_text.ends_with('\n') {
                view.push('\n');
            }
        }
        view
    }

    /// Write the full log to disk (log.txt in cwd). Safe to call often; overwrites.
    pub fn write_to_disk(&self) {
        let _ = fs::write("log.txt", &self.full);
    }
}

/// Replace "Player N" with "Player N (you)" for the viewing player.
fn personalize_for_player(text: &str, player: usize) -> String {
    let marker = format!("Player {} ", player);
    let you_marker = format!("Player {} (you) ", player);
    text.replace(&marker, &you_marker)
}

impl Default for GameLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::GameLog;

    #[test]
    fn plain_text_log_format_matches_agent_spec() {
        let mut log = GameLog::new();
        log.start_age(1);
        log.start_round(1);
        log.begin_player_decision(
            1,
            "hand: [marketplace, baths]\ncoins: 3\nwonder_stages_built: 0\n\
your_production: []\nleft (Player 0) production: []\nright (Player 2) production: []\n"
                .to_string(),
        );
        log.append_simple_player_action(&["Player 0 played ore_vein".to_string()]);
        log.close_current_decision(&["Player 1 played marketplace".to_string()]);
        log.append_simple_player_action(&["Player 2 played glassworks".to_string()]);
        log.add_round_summary(1);

        let view = log.get_decision_view_for(1);
        assert!(view.contains("=== AGE 1 ==="));
        assert!(view.contains("Round 1 Summary:"));
        assert!(view.contains("Player 1 (you) played marketplace"));
        assert!(!view.contains("<round"));
        assert!(!view.contains("<decision"));

        log.start_round(2);
        log.begin_player_decision(
            1,
            "hand: [apothecary]\ncoins: 3\nwonder_stages_built: 0\n\
your_production: []\nleft (Player 0) production: [ore]\nright (Player 2) production: [glass]\n"
                .to_string(),
        );
        let turn_view = log.get_decision_view_for(1);
        assert!(turn_view.contains("--- Round 2 ---"));
        assert!(turn_view.contains("--- Your Turn ---"));
        assert!(turn_view.contains("hand: [apothecary]"));
        assert!(!turn_view.contains("--- Round 1 ---"));
    }

    #[test]
    fn personalize_marks_viewing_player_as_you() {
        let text = "Round 1 Summary:\nPlayer 0 played ore_vein\nPlayer 1 played marketplace\n";
        let view = super::personalize_for_player(text, 1);
        assert!(view.contains("Player 1 (you) played marketplace"));
        assert!(view.contains("Player 0 played ore_vein"));
    }
}