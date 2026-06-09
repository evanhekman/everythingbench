#!/usr/bin/env python3
"""Tests for generate_card_json.py."""

import importlib.util
import json
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "generate_card_json.py"
CARDS_TXT = Path(__file__).resolve().parent / "cards.txt"
DATA_DIR = Path(__file__).resolve().parent.parent / "data" / "cards"


def load_module():
    spec = importlib.util.spec_from_file_location("generate_card_json", SCRIPT)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


gen = load_module()


class TestParsing(unittest.TestCase):
    def test_clay_pit_combo_production(self):
        effect = gen.parse_benefit("+1 ore or +1 brick")
        self.assertEqual(effect, {"production": {"or": ["ore", "clay"]}})

    def test_guard_tower_cost_and_military(self):
        card = gen.build_card(
            name="Guard Tower",
            card_id="guard_tower",
            age=1,
            color="Red",
            cost="[brick]",
            benefit="+1 military",
            chains="None",
            thresholds=[3, 4],
        )
        self.assertEqual(card["cost"], {"clay": 1})
        self.assertEqual(card["effect"], {"military": 1})

    def test_marketplace_trade_discount(self):
        benefit = (
            "buy [paper, glass, cloth] from either neighbor for 1 coin instead of 2"
        )
        effect = gen.parse_benefit(benefit)
        self.assertEqual(
            effect["trade_discount"],
            {"direction": None, "kind": "manufactured", "cost": 1},
        )


class TestGeneratedJson(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        text = CARDS_TXT.read_text()
        cls.by_age = gen.cards_from_source(text)

    def test_guard_tower_in_age1(self):
        gt = next(c for c in self.by_age[1] if c["id"] == "guard_tower")
        self.assertEqual(gt["cost"], {"clay": 1})

    def test_all_ages_written_match_parser(self):
        for age in (1, 2, 3):
            on_disk = json.loads((DATA_DIR / f"age{age}.json").read_text())
            self.assertEqual(len(on_disk), len(self.by_age[age]))


if __name__ == "__main__":
    unittest.main()