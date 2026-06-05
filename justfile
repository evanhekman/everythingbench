# Run a benchmark
# Examples:
#   just run grok-build-0.1 bullshit-dict
#   just run grok-build-0.1 bullshit-dict --publish
#   just run grok-build-0.1 seven-wonders   # runs a 3-agent SMOKE (only 2 rounds) with that model
#   For full game, edit runner.rs temporarily to use run_game instead of run_smoke_game.

run model benchmark *FLAGS:
    cargo run -- run {{model}} {{benchmark}} {{FLAGS}}

# Publish the latest raw run for a model+benchmark to the website
# Example: just publish grok-build-0.1 bullshit-dict
publish model benchmark:
    cargo run -- publish {{model}} {{benchmark}}

# Build the project
build:
    cargo build --release

# Quick check
check:
    cargo check
