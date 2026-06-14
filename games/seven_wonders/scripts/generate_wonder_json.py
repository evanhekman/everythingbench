#!/usr/bin/env python3
"""Regenerate data/wonders.json from scripts/wonders.txt."""

from __future__ import annotations

import json
import re
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
INPUT = SCRIPT_DIR / "wonders.txt"
OUTPUT = SCRIPT_DIR.parent / "data" / "wonders.json"

RESOURCE_KEYS = {
    "wood": "wood",
    "stone": "stone",
    "brick": "clay",
    "ore": "ore",
    "glass": "glass",
    "paper": "papyrus",
    "cloth": "loom",
}

HEADER_RE = re.compile(r"^([a-z]+),\s*(\w+)\s*$")
STAGE_RE = re.compile(r"^\[([^\]]+)\]\s+(.+)$")
VP_RE = re.compile(r"\+(\d+)\s+points?", re.I)
COINS_RE = re.compile(r"\+(\d+)\s+coins?", re.I)
MILITARY_RE = re.compile(r"\+?(\d+)\s+military", re.I)


def parse_cost(raw: str) -> list[str]:
    parts = [p.strip().lower() for p in raw.split(",") if p.strip()]
    out: list[str] = []
    for part in parts:
        key = RESOURCE_KEYS.get(part)
        if not key:
            raise ValueError(f"unknown resource in cost: {part!r}")
        out.append(key)
    return out


def parse_effect(text: str) -> str | None:
    t = text.lower()
    if "wood/stone/ore/brick" in t:
        return "produce_raw_choice"
    if "glass/paper/cloth" in t:
        return "produce_manufactured_choice"
    if "tablet/compass/cog" in t:
        return "science_choice"
    if "play a card from the discard" in t:
        return "play_from_discard"
    if "6th round" in t:
        return "sixth_round_extra_play"
    if "first card of each color" in t:
        return "first_per_color_free"
    if "first card you play each age" in t:
        return "first_per_age_free"
    if "last card you play each age" in t:
        return "last_per_age_free"
    return None


def parse_benefit(text: str) -> dict:
    vp = 0
    coins = 0
    military = 0
    for part in text.split(","):
        part = part.strip()
        if m := VP_RE.search(part):
            vp += int(m.group(1))
        if m := COINS_RE.search(part):
            coins += int(m.group(1))
        if m := MILITARY_RE.search(part):
            military += int(m.group(1))
    effect = parse_effect(text)
    return {
        "benefit_text": text.strip(),
        "vp": vp,
        "coins": coins,
        "military": military,
        "effect": effect,
    }


def title_case(name: str) -> str:
    return name.replace("_", " ").title()


def parse_wonders_txt(path: Path) -> list[dict]:
    lines = path.read_text().splitlines()
    boards: list[dict] = []
    wonder: str | None = None
    token: str | None = None
    side: str | None = None
    stages: list[dict] = []

    def flush_side() -> None:
        nonlocal stages, side, wonder, token
        if wonder is None or side is None:
            return
        if not stages:
            raise ValueError(f"{wonder} {side} has no stages")
        board_id = f"{wonder}_{side}"
        boards.append(
            {
                "id": board_id,
                "wonder": wonder,
                "name": title_case(wonder),
                "side": side,
                "token": token,
                "display_name": f"{title_case(wonder)} ({side})",
                "stages": stages,
            }
        )
        stages = []

    for raw in lines:
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if m := HEADER_RE.match(line):
            flush_side()
            wonder = m.group(1)
            token = m.group(2)
            side = None
            continue
        if line in ("day", "night"):
            flush_side()
            side = line
            continue
        if m := STAGE_RE.match(line):
            if side is None:
                raise ValueError(f"stage line before side: {line}")
            cost = parse_cost(m.group(1))
            benefit = parse_benefit(m.group(2))
            stages.append(
                {
                    "stage": len(stages) + 1,
                    "cost": cost,
                    **benefit,
                }
            )
            continue
        raise ValueError(f"unparseable line: {raw!r}")

    flush_side()
    if not boards:
        raise ValueError("no wonder boards parsed")
    return boards


def main() -> None:
    boards = parse_wonders_txt(INPUT)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(json.dumps({"boards": boards}, indent=2) + "\n")
    print(f"Wrote {len(boards)} boards to {OUTPUT}")


if __name__ == "__main__":
    main()