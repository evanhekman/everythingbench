# Run a benchmark: just run <model> <benchmark>
# Example: just run grok-build-0.1 bullshit-dict

run model benchmark:
    cargo run -- run {{model}} {{benchmark}}

# Build the project
build:
    cargo build --release

# Quick check
check:
    cargo check
