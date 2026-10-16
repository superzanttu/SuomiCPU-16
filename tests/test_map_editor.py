import runpy
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import pygame


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
    def test_1024_square_map_supports_last_cell_on_every_layer(self):
        data = new_map("BIG", 1024, 1024)
        set_map_tile(data, 1023, 1023, 0, 4, 10, 6)
        set_layer_tile(data, "obstacles", 1023, 1023, (0, 0), 7, 7)
        set_layer_tile(data, "objects", 1023, 1023, (1, 0), 7, 7, 20, "fixed")
        loaded = validate_map(data, 10, 6)
        self.assertEqual((loaded["width"], loaded["height"]), (1024, 1024))
        self.assertEqual(loaded["tiles"][1023][1023], [0, 4])
        self.assertEqual(loaded["obstacles"][1023][1023], [0, 0])
        self.assertEqual(loaded["objects"][1023][1023]["hp_change"], 20)
        self.assertEqual(loaded["tiles"][0][0], [0, 0])

    def test_1024_dimensions_round_trip_and_1025_is_rejected(self):
        for width, height in ((1024, 1), (1, 1024)):
            with self.subTest(width=width, height=height):
                data = new_map("EDGE", width, height)
                with tempfile.TemporaryDirectory() as directory:
                    self.assertEqual(load_map(save_map(data, Path(directory))), data)
        for width, height in ((1025, 1), (1, 1025), (True, 1), (1, True)):
            with self.subTest(width=width, height=height), self.assertRaises(ValueError):
                new_map("BAD", width, height)
        for dimension in ("width", "height"):
            data = new_map("BAD", 1, 1)
            data[dimension] = 1025
            with self.assertRaises(ValueError):
                validate_map(data, 10, 6)

    def test_overview_is_cached_and_updates_after_editing_any_layer(self):
        data = new_map("SMALL", 8, 8)
        terrain = pygame.Surface((128, 64))
        terrain.fill((20, 180, 60), (0, 0, 64, 64))
        terrain.fill((20, 90, 180), (64, 0, 64, 64))
        objects = pygame.Surface((64, 64), pygame.SRCALPHA)
        objects.fill((250, 10, 10))
        overview = EDITOR["_map_overview"](data, terrain, objects)
        self.assertIs(EDITOR["_map_overview"](data, terrain, objects), overview)
        set_map_tile(data, 7, 7, 1, 0, 2, 1)
        updated = EDITOR["_map_overview"](data, terrain, objects)
        self.assertIsNot(updated, overview)
        self.assertEqual(tuple(updated.get_at((7, 7)))[:3], (20, 90, 180))
        set_layer_tile(data, "objects", 7, 7, (0, 0), 1, 1)
        updated = EDITOR["_map_overview"](data, terrain, objects)
        self.assertEqual(tuple(updated.get_at((7, 7)))[:3], (250, 10, 10))
        set_layer_tile(data, "objects", 7, 7, None, 1, 1)
        updated = EDITOR["_map_overview"](data, terrain, objects)
        self.assertEqual(tuple(updated.get_at((7, 7)))[:3], (20, 90, 180))
        window = pygame.Surface((20, 20))
        clip = window.get_clip()
        EDITOR["_draw_map"](window, data, terrain, pygame.Rect(4, 4, 8, 8),
                            [0, 0], 0.5, objects)
        self.assertEqual(window.get_clip(), clip)
        self.assertNotEqual(window.get_at((4, 4)), window.get_at((0, 0)))

    def test_layer_shortcuts_paint_the_selected_layer_without_changing_zoom(self):
        buttons = EDITOR["_button_rects"]()
        cell_position = (180 + 2 * 64 + 32, 40 + 2 * 64 + 32)
        list_top = buttons["REMOVE"].bottom + 14

        def click(position):
            return pygame.event.Event(
                pygame.MOUSEBUTTONDOWN, pos=position, button=1)

        def key(keycode):
            return pygame.event.Event(
                pygame.KEYDOWN, key=keycode, unicode="")

        events = [
            [key(pygame.K_g), click((90, list_top + 89 + 32)),
             click(cell_position)],
            [key(pygame.K_o), click(cell_position)],
            [key(pygame.K_b), click(cell_position)],
            [click(buttons["SAVE"].center)],
            [pygame.event.Event(pygame.QUIT)],
        ]
        with tempfile.TemporaryDirectory() as directory:
            with patch.dict(os.environ, {"SDL_VIDEODRIVER": "dummy"}), \
                    patch.dict(EDITOR["main"].__globals__, {
                        "MAPS_DIR": Path(directory),
                        "save_map": lambda data: save_map(data, Path(directory)),
                    }), \
                    patch("pygame.event.get", side_effect=events):
                self.assertEqual(EDITOR["main"](), 0)
            loaded = load_map(Path(directory) / "UNTITLED.json")
        self.assertEqual(loaded["tiles"][2][2], [1, 0])
        self.assertEqual(loaded["obstacles"][2][2], [0, 0])
        self.assertEqual(loaded["objects"][2][2], {
            "tile": [0, 0], "hp_change": -25, "removal": "explosion",
        })
        self.assertEqual(loaded["tiles"][1][1], [0, 0])

    def test_editor_buttons_paint_properties_erase_and_save_layers(self):
        buttons = EDITOR["_button_rects"]()

        def click(position, button=1):
            return pygame.event.Event(
                pygame.MOUSEBUTTONDOWN, pos=position, button=button)

        def key(character, keycode=0):
            return pygame.event.Event(
                pygame.KEYDOWN, key=keycode, unicode=character)

        obstacle_position = (180 + 64 + 32, 40 + 64 + 32)
        object_position = (180 + 3 * 64 + 32, 40 + 2 * 64 + 32)
        events = [
            [click(buttons["LAYER"].center), click(obstacle_position)],
            [click(buttons["LAYER"].center)],
            [click(buttons["HP"].center)],
            [key("+"), key("4"), key("0"), key("", pygame.K_RETURN)],
            [click(buttons["REMOVE"].center), click(object_position)],
            [click(object_position, 2)],
            [click(object_position, 3)],
            [click(object_position)],
            [click(buttons["SAVE"].center)],
            [pygame.event.Event(pygame.QUIT)],
        ]
        with tempfile.TemporaryDirectory() as directory:
            with patch.dict(os.environ, {"SDL_VIDEODRIVER": "dummy"}), \
                    patch.dict(EDITOR["main"].__globals__, {
                        "MAPS_DIR": Path(directory),
                        "save_map": lambda data: save_map(data, Path(directory)),
                    }), \
                    patch("pygame.event.get", side_effect=events):
                self.assertEqual(EDITOR["main"](), 0)
            loaded = load_map(Path(directory) / "UNTITLED.json")
        self.assertEqual(loaded["obstacles"][1][1], [0, 0])
        self.assertEqual(loaded["objects"][2][3], {
            "tile": [0, 0], "hp_change": 40, "removal": "disappear",
        })

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
            loaded = load_map(path, 10, 6)

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
