use anyhow::{bail, Result};

/// Known valid models we are willing to call.
/// This acts as a safety valve so we don't accidentally send unknown model strings to the API.
pub const KNOWN_MODELS: &[&str] = &[
    "grok-build-0.1",
    "grok-4.3",
    // Add more as we validate them
];

pub fn validate_model(model: &str) -> Result<()> {
    if KNOWN_MODELS.contains(&model) {
        Ok(())
    } else {
        bail!(
            "Unknown model '{}'. Allowed models:\n  {}",
            model,
            KNOWN_MODELS.join("\n  ")
        );
    }
}