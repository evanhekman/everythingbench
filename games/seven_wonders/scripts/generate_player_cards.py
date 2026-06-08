#!/usr/bin/env python3
"""Generate cardsN.txt from the shared cards.txt for each player count (3–7).

Threshold notation in cards.txt (last field):
  [3]       → 1 copy in any 3+ player game
  [4, 6]    → 1 copy at 4+ players, 2 copies at 6+ players
  [3, 6, 7] → 1 copy at 3–5 players, 2 at 6, 3 at 7

Guilds (purple, under ### GUILDS) have [] and are always listed as candidates;
exactly (N + 2) of them are chosen per game at deal time.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

MIN_PLAYERS = 3
MAX_PLAYERS = 7

CARD_LINE_RE = re.compile(r"^(?P<body>.+)\|\s*\[(?P<thresholds>[^\]]*)\]\s*$")


def parse_thresholds(raw: str) -> list[int]:
    raw = raw.strip()
    if not raw:
        return []
    return [int(x.strip()) for x in raw.split(",") if x.strip()]


def copies_for_players(thresholds: list[int], players: int) -> int:
    return sum(1 for t in thresholds if t <= players)


def parse_cards_txt(
    text: str,
) -> tuple[str, list[tuple[str, list[tuple[str, list[int]]]]], list[str]]:
    """Return (preamble, age_sections, guild_card_bodies).

    age_sections: list of (header, [(line_body_without_thresholds, thresholds), ...])
    guild_card_bodies: card lines from ### GUILDS (thresholds always empty in source)
    """
    lines = text.splitlines()
    preamble: list[str] = []
    ages: list[tuple[str, list[tuple[str, list[int]]]]] = []
    guild_cards: list[str] = []

    section: str | None = None
    current_age: tuple[str, list[tuple[str, list[int]]]] | None = None

    for line in lines:
        if line.startswith("### GUILDS"):
            if current_age is not None:
                ages.append(current_age)
                current_age = None
            section = "guilds"
            continue
        if line.startswith("### "):
            if current_age is not None:
                ages.append(current_age)
            section = "age"
            current_age = (line, [])
            continue

        if section is None:
            preamble.append(line)
            continue

        if section == "guilds":
            m = CARD_LINE_RE.match(line)
            if m:
                guild_cards.append(m.group("body").rstrip())
            continue

        if section == "age" and current_age is not None:
            m = CARD_LINE_RE.match(line)
            if m:
                thresholds = parse_thresholds(m.group("thresholds"))
                current_age[1].append((m.group("body").rstrip(), thresholds))

    if current_age is not None:
        ages.append(current_age)

    return "\n".join(preamble), ages, guild_cards


def render_player_cards(players: int, ages: list, guild_cards: list[str]) -> str:
    out: list[str] = []
    out.append(f"# SEVEN WONDERS CARD LIST ({players} players)")
    out.append("#")
    out.append("# Generated from cards.txt. Copy counts are exact for this player count.")
    out.append(f"# Guilds: {players + 2} of the 10 guilds below are included each game.")
    out.append("#")
    out.append("Format:")
    out.append("Card Name (card_id) | Color | Cost | Benefit | Chains | Copies")
    out.append("")

    for age_header, cards in ages:
        included: list[tuple[str, int]] = []
        for body, thresholds in cards:
            count = copies_for_players(thresholds, players)
            if count > 0:
                included.append((body, count))
        if not included:
            continue
        out.append(age_header)
        for body, count in included:
            out.append(f"{body} | {count}")
        out.append("")

    out.append("### GUILDS")
    out.append(f"There are 10 guilds; {players + 2} are included in each game.")
    out.append("")
    for body in guild_cards:
        out.append(f"{body} | 1")
    out.append("")

    return "\n".join(out).rstrip() + "\n"


def verify_deck_sizes(ages: list, players: int) -> int:
    """Return total non-guild cards across all ages for sanity checks."""
    total = 0
    for _, cards in ages:
        for _, thresholds in cards:
            total += copies_for_players(thresholds, players)
    return total


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--input",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "prompts" / "cards.txt",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path(__file__).resolve().parent.parent / "prompts",
    )
    parser.add_argument("--players", type=int, nargs="*", help="Only generate for these counts (default 3–7)")
    parser.add_argument("--verify", action="store_true", help="Print deck size summary and exit")
    args = parser.parse_args()

    text = args.input.read_text()
    _, ages, guild_cards = parse_cards_txt(text)

    player_counts = args.players or list(range(MIN_PLAYERS, MAX_PLAYERS + 1))
    for n in player_counts:
        if n < MIN_PLAYERS:
            print(f"error: player count must be >= {MIN_PLAYERS}, got {n}", file=sys.stderr)
            return 1

    if args.verify:
        print("Non-guild cards per age (expected 7 * players each):")
        for n in player_counts:
            per_age = [sum(copies_for_players(t, n) for _, t in cards) for _, cards in ages]
            print(f"  {n} players: ages {per_age}, total {sum(per_age)}")
        return 0

    args.output_dir.mkdir(parents=True, exist_ok=True)
    for n in player_counts:
        out_path = args.output_dir / f"cards{n}.txt"
        out_path.write_text(render_player_cards(n, ages, guild_cards))
        print(f"wrote {out_path}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())