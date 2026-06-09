# Unified run entrypoint (dispatches by first argument)
# Examples:
#   just run grok-build-0.1 bullshit-dict
#   just run grok-build-0.1 bullshit-dict --publish
#   just run seven-wonders 3 auto human-agent auto --rounds 6
#
# Seven Wonders player types: human | human-agent | auto | <model-name>
#   human       — user.txt + live log only
#   human-agent — full LLM context (system_prompt + agent.txt + cards + log), terminal input

run +ARGS:
    bash -eu -o pipefail -c 'if [ "$1" = "seven-wonders" ]; then shift; exec cargo run -- seven-wonders "$@"; else exec cargo run -- run "$@"; fi' -- {{ARGS}}

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