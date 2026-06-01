# Seven Wonders (Base Game)

This is the game engine + data for running 7 Wonders as a benchmark.

## Structure

- `data/` → JSON definitions for cards, wonders, etc. (to be populated from card images)
- `DESIGN.md` → Current design decisions and open questions around the agent interaction model

The actual Rust implementation lives in `src/games/seven_wonders/`.

## Current Status

- Basic action model defined (terminal vs observation actions)
- Agentic loop design in progress (multiple tool calls per turn allowed, with limits)
- Data model not yet started

See `DESIGN.md` for the latest thinking.
