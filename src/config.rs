use anyhow::{bail, Result};

/// Known valid models we are willing to call.
/// This acts as a safety valve so we don't accidentally send unknown model strings to the API.
pub const KNOWN_MODELS: &[&str] = &[
    "grok-build-0.1",
    "grok-4.3",
    // Add more as we validate them
];

pub fn is_known_model(model: &str) -> bool {
    KNOWN_MODELS.contains(&model)
}

pub fn validate_model(model: &str) -> Result<()> {
    if is_known_model(model) {
        Ok(())
    } else {
        bail!(
            "Unknown model '{}'. Allowed models:\n  {}",
            model,
            KNOWN_MODELS.join("\n  ")
        );
    }
}