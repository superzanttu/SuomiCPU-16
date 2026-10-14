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
find_sprite_squares = runpy.run_path(
    str(SPRITE_VIEWER_PATH))["find_sprite_squares"]


class SpriteViewerTests(unittest.TestCase):
    def test_finds_square_sprites_of_different_sizes(self):
        atlas = pygame.Surface((96, 80), pygame.SRCALPHA)
        pygame.draw.rect(atlas, (255, 0, 0, 255), (4, 6, 12, 12))
        pygame.draw.rect(atlas, (0, 255, 0, 255), (32, 8, 24, 24))
        pygame.draw.rect(atlas, (0, 0, 255, 255), (74, 48, 8, 8))

        self.assertEqual(
            find_sprite_squares(atlas),
            [
                pygame.Rect(4, 6, 12, 12),
                pygame.Rect(32, 8, 24, 24),
                pygame.Rect(74, 48, 8, 8),
            ],
        )

    def test_does_not_infer_sprites_from_opaque_atlas(self):
        atlas = pygame.Surface((32, 32))
        atlas.fill((255, 255, 255))

        self.assertEqual(find_sprite_squares(atlas), [])


if __name__ == "__main__":
    unittest.main()
