import runpy
import json
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
set_layer_tile = EDITOR["set_layer_tile"]
validate_map = EDITOR["validate_map"]


class MapEditorDataTests(unittest.TestCase):
    def test_layer_properties_round_trip_and_erase_without_changing_terrain(self):
        map_data = new_map("LAYERS", 3, 2)
        set_layer_tile(map_data, "obstacles", 0, 1, (2, 3), 7, 7)
        set_layer_tile(map_data, "objects", 1, 1, (1, 2), 7, 7,
                       35, "disappear")
        set_layer_tile(map_data, "objects", 2, 1, (0, 0), 7, 7,
                       -80, "explosion")
        with tempfile.TemporaryDirectory() as directory:
            loaded = load_map(save_map(map_data, Path(directory)))
        self.assertEqual(loaded, map_data)
        self.assertEqual(loaded["objects"][1][1]["hp_change"], 35)
        self.assertEqual(loaded["objects_tileset"], "tiles_objects.png")
        set_layer_tile(loaded, "objects", 2, 1, None, 7, 7)
        self.assertIsNone(loaded["objects"][1][2])
        self.assertEqual(loaded["tiles"][1][2], [0, 0])
        self.assertEqual(loaded["obstacles"][1][0], [2, 3])

    def test_legacy_maps_load_with_empty_layers(self):
        map_data = new_map("OLD", 2, 2)
        for field in ("objects_tileset", "obstacles", "objects"):
            del map_data[field]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "OLD.json"
            path.write_text(json.dumps(map_data), encoding="utf-8")
            loaded = load_map(path)
        self.assertEqual(loaded["obstacles"], [[None, None], [None, None]])
        self.assertEqual(loaded["objects"], [[None, None], [None, None]])

    def test_rejects_invalid_layer_properties_and_shapes(self):
        for change in (101, -101, True, "25"):
            with self.subTest(change=change), self.assertRaises(ValueError):
                set_layer_tile(new_map("BAD", 2, 2), "objects", 0, 0,
                               (0, 0), 7, 7, change, "fixed")
        for field, value in (("hp_change", None), ("removal", "respawn"),
                             ("tile", [99, 0])):
            data = new_map("BAD", 2, 2)
            data["objects"][0][0] = {
                "tile": [0, 0], "hp_change": -25, "removal": "explosion",
                field: value,
            }
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_map(data, 10, 4)
        data = new_map("BAD", 2, 2)
        data["obstacles"] = [[None]]
        with self.assertRaises(ValueError):
            validate_map(data, 10, 4)

    def test_object_atlas_padding_keeps_fixed_64_pixel_cells(self):
        tileset, columns, rows = EDITOR["load_tileset"](
            EDITOR["OBJECT_TILESET_PATH"], pad_edges=True)
        self.assertEqual(tileset.get_size(), (columns * 64, rows * 64))
        self.assertEqual(tileset.get_at((columns * 64 - 1, rows * 64 - 1)).a, 0)

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
