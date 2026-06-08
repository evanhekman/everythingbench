//! GameLog: single canonical XML log for transparency + personalized views for agents.
//!
//! - full: always contains every <decision> with full private info (for the deciding player at the time).
//! - For decision views supplied to an agent: only up to the end of the previous round (committed),
//!   plus the *current* open <decision> block for that player (no same-round actions from others).
//! - Autos and post-round actions are recorded as simple action lines (never private <decision>).
//! - After each round completes, summaries + all round actions are committed so future decisions see them.
//! - Always writes the full log to log.txt on disk for easy `cat log.txt` / `tail -f`.

use std::fs;

pub struct GameLog {
    /// The complete unredacted log (with all private <decision> blocks as they were built).
    /// This is what gets written to log.txt.
    pub full: String,
    /// Safe prefix containing only complete prior rounds (with their public actions + summaries).
    /// Decision views for agents are built from this + the current open decision (if any).
    committed: String,
    /// Current open age tag (e.g. "1")
    current_age: Option<u8>,
    /// Current open round tag (e.g. "1")
    current_round: Option<u8>,
    /// If a decision is open for a player, which one (0-based).
    current_decision_player: Option<usize>,
    /// Accumulating content inside the current <decision> (private info + attempt errors + neighbors on 2nd prompt).
    current_decision_text: String,
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
        }
    }

    /// Call when a new age begins (before its rounds).
    pub fn start_age(&mut self, age: u8) {
        if let Some(prev) = self.current_age {
            // close previous if open (shouldn't normally happen)
            self.full.push_str(&format!("</age_{}>\n", prev));
        }
        self.full.push_str(&format!("<age_{}>\n", age));
        self.current_age = Some(age);
        self.write_to_disk();
    }

    /// Call at the beginning of each round (before players decide).
    pub fn start_round(&mut self, round: u8) {
        if let Some(prev) = self.current_round {
            // shouldn't happen
            self.full.push_str(&format!("</round_{}>\n", prev));
        }
        self.full.push_str(&format!("<round_{}>\n", round));
        self.current_round = Some(round);
        self.write_to_disk();
    }

    /// Begin a rich decision block for a log-using player (LLM). Private info goes here.
    /// The view passed to the agent will be committed + this open block.
    /// Does not yet appear in committed.
    pub fn begin_player_decision(&mut self, player: usize, private_info: String) {
        self.current_decision_player = Some(player);
        self.current_decision_text = private_info;
        // Do not append to full yet; we append the closed version on close_current_decision.
        // This keeps "as it is being built" visible only after close (or we can append an open version if desired).
    }

    /// Append more text inside the open <decision> (e.g. "Attempt 1: ... Error: ...", neighbors info, etc.).
    /// This will be visible in the full log and in re-try views for this same player.
    pub fn append_to_current_decision(&mut self, text: &str) {
        if self.current_decision_player.is_some() {
            self.current_decision_text.push_str(text);
            // For live visibility of the building decision, we can append a snapshot to full (as a comment or progressive).
            // But to keep XML clean, we only write the final closed block. User sees progress via console prints instead.
        }
    }

    /// Close the current player's <decision>, append the full private block (with outcome) to the full log.
    /// The outcome_line is usually "player_N played xxx" or "player_N built wonder stage S" or "player_N burned yyy".
    pub fn close_current_decision(&mut self, outcome_line: &str) {
        if let Some(player) = self.current_decision_player {
            self.current_decision_text.push_str(&format!("\n{}\n", outcome_line));
            let block = format!(
                "<player_{}>\n<decision>\n{}\n</decision>\n</player_{}>\n",
                player, self.current_decision_text, player
            );
            self.full.push_str(&block);
            self.current_decision_player = None;
            self.current_decision_text.clear();
            self.write_to_disk();
        }
    }

    /// For autos (and any non-log decider): record a simple action line (no private decision wrapper).
    /// These are visible in full immediately (for transparency) but *not* shown to agents in same-round decision views.
    pub fn append_simple_player_action(&mut self, player: usize, action_line: &str) {
        let block = format!("<player_{}>\n{}\n</player_{}>\n", player, action_line, player);
        self.full.push_str(&block);
        self.write_to_disk();
    }

    /// Add the public <summary> for the just-completed round, close the round tag,
    /// then commit everything so far so the *next* round's decisions can see prior actions + this summary.
    pub fn add_round_summary(&mut self, summary_text: &str) {
        if let Some(round) = self.current_round {
            self.full.push_str(&format!("<summary>\n{}\n</summary>\n", summary_text));
            self.full.push_str(&format!("</round_{}>\n", round));
            self.current_round = None;
            // Now this round (its player actions + summary) is complete and safe to show future decisions.
            self.committed = self.full.clone();
            self.write_to_disk();
        }
    }

    /// Close any open age (at end of game or age change).
    pub fn close_age(&mut self) {
        if let Some(age) = self.current_age {
            self.full.push_str(&format!("</age_{}>\n", age));
            self.current_age = None;
            self.committed = self.full.clone();
            self.write_to_disk();
        }
    }

    /// Build the string to supply as context for a deciding player.
    /// This is the "personalized" view: only prior complete rounds + (if this player has an open decision) the open <player_N><decision>private...</decision> (left open).
    /// No same-round actions from other players, even if they have already been recorded in .full.
    pub fn get_decision_view_for(&self, player: usize) -> String {
        let mut view = self.committed.clone();
        if self.current_decision_player == Some(player) {
            if !view.ends_with('\n') {
                view.push('\n');
            }
            view.push_str(&format!("<player_{}>\n<decision>\n{}\n", player, self.current_decision_text));
            // deliberately leave </decision> and </player> off so the model "completes" the decision
        }
        view
    }

    /// Write the full log to disk (log.txt in cwd). Safe to call often; overwrites.
    pub fn write_to_disk(&self) {
        // Best-effort; ignore errors so game doesn't crash on fs issues.
        let _ = fs::write("log.txt", &self.full);
    }

    /// Convenience: return current full for console printing / debugging.
    pub fn full_as_str(&self) -> &str {
        &self.full
    }
}

impl Default for GameLog {
    fn default() -> Self {
        Self::new()
    }
}
