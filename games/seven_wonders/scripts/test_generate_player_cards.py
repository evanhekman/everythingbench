#!/usr/bin/env python3
"""Tests for generate_player_cards.py (run: python3 -m unittest games.seven_wonders.scripts.test_generate_player_cards)."""

import importlib.util
import re
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "generate_player_cards.py"
SCRIPT_DIR = Path(__file__).resolve().parent
CARDS_TXT = SCRIPT_DIR / "cards.txt"
PROMPTS = SCRIPT_DIR.parent / "prompts"


def load_module():
    spec = importlib.util.spec_from_file_location("generate_player_cards", SCRIPT)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


gen = load_module()
CARD_LINE_RE = re.compile(r"^(?P<body>.+)\|\s*(?P<count>\d+)\s*$")


class TestThresholdLogic(unittest.TestCase):
    def test_single_threshold(self):
        self.assertEqual(gen.copies_for_players([3], 3), 1)
        self.assertEqual(gen.copies_for_players([3], 4), 1)
        self.assertEqual(gen.copies_for_players([4], 3), 0)

    def test_multi_threshold(self):
        self.assertEqual(gen.copies_for_players([4, 6], 4), 1)
        self.assertEqual(gen.copies_for_players([4, 6], 5), 1)
        self.assertEqual(gen.copies_for_players([4, 6], 6), 2)

    def test_triple_threshold(self):
        self.assertEqual(gen.copies_for_players([3, 6, 7], 5), 1)
        self.assertEqual(gen.copies_for_players([3, 6, 7], 6), 2)
        self.assertEqual(gen.copies_for_players([3, 6, 7], 7), 3)


class TestGeneratedFiles(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = CARDS_TXT.read_text()
        cls.preamble, cls.ages, cls.guild_cards = gen.parse_cards_txt(cls.text)

    def _age_totals(self, players: int) -> list[int]:
        return [
            sum(gen.copies_for_players(thresholds, players) for _, thresholds in cards)
            for _, cards in self.ages
        ]

    def test_age1_and_age2_deck_sizes(self):
        for players in range(3, 8):
            age1, age2, _ = self._age_totals(players)
            expected = 7 * players
            self.assertEqual(age1, expected, f"age1 @ {players}p")
            self.assertEqual(age2, expected, f"age2 @ {players}p")

    def test_ludus_excluded_for_four_players(self):
        cards4 = (PROMPTS / "cards4.txt").read_text()
        self.assertNotIn("Ludus (ludus)", cards4)
        cards5 = (PROMPTS / "cards5.txt").read_text()
        self.assertIn("Ludus (ludus)", cards5)

    def test_tree_farm_only_six_plus(self):
        for n in (3, 4, 5):
            self.assertNotIn("Tree Farm", (PROMPTS / f"cards{n}.txt").read_text())
        self.assertIn("Tree Farm", (PROMPTS / "cards6.txt").read_text())

    def test_generated_files_use_numeric_copies(self):
        for n in range(3, 8):
            content = (PROMPTS / f"cards{n}.txt").read_text()
            self.assertIn(f"({n} players)", content)
            for line in content.splitlines():
                if line.startswith("###") or not line.strip() or line.startswith("#"):
                    continue
                if (
                    "Format:" in line
                    or line.startswith("There are 10 guilds")
                    or line.startswith("Card Name (card_id)")
                ):
                    continue
                self.assertRegex(line, r"\| \d+\s*$", msg=f"bad line in cards{n}.txt: {line}")

    def test_all_guilds_listed_in_each_file(self):
        for n in range(3, 8):
            content = (PROMPTS / f"cards{n}.txt").read_text()
            for body in self.guild_cards:
                card_name = body.split("(")[0].strip()
                self.assertIn(card_name, content)


if __name__ == "__main__":
    unittest.main()