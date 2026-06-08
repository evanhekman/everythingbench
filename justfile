# Run a benchmark
# Examples:
#   just run grok-build-0.1 bullshit-dict
#   just run grok-build-0.1 bullshit-dict --publish
#   just run grok-build-0.1 seven-wonders        # normal: 2 humans + 1 AI, full game
#   just run grok-build-0.1 seven-wonders-smoke  # dedicated smoke: 4 turns, only grok for player 0 (1), autos for 1+2 (2+3)

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
