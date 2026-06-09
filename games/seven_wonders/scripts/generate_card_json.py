#!/usr/bin/env python3
"""Regenerate data/cards/age{1,2,3}.json from the verified scripts/cards.txt source.

Usage:
  python3 games/seven_wonders/scripts/generate_card_json.py
  python3 games/seven_wonders/scripts/generate_card_json.py --verify
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_INPUT = SCRIPT_DIR / "cards.txt"
DEFAULT_OUTPUT_DIR = SCRIPT_DIR.parent / "data" / "cards"

COLOR_MAP = {
    "brown": "brown",
    "gray": "grey",
    "grey": "grey",
    "blue": "blue",
    "red": "red",
    "green": "green",
    "yellow": "yellow",
    "purple": "purple",
}

RESOURCE_MAP = {
    "wood": "wood",
    "stone": "stone",
    "brick": "clay",
    "ore": "ore",
    "glass": "glass",
    "paper": "papyrus",
    "cloth": "loom",
    "tree": "wood",
}

SCIENCE_MAP = {
    "tablet": "tablet",
    "compass": "compass",
    "cog": "cog",
    "gear": "cog",
}

NAME_ID_RE = re.compile(r"^(?P<name>.+?)\s+\((?P<id>[^)]+)\)\s*$")
COIN_COST_RE = re.compile(r"^(\d+)\s+coin", re.I)
BRACKET_COST_RE = re.compile(r"^\[(.+)\]$")
PROD_FIXED_RE = re.compile(
    r"^\+(\d+)\s+(wood|stone|brick|ore|glass|paper|cloth|tree)\b", re.I
)
PROD_OR_RE = re.compile(
    r"\+1\s+(wood|stone|brick|ore|glass|paper|cloth|tree)\b", re.I
)
PROD_OR_AMOUNTS_RE = re.compile(
    r"\+(\d+)\s+(wood|stone|brick|ore|glass|paper|cloth|tree)\b", re.I
)
POINTS_RE = re.compile(r"\+(\d+)\s+points?", re.I)
COINS_RE = re.compile(r"\+(\d+)\s+coins?", re.I)
MILITARY_RE = re.compile(r"\+(\d+)\s+military", re.I)
SCIENCE_RE = re.compile(r"\+1\s+(tablet|compass|cog|gear)\b", re.I)
TRADE_MANUF_RE = re.compile(
    r"buy\s+\[paper,\s*glass,\s*cloth\]\s+from\s+either\s+neighbor", re.I
)
TRADE_RAW_LEFT_RE = re.compile(
    r"buy\s+\[wood,\s*stone,\s*brick,\s*ore\]\s+from\s+left_neighbor", re.I
)
TRADE_RAW_RIGHT_RE = re.compile(
    r"buy\s+\[wood,\s*stone,\s*brick,\s*ore\]\s+from\s+right_neighbor", re.I
)


def load_cards_txt_parser():
    path = SCRIPT_DIR / "generate_player_cards.py"
    spec = importlib.util.spec_from_file_location("generate_player_cards", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def parse_card_fields(body: str) -> tuple[str, str, str, str, str, str]:
    parts = [p.strip() for p in body.split("|")]
    if len(parts) != 5:
        raise ValueError(f"Expected 5 pipe-separated fields, got {len(parts)}: {body!r}")
    name_id, color, cost, benefit, chains = parts
    m = NAME_ID_RE.match(name_id)
    if not m:
        raise ValueError(f"Could not parse card name/id: {name_id!r}")
    return m.group("name"), m.group("id"), color, cost, benefit, chains


def map_resource(token: str) -> str:
    key = token.strip().lower()
    if key not in RESOURCE_MAP:
        raise ValueError(f"Unknown resource token: {token!r}")
    return RESOURCE_MAP[key]


def parse_cost(cost: str) -> dict[str, int]:
    c = cost.strip()
    if not c or c.lower() == "free":
        return {}
    m = COIN_COST_RE.match(c)
    if m:
        return {"coins": int(m.group(1))}
    m = BRACKET_COST_RE.match(c)
    if not m:
        raise ValueError(f"Unrecognized cost format: {cost!r}")
    out: dict[str, int] = {}
    for token in m.group(1).split(","):
        res = map_resource(token)
        out[res] = out.get(res, 0) + 1
    return out


def parse_chains(chains: str) -> tuple[Any, list[str]]:
    c = chains.strip()
    if not c or c.lower() == "none":
        return None, []
    from_ids: list[str] = []
    to_ids: list[str] = []
    for part in c.split(","):
        part = part.strip()
        if part.startswith("from "):
            from_ids.append(part[5:].strip())
        elif part.startswith("to "):
            to_ids.append(part[3:].strip())
        else:
            raise ValueError(f"Unrecognized chain segment: {part!r} in {chains!r}")
    if not from_ids:
        chain_from = None
    elif len(from_ids) == 1:
        chain_from = from_ids[0]
    else:
        chain_from = from_ids
    return chain_from, to_ids


def parse_benefit(benefit: str) -> dict[str, Any]:
    b = benefit.strip()
    lower = b.lower()

    if TRADE_MANUF_RE.search(lower):
        return {
            "trade_discount": {
                "direction": None,
                "kind": "manufactured",
                "cost": 1,
            }
        }
    if TRADE_RAW_LEFT_RE.search(lower):
        return {
            "trade_discount": {
                "direction": "left",
                "kind": "raw",
                "cost": 1,
            }
        }
    if TRADE_RAW_RIGHT_RE.search(lower):
        return {
            "trade_discount": {
                "direction": "right",
                "kind": "raw",
                "cost": 1,
            }
        }

    if " or " in lower:
        if "each turn" in lower:
            resources = [map_resource(m.group(1)) for m in PROD_OR_RE.finditer(b)]
            if resources:
                return {"production": {"or": resources}}
        pairs = PROD_OR_AMOUNTS_RE.findall(b)
        if pairs and all(int(amount) == 1 for amount, _ in pairs):
            resources = [map_resource(token) for _, token in pairs]
            if resources:
                return {"production": {"or": resources}}

    m = PROD_FIXED_RE.match(b)
    if m:
        amount = int(m.group(1))
        res = map_resource(m.group(2))
        return {"production": {res: amount}}

    m = POINTS_RE.search(b)
    if m:
        return {"points": int(m.group(1))}

    m = COINS_RE.search(b)
    if m:
        return {"coins": int(m.group(1))}

    m = MILITARY_RE.search(b)
    if m:
        return {"military": int(m.group(1))}

    m = SCIENCE_RE.search(b)
    if m:
        sym = SCIENCE_MAP[m.group(1).lower()]
        return {"science": sym}

    # Guilds, vineyard, lighthouse, etc. — scoring uses card id; keep effect empty.
    return {}


def build_card(
    *,
    name: str,
    card_id: str,
    age: int,
    color: str,
    cost: str,
    benefit: str,
    chains: str,
    thresholds: list[int],
) -> dict[str, Any]:
    color_key = color.strip().lower()
    if color_key not in COLOR_MAP:
        raise ValueError(f"Unknown color {color!r} for {card_id}")
    chain_from, chain_to = parse_chains(chains)
    player_count = thresholds if thresholds else ([3] if color_key == "purple" else [])
    return {
        "id": card_id,
        "name": name,
        "age": age,
        "color": COLOR_MAP[color_key],
        "player_count": player_count,
        "cost": parse_cost(cost),
        "effect": parse_benefit(benefit),
        "chain_from": chain_from,
        "chain_to": chain_to,
    }


def cards_from_source(text: str) -> dict[int, list[dict[str, Any]]]:
    gen = load_cards_txt_parser()
    _, ages, guild_cards = gen.parse_cards_txt(text)

    out: dict[int, list[dict[str, Any]]] = {1: [], 2: [], 3: []}
    age_num = 0
    for header, cards in ages:
        if "AGE 1" in header:
            age_num = 1
        elif "AGE 2" in header:
            age_num = 2
        elif "AGE 3" in header:
            age_num = 3
        else:
            raise ValueError(f"Unknown age header: {header}")
        for body, thresholds in cards:
            name, card_id, color, cost, benefit, chains = parse_card_fields(body)
            out[age_num].append(
                build_card(
                    name=name,
                    card_id=card_id,
                    age=age_num,
                    color=color,
                    cost=cost,
                    benefit=benefit,
                    chains=chains,
                    thresholds=thresholds,
                )
            )

    for body in guild_cards:
        name, card_id, color, cost, benefit, chains = parse_card_fields(body)
        out[3].append(
            build_card(
                name=name,
                card_id=card_id,
                age=3,
                color=color,
                cost=cost,
                benefit=benefit,
                chains=chains,
                thresholds=[],
            )
        )

    return out


def write_json(path: Path, cards: list[dict[str, Any]]) -> None:
    path.write_text(json.dumps(cards, indent=2) + "\n")


def verify_deck_sizes(text: str) -> None:
    gen = load_cards_txt_parser()
    _, ages, _ = gen.parse_cards_txt(text)
    print("Non-guild cards per age (expected 7 * players each):")
    for players in range(3, 8):
        per_age = [
            sum(gen.copies_for_players(thresholds, players) for _, thresholds in cards)
            for _, cards in ages
        ]
        print(f"  {players} players: ages {per_age}, total {sum(per_age)}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, default=DEFAULT_INPUT)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT_DIR)
    parser.add_argument("--verify", action="store_true", help="Print deck size summary only")
    args = parser.parse_args()

    text = args.input.read_text()
    if args.verify:
        verify_deck_sizes(text)
        return 0

    by_age = cards_from_source(text)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for age in (1, 2, 3):
        out_path = args.output_dir / f"age{age}.json"
        write_json(out_path, by_age[age])
        print(f"wrote {out_path} ({len(by_age[age])} cards)")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())