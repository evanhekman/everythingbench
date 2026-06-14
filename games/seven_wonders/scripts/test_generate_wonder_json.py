#!/usr/bin/env python3
"""Tests for generate_wonder_json.py."""

import json
import unittest
from pathlib import Path

from games.seven_wonders.scripts.generate_wonder_json import parse_wonders_txt

SCRIPT_DIR = Path(__file__).resolve().parent
WONDERS_TXT = SCRIPT_DIR / "wonders.txt"
WONDERS_JSON = SCRIPT_DIR.parent / "data" / "wonders.json"


class TestGenerateWonderJson(unittest.TestCase):
    def test_parses_all_fourteen_boards(self):
        boards = parse_wonders_txt(WONDERS_TXT)
        self.assertEqual(len(boards), 14)

    def test_gizah_day_matches_engine(self):
        boards = parse_wonders_txt(WONDERS_TXT)
        gizah = next(b for b in boards if b["id"] == "gizah_day")
        self.assertEqual(len(gizah["stages"]), 3)
        self.assertEqual(gizah["stages"][0]["vp"], 3)
        self.assertEqual(gizah["stages"][0]["cost"], ["wood", "wood"])
        self.assertEqual(gizah["stages"][2]["vp"], 7)

    def test_json_file_exists_and_matches_txt(self):
        boards = parse_wonders_txt(WONDERS_TXT)
        data = json.loads(WONDERS_JSON.read_text())
        self.assertEqual(data["boards"], boards)

    def test_halikarnassos_stage_two_uses_cloth_not_papyrus(self):
        boards = parse_wonders_txt(WONDERS_TXT)
        for board_id in ("halikarnassos_day", "halikarnassos_night"):
            board = next(b for b in boards if b["id"] == board_id)
            stage2 = board["stages"][1]
            self.assertEqual(stage2["cost"], ["glass", "loom"])


if __name__ == "__main__":
    unittest.main()