# Seven Wonders Data

This folder will contain the JSON definitions for:

- Cards (Age 1, 2, 3 + Guilds)
- Wonders (all 7 base game wonders + both sides where relevant)
- Possibly other static data (starting coins per wonder, etc.)

## player_count field (on cards)

`player_count` is a **list of integers** representing the player count thresholds where copies of this card are added to the deck.

Examples:
- `[3]`        → 1 copy when playing with 3+ players
- `[3, 5]`     → 1 copy from 3+, +1 copy from 5+ players (2 copies total at 5+)
- `[4]`        → 1 copy from 4+ players only

The first number in the list is what is printed in the bottom-right corner of the physical card.

This list approach allows us to represent cards that have multiple copies at higher player counts without duplicating card definitions.

## Current Status

Start by adding small batches of cards. The schema will be refined iteratively based on the actual card data.
