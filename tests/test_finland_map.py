import hashlib
import json
import runpy
import tempfile
import unittest
from pathlib import Path
import sys

import pygame

GAME_DIR = Path(__file__).resolve().parents[1] / "modules" / "Tank-16"
if str(GAME_DIR) not in sys.path:
    sys.path.insert(0, str(GAME_DIR))

from generate_finland_map import (
    ATLAS_PATH, GAME_DIR, bridge_tile, extend_atlas, generate_map,
    project, rail_bridge_tile, train_track_tile, water_tile,
)


class FinlandMapTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.map_path = GAME_DIR / "maps" / "FINLAND.json"
        cls.runtime_document = json.loads(
            cls.map_path.read_text(encoding="utf-8"))
        editor = runpy.run_path(str(GAME_DIR / "map_editor.py"))
        cls.data = editor["load_map"](cls.map_path)

    def test_generated_finland_has_natural_features_and_runtime_tile_ids(self):
        data = self.data
        self.assertEqual((data["width"], data["height"]), (100, 100))
        self.assertEqual(data["tile_size"], 64)
        self.assertEqual(data["tileset"], "tiles_terrain.png")
        self.assertEqual(data["name"], "FINLAND")
        self.assertEqual(len(data["tiles"]), 100)
        terrain_ids = set()
        for row in data["tiles"]:
            self.assertEqual(len(row), 100)
            for tile in row:
                self.assertEqual(len(tile), 2)
                self.assertTrue(all(type(component) is int for component in tile))
                self.assertTrue(0 <= tile[0] < 10 and 0 <= tile[1] < 9)
                tile_id = tile[1] * 10 + tile[0]
                terrain_ids.add(tile_id)
        counts = data["geography"]["tile_counts"]
        self.assertEqual(
            counts["finland_land"] + counts["neighbor_land"] +
            counts["water"], 10_000)
        self.assertGreater(counts["road_tiles"], 0)
        self.assertGreater(counts["rail_tiles"], 0)
        self.assertGreater(counts["road_bridges"], 0)
        self.assertGreater(counts["rail_bridges"], 0)
        self.assertTrue(any(40 <= tile_id <= 59 for tile_id in terrain_ids))
        self.assertTrue(any(60 <= tile_id <= 69 for tile_id in terrain_ids))
        self.assertTrue(any(70 <= tile_id <= 85 for tile_id in terrain_ids))
        self.assertGreater(data["geography"]["road_features"], 0)
        self.assertIn("ne_10m_roads.geojson", data["geography"]["roads_source"])
        self.assertGreater(data["geography"]["railway_features"], 0)
        self.assertIn(
            "ne_10m_railroads.geojson",
            data["geography"]["railways_source"])
        self.assertGreater(len(data["geography"]["cities"]), 0)
        self.assertGreater(
            sum(cell is not None for row in data["objects"] for cell in row),
            0)
        self.assertGreater(
            sum(cell is not None for row in data["obstacles"] for cell in row),
            0)
        self.assertGreater(counts["finland_land"], 1_000)
        self.assertGreater(counts["water"], 1_000)

    def test_runtime_json_contains_only_referenced_tile_indices(self):
        document = self.runtime_document
        self.assertEqual(document["runtime_encoding"], "flat-v1")
        self.assertLess(self.map_path.stat().st_size, 65_535)
        disk_map = GAME_DIR / "disks" / "maps" / self.map_path.name
        self.assertEqual(disk_map.read_bytes(), self.map_path.read_bytes())
        expected_terrain = {
            tile_y * 10 + tile_x
            for row in self.data["tiles"]
            for tile_x, tile_y in row
        }
        self.assertEqual(set(document["tiles"]), expected_terrain)
        expected_objects = set()
        for layer in ("obstacles", "objects"):
            for row in self.data[layer]:
                for cell in row:
                    if cell is not None:
                        tile = cell if layer == "obstacles" else cell["tile"]
                        expected_objects.add(tile[1] * 6 + tile[0])
        stored_objects = {
            entry[1]
            for layer in ("obstacles", "objects")
            for entry in document[layer]
        }
        self.assertEqual(stored_objects, expected_objects)

    def test_land_cities_and_real_lakes_are_at_correct_projected_locations(self):
        bounds = self.data["geography"]["projected_bounds_km"]
        span = bounds[2] - bounds[0]
        for longitude, latitude, water in (
            (24.938, 60.17, False), (23.76, 61.5, False),
            (25.73, 66.5, False), (27.03, 69.91, False),
            (27.7, 68.95, True), (25.45, 61.5, True), (21, 64, True),
        ):
            with self.subTest(longitude=longitude, latitude=latitude):
                x, y = project(longitude, latitude)
                column = round((x - bounds[0]) / span * 99)
                row = round((bounds[3] - y) / span * 99)
                nearby_water = [
                    40 <= (
                        self.data["tiles"][nearby_row][nearby_column][1] * 10
                        + self.data["tiles"][nearby_row][nearby_column][0]
                    ) <= 69
                    for nearby_row in range(max(0, row - 1), min(100, row + 2))
                    for nearby_column in range(
                        max(0, column - 1), min(100, column + 2))
                ]
                self.assertIn(water, nearby_water)
        self.assertTrue(self.data["geography"]["north_up"])

    def test_finland_opens_in_editor_and_retains_source_metadata(self):
        editor = runpy.run_path(str(GAME_DIR / "map_editor.py"))
        loaded = editor["load_map"](GAME_DIR / "maps" / "FINLAND.json")
        self.assertEqual(loaded["geography"], self.data["geography"])
        self.assertEqual(loaded["tiles"][-1], self.data["tiles"][-1])
        self.assertEqual(len(loaded["obstacles"]), 100)
        self.assertEqual(len(loaded["objects"][-1]), 100)
        self.assertIsNone(loaded["objects"][-1][-1])
        with tempfile.TemporaryDirectory() as directory:
            saved = editor["save_map"](loaded, Path(directory))
            self.assertLess(saved.stat().st_size, 65_535)
            restored = editor["load_map"](saved)
        self.assertEqual(restored, loaded)

    def test_finland_uploads_all_ten_thousand_tiles_to_gpu(self):
        from c_compiler import GLOBAL_BASE, compile_source
        from gfx_harness import make_cpu, run_frame

        expected = bytes(tile[1] * 10 + tile[0]
                         for row in self.data["tiles"] for tile in row)
        page_size = 32768
        pages = "".join(
            f"unsigned char map_data_{page}["
            f"{min(page_size, len(expected) - offset)}];\n"
            for page, offset in enumerate(range(0, len(expected), page_size))
        )
        source = (
            '#include "suomi_gfx.h"\n'
            + pages
            + "int main(void) {\n"
            + "  gfx_layer_select(LAYER_TILES);\n"
            + "  gfx_tilemap(map_data_0, 100, 100, 0, 0);\n"
            + "  gfx_present();\n"
            + "  return 0;\n"
            + "}\n"
        )
        cpu = make_cpu(compile_source(source, str(GAME_DIR / "main.c")))
        cpu.memory[GLOBAL_BASE:GLOBAL_BASE + len(expected)] = expected
        run_frame(cpu)
        self.assertTrue(cpu.frame_yield)
        self.assertEqual((cpu.tilemap_width, cpu.tilemap_height), (100, 100))
        self.assertEqual(len(cpu.tilemap_memory), 10_000)
        self.assertEqual(cpu.tilemap_memory, expected)

    def test_water_atlas_preserves_original_pixels_and_64_pixel_grid(self):
        atlas = pygame.image.load(str(ATLAS_PATH))
        self.assertEqual(atlas.get_size(), (640, 576))
        original = pygame.image.tobytes(atlas.subsurface((0, 0, 640, 256)), "RGBA")
        self.assertEqual(hashlib.sha256(original).hexdigest(),
                         "bf26f8100fab109cbfda6d152a86b8777cf0a4e5b278692830d1ff5ddda5421c")
        for index in range(20):
            tile_id = 40 + index
            tile = atlas.subsurface(((tile_id % 10) * 64, (tile_id // 10) * 64, 64, 64))
            self.assertEqual(pygame.image.tobytes(tile, "RGBA"),
                             pygame.image.tobytes(water_tile(
                                 index if index < 16 else 0, max(0, index - 15)), "RGBA"))
            self.assertEqual(tile.get_at((32, 32)).a, 255)
            self.assertGreater(tile.get_at((32, 32)).b, tile.get_at((32, 32)).r)
        shoreline_depths = []
        for x in range(4, 61, 4):
            shoreline_depths.append(next(
                y for y in range(16)
                if atlas.get_at((64 + x, 256 + y)).b > 140))
        self.assertGreater(max(shoreline_depths) - min(shoreline_depths), 3)
        bridge_specs = (
            ("horizontal", "wood", False, 0),
            ("vertical", "wood", False, 0),
            ("horizontal", "wood", True, 0),
            ("vertical", "wood", True, 0),
            ("horizontal", "stone", False, 0),
            ("vertical", "stone", False, 0),
            ("horizontal", "stone", True, 0),
            ("vertical", "stone", True, 0),
            ("horizontal", "wood", False, 1),
            ("vertical", "wood", False, 1),
        )
        for index, specification in enumerate(bridge_specs):
            bridge_id = 60 + index
            tile = atlas.subsurface((
                (bridge_id % 10) * 64, (bridge_id // 10) * 64, 64, 64))
            self.assertEqual(
                pygame.image.tobytes(tile, "RGBA"),
                pygame.image.tobytes(bridge_tile(*specification), "RGBA"))
            self.assertTrue(all(
                tile.get_at((x, y)).a == 255
                for x, y in ((0, 0), (32, 32), (63, 63))))
        for connections in range(16):
            tile_id = 70 + connections
            tile = atlas.subsurface((
                (tile_id % 10) * 64, (tile_id // 10) * 64, 64, 64))
            self.assertEqual(
                pygame.image.tobytes(tile, "RGBA"),
                pygame.image.tobytes(train_track_tile(connections), "RGBA"))
            self.assertEqual(tile.get_at((32, 32)).a, 255)
        for index, specification in enumerate((
                ("horizontal", "wood"), ("vertical", "wood"),
                ("horizontal", "stone"), ("vertical", "stone"))):
            tile_id = 86 + index
            tile = atlas.subsurface((
                (tile_id % 10) * 64, (tile_id // 10) * 64, 64, 64))
            self.assertEqual(
                pygame.image.tobytes(tile, "RGBA"),
                pygame.image.tobytes(rail_bridge_tile(*specification), "RGBA"))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tiles_test.png"
            pygame.image.save(atlas.subsurface((0, 0, 640, 256)), str(path))
            extend_atlas(path)
            generated = path.read_bytes()
            extend_atlas(path)
            self.assertEqual(path.read_bytes(), generated)
            extended = pygame.image.load(str(path))
            self.assertEqual(extended.get_size(), (640, 576))

    def test_generator_rasterizes_polygons_holes_and_lakes_without_network(self):
        countries = {"features": [{
            "properties": {"ADMIN": "Finland"},
            "geometry": {"type": "Polygon", "coordinates": [
                [[22, 60], [30, 60], [30, 69], [22, 69], [22, 60]],
                [[25, 63], [27, 63], [27, 65], [25, 65], [25, 63]],
            ]},
        }]}
        lakes = {"features": [{
            "geometry": {"type": "Polygon", "coordinates": [
                [[23, 66], [24, 66], [24, 67], [23, 67], [23, 66]],
            ]},
        }]}
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            countries_path = folder / "countries.json"
            lakes_path = folder / "lakes.json"
            roads_path = folder / "roads.json"
            railways_path = folder / "railways.json"
            cities_path = folder / "cities.json"
            output = folder / "map.json"
            countries_path.write_text(json.dumps(countries), encoding="utf-8")
            lakes_path.write_text(json.dumps(lakes), encoding="utf-8")
            roads_path.write_text(json.dumps({"features": [{
                "geometry": {"type": "LineString", "coordinates": [
                    [22, 62], [30, 62],
                ]},
            }]}), encoding="utf-8")
            railways_path.write_text(json.dumps({"features": [{
                "geometry": {"type": "LineString", "coordinates": [
                    [23, 61], [29, 68],
                ]},
            }]}), encoding="utf-8")
            cities_path.write_text(json.dumps({"features": [{
                "properties": {
                    "NAME_EN": "Test City", "SOV0NAME": "Finland",
                    "POP_MAX": 100_000,
                },
                "geometry": {"type": "Point", "coordinates": [24, 64]},
            }]}), encoding="utf-8")
            data = generate_map(
                countries_path, lakes_path, output, 64,
                roads_path=roads_path, railways_path=railways_path,
                cities_path=cities_path)
            editor = runpy.run_path(str(GAME_DIR / "map_editor.py"))
            self.assertEqual(editor["load_map"](output), data)
            counts = data["geography"]["tile_counts"]
            self.assertEqual(
                counts["finland_land"] + counts["neighbor_land"] +
                counts["water"], 4096)
            self.assertGreater(counts["water"], 100)
            self.assertGreater(counts["road_tiles"], 0)
            self.assertGreater(counts["rail_tiles"], 0)
            self.assertEqual(data["geography"]["road_features"], 1)
            self.assertEqual(data["geography"]["railway_features"], 1)
            self.assertEqual(data["geography"]["cities"][0]["name"], "Test City")
            self.assertTrue(any(
                cell is not None
                for row in data["objects"] for cell in row))
            for size in (0, 101):
                with self.assertRaises(ValueError):
                    generate_map(countries_path, lakes_path, output, size)


if __name__ == "__main__":
    unittest.main()
