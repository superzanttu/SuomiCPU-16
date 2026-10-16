import runpy
import unittest
from pathlib import Path

import pygame


SPRITE_VIEWER_PATH = (
    Path(__file__).resolve().parents[1]
    / "modules"
    / "Tank-16"
    / "sprite_viewer.py"
)
VIEWER = runpy.run_path(str(SPRITE_VIEWER_PATH))
extract_sprite = VIEWER["extract_sprite"]
load_atlas = VIEWER["load_atlas"]
padded_atlas = VIEWER["padded_atlas"]
sprite_bounds = VIEWER["sprite_bounds"]
sprite_id_at = VIEWER["sprite_id_at"]


class SpriteViewerTests(unittest.TestCase):
    def test_maps_coordinates_to_row_major_64_pixel_sprite_ids(self):
        atlas = pygame.Surface((192, 128), pygame.SRCALPHA)

        self.assertEqual(sprite_id_at(atlas, 0, 0), 0)
        self.assertEqual(sprite_id_at(atlas, 191, 63), 2)
        self.assertEqual(sprite_id_at(atlas, 64, 64), 4)
        self.assertEqual(sprite_bounds(atlas, 4), pygame.Rect(64, 64, 64, 64))

    def test_pads_incomplete_edge_cells_and_extracts_64_pixel_sprites(self):
        atlas = pygame.Surface((70, 65), pygame.SRCALPHA)
        atlas.set_at((69, 64), (255, 0, 0, 255))
        normalized = padded_atlas(atlas)

        self.assertEqual(normalized.get_size(), (128, 128))
        self.assertEqual(sprite_id_at(normalized, 69, 64), 3)
        sprite = extract_sprite(atlas, 3)
        self.assertEqual(sprite.get_size(), (64, 64))
        self.assertEqual(sprite.get_at((5, 0)), (255, 0, 0, 255))
        self.assertEqual(sprite.get_at((0, 0)), (0, 0, 0, 0))

    def test_rejects_coordinates_and_ids_outside_the_atlas(self):
        atlas = pygame.Surface((64, 64), pygame.SRCALPHA)

        with self.assertRaises(ValueError):
            sprite_id_at(atlas, 64, 0)
        with self.assertRaises(ValueError):
            sprite_bounds(atlas, 1)
        with self.assertRaises(ValueError):
            sprite_bounds(atlas, -1)

    def test_atlas_files_use_64_pixel_cell_dimensions(self):
        root = SPRITE_VIEWER_PATH.parent / "gfx"
        terrain = pygame.image.load(str(root / "tiles_terrain.png"))
        self.assertEqual(terrain.get_size(), (640, 384))
        self.assertEqual(sprite_bounds(terrain, 39), pygame.Rect(576, 192, 64, 64))
        self.assertEqual(sprite_bounds(terrain, 40), pygame.Rect(0, 256, 64, 64))
        self.assertEqual(sprite_bounds(terrain, 59), pygame.Rect(576, 320, 64, 64))

        objects = load_atlas(root / "tiles_objects.png")
        self.assertEqual(objects.get_width() % 64, 0)
        self.assertEqual(objects.get_height() % 64, 0)
        self.assertEqual(objects.get_size(), (384, 512))

    def test_template_is_a_transparent_numbered_64_pixel_grid(self):
        template_path = SPRITE_VIEWER_PATH.parent / "gfx" / "tiles_template.png"
        template = pygame.image.load(str(template_path))

        self.assertEqual(template.get_size(), (640, 640))
        self.assertEqual(template.get_at((32, 32)).a, 0)
        self.assertGreater(template.get_at((0, 32)).a, 0)
        self.assertGreater(template.get_at((4, 4)).a, 0)


if __name__ == "__main__":
    unittest.main()
