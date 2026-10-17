import runpy
import tempfile
import unittest
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
GAME_DIR = ROOT / "modules" / "Tank-16"
if str(GAME_DIR) not in sys.path:
    sys.path.insert(0, str(GAME_DIR))

from generate_natural_maps import MAPS, generate_maps


EDITOR_PATH = GAME_DIR / "map_editor.py"


class NaturalMapTests(unittest.TestCase):
    def test_generated_maps_are_installed_in_runtime_folder(self):
        editor = runpy.run_path(str(EDITOR_PATH))
        runtime_dir = GAME_DIR / "disks" / "maps"
        for name, _, _ in MAPS:
            runtime_path = runtime_dir / f"{name}.json"
            self.assertTrue(runtime_path.is_file())
            self.assertEqual(editor["load_map"](runtime_path)["name"], name)

    def test_generates_ten_distinct_runtime_ready_natural_maps(self):
        editor = runpy.run_path(str(EDITOR_PATH))
        with tempfile.TemporaryDirectory() as directory:
            runtime_dir = Path(directory) / "disks" / "maps"
            generated = generate_maps(runtime_dir)

            self.assertEqual(len(generated), 10)
            self.assertEqual(len(list(runtime_dir.glob("*.json"))), 10)
            terrain_layouts = set()
            for name, _, _ in MAPS:
                runtime_path = runtime_dir / f"{name}.json"
                map_data = editor["load_map"](runtime_path)
                self.assertEqual(map_data["name"], name)
                self.assertEqual((map_data["width"], map_data["height"]),
                                 (48, 32))
                self.assertEqual(map_data["tileset"], "tiles_terrain.png")
                self.assertEqual(map_data["objects_tileset"], "tiles_objects.png")
                terrain_layouts.add(tuple(
                    tile_id
                    for row in map_data["tiles"]
                    for x, y in row
                    for tile_id in (y * 10 + x,)
                ))
                counts = map_data["geography"]["tile_counts"]
                self.assertGreater(counts["water"], 0)
                self.assertGreater(counts["roads"], 0)
                self.assertGreater(counts["railways"], 0)
                self.assertGreater(counts["road_bridges"], 0)
                self.assertGreater(counts["rail_bridges"], 0)
                self.assertGreater(counts["trees_and_rocks"], 0)
                self.assertGreater(counts["city_tiles"], 0)
            self.assertEqual(len(terrain_layouts), 10)


if __name__ == "__main__":
    unittest.main()
