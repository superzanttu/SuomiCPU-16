import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GAME_DIR = ROOT / "modules" / "Tank-16"


class RustMapEditorTests(unittest.TestCase):
    def run_rust(self, binary, *arguments):
        return subprocess.run(
            ["cargo", "run", "--quiet", "--bin", binary, "--",
             *map(str, arguments)],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
            encoding="utf-8",
        )

    def test_editor_help_describes_rust_workflows(self):
        result = self.run_rust("sc16mapedit", "--help")
        self.assertIn("Paint terrain and object layers", result.stdout)

    def test_map_load_save_round_trip_uses_rust_validation(self):
        source = GAME_DIR / "disks" / "maps" / "FINLAND.json"
        loaded = json.loads(self.run_rust("sc16tank", "load-map", source).stdout)
        self.assertEqual((loaded["width"], loaded["height"]), (100, 100))
        self.assertEqual(loaded["name"], "FINLAND")
        with tempfile.TemporaryDirectory() as directory:
            destination = self.run_rust(
                "sc16tank", "save-map", source, "--output-dir", directory
            ).stdout.strip()
            restored = json.loads(
                self.run_rust("sc16tank", "load-map", destination).stdout
            )
        self.assertEqual(restored, loaded)
