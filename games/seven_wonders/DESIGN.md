# Seven Wonders Engine - Design Notes

## Interaction Model (Agentic / Tool Use)

The controlling agent (LLM) does **not** receive a complete game state dump every turn.

Instead, it operates in a loop:

1. The agent can issue multiple **observation actions** (tools) to gather information.
2. At any point it can issue one of the three **terminal actions** to end its turn:
   - `play`
   - `wonder` (build wonder stage using the chosen card)
   - `burn`

The card used for `wonder` or `burn` is removed from the hand (tucked under the wonder or discarded).

### Current Action Set

**Terminal Actions**
- `play` + card
- `wonder` + card + stage
- `burn` + card

**Observation Actions** (current starting set)
- `check_my_cards`
- `check_all_cards`
- `check_my_resources`
- `check_all_resources`
- `check_all_military`
- `check_civilizations`
- `check_my_wonder`
- `check_wonders`

More tools will be added (especially around trading).

### Rules for the Loop

- The agent may take up to N actions (to be configured) before it must output a terminal action.
- Invalid actions are rejected with an explanation. The agent is asked to choose again.
- Excessive invalid actions or tool calls → the run is flagged in results.
- Trading and other "interventions" should trigger additional tool calls rather than being auto-resolved.

## Scope

- Full base game of 7 Wonders
- 2–7 players supported
- All three ages, guilds in Age III, full wonder sides, trading, military, science, etc.

## Open / In-Progress Questions

- Exact limit on tool calls + invalid attempts per turn (N)
- How trading decisions are surfaced to the agent
- Default information given at the start of a turn (if any)
- Exact output format expected from the model (JSON? structured text?)
- Whether some observation actions should eventually cost resources

This document will be updated as the engine is built.
