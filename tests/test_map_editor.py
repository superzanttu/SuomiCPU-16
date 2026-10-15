import runpy
import tempfile
import unittest
from pathlib import Path


EDITOR_PATH = (
    Path(__file__).resolve().parents[1]
    / "modules"
    / "Tank-16"
    / "map_editor.py"
)
EDITOR = runpy.run_path(str(EDITOR_PATH))
load_map = EDITOR["load_map"]
new_map = EDITOR["new_map"]
save_map = EDITOR["save_map"]
set_map_tile = EDITOR["set_map_tile"]
validate_map = EDITOR["validate_map"]


class MapEditorDataTests(unittest.TestCase):
    def test_creates_and_paints_map_with_xy_tiles(self):
        map_data = new_map("test", 3, 2)
        set_map_tile(map_data, 2, 1, 4, 2, 10, 4)

        self.assertEqual(map_data["name"], "TEST")
        self.assertEqual((map_data["width"], map_data["height"]), (3, 2))
        self.assertEqual(map_data["tiles"][1][2], [4, 2])
        self.assertEqual(map_data["tiles"][0][0], [0, 0])

    def test_rejects_invalid_name_size_and_tile_coordinates(self):
        with self.assertRaises(ValueError):
            new_map("../bad", 2, 2)
        with self.assertRaises(ValueError):
            new_map("TOOLONGNAME", 2, 2)
        with self.assertRaises(ValueError):
            new_map("MAP", 0, 2)

        map_data = new_map("MAP", 2, 2)
        with self.assertRaises(ValueError):
            set_map_tile(map_data, 2, 0, 0, 0, 10, 4)
        with self.assertRaises(ValueError):
            set_map_tile(map_data, 0, 0, 10, 0, 10, 4)

    def test_saves_and_loads_json_maps(self):
        map_data = new_map("arena", 2, 3)
        set_map_tile(map_data, 1, 2, 5, 3, 10, 4)
        with tempfile.TemporaryDirectory() as directory:
            path = save_map(map_data, Path(directory))
            loaded = load_map(path, 10, 4)

        self.assertEqual(path.name, "ARENA.json")
        self.assertEqual(loaded, map_data)
        self.assertEqual(loaded["tileset"], "tiles_terrain.png")

    def test_saves_and_loads_map_with_its_tileset_filename(self):
        map_data = new_map("atlas", 2, 2)
        map_data["tileset"] = "tiles_terrain.png"
        map_data["tiles"][0][0] = [1, 0]
        with tempfile.TemporaryDirectory() as directory:
            path = save_map(map_data, Path(directory))
            loaded = load_map(path)

        self.assertEqual(loaded["tileset"], "tiles_terrain.png")
        self.assertEqual(loaded["tiles"][0][0], [1, 0])

    def test_validates_map_dimensions_and_tile_sheet_coordinates(self):
        map_data = new_map("MAP", 2, 2)
        malformed = dict(map_data)
        malformed["tiles"] = map_data["tiles"][:1]
        with self.assertRaises(ValueError):
            validate_map(malformed, 10, 4)

        malformed = new_map("MAP", 2, 2)
        malformed["tiles"][0][0] = [0, 4]
        with self.assertRaises(ValueError):
            validate_map(malformed, 10, 4)

        malformed = new_map("MAP", 2, 2)
        malformed["tileset"] = "../tiles_terrain.png"
        with self.assertRaises(ValueError):
            validate_map(malformed, 10, 4)


if __name__ == "__main__":
    unittest.main()
