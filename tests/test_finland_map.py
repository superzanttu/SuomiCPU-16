import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import pygame

ROOT = Path(__file__).resolve().parents[1]
GAME_DIR = ROOT / "modules" / "Tank-16"
ATLAS_PATH = GAME_DIR / "gfx" / "tiles_terrain.png"
TOOLS_DIR = ROOT / "tools"
if str(TOOLS_DIR) not in sys.path:
    sys.path.insert(0, str(TOOLS_DIR))
TESTS_DIR = ROOT / "tests"
if str(TESTS_DIR) not in sys.path:
    sys.path.insert(0, str(TESTS_DIR))


class FinlandMapTests(unittest.TestCase):
    def run_rust_tool(self, *arguments, check=True):
        return subprocess.run(
            ["cargo", "run", "--quiet", "--bin", "sc16tank", "--", *map(str, arguments)],
            cwd=ROOT,
            check=check,
            capture_output=True,
            text=True,
            encoding="utf-8",
        )

    def load_map(self, path):
        return json.loads(self.run_rust_tool("load-map", path).stdout)

    def save_map(self, map_data, output_dir):
        source = Path(output_dir) / f"{map_data['name']}.source.json"
        source.write_text(json.dumps(map_data), encoding="utf-8")
        result = self.run_rust_tool(
            "save-map", source, "--output-dir", output_dir)
        return Path(result.stdout.strip())

    @classmethod
    def setUpClass(cls):
        cls.map_path = GAME_DIR / "disks" / "maps" / "FINLAND.json"
        cls.runtime_document = json.loads(
            cls.map_path.read_text(encoding="utf-8"))
        cls.data = json.loads(
            subprocess.run(
                ["cargo", "run", "--quiet", "--bin", "sc16tank", "--",
                 "load-map", str(cls.map_path)],
                cwd=ROOT,
                check=True,
                capture_output=True,
                text=True,
                encoding="utf-8",
            ).stdout
        )

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
                self.assertTrue(0 <= tile[0] < 10 and 0 <= tile[1] < 10)
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
        self.assertIn(90, terrain_ids)
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
        self.assertEqual(document["runtime_encoding"], "grid-hex-v2")
        self.assertLess(self.map_path.stat().st_size, 65_535)
        self.assertFalse((GAME_DIR / "maps").exists())
        expected_terrain = {
            tile_y * 10 + tile_x
            for row in self.data["tiles"]
            for tile_x, tile_y in row
        }
        self.assertEqual(
            {tile for tile, _ in document["used_tiles"]}, expected_terrain)
        self.assertEqual(
            sum(count for _, count in document["used_tiles"]),
            document["width"] * document["height"])
        expected_obstacles = {
            cell[1] * 6 + cell[0]
            for row in self.data["obstacles"] for cell in row
            if cell is not None
        }
        self.assertEqual(
            {tile for tile, _ in document["used_obstacles"]},
            expected_obstacles)
        expected_objects = {
            cell["tile"][1] * 6 + cell["tile"][0]
            for row in self.data["objects"] for cell in row
            if cell is not None
        }
        self.assertEqual(
            {entry[1] for entry in document["objects"]}, expected_objects)
        raw = self.map_path.read_bytes()
        stride = document["width"] * 4 + 4
        self.assertLessEqual(document["grid_offset"], 4096)
        for row in (0, document["height"] // 2, document["height"] - 1):
            start = document["grid_offset"] + row * stride + 1
            self.assertEqual(
                raw[start:start + document["width"] * 4].decode("ascii"),
                document["grid"][row])

    def test_rust_cli_validates_the_generated_finland_map(self):
        result = self.run_rust_tool("validate-map", self.map_path)
        summary = json.loads(result.stdout)
        self.assertTrue(summary["valid"])
        self.assertEqual(summary["name"], "FINLAND")
        self.assertEqual((summary["width"], summary["height"]), (100, 100))
        self.assertTrue(self.data["geography"]["north_up"])

    def test_finland_opens_in_editor_and_retains_source_metadata(self):
        loaded = self.load_map(GAME_DIR / "disks" / "maps" / "FINLAND.json")
        self.assertEqual(loaded["geography"], self.data["geography"])
        self.assertEqual(loaded["tiles"][-1], self.data["tiles"][-1])
        self.assertEqual(len(loaded["obstacles"]), 100)
        self.assertEqual(len(loaded["objects"][-1]), 100)
        self.assertIsNone(loaded["objects"][-1][-1])
        with tempfile.TemporaryDirectory() as directory:
            saved = self.save_map(loaded, Path(directory))
            self.assertLess(saved.stat().st_size, 65_535)
            restored = self.load_map(saved)
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
        self.assertEqual(atlas.get_size(), (640, 640))
        original = pygame.image.tobytes(atlas.subsurface((0, 0, 640, 256)), "RGBA")
        self.assertEqual(hashlib.sha256(original).hexdigest(),
                         "bf26f8100fab109cbfda6d152a86b8777cf0a4e5b278692830d1ff5ddda5421c")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tiles_test.png"
            pygame.image.save(atlas.subsurface((0, 0, 640, 256)), str(path))
            self.run_rust_tool("extend-atlas", "--path", path)
            generated = path.read_bytes()
            self.run_rust_tool("extend-atlas", "--path", path)
            self.assertEqual(path.read_bytes(), generated)
            extended = pygame.image.load(str(path))
            self.assertEqual(extended.get_size(), (640, 640))
            self.assertEqual(
                pygame.image.tobytes(extended.subsurface((0, 0, 640, 256)), "RGBA"),
                original)
            for tile_id in range(40, 91):
                tile = extended.subsurface((
                    tile_id % 10 * 64, tile_id // 10 * 64, 64, 64))
                self.assertNotEqual(tile.get_bounding_rect().size, (0, 0))

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
            result = self.run_rust_tool(
                "generate-finland-map",
                "--countries", countries_path,
                "--lakes", lakes_path,
                "--roads", roads_path,
                "--railways", railways_path,
                "--cities", cities_path,
                "--output", output,
                "--size", 64,
            )
            summary = json.loads(result.stdout)
            data = self.load_map(output)
            self.assertEqual(summary["dimensions"], [64, 64])
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
                result = self.run_rust_tool(
                    "generate-finland-map",
                    "--countries", countries_path,
                    "--lakes", lakes_path,
                    "--output", output,
                    "--size", size,
                    check=False,
                )
                self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
