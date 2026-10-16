import hashlib
import json
import runpy
import tempfile
import unittest
from pathlib import Path

import pygame

from tools.generate_finland_map import (
    ATLAS_PATH, GAME_DIR, extend_atlas, generate_map, project, water_tile,
)


class FinlandMapTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = json.loads((GAME_DIR / "maps" / "suomi.json").read_text(encoding="utf-8"))

    def test_generated_finland_has_exact_million_cells_and_valid_water_tiles(self):
        data = self.data
        self.assertEqual((data["width"], data["height"]), (1000, 1000))
        self.assertEqual(data["tile_size"], 64)
        self.assertEqual(data["tileset"], "tiles_terrain.png")
        self.assertEqual(data["name"], "SUOMI")
        self.assertEqual(len(data["tiles"]), 1000)
        counts = {"finland_land": 0, "neighbor_land": 0, "water": 0}
        for row in data["tiles"]:
            self.assertEqual(len(row), 1000)
            for tile in row:
                self.assertEqual(len(tile), 2)
                self.assertTrue(all(type(component) is int for component in tile))
                self.assertTrue(0 <= tile[0] < 10 and 0 <= tile[1] < 6)
                if tile == [0, 0]:
                    counts["finland_land"] += 1
                elif tile == [0, 2]:
                    counts["neighbor_land"] += 1
                else:
                    self.assertTrue(40 <= tile[1] * 10 + tile[0] < 56)
                    counts["water"] += 1
        self.assertEqual(sum(counts.values()), 1_000_000)
        self.assertEqual(counts, data["geography"]["tile_counts"])
        self.assertGreater(counts["finland_land"], 200_000)
        self.assertGreater(counts["water"], 200_000)

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
                column = round((x - bounds[0]) / span * 999)
                row = round((bounds[3] - y) / span * 999)
                tile = self.data["tiles"][row][column]
                self.assertEqual(tile[1] >= 4, water)
        self.assertTrue(self.data["geography"]["north_up"])

    def test_finland_opens_in_editor_and_retains_source_metadata(self):
        editor = runpy.run_path(str(GAME_DIR / "map_editor.py"))
        loaded = editor["load_map"](GAME_DIR / "maps" / "suomi.json")
        self.assertEqual(loaded["geography"], self.data["geography"])
        self.assertEqual(loaded["tiles"][-1], self.data["tiles"][-1])
        self.assertEqual(len(loaded["obstacles"]), 1000)
        self.assertEqual(len(loaded["objects"][-1]), 1000)
        self.assertIsNone(loaded["objects"][-1][-1])
        with tempfile.TemporaryDirectory() as directory:
            saved = editor["save_map"](loaded, Path(directory))
            self.assertLess(saved.stat().st_size, 17_000_000)
            restored = editor["load_map"](saved)
        self.assertEqual(restored, loaded)

    def test_finland_uploads_all_million_tiles_to_gpu(self):
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
            + "  gfx_tilemap(map_data_0, 1000, 1000, 0, 0);\n"
            + "  gfx_present();\n"
            + "  return 0;\n"
            + "}\n"
        )
        cpu = make_cpu(compile_source(source, str(GAME_DIR / "main.c")))
        cpu.memory[GLOBAL_BASE:GLOBAL_BASE + len(expected)] = expected
        run_frame(cpu)
        self.assertTrue(cpu.frame_yield)
        self.assertEqual((cpu.tilemap_width, cpu.tilemap_height), (1000, 1000))
        self.assertEqual(len(cpu.tilemap_memory), 1_000_000)
        self.assertEqual(cpu.tilemap_memory, expected)

    def test_water_atlas_preserves_original_pixels_and_64_pixel_grid(self):
        atlas = pygame.image.load(str(ATLAS_PATH))
        self.assertEqual(atlas.get_size(), (640, 384))
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
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tiles_test.png"
            pygame.image.save(atlas.subsurface((0, 0, 640, 256)), str(path))
            extend_atlas(path)
            generated = path.read_bytes()
            extend_atlas(path)
            self.assertEqual(path.read_bytes(), generated)

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
            output = folder / "map.json"
            countries_path.write_text(json.dumps(countries), encoding="utf-8")
            lakes_path.write_text(json.dumps(lakes), encoding="utf-8")
            data = generate_map(countries_path, lakes_path, output, 64)
            self.assertEqual(json.loads(output.read_text()), data)
            self.assertEqual(sum(data["geography"]["tile_counts"].values()), 4096)
            self.assertGreater(data["geography"]["tile_counts"]["water"], 100)
            for size in (0, 1025):
                with self.assertRaises(ValueError):
                    generate_map(countries_path, lakes_path, output, size)


if __name__ == "__main__":
    unittest.main()
