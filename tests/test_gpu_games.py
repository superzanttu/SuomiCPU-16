import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import pygame

from c_compiler import GLOBAL_BASE, CCompilerError, compile_source
from gfx_harness import ROOT, compile_file, layer_pixel, make_cpu, pixel, run_frame, vram
from src.SuomiCPU import (
    GPU_BASE, INSTRUCTIONS_PER_FRAME, KEYS_ADDR, SPEED_FAST, SPEED_MAX,
    GFX_MODE_640_480, GFX_MODE_ADDR, MOUSE_ADDR, SCREEN_WIDTH, SPEED_ADDR,
    GPU_LAYER_ORDER, GPU_LAYER_SELECT, VRAM_SIZE, VRAM_START, VSYNC_ADDR, SuomiCompute16,
    create_memory,
)

HEADER = '#include "suomi_gfx.h"\n'


def run_c(body, frames=1):
    cpu = make_cpu(compile_source(HEADER + body, str(ROOT / "inline.c")))
    for _ in range(frames):
        run_frame(cpu)
    return cpu


def bare_cpu():
    cpu = SuomiCompute16.__new__(SuomiCompute16)
    cpu.memory = create_memory()
    cpu.frame_yield = False
    return cpu


def gpu(cpu, command, color=0, x=0, y=0, w=0, h=0, source=0):
    for offset, value in ((2, x), (4, y), (6, w), (8, h)):
        cpu.write_16(GPU_BASE + offset, value & 0xFFFF)
    cpu.write(GPU_BASE + 1, color)
    cpu.write_16(GPU_BASE + 10, source & 0xFFFF)
    cpu.write_16(GPU_BASE + 12, (source >> 16) & 0x3FFF)
    cpu.write(GPU_BASE, command)


def back(cpu, x, y, layer=0):
    return layer_pixel(cpu, layer, x, y)


class GpuTests(unittest.TestCase):
    def test_world_effect_wrappers_clip_to_map_and_preserve_both_camera_views(self):
        cpu = run_c(
            "typedef struct { int x; int y; int life; int mask; } Emitter;\n"
            "Emitter sites[2];\n"
            "unsigned char tile[1024]; unsigned char map[1];\n"
            "int main(void) {\n"
            "  gfx_mode(GFX_MODE_640_480); gfx_tile_size(32);\n"
            "  gfx_tile_upload(0, tile); gfx_tilemap(map, 1, 1, 0, 0);\n"
            "  gfx_split_camera(0, 0, 0, 0); gfx_layer_select(LAYER_3D);\n"
            "  gfx_world_rect(-2, -2, 6, 6, 42);\n"
            "  gfx_world_particle(30, 30, 4, 8, 16);\n"
            "  sites[0].x = 16; sites[0].y = 24; sites[0].life = 11; sites[0].mask = -1;\n"
            "  sites[1].x = 28; sites[1].y = 10; sites[1].life = 0; sites[1].mask = -1;\n"
            "  gfx_world_emitters(sites, 2, 12);\n"
            "  gfx_present(); return 0;\n"
            "}\n"
        )
        for offset in (0, 320):
            self.assertEqual(back(cpu, offset + 0, 0, 3), 42)
            self.assertEqual(back(cpu, offset + 3, 3, 3), 42)
            self.assertEqual(back(cpu, offset + 4, 4, 3), 0)
            self.assertEqual(back(cpu, offset + 30, 30, 3), 8)
            self.assertEqual(back(cpu, offset + 32, 30, 3), 0)
            self.assertEqual(back(cpu, offset + 30, 32, 3), 0)
            self.assertIn(back(cpu, offset + 16, 24, 3), (1, 5, 8, 11))
        self.assertEqual(cpu.gpu_layers[3][:320], cpu.gpu_layers[3][320:640])

    def test_emitter_animation_is_repeatable_and_wrecks_use_opaque_sprite_pixels(self):
        cpu = bare_cpu()
        cpu.memory[GFX_MODE_ADDR] = GFX_MODE_640_480
        cpu._ensure_gpu_state()
        cpu.split_cameras = ((0, 0), (0, 0))
        cpu.memory[GPU_LAYER_SELECT] = 2
        cpu.sprite_memory[0] = (32, 32, bytes(
            2 if x == 2 and y == 28 else 0
            for y in range(32) for x in range(32)))
        address = 0x12000
        for offset, value in ((0, 100), (2, 120), (4, 11), (6, 0)):
            cpu.write_16(address + offset, value)
        gpu(cpu, 34, x=1, y=0, source=address)
        first = bytes(cpu.gpu_layers[2])
        self.assertEqual(back(cpu, 86, 132, 2), 12)
        self.assertEqual(back(cpu, 85, 132, 2), 0)
        cpu.gpu_layers[2][:] = bytes(len(cpu.gpu_layers[2]))
        gpu(cpu, 34, x=1, y=0, source=address)
        self.assertEqual(bytes(cpu.gpu_layers[2]), first)
        cpu.gpu_layers[2][:] = bytes(len(cpu.gpu_layers[2]))
        gpu(cpu, 34, x=1, y=12, source=address)
        self.assertNotEqual(bytes(cpu.gpu_layers[2]), first)
        self.assertTrue(any(pixel in (9, 10) for pixel in cpu.gpu_layers[2]))
        cpu.write_16(address + 4, 0)
        cpu.gpu_layers[2][:] = bytes(len(cpu.gpu_layers[2]))
        gpu(cpu, 34, x=1, y=12, source=address)
        self.assertFalse(any(cpu.gpu_layers[2]))
        with self.assertRaises(ValueError):
            gpu(cpu, 35, w=33, h=16)
        with self.assertRaises(ValueError):
            gpu(cpu, 35, w=2, h=17)
        with self.assertRaises(ValueError):
            gpu(cpu, 34, x=-1, source=address)

    def test_pixel_masks_use_opaque_pixels_at_every_supported_tile_scale(self):
        cpu = bare_cpu()
        cpu._ensure_gpu_state()
        cpu.sprite_memory[0] = (3, 3, bytes((1, 0, 1, 0, 0, 0, 1, 0, 1)))
        cpu.sprite_memory[1] = (1, 1, bytes((9,)))
        for size in (24, 32, 64):
            cpu.tile_pixel_size = size
            cpu.tile_memory[:size * size] = bytes(size * size)
            cpu.tile_memory[size + 1] = 42
            for first, second in ((0, 1), (0, 64), (320, 0), (320, 320)):
                a = cpu._gpu_collision_mask(first)
                b = cpu._gpu_collision_mask(second)
                for x in range(-8, 9):
                    for y in range(-8, 9):
                        opaque_a = {(px - 2, py - 3)
                                    for py in range(a[1]) for px in range(a[0])
                                    if a[2][py * a[0] + px]}
                        opaque_b = {(px + x, py + y)
                                    for py in range(b[1]) for px in range(b[0])
                                    if b[2][py * b[0] + px]}
                        gpu(cpu, 31, x=-2, y=-3, w=x, h=y,
                            source=first | (second << 16))
                        self.assertEqual(cpu.read_16(GPU_BASE + 14),
                                         int(bool(opaque_a & opaque_b)))
        with self.assertRaises(ValueError):
            gpu(cpu, 31, source=63)
        with self.assertRaises(ValueError):
            cpu._gpu_collision_mask(321)

    def test_c_mask_wrapper_returns_result_and_ammo_matches_visible_mask(self):
        cpu = run_c(
            "int results[3];\n"
            "unsigned char sprite[9] = {1,0,1,0,0,0,1,0,1};\n"
            "int main(void) {\n"
            "  gfx_mode(GFX_MODE_640_480);\n"
            "  gfx_sprite_upload(0, 3, 3, sprite);\n"
            "  results[0] = gfx_mask_overlap(0, -1, -1, 0, 0, 0);\n"
            "  results[1] = gfx_mask_overlap(0, -1, -1, 0, 1, 1);\n"
            "  results[2] = gfx_mask_overlap(GFX_MASK_AMMO, 0, 0, 0, 3, 3);\n"
            "  gfx_layer_select(LAYER_HUD);\n"
            "  gfx_split_camera(0, 0, 100, 0);\n"
            "  gfx_world_ammo(150, 100);\n"
            "  gfx_present(); return 0;\n"
            "}\n"
        )
        self.assertEqual([cpu.read_16(GLOBAL_BASE + i * 2) for i in range(3)],
                         [0, 1, 1])
        for center in (150, 370):
            for dy in range(-4, 5):
                for dx in range(-4, 5):
                    distance = abs(dx) + abs(dy)
                    self.assertEqual(back(cpu, center + dx, 100 + dy, 4),
                                     1 if distance <= 1 else 5 if distance <= 3 else 0)
        cpu.gpu_layers[4][:] = bytes(len(cpu.gpu_layers[4]))
        cpu.split_cameras = ((0, 0), (320, 0))
        gpu(cpu, 32, x=319, y=100)
        for x in range(316, 323):
            self.assertEqual(back(cpu, x, 100, 4),
                             1 if x in (318, 319, 320) else 5)
        cpu.gpu_layers[4][:] = bytes(len(cpu.gpu_layers[4]))
        cpu.split_cameras = ((0, 0), (0, 0))
        gpu(cpu, 32, x=0, y=0)
        self.assertEqual(back(cpu, 319, 0, 4), 0)
        self.assertEqual(back(cpu, 320, 0, 4), 1)

    def test_vram_reserves_only_640_by_480_four_bit_framebuffer(self):
        self.assertEqual(VRAM_SIZE, 640 * 480 // 2)
        self.assertEqual(VRAM_START + VRAM_SIZE, 0x75800)
        self.assertFalse(hasattr(__import__("src.SuomiCPU", fromlist=[""]), "BACK_START"))

    def test_hardware_layers_compose_in_priority_order_with_visibility_and_keys(self):
        cpu = bare_cpu()
        cpu._ensure_gpu_state()
        cpu.palette = [pygame.Color(i, i, i) for i in range(256)]
        cpu.palette[1] = (255, 255, 255)
        cpu.palette[2] = (255, 0, 0)
        cpu.palette[3] = (0, 255, 0)
        cpu.palette[4] = (0, 0, 255)
        cpu._create_vram_surface()
        cpu.memory[VRAM_START] = 2
        cpu.gpu_layers[3][0] = 3
        cpu.gpu_layers[4][0] = 4

        cpu._composite_layers()
        self.assertEqual(tuple(cpu.vram_surface.get_at((0, 0)))[:3], (0, 0, 255))

        cpu.write(GPU_BASE + 22, 4)
        cpu._composite_layers()
        self.assertEqual(tuple(cpu.vram_surface.get_at((0, 0)))[:3], (0, 255, 0))

        cpu.write(GPU_BASE + 17, 0b00001)
        cpu._composite_layers()
        self.assertEqual(tuple(cpu.vram_surface.get_at((0, 0)))[:3], (255, 0, 0))

        cpu.write(GPU_BASE + 17, 0x1F)
        cpu.write(GPU_LAYER_ORDER, 10)
        cpu.write(GPU_LAYER_ORDER + 3, 1)
        cpu.write(GPU_LAYER_ORDER + 4, 0)
        cpu._composite_layers()
        self.assertEqual(tuple(cpu.vram_surface.get_at((0, 0)))[:3], (255, 0, 0))

    def test_hardware_sprite_and_tile_memories_render_to_their_layers(self):
        cpu = bare_cpu()
        cpu.memory[0x1000:0x1004] = bytes((0, 5, 6, 0))
        gpu(cpu, 23, color=0, w=2, h=2, source=0x1000)
        gpu(cpu, 24, color=0, x=10, y=11, w=1)
        self.assertEqual(cpu.read_16(GPU_BASE + 14), 0)

        cpu.memory[0x1100:0x1100 + 256] = bytes([9]) * 256
        gpu(cpu, 25, color=2, source=0x1100)
        cpu.memory[0x1200] = 2
        gpu(cpu, 21, x=0, y=0, w=1, h=1, source=0x1200)
        cpu._composite_layers()

        self.assertEqual(cpu.gpu_layers[2][11 * 320 + 11], 5)
        self.assertEqual(cpu.gpu_layers[2][11 * 320 + 10], 0)
        self.assertEqual(cpu.gpu_layers[1][0], 9)
        self.assertEqual(cpu.gpu_layers[1][15 * 320 + 15], 9)

    def test_configurable_tile_size_uploads_and_draws_variable_cells(self):
        for size in (24, 32):
            with self.subTest(size=size):
                pixel_count = size * size
                cpu = run_c(
                    HEADER
                    + f"unsigned char tile[{pixel_count}];\n"
                    "unsigned char map[1] = {0};\n"
                    "int main(void) {\n"
                    "  int i;\n"
                    f"  for (i = 0; i < {pixel_count}; i++) tile[i] = 42;\n"
                    "  gfx_mode(GFX_MODE_640_480);\n"
                    f"  gfx_tile_size({size});\n"
                    "  gfx_tile_upload(0, tile);\n"
                    "  gfx_tilemap(map, 1, 1, 0, 0);\n"
                    "  gfx_present();\n"
                    "  return 0;\n"
                    "}\n",
                )

                self.assertEqual(cpu.tile_pixel_size, size)
                self.assertEqual(cpu.gpu_layers[1][(size - 1) * 640 + size - 1], 42)
                self.assertEqual(cpu.gpu_layers[1][(size - 1) * 640 + size], 0)

    def test_split_cameras_clip_tiles_and_sprites_to_each_viewport(self):
        cpu = bare_cpu()
        cpu.memory[GFX_MODE_ADDR] = GFX_MODE_640_480
        cpu._ensure_gpu_state()
        cpu.tile_pixel_size = 2
        cpu.tilemap_width = 4
        cpu.tilemap_height = 1
        cpu.tilemap_memory = bytes((1, 2, 3, 4))
        for tile in range(1, 5):
            start = tile * 64 * 64
            cpu.tile_memory[start:start + 4] = bytes((tile,)) * 4
        cpu.split_cameras = ((0, 0), (4, 0))
        cpu._render_tile_layer()

        self.assertEqual(cpu.gpu_layers[1][0], 1)
        self.assertEqual(cpu.gpu_layers[1][2], 2)
        self.assertEqual(cpu.gpu_layers[1][320], 3)
        self.assertEqual(cpu.gpu_layers[1][322], 4)

        cpu.sprite_memory[0] = (2, 2, bytes((9, 9, 9, 9)))
        cpu.sprites[0] = {"x": 4, "y": 1, "flags": 1}
        cpu._render_sprites()
        self.assertEqual(cpu.gpu_layers[2][1 * 640 + 4], 9)
        self.assertEqual(cpu.gpu_layers[2][1 * 640 + 320], 9)

    def test_world_tiles_preserve_palette_transparency_and_viewport_clipping(self):
        cpu = bare_cpu()
        cpu.memory[GFX_MODE_ADDR] = GFX_MODE_640_480
        cpu._ensure_gpu_state()
        cpu.tile_pixel_size = 4
        start = 3 * 64 * 64
        cpu.tile_memory[start:start + 16] = bytes((0, 42, 43, 44)) * 4
        cpu.gpu_layers[3][:] = bytes((9,)) * (640 * 480)
        cpu.memory[GPU_BASE + 16] = 3
        gpu(cpu, 29, x=0, y=0, w=318, h=0)
        gpu(cpu, 30, color=3, x=318, y=10)
        self.assertEqual(list(cpu.gpu_layers[3][10 * 640 + 318:10 * 640 + 324]),
                         [9, 42, 9, 42, 43, 44])
        self.assertEqual(cpu.gpu_layers[3][10 * 640 + 324], 9)
        self.assertEqual(cpu.read_16(GPU_BASE + 14), 0)

    def test_world_tiles_survive_deferred_tilemap_rendering(self):
        cpu = run_c(
            "unsigned char tile[256]; unsigned char map[1] = {0};\n"
            "int main(void) {\n"
            "  int i; for (i = 0; i < 256; i++) tile[i] = 5;\n"
            "  gfx_tile_size(16); gfx_tile_upload(0, tile);\n"
            "  gfx_tilemap(map, 1, 1, 0, 0);\n"
            "  gfx_layer_select(LAYER_TILES); gfx_world_tile(0, 16, 0);\n"
            "  gfx_present(); return 0;\n"
            "}\n",
        )

        self.assertEqual(cpu.gpu_layers[1][0], 5)
        self.assertEqual(cpu.gpu_layers[1][16], 5)

    def test_c_layer_sprite_and_tile_apis_program_hardware_state(self):
        cpu = run_c(
            "unsigned char sprite[4] = {0,5,6,0};\n"
            "unsigned char tile[256];\n"
            "unsigned char map[2] = {2,3};\n"
            "int main(void) {\n"
            "  gfx_layer_reset(1 << LAYER_HUD);\n"
            "  gfx_layer_select(LAYER_HUD);\n"
            "  gfx_layer_visibility(LAYER_ALL);\n"
            "  gfx_layer_transparency(LAYER_HUD, 0);\n"
            "  gfx_layer_order(LAYER_HUD, 9);\n"
            "  gfx_sprite_upload(0, 2, 2, sprite);\n"
            "  gfx_sprite_move(0, 7, 8, SPRITE_VISIBLE);\n"
            "  gfx_tile_upload(2, tile);\n"
            "  gfx_tilemap(map, 2, 1, 0, 0);\n"
            "  gfx_tile_camera(16, 32);\n"
            "  gfx_split_camera(-10, -20, 30, 40);\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )
        self.assertEqual(cpu.memory[GPU_BASE + 16], 4)
        self.assertEqual(cpu.memory[GPU_BASE + 17], 0x1F)
        self.assertEqual(cpu.memory[GPU_LAYER_ORDER + 4], 9)
        self.assertEqual(
            list(cpu.memory[GPU_LAYER_ORDER:GPU_LAYER_ORDER + 5]),
            [0, 1, 2, 3, 9],
        )
        self.assertEqual(cpu.sprites[0], {"x": 7, "y": 8, "flags": 1})
        self.assertEqual(cpu.sprite_memory[0], (2, 2, bytes((0, 5, 6, 0))))
        self.assertEqual(cpu.tilemap_memory, b"\x02\x03")
        self.assertEqual(cpu.tilemap_width, 2)
        self.assertEqual(cpu.tilemap_height, 1)
        self.assertEqual(cpu.tile_camera, (16, 32))
        self.assertEqual(cpu.split_cameras, ((-10, -20), (30, 40)))

    def test_640x480_packed_clear_pixel_rect_and_present(self):
        cpu = bare_cpu()
        cpu.memory[GFX_MODE_ADDR] = GFX_MODE_640_480

        gpu(cpu, 1, color=2)
        gpu(cpu, 2, color=3, x=1, y=0)
        gpu(cpu, 3, color=1, x=2, y=0, w=3, h=2)

        packed_size = 640 * 480 // 2
        self.assertEqual(VRAM_SIZE, packed_size)
        self.assertEqual(cpu.memory[VRAM_START], 0x23)
        self.assertEqual(cpu.memory[VRAM_START + 1], 0x11)
        self.assertEqual(cpu.memory[VRAM_START + 2], 0x12)
        self.assertEqual(cpu.memory[VRAM_START + packed_size - 1], 0x22)

        gpu(cpu, 8)

        self.assertTrue(cpu.frame_yield)
        decoded = cpu._decode_framebuffer(VRAM_START)
        self.assertEqual(
            list(decoded[:8]),
            [2, 3, 1, 1, 1, 2, 2, 2],
        )
        self.assertEqual(
            list(decoded[640:648]),
            [2, 2, 1, 1, 1, 2, 2, 2],
        )
        self.assertEqual(cpu.memory[VRAM_START + packed_size - 1], 0x22)

    def test_640x480_mouse_coordinates_map_to_logical_pixels(self):
        cpu = bare_cpu()
        cpu.memory[GFX_MODE_ADDR] = GFX_MODE_640_480
        cpu.screen = Mock()
        cpu.screen.get_size.return_value = (1920, 1440)

        with patch("src.SuomiCPU.pygame.mouse.get_pos", return_value=(1917, 1437)):
            cpu.begin_mouse_frame()

        self.assertEqual(cpu.read_16(MOUSE_ADDR), 639)
        self.assertEqual(cpu.read_16(MOUSE_ADDR + 2), 479)

    def test_clear_pixel_rect_and_clipping(self):
        cpu = bare_cpu()
        gpu(cpu, 1, color=4)
        gpu(cpu, 2, color=1, x=5, y=6)
        gpu(cpu, 3, color=2, x=-3, y=-3, w=6, h=6)
        gpu(cpu, 3, color=3, x=318, y=238, w=10, h=10)
        gpu(cpu, 2, color=1, x=-1, y=0)
        self.assertEqual((back(cpu, 9, 9), back(cpu, 5, 6)), (4, 1))
        self.assertEqual((back(cpu, 0, 0), back(cpu, 2, 2), back(cpu, 3, 3)), (2, 2, 4))
        self.assertEqual((back(cpu, 319, 239), back(cpu, 318, 238)), (3, 3))

    def test_line_sprite_bitmap_and_poly(self):
        cpu = bare_cpu()
        gpu(cpu, 4, color=5, x=0, y=0, w=10, h=10)
        self.assertEqual((back(cpu, 0, 0), back(cpu, 5, 5), back(cpu, 10, 10)), (5, 5, 5))
        cpu.memory[0x50000:0x50004] = bytes([7, 0, 0, 8])
        gpu(cpu, 5, x=20, y=20, w=2, h=2, source=0x50000)
        self.assertEqual((back(cpu, 20, 20), back(cpu, 21, 20), back(cpu, 21, 21)), (7, 0, 8))
        cpu.memory[0x1000:0x1002] = bytes([0b10100000, 0b01000000])
        gpu(cpu, 6, color=3, x=40, y=40, w=3, h=2, source=0x1000)
        self.assertEqual([back(cpu, 40 + i, 40) for i in range(3)], [3, 0, 3])
        self.assertEqual(back(cpu, 41, 41), 3)
        cpu.memory[0x1100:0x1106] = bytes([0, 0, 10, 0, 0, 10])
        gpu(cpu, 10, color=6, x=100, y=100, w=3, source=0x1100)
        self.assertEqual((back(cpu, 100, 100), back(cpu, 110, 100), back(cpu, 100, 110)), (6, 6, 6))

    def test_text_present_and_random(self):
        cpu = bare_cpu()
        font = (ROOT / "fonts" / "font5x7.asm")
        self.assertTrue(font.exists())
        cpu.memory[0x4400 + 33 * 8:0x4400 + 33 * 8 + 8] = bytes([0xF8] * 8)
        cpu.memory[0x1200:0x1204] = "A\n?".encode("utf-8") + b"\0"
        gpu(cpu, 7, color=9, x=10, y=10, source=0x1200)
        self.assertEqual(back(cpu, 10, 10), 9)
        self.assertEqual(back(cpu, 14, 16), 9)
        gpu(cpu, 8)
        self.assertTrue(cpu.frame_yield)
        self.assertEqual(vram_pixel(cpu, 10, 10), 9)
        seen = set()
        for _ in range(40):
            gpu(cpu, 9)
            seen.add(cpu.memory[GPU_BASE + 14])
        self.assertGreater(len(seen), 5)

    def test_unknown_command_is_ignored(self):
        cpu = bare_cpu()
        gpu(cpu, 99)
        self.assertEqual(cpu.memory[VRAM_START], 0)


class Gpu3DTests(unittest.TestCase):
    def test_triangle_rasterizer_clips_and_draws_on_selected_layer(self):
        cpu = bare_cpu()
        cpu._ensure_gpu_state()
        cpu._drawing_layer = 3
        points = ((-12, 8), (71, 19), (23, 91))
        cpu._gpu3d_triangle(points, 13)

        width, _ = cpu._screen_size()
        self.assertEqual(cpu.gpu_layers[3][30 * width + 30], 13)
        self.assertEqual(cpu.gpu_layers[3][100 * width + 100], 0)

    def render_scene(self, scene, mode=0):
        cpu = bare_cpu()
        cpu.memory[GFX_MODE_ADDR] = mode
        address = 0x1000
        encoded = bytearray()
        for word in scene:
            encoded.extend(((word & 0xFFFF) >> 8, word & 0xFF))
        cpu.memory[address:address + len(encoded)] = encoded
        gpu(cpu, 22, source=address)
        return cpu

    def test_fixed_point_projection_fills_and_painter_order_hides_back_edges(self):
        scene = [6, 2, 0, 0, 0, 0, 0, 80 * 256, 80 * 256, 3]
        vertices = [
            (-40, -30, 20), (40, -30, 20), (0, 40, 20),
            (-40, -30, 0), (40, -30, 0), (0, 40, 0),
        ]
        scene.extend(value * 256 for vertex in vertices for value in vertex)
        scene.extend((0, 1, 2, 2, 3, 4, 5, 4))

        cpu = self.render_scene(scene)

        self.assertEqual(cpu.read_16(GPU_BASE + 14), 0)
        self.assertEqual(back(cpu, 160, 120, 3), 4)
        self.assertEqual(back(cpu, 160, 105, 3), 4)

    def test_scene_renders_to_the_packed_640_by_480_back_buffer(self):
        scene = [3, 1, 0, 0, 0, 0, 0, 80 * 256, 64 * 256, 1]
        scene.extend(value * 256 for vertex in [(-20, -20, 0), (20, -20, 0), (0, 20, 0)]
                     for value in vertex)
        scene.extend((0, 1, 2, 3))

        cpu = self.render_scene(scene, GFX_MODE_640_480)
        decoded = cpu.gpu_layers[3]

        self.assertEqual(cpu.read_16(GPU_BASE + 14), 0)
        self.assertEqual(decoded[240 * 640 + 320], 3)

    def test_rotation_changes_projected_geometry_and_invalid_indices_report_error(self):
        vertices = [(-40, -10, 0), (40, -10, 0), (0, 30, 0)]
        scene = [3, 1, 0, 0, 0, 0, 0, 80 * 256, 64 * 256, 1]
        scene.extend(value * 256 for vertex in vertices for value in vertex)
        scene.extend((0, 1, 2, 3))
        unrotated = self.render_scene(scene)
        scene[4] = 64
        rotated = self.render_scene(scene)

        def bounds(cpu):
            pixels = [
                (x, y)
                for y in range(240)
                for x in range(320)
                if back(cpu, x, y, 3) == 3
            ]
            return (
                min(x for x, _ in pixels), max(x for x, _ in pixels),
                min(y for _, y in pixels), max(y for _, y in pixels),
            )

        self.assertNotEqual(bounds(unrotated), bounds(rotated))

        scene[-4] = 9
        invalid = self.render_scene(scene)
        self.assertEqual(invalid.read_16(GPU_BASE + 14), 2)
        self.assertEqual(back(invalid, 160, 120), 0)

    def test_c_graphics_library_submits_scene_and_returns_gpu_status(self):
        source = (
            HEADER
            + "int scene[23] = {3,1,0,0,0,0,0,24576,20480,1,"
            "0xD800,0xE800,0,0x2800,0xE800,0,0,0x5000,0,"
            "0,1,2,6};\n"
            + "int result;\n"
            + "int main(void) { result = gfx3d_draw(scene); gfx_present(); return 0; }\n"
        )
        cpu = run_c(source)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 46), 0)
        self.assertEqual(layer_pixel(cpu, 3, 160, 120), 6)

    def test_showcase_ship_page_uses_the_3d_gpu(self):
        path = ROOT / "modules" / "SC-16 showcase" / "sc16_showcase.c"
        source = path.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n{")]
        source += (
            "\nint main(void) {\n"
            "  gfx_clear(BLACK);\n"
            "  ship_init();\n"
            "  frame = 0;\n"
            "  page_ship();\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(path)))
        self.addCleanup(lambda: cpu.net.transport.close() if getattr(cpu, "net", None) else None)
        steps = 0
        while cpu.running and steps < 200_000:
            cpu.step()
            steps += 1
        self.assertFalse(cpu.running, "3D showcase page did not finish")
        self.assertEqual(cpu.read_16(GPU_BASE + 14), 0)
        self.assertIn(4, cpu.gpu_layers[3])


def vram_pixel(cpu, x, y):
    from src.SuomiCPU import VRAM_START
    return cpu.memory[VRAM_START + y * SCREEN_WIDTH + x]


class CGraphicsTests(unittest.TestCase):
    def test_program_can_set_rgb_palette_entries(self):
        cpu = run_c(
            HEADER
            + "int main(void) {\n"
            "  gfx_palette(42, 12, 34, 56);\n"
            "  gfx_layer_select(LAYER_HUD);\n"
            "  gfx_pixel(5, 6, 42);\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )

        self.assertEqual(tuple(cpu.palette[42])[:3], (12, 34, 56))
        self.assertEqual(
            tuple(cpu.vram_surface.get_at((5, 6)))[:3], (12, 34, 56))

    def test_every_builtin_draws_and_presents(self):
        cpu = run_c(
            "unsigned char spr[4] = {2, 0, 0, 3};\n"
            "unsigned char bm[2] = {0xF0, 0x0F};\n"
            "char tri[6] = {0, 0, 8, 0, 0, -8};\n"
            "int main(void) {\n"
            "  gfx_clear(BLUE);\n"
            "  gfx_rect(10, 20, 30, 40, RED);\n"
            "  gfx_pixel(100, 100, WHITE);\n"
            "  gfx_line(0, 0, 50, 50, YELLOW);\n"
            "  gfx_sprite(60, 60, 2, 2, spr);\n"
            "  gfx_bitmap(80, 80, 8, 2, bm, GREEN);\n"
            "  gfx_poly(200, 100, 3, tri, CYAN);\n"
            "  gfx_text(5, 200, \"Hi\", WHITE);\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )
        self.assertEqual(pixel(cpu, 15, 25), 2)
        self.assertEqual(pixel(cpu, 100, 100), 1)
        self.assertEqual((pixel(cpu, 60, 60), pixel(cpu, 61, 60), pixel(cpu, 61, 61)), (2, 4, 3))
        self.assertEqual((pixel(cpu, 80, 80), pixel(cpu, 88, 80), pixel(cpu, 84, 81)), (3, 4, 3))
        self.assertEqual((pixel(cpu, 200, 100), pixel(cpu, 208, 100), pixel(cpu, 200, 92)), (6, 6, 6))
        self.assertEqual(pixel(cpu, 300, 10), 4)
        self.assertIn(1, vram(cpu)[200 * 320:208 * 320])

    def test_keys_and_random_values(self):
        cpu = make_cpu(compile_source(
            HEADER + "unsigned int k; unsigned int r; int s;\n"
            "int main(void) { k = gfx_keys(); r = gfx_random(); s = r < 256; gfx_present(); return 0; }\n",
            str(ROOT / "inline.c")))
        cpu.memory[KEYS_ADDR] = 0x15
        run_frame(cpu)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 0x15)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 1)

    def test_mouse_click_events_remain_latched_until_guest_reads_them(self):
        cpu = make_cpu(compile_source(
            HEADER + "int click_events;\n"
            "int main(void) { click_events = gfx_mouse_events(); return 0; }\n",
            str(ROOT / "mouse_events.c")))
        cpu.set_mouse_position(70, 225)
        cpu.mouse_button(0, True, 70, 225, 1000)
        cpu.end_mouse_frame()
        while cpu.running:
            cpu.step()
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 1)
        self.assertEqual(cpu.read(MOUSE_ADDR + 5), 0)

    def test_signed_char_loads_are_sign_extended(self):
        source = ROOT / "tests" / "_sc.c"
        source.write_text(
            "char d[2] = {-1, 1};\nunsigned char u[1] = {255};\nint r;\nint q;\n"
            "int main(void) { int a; a = 5; r = a + d[0]; q = u[0]; return 0; }\n"
        )
        try:
            cpu = make_cpu(compile_file(source))
        finally:
            source.unlink()
        for _ in range(3000):
            cpu.step()
            if not cpu.running:
                break
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 3), 4)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 5), 255)
    def test_shift_operators(self):
        cpu = run_c(
            "int a; unsigned int b; int c; int d;\n"
            "int main(void) { int n; unsigned int u; n = -64; u = 0x8000;\n"
            "  a = 3 << 4; b = u >> 15; c = n >> 2; d = 1 << 15; return 0; }\n"
        )
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 48)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 1)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 0xFFF0)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 6), 0x8000)

    def test_constant_operand_fast_paths(self):
        cpu = run_c(
            "int r[16]; unsigned int u;\n"
            "int main(void) { int n; n = -5; u = 0xFFF0;\n"
            "  r[0] = n < 3; r[1] = n > -9; r[2] = u > 5; r[3] = u < 300;\n"
            "  r[4] = n >= -5; r[5] = n <= -6; r[6] = (n + 1000) - 7; r[7] = 77 % 10 + (n * 3);\n"
            "  r[8] = (n & 0xFF00) | 0x12; r[9] = u >> 4; r[10] = n << 2; r[11] = n == -5;\n"
            "  r[12] = n != -5; r[13] = u >= 0xFFF0; r[14] = 30000 / 3; return 0; }\n"
        )
        got = [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(15)]
        self.assertEqual(got, [1, 1, 1, 0, 1, 0, 988, 0xFFF8, 0xFF12, 0x0FFF, 0xFFEC, 1, 0, 1, 10000])

    def test_library_names_are_reserved(self):
        with self.assertRaises(CCompilerError):
            compile_source("void gfx_clear(unsigned char c) {}\nint main(void) { return 0; }\n")


class MotoRobotsTests(unittest.TestCase):
    def test_game_uses_maximum_speed_register(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "MotoRobots" / "main.c"))
        run_frame(cpu, limit=400000)

        self.assertEqual(cpu.memory[SPEED_ADDR], SPEED_MAX)
        self.assertEqual(cpu.memory[VSYNC_ADDR], 1)

    def test_pathfinding_search_does_not_draw_progress_on_map(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "GameState test_state;\n"
            "int main(void) {\n"
            "  game_init(&test_state);\n"
            "  test_state.camera_x = 0;\n"
            "  test_state.camera_y = 0;\n"
            "  game_draw(&test_state);\n"
            "  gfx_present();\n"
            "  test_state.pathfinding_robot = 0;\n"
            "  test_state.robots[0].path_status = ROBOT_PATH_SEARCHING;\n"
            "  test_state.path_queue[0] = 8 * 48 + 18;\n"
            "  test_state.path_queue[1] = 8 * 48 + 19;\n"
            "  test_state.path_queue_head = 1;\n"
            "  test_state.path_queue_tail = 2;\n"
            "  game_draw(&test_state);\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "progress_test.c")))
        run_frame(cpu)
        explored_pixel = pixel(cpu, 18 * 16 + 4, 36 + 8 * 16 + 4)
        frontier_pixel = pixel(cpu, 19 * 16 + 4, 36 + 8 * 16 + 4)
        run_frame(cpu)
        self.assertEqual(
            pixel(cpu, 18 * 16 + 4, 36 + 8 * 16 + 4),
            explored_pixel,
        )
        self.assertEqual(
            pixel(cpu, 19 * 16 + 4, 36 + 8 * 16 + 4),
            frontier_pixel,
        )

    def test_found_robot_route_is_drawn_ahead_of_robot(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "GameState test_state;\n"
            "int main(void) {\n"
            "  Robot *robot;\n"
            "  game_init(&test_state);\n"
            "  test_state.camera_x = 0;\n"
            "  test_state.camera_y = 0;\n"
            "  robot = &test_state.robots[0];\n"
            "  robot->state = ROBOT_TO_TARGET;\n"
            "  robot->position.x = 17;\n"
            "  robot->position.y = 8;\n"
            "  robot->path_status = ROBOT_PATH_READY;\n"
            "  robot->path_length = 2;\n"
            "  robot->path_position = 0;\n"
            "  test_state.robot_paths[0][0] = 1;\n"
            "  test_state.robot_paths[0][1] = 3;\n"
            "  game_draw(&test_state);\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "found_route_test.c")))
        run_frame(cpu)

        self.assertEqual(pixel(cpu, 18 * 16 + 4, 36 + 8 * 16 + 4), 7)
        self.assertEqual(pixel(cpu, 18 * 16 + 4, 36 + 9 * 16 + 4), 7)

    def test_robots_can_share_a_destination_building_tile(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int results[2];\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  game_init(&test_state);\n"
            "  test_state.buildings[1].type = BUILDING_FACTORY;\n"
            "  test_state.buildings[1].position.x = 25;\n"
            "  test_state.buildings[1].position.y = 24;\n"
            "  test_state.building_count = 2;\n"
            "  test_state.robot_count = 2;\n"
            "  test_state.robots[0].state = ROBOT_UNLOADING;\n"
            "  test_state.robots[0].position.x = 25;\n"
            "  test_state.robots[0].position.y = 24;\n"
            "  test_state.robots[1].state = ROBOT_TO_TARGET;\n"
            "  test_state.robots[1].position.x = 24;\n"
            "  test_state.robots[1].position.y = 24;\n"
            "  results[0] = robot_tile_blocked(&test_state,\n"
            "      &test_state.robots[1], 25, 24, 1);\n"
            "  test_state.robots[0].position.x = 24;\n"
            "  test_state.robots[0].position.y = 23;\n"
            "  results[1] = robot_tile_blocked(&test_state,\n"
            "      &test_state.robots[1], 24, 23, 1);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "target_collision_test.c")))
        while cpu.running:
            cpu.step()

        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(2)],
            [0, 1],
        )

    def test_robots_cannot_move_onto_rock_and_rock_differs_from_coal(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int results[5];\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  MapFeature *rock;\n"
            "  MapFeature *coal;\n"
            "  Robot *robot;\n"
            "  int rock_tile_x;\n"
            "  game_init(&test_state);\n"
            "  rock = &test_state.map.features[0];\n"
            "  coal = &test_state.map.features[3];\n"
            "  rock_tile_x = rock->center_x + ((rock->seed & 1) - 1) +\n"
            "      rock->radius;\n"
            "  test_state.buildings[1].type = BUILDING_FACTORY;\n"
            "  test_state.buildings[1].position.x = 30;\n"
            "  test_state.buildings[1].position.y = 30;\n"
            "  test_state.building_count = 2;\n"
            "  test_state.robot_count = 1;\n"
            "  robot = &test_state.robots[0];\n"
            "  robot->state = ROBOT_TO_TARGET;\n"
            "  robot->target_building = 1;\n"
            "  robot->position.x = rock_tile_x + 1;\n"
            "  robot->position.y = rock->center_y;\n"
            "  robot->path_status = ROBOT_PATH_READY;\n"
            "  robot->path_length = 1;\n"
            "  test_state.robot_paths[0][0] = 0;\n"
            "  results[0] = map_get_tile(&test_state.map,\n"
            "      rock_tile_x, rock->center_y);\n"
            "  results[1] = map_get_tile(&test_state.map,\n"
            "      coal->center_x, coal->center_y);\n"
            "  update_robots(&test_state, 30.0f);\n"
            "  results[2] = robot->position.x;\n"
            "  results[3] = robot->path_status;\n"
            "  results[4] = rock_tile_x + 1;\n"
            "  map_draw(&test_state.map, rock->center_x * 16,\n"
            "      rock->center_y * 16, 16, MAP_VIEW_TOP, 192);\n"
            "  map_draw(&test_state.map, coal->center_x * 16,\n"
            "      coal->center_y * 16, 16, MAP_VIEW_TOP, 192);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "rock_collision_test.c")))
        while cpu.running:
            cpu.step()

        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(5)][:4],
            [1, 0, cpu.read_16(GLOBAL_BASE + 8), 4],
        )
        self.assertEqual(
            cpu.read_16(GLOBAL_BASE + 4),
            cpu.read_16(GLOBAL_BASE + 8),
        )
        self.assertEqual(back(cpu, 8, 55), 9)

    def test_toolbar_button_activates_on_press(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "MotoRobots" / "main.c"))
        for _ in range(4):
            run_frame(cpu, limit=30000)
            if any(vram(cpu)):
                break

        cpu.set_mouse_position(70, 225)
        cpu.mouse_button(0, True, 70, 225, 1000)
        for _ in range(4):
            run_frame(cpu, limit=30000)
            if "Build tool: Factory" in cpu.console_device.output.decode("ascii"):
                break
        self.assertIn("Build tool: Factory",
                      cpu.console_device.output.decode("ascii"))

    def test_toolbar_led_blinks_with_frame_counter(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "MotoRobots" / "main.c"))
        seen_colors = set()
        for _ in range(70):
            run_frame(cpu, limit=30000)
            if any(vram(cpu)):
                seen_colors.add(vram(cpu)[207 * 320 + 309])
        self.assertTrue({3, 11}.issubset(seen_colors))

    def test_toolbar_ignores_footer_header_and_button_gaps(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "MotoRobots" / "main.c"))
        for _ in range(4):
            run_frame(cpu, limit=30000)
            if any(vram(cpu)):
                break

        cpu.set_mouse_position(70, 207)
        cpu.mouse_button(0, True, 70, 207, 1000)
        run_frame(cpu, limit=30000)
        cpu.end_mouse_frame()
        cpu.mouse_button(0, False, 70, 207, 1010)
        for _ in range(4):
            run_frame(cpu, limit=30000)
        self.assertNotIn("Build tool: Factory",
                         cpu.console_device.output.decode("ascii"))

        cpu.set_mouse_position(64, 225)
        cpu.mouse_button(0, True, 64, 225, 1100)
        for _ in range(4):
            run_frame(cpu, limit=30000)
        self.assertNotIn("Build tool: Factory",
                         cpu.console_device.output.decode("ascii"))

    def test_initial_screen_presents_within_four_frame_budgets(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "MotoRobots" / "main.c"))
        for _ in range(4):
            run_frame(cpu, limit=30000)
            if any(vram(cpu)):
                break
        self.assertTrue(any(vram(cpu)))

    def test_static_scene_cache_keeps_later_frames_within_budget(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "MotoRobots" / "main.c"))
        run_frame(cpu, limit=120000)
        cached_frame_instructions = run_frame(cpu, limit=120000)
        self.assertLess(cached_frame_instructions, INSTRUCTIONS_PER_FRAME)

    def test_buildings_and_carrying_robot_use_distinct_pixel_art(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "GameState preview;\n"
            "int main(void) {\n"
            "  game_init(&preview);\n"
            "  preview.buildings[1].type = BUILDING_MINE;\n"
            "  preview.buildings[1].position.x = 15;\n"
            "  preview.buildings[1].position.y = 19;\n"
            "  preview.buildings[1].resource_type = RES_COAL;\n"
            "  preview.buildings[2].type = BUILDING_FACTORY;\n"
            "  preview.buildings[2].position.x = 20;\n"
            "  preview.buildings[2].position.y = 24;\n"
            "  preview.buildings[3].type = BUILDING_STORAGE;\n"
            "  preview.buildings[3].position.x = 28;\n"
            "  preview.buildings[3].position.y = 24;\n"
            "  preview.building_count = 4;\n"
            "  preview.selected_building = -1;\n"
            "  preview.robots[0].state = ROBOT_TO_TARGET;\n"
            "  preview.robots[0].position.x = 22;\n"
            "  preview.robots[0].position.y = 24;\n"
            "  preview.robots[0].carrying_type = RES_IRON_ORE;\n"
            "  game_draw(&preview);\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "graphics_test.c")))
        while cpu.running:
            cpu.step()

        self.assertEqual(pixel(cpu, 106, 119), 8)
        self.assertEqual(pixel(cpu, 20, 43), 2)
        self.assertEqual(pixel(cpu, 98, 123), 4)
        self.assertEqual(pixel(cpu, 230, 123), 6)
        self.assertEqual(pixel(cpu, 138, 125), 8)

    def test_irregular_map_has_rock_water_and_mineable_deposits(self):
        source = (
            '#include "map.c"\n'
            "int results[4];\n"
            "GameMap test_map;\n"
            "int main(void) {\n"
            "  map_init(&test_map);\n"
            "  results[0] = map_get_tile(&test_map, 10, 9);\n"
            "  results[1] = map_get_tile(&test_map, test_map.river_x[12], 12);\n"
            "  results[2] = map_get_resource(&test_map, 15, 19);\n"
            "  results[3] = map_get_tile(&test_map, 24, 24);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "map_test.c")))
        while cpu.running:
            cpu.step()
        values = [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(4)]
        self.assertEqual(values, [1, 2, 1, 0])

    def test_robot_transports_mine_inventory_to_factory(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int results[4];\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  int step;\n"
            "  Building* mine;\n"
            "  Building* factory;\n"
            "  game_init(&test_state);\n"
            "  mine = &test_state.buildings[1];\n"
            "  mine->type = BUILDING_MINE;\n"
            "  mine->position.x = 15;\n"
            "  mine->position.y = 19;\n"
            "  mine->resource_type = RES_IRON_ORE;\n"
            "  mine->inventory_type = RES_IRON_ORE;\n"
            "  mine->inventory_count = 3;\n"
            "  factory = &test_state.buildings[2];\n"
            "  factory->type = BUILDING_FACTORY;\n"
            "  factory->position.x = 16;\n"
            "  factory->position.y = 19;\n"
            "  factory->inventory_type = RES_NONE;\n"
            "  factory->inventory_count = 0;\n"
            "  test_state.building_count = 3;\n"
            "  assign_robot_route(&test_state, 1, 2);\n"
            "  test_state.robots[0].position.x = 15;\n"
            "  test_state.robots[0].position.y = 19;\n"
            "  for (step = 0; step < 30; step++) {\n"
            "    update_pathfinding(&test_state);\n"
            "    update_robots(&test_state, 0.5f);\n"
            "  }\n"
            "  results[0] = mine->inventory_count;\n"
            "  results[1] = factory->inventory_count;\n"
            "  results[2] = test_state.robots[0].carrying_amount;\n"
            "  results[3] = test_state.robots[0].state;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "robot_test.c")))
        while cpu.running:
            cpu.step()
        values = [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(4)]
        self.assertEqual(values[:3], [0, 3, 0])
        self.assertEqual(values[3], 2)

    def test_robot_plans_incrementally_and_replans_when_next_tile_is_blocked(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int results[10];\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  int step;\n"
            "  int direction;\n"
            "  int blocked_x;\n"
            "  int blocked_y;\n"
            "  game_init(&test_state);\n"
            "  test_state.buildings[1].type = BUILDING_MINE;\n"
            "  test_state.buildings[1].position.x = 20;\n"
            "  test_state.buildings[1].position.y = 30;\n"
            "  test_state.buildings[1].resource_type = RES_IRON_ORE;\n"
            "  test_state.buildings[2].type = BUILDING_FACTORY;\n"
            "  test_state.buildings[2].position.x = 26;\n"
            "  test_state.buildings[2].position.y = 30;\n"
            "  test_state.building_count = 3;\n"
            "  assign_robot_route(&test_state, 1, 2);\n"
            "  update_pathfinding(&test_state);\n"
            "  results[0] = test_state.robots[0].position.x;\n"
            "  results[1] = test_state.robots[0].position.y;\n"
            "  results[2] = test_state.robots[0].path_status;\n"
            "  results[9] = test_state.path_queue_head;\n"
            "  for (step = 0; step < 120 &&\n"
            "       test_state.robots[0].path_status != ROBOT_PATH_READY;\n"
            "       step++) update_pathfinding(&test_state);\n"
            "  results[3] = test_state.robots[0].path_status;\n"
            "  direction = test_state.robot_paths[0][0];\n"
            "  blocked_x = test_state.robots[0].position.x;\n"
            "  blocked_y = test_state.robots[0].position.y;\n"
            "  if (direction == 0) blocked_x--;\n"
            "  else if (direction == 1) blocked_x++;\n"
            "  else if (direction == 2) blocked_y--;\n"
            "  else blocked_y++;\n"
            "  test_state.buildings[3].type = BUILDING_STORAGE;\n"
            "  test_state.buildings[3].position.x = blocked_x;\n"
            "  test_state.buildings[3].position.y = blocked_y;\n"
            "  test_state.building_count = 4;\n"
            "  update_robots(&test_state, 1.0f);\n"
            "  results[4] = test_state.robots[0].path_status;\n"
            "  update_robots(&test_state, 0.5f);\n"
            "  results[5] = test_state.robots[0].path_status;\n"
            "  update_robots(&test_state, 0.5f);\n"
            "  results[6] = test_state.robots[0].path_status;\n"
            "  for (step = 0; step < 120 &&\n"
            "       test_state.robots[0].path_status != ROBOT_PATH_READY;\n"
            "       step++) update_pathfinding(&test_state);\n"
            "  results[7] = test_state.path_queue_head;\n"
            "  update_robots(&test_state, 1.0f);\n"
            "  results[8] = test_state.robots[0].position.x == blocked_x &&\n"
            "       test_state.robots[0].position.y == blocked_y;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "path_test.c")))
        while cpu.running:
            cpu.step()
        values = [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(10)]
        self.assertEqual(values[:4], [
            24, 24, 2, 3,
        ])
        self.assertEqual(values[4:7], [4, 4, 1])
        self.assertEqual(values[8], 0)
        self.assertLessEqual(values[9], 32)

    def test_robot_reverses_and_reuses_its_cached_route(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int results[6];\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  Robot* robot;\n"
            "  game_init(&test_state);\n"
            "  test_state.buildings[1].type = BUILDING_MINE;\n"
            "  test_state.buildings[1].position.x = 10;\n"
            "  test_state.buildings[1].position.y = 10;\n"
            "  test_state.buildings[2].type = BUILDING_FACTORY;\n"
            "  test_state.buildings[2].position.x = 11;\n"
            "  test_state.buildings[2].position.y = 11;\n"
            "  test_state.building_count = 3;\n"
            "  robot = &test_state.robots[0];\n"
            "  robot->source_building = 1;\n"
            "  robot->target_building = 2;\n"
            "  robot->route_valid = 1;\n"
            "  robot->route_direction = 1;\n"
            "  robot->path_length = 2;\n"
            "  test_state.robot_paths[0][0] = 1;\n"
            "  test_state.robot_paths[0][1] = 3;\n"
            "  robot->position.x = 11;\n"
            "  robot->position.y = 11;\n"
            "  results[0] = use_cached_robot_route(&test_state, 0, -1);\n"
            "  results[1] = test_state.robot_paths[0][0];\n"
            "  results[2] = test_state.robot_paths[0][1];\n"
            "  robot->position.x = 10;\n"
            "  robot->position.y = 10;\n"
            "  results[3] = use_cached_robot_route(&test_state, 0, 1);\n"
            "  results[4] = test_state.robot_paths[0][0];\n"
            "  results[5] = test_state.robot_paths[0][1];\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "cached_route_test.c")))
        while cpu.running:
            cpu.step()
        values = [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(6)]
        self.assertEqual(values, [1, 2, 0, 1, 1, 3])

    def test_robot_moves_one_block_per_second(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int results[2];\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  int step;\n"
            "  game_init(&test_state);\n"
            "  test_state.buildings[1].id = 2;\n"
            "  test_state.buildings[1].type = BUILDING_MINE;\n"
            "  test_state.buildings[1].position.x = 25;\n"
            "  test_state.buildings[1].position.y = 24;\n"
            "  test_state.buildings[1].inventory_type = RES_IRON_ORE;\n"
            "  test_state.buildings[1].inventory_count = 1;\n"
            "  test_state.buildings[2].id = 3;\n"
            "  test_state.buildings[2].type = BUILDING_FACTORY;\n"
            "  test_state.buildings[2].position.x = 26;\n"
            "  test_state.buildings[2].position.y = 24;\n"
            "  test_state.building_count = 3;\n"
            "  test_state.robots[0].state = ROBOT_TO_SOURCE;\n"
            "  test_state.robots[0].source_building = 1;\n"
            "  test_state.robots[0].target_building = 2;\n"
            "  test_state.robots[0].path_status = ROBOT_PATH_READY;\n"
            "  test_state.robots[0].path_length = 1;\n"
            "  test_state.robot_paths[0][0] = 1;\n"
            "  for (step = 0; step < 19; step++)\n"
            "    update_robots(&test_state, 0.05f);\n"
            "  results[0] = test_state.robots[0].position.x;\n"
            "  update_robots(&test_state, 0.05f);\n"
            "  results[1] = test_state.robots[0].position.x;\n"
            "  update_robots(&test_state, 0.0f);\n"
            "  update_robots(&test_state, 0.5f);\n"
            "  update_pathfinding(&test_state);\n"
            "  update_robots(&test_state, 0.5f);\n"
            "  update_robots(&test_state, 0.0f);\n"
            "  update_robots(&test_state, 0.5f);\n"
            "  update_robots(&test_state, 0.0f);\n"
            "  update_robots(&test_state, 0.5f);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "robot_speed_test.c")))
        while cpu.running:
            cpu.step()
        values = [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(2)]
        self.assertEqual(values, [24, 25])
        console = cpu._console_device().output.decode("ascii")
        self.assertIn(
            "Robot arrived: Robot 1 at Mine(2) at (25,24)",
            console,
        )
        self.assertIn(
            "Robot departs: Robot 1 from Mine(2) at (25,24)",
            console,
        )
        self.assertIn(
            "Robot 1 route found: Mine(2) at (25,24) -> "
            "Factory(3) at (26,24), 1 blocks",
            console,
        )
        for event in (
            "Robot load started: Robot 1 at Mine(2) at (25,24)",
            "Robot load ended: Robot 1 at Mine(2) at (25,24)",
            "Robot unload started: Robot 1 at Factory(3) at (26,24)",
            "Robot unload ended: Robot 1 at Factory(3) at (26,24)",
        ):
            self.assertIn(event, console)

    def test_factory_empty_and_full_events_are_logged_on_transitions(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "GameState test_state;\n"
            "int main(void) {\n"
            "  Building* factory;\n"
            "  game_init(&test_state);\n"
            "  factory = &test_state.buildings[1];\n"
            "  factory->id = 2;\n"
            "  factory->type = BUILDING_FACTORY;\n"
            "  factory->position.x = 12;\n"
            "  factory->position.y = 13;\n"
            "  factory->factory_status = 0;\n"
            "  test_state.building_count = 2;\n"
            "  update_factory_status(&test_state);\n"
            "  update_factory_status(&test_state);\n"
            "  factory->inventory_count = BUILDING_BUFFER_CAP;\n"
            "  update_factory_status(&test_state);\n"
            "  update_factory_status(&test_state);\n"
            "  factory->inventory_count = 0;\n"
            "  update_factory_status(&test_state);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "factory_status_test.c")))
        while cpu.running:
            cpu.step()
        console = cpu._console_device().output.decode("ascii")
        self.assertEqual(
            console.count("Factory(2) at (12,13) is empty"),
            2,
        )
        self.assertEqual(
            console.count("Factory(2) at (12,13) is full"),
            1,
        )

    def test_factory_refines_iron_and_copper_ore_in_batches(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int results[9];\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  Building* iron;\n"
            "  Building* copper;\n"
            "  Building* coal;\n"
            "  Robot* waiting;\n"
            "  game_init(&test_state);\n"
            "  iron = &test_state.buildings[1];\n"
            "  iron->type = BUILDING_FACTORY;\n"
            "  iron->inventory_type = RES_IRON_ORE;\n"
            "  iron->inventory_count = 2;\n"
            "  copper = &test_state.buildings[2];\n"
            "  copper->type = BUILDING_FACTORY;\n"
            "  copper->inventory_type = RES_COPPER_ORE;\n"
            "  copper->inventory_count = 1;\n"
            "  coal = &test_state.buildings[3];\n"
            "  coal->type = BUILDING_FACTORY;\n"
            "  coal->inventory_type = RES_COAL;\n"
            "  coal->inventory_count = 1;\n"
            "  test_state.building_count = 4;\n"
            "  waiting = &test_state.robots[1];\n"
            "  waiting->state = ROBOT_UNLOADING;\n"
            "  waiting->source_building = 0;\n"
            "  waiting->target_building = 1;\n"
            "  waiting->carrying_type = RES_IRON_ORE;\n"
            "  waiting->carrying_amount = 1;\n"
            "  test_state.robot_count = 2;\n"
            "  update_factories(&test_state, 0.5f);\n"
            "  results[0] = iron->inventory_type;\n"
            "  results[1] = copper->inventory_type;\n"
            "  update_robots(&test_state, 0.5f);\n"
            "  results[2] = iron->inventory_count;\n"
            "  results[3] = waiting->carrying_amount;\n"
            "  update_factories(&test_state, 0.5f);\n"
            "  results[4] = copper->inventory_type;\n"
            "  update_factories(&test_state, 1.0f);\n"
            "  results[5] = iron->inventory_type;\n"
            "  results[6] = iron->inventory_count;\n"
            "  results[7] = coal->inventory_type;\n"
            "  assign_robot_route(&test_state, 3, 1);\n"
            "  results[8] = test_state.robot_count;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "factory_test.c")))
        while cpu.running:
            cpu.step()
        values = [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(9)]
        self.assertEqual(values, [
            2, 3, 2, 1, 5, 4, 2, 1, 2,
        ])

    def test_factory_assembles_rombonent_from_the_required_plate_ratio(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int results[7];\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  Building* factory;\n"
            "  game_init(&test_state);\n"
            "  factory = &test_state.buildings[1];\n"
            "  factory->type = BUILDING_FACTORY;\n"
            "  factory->factory_iron_plate_input = 2;\n"
            "  factory->factory_copper_plate_input = 1;\n"
            "  test_state.building_count = 2;\n"
            "  update_factories(&test_state, 1.0f);\n"
            "  results[0] = factory->inventory_count;\n"
            "  results[1] = factory->factory_iron_plate_input;\n"
            "  results[2] = factory->factory_copper_plate_input;\n"
            "  factory->factory_copper_plate_input = 4;\n"
            "  update_factories(&test_state, 0.5f);\n"
            "  update_factories(&test_state, 0.5f);\n"
            "  results[3] = factory->inventory_type;\n"
            "  results[4] = factory->inventory_count;\n"
            "  results[5] = factory->factory_copper_plate_input;\n"
            "  update_resource_totals(&test_state);\n"
            "  results[6] = test_state.total_resources[RES_ROMBONENT];\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "rombonent_test.c")))
        while cpu.running:
            cpu.step()
        values = [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(7)]
        self.assertEqual(values, [0, 2, 1, 6, 1, 2, 1])

    def test_factory_accepts_plate_deliveries_into_separate_recipe_buffers(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int results[5];\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  Building* factory;\n"
            "  Robot* robot;\n"
            "  game_init(&test_state);\n"
            "  factory = &test_state.buildings[1];\n"
            "  factory->type = BUILDING_FACTORY;\n"
            "  factory->position.x = 20;\n"
            "  factory->position.y = 20;\n"
            "  test_state.building_count = 2;\n"
            "  robot = &test_state.robots[1];\n"
            "  robot->position = factory->position;\n"
            "  robot->state = ROBOT_UNLOADING;\n"
            "  robot->target_building = 1;\n"
            "  robot->carrying_type = RES_IRON_PLATE;\n"
            "  robot->carrying_amount = 1;\n"
            "  test_state.robot_count = 2;\n"
            "  update_robots(&test_state, 0.5f);\n"
            "  robot->state = ROBOT_UNLOADING;\n"
            "  robot->carrying_type = RES_COPPER_PLATE;\n"
            "  robot->carrying_amount = 1;\n"
            "  update_robots(&test_state, 0.5f);\n"
            "  update_resource_totals(&test_state);\n"
            "  results[0] = factory->factory_iron_plate_input;\n"
            "  results[1] = factory->factory_copper_plate_input;\n"
            "  results[2] = factory->inventory_count;\n"
            "  results[3] = test_state.total_resources[RES_IRON_PLATE];\n"
            "  results[4] = test_state.total_resources[RES_COPPER_PLATE];\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "factory_input_test.c")))
        while cpu.running:
            cpu.step()
        values = [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(5)]
        self.assertEqual(values, [1, 1, 0, 1, 1])

    def test_factory_builds_a_robot_from_rombonent_and_iron_plate(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int results[9];\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  Building* factory;\n"
            "  Building* storage;\n"
            "  Robot* robot;\n"
            "  game_init(&test_state);\n"
            "  factory = &test_state.buildings[1];\n"
            "  factory->type = BUILDING_FACTORY;\n"
            "  factory->position.x = 20;\n"
            "  factory->position.y = 20;\n"
            "  factory->factory_robot_rombonent_input = 1;\n"
            "  factory->factory_iron_plate_input = 1;\n"
            "  storage = &test_state.buildings[2];\n"
            "  storage->type = BUILDING_STORAGE;\n"
            "  storage->position.x = 19;\n"
            "  storage->position.y = 20;\n"
            "  storage->inventory_type = RES_ROMBONENT;\n"
            "  storage->inventory_count = 1;\n"
            "  test_state.building_count = 3;\n"
            "  assign_robot_route(&test_state, 2, 1);\n"
            "  update_factories(&test_state, 1.0f);\n"
            "  robot = &test_state.robots[1];\n"
            "  results[0] = test_state.robot_count;\n"
            "  results[1] = robot->state;\n"
            "  results[2] = robot->position.x;\n"
            "  results[3] = robot->position.y;\n"
            "  results[4] = factory->factory_robot_rombonent_input;\n"
            "  results[5] = factory->factory_iron_plate_input;\n"
            "  update_resource_totals(&test_state);\n"
            "  results[6] = test_state.total_resources[RES_ROMBONENT];\n"
            "  results[7] = test_state.robots[0].path_status;\n"
            "  results[8] = test_state.robots[0].state;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "robot_factory_test.c")))
        while cpu.running:
            cpu.step()
        values = [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(9)]
        self.assertEqual(values, [2, 0, 20, 20, 0, 0, 1, 1, 1])

    def test_factory_preserves_robot_recipe_inputs_at_fleet_limit(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int results[3];\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  Building* factory;\n"
            "  game_init(&test_state);\n"
            "  factory = &test_state.buildings[1];\n"
            "  factory->type = BUILDING_FACTORY;\n"
            "  factory->factory_robot_rombonent_input = 1;\n"
            "  factory->factory_iron_plate_input = 1;\n"
            "  test_state.building_count = 2;\n"
            "  test_state.robot_count = MAX_ROBOTS;\n"
            "  update_factories(&test_state, 2.0f);\n"
            "  results[0] = test_state.robot_count;\n"
            "  results[1] = factory->factory_robot_rombonent_input;\n"
            "  results[2] = factory->factory_iron_plate_input;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "robot_limit_test.c")))
        while cpu.running:
            cpu.step()
        values = [cpu.read_16(GLOBAL_BASE + 2 * i) for i in range(3)]
        self.assertEqual(values, [8, 1, 1])

    def test_right_click_deletes_building_but_preserves_base(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int results[1];\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  game_init(&test_state);\n"
            "  test_state.buildings[1].type = BUILDING_FACTORY;\n"
            "  test_state.buildings[1].position.x = 25;\n"
            "  test_state.buildings[1].position.y = 24;\n"
            "  test_state.building_count = 2;\n"
            "  while (1) {\n"
            "    game_update(&test_state, 0.016f);\n"
            "    results[0] = test_state.building_count;\n"
            "    game_draw(&test_state);\n"
            "    gfx_present();\n"
            "  }\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "delete_test.c")))
        for _ in range(4):
            run_frame(cpu, limit=30000)
            if any(vram(cpu)):
                break

        cpu.set_mouse_position(176, 119)
        cpu.mouse_button(1, True, 176, 119, 1000)
        cpu.end_mouse_frame()
        cpu.mouse_button(1, False, 176, 119, 1010)
        for _ in range(5):
            run_frame(cpu, limit=30000)
            if cpu.read_16(GLOBAL_BASE) == 1:
                break
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 1)

        cpu.set_mouse_position(160, 119)
        cpu.mouse_button(1, True, 160, 119, 1200)
        cpu.end_mouse_frame()
        cpu.mouse_button(1, False, 160, 119, 1210)
        for _ in range(5):
            run_frame(cpu, limit=30000)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 1)

    def test_mouse_wheel_changes_map_zoom(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "MotoRobots" / "main.c"))
        for _ in range(4):
            run_frame(cpu, limit=30000)
            if any(vram(cpu)):
                break
        before = vram(cpu)
        cpu.set_mouse_position(160, 120)
        cpu.set_mouse_wheel(1)
        for _ in range(4):
            run_frame(cpu, limit=30000)
            cpu.end_mouse_frame()
            if cpu.frame_yield:
                break
        self.assertNotEqual(before, vram(cpu))
        self.assertIn("Zoom level 3 of 3", cpu.console_device.output.decode("ascii"))

    def test_mouse_wheel_multi_notch_input_advances_only_one_zoom_level(self):
        source = (
            '#include "game.c"\n'
            '#include "map.c"\n'
            "int zoom_result;\n"
            "GameState test_state;\n"
            "int main(void) {\n"
            "  game_init(&test_state);\n"
            "  test_state.zoom = 0;\n"
            "  while (1) {\n"
            "    game_update(&test_state, 0.016f);\n"
            "    zoom_result = test_state.zoom;\n"
            "    game_draw(&test_state);\n"
            "    gfx_present();\n"
            "  }\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            source, str(ROOT / "modules" / "MotoRobots" / "zoom_step_test.c")))
        run_frame(cpu, limit=200000)
        cpu.set_mouse_position(160, 120)
        cpu.set_mouse_wheel(2)
        run_frame(cpu, limit=50000)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 1)


class PreprocessorTests(unittest.TestCase):
    def test_define_and_include(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.h").write_text("#define TWO (1 + 1)\n#define FOUR TWO + TWO // note\n")
            source = root / "m.c"
            source.write_text('#include "a.h"\nint r;\nint main(void) { r = FOUR; return 0; }\n')
            cpu = make_cpu(compile_file(source))
            while cpu.running:
                cpu.step()
            self.assertEqual(cpu.read_16(GLOBAL_BASE), 4)

    def test_function_macros_conditionals_stringification_and_token_pasting(self):
        source = (
            "#define JOIN(a, b) a ## b\n"
            "#define STRINGIFY(value) #value\n"
            "#define TWICE(value) ((value) + (value))\n"
            "#define VERSION 3\n"
            "#if defined(VERSION) && VERSION == 3 && (0 ? 1 / 0 : 1)\n"
            "#define TARGET JOIN(an, swer)\n"
            "#else\n"
            '#error "unexpected conditional branch"\n'
            "#endif\n"
            "#undef VERSION\n"
            "#ifdef VERSION\n"
            "#error VERSION should have been undefined\n"
            "#endif\n"
            "int answer;\n"
            "char word[3] = STRINGIFY(ok);\n"
            "int main(void) { TARGET = TWICE(10); return 0; }\n"
        )
        cpu = make_cpu(compile_source(source))
        while cpu.running:
            cpu.step()
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 20)
        self.assertEqual([cpu.read(GLOBAL_BASE + offset) for offset in (2, 3, 4)], [111, 107, 0])

    def test_c90_translation_phases_and_pragma(self):
        source = (
            "??=define VALUE 4\\\n"
            "2\n"
            "#pragma implementation_specific\n"
            "int result;\n"
            'char unrecognized_trigraph[4] = "??x";\n'
            "int main(void) {\n"
            "#if VALUE == 42\n"
            "  result = VALUE;\n"
            "#else\n"
            "  result = 0;\n"
            "#endif\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source))
        while cpu.running:
            cpu.step()
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 42)
        self.assertEqual(
            [cpu.read(GLOBAL_BASE + offset) for offset in (2, 3, 4, 5)],
            [ord("?"), ord("?"), ord("x"), 0],
        )

    def test_preprocessor_errors(self):
        cases = [
            "#define\nint main(void) { return 0; }\n",
            '#include "missing_file.h"\nint main(void) { return 0; }\n',
            "#include nothing\nint main(void) { return 0; }\n",
            "#error deliberate failure\nint main(void) { return 0; }\n",
        ]
        for source in cases:
            with self.subTest(source=source), self.assertRaises(CCompilerError):
                compile_source(source)

    def test_line_directive_remaps_diagnostic_line(self):
        source = (
            "#define ERROR_LINE 60\n"
            '#line ERROR_LINE "virtual.c"\n'
            "int main(void) { int broken = ; return 0; }\n"
        )
        with self.assertRaisesRegex(CCompilerError, r"virtual\.c:60:"):
            compile_source(source)

    def test_line_directive_rejects_zero(self):
        with self.assertRaisesRegex(CCompilerError, r"<input>:1: invalid #line"):
            compile_source("#line 0\nint main(void) { return 0; }\n")

    def test_line_directive_remaps_preprocessor_diagnostic(self):
        with self.assertRaisesRegex(CCompilerError, r"<input>:72: #error"):
            compile_source("#line 72\n#error deliberate failure\n")

    def test_line_directive_remaps_preprocessor_filename(self):
        with self.assertRaisesRegex(CCompilerError, r"virtual\.c:72: #error"):
            compile_source('#line 72 "virtual.c"\n#error deliberate failure\n')

    def test_line_directive_filename_is_restored_after_include(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "main.c"
            (Path(directory) / "header.h").write_text(
                '#line 4 "included.c"\nint header_value;\n'
            )
            source.write_text(
                '#line 80 "parent.c"\n'
                '#include "header.h"\n'
                "int main(void) { int broken = ; return 0; }\n"
            )
            with self.assertRaisesRegex(CCompilerError, r"parent\.c:81:"):
                compile_source(source.read_text(), str(source))

    def test_recursive_include_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            loop = Path(directory) / "loop.h"
            loop.write_text('#include "loop.h"\n')
            source = Path(directory) / "m.c"
            source.write_text('#include "loop.h"\nint main(void) { return 0; }\n')
            with self.assertRaises(CCompilerError):
                compile_source(source.read_text(), str(source))


class GameTests(unittest.TestCase):
    def play(self, name, keys, frames):
        module_directories = {
            "anaclock.c": "AnaClock",
            "asteroids.c": "Asteroids",
            "elitedemo.c": "Elite",
            "lunarlander.c": "Lunar Lander",
            "pacman.c": "PacMan",
            "paint.c": "Paint",
            "space_invaders.c": "Space Invaders",
            "sc16_showcase.c": "SC-16 showcase",
        }
        path = ROOT / "modules" / module_directories.get(name, "") / name
        cpu = make_cpu(compile_file(path))
        for frame in range(frames):
            cpu.memory[KEYS_ADDR] = keys(frame)
            self.assertGreater(run_frame(cpu), 0)
        return cpu

    def test_asteroids_runs_and_scores_with_input(self):
        cpu = self.play("asteroids.c", lambda f: 4 | 16 | (1 if f % 40 < 10 else 0), 120)
        self.assertTrue(cpu.running)
        self.assertGreater(sum(1 for byte in vram(cpu) if byte), 100)

    def test_asteroids_is_deterministic_idle_and_ship_visible(self):
        cpu = self.play("asteroids.c", lambda f: 0, 3)
        self.assertEqual(pixel(cpu, 0, 0) in range(12), True)
        self.assertIn(6, vram(cpu))

    def test_space_invaders_runs_and_player_moves(self):
        idle = self.play("space_invaders.c", lambda f: 0, 30)
        moving = self.play("space_invaders.c", lambda f: 2, 30)
        self.assertTrue(idle.running and moving.running)
        self.assertNotEqual(vram(idle), vram(moving))
        self.assertIn(3, vram(idle))
        self.assertIn(7, vram(idle))

    def test_pacman_runs_eats_pellets_and_shows_ghosts(self):
        idle = self.play("pacman.c", lambda f: 0, 20)
        self.assertTrue(idle.running)
        for color in (2, 4, 5, 6, 7, 8):
            self.assertIn(color, vram(idle))
        moving = self.play("pacman.c", lambda f: [1, 4, 2, 8][(f // 25) % 4], 300)
        self.assertTrue(moving.running)
        self.assertNotEqual(vram(idle), vram(moving))

    def test_anaclock_cycles_through_seven_distinct_faces(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "AnaClock" / "anaclock.c"))
        faces = []

        cpu.memory[KEYS_ADDR] = 0
        self.assertGreater(run_frame(cpu, 3_000_000), 0)
        faces.append(vram(cpu))
        for _ in range(6):
            cpu.memory[KEYS_ADDR] = 64
            self.assertGreater(run_frame(cpu, 3_000_000), 0)
            faces.append(vram(cpu))
            cpu.memory[KEYS_ADDR] = 0
            self.assertGreater(run_frame(cpu, 3_000_000), 0)

        self.assertTrue(cpu.running)
        self.assertEqual(len(set(faces)), 7)

    def play_elite(self, keys, frames):
        cpu = make_cpu(compile_file(ROOT / "modules" / "Elite" / "elitedemo.c"))
        for frame in range(frames):
            low, ext = keys(frame)
            cpu.memory[KEYS_ADDR] = low
            cpu.memory[KEYS_ADDR + 1] = ext
            self.assertGreater(run_frame(cpu, 5_000_000), 0)
        return cpu

    def test_elitedemo_flies_with_thrust_turn_roll_and_fire(self):
        cpu = self.play_elite(
            lambda f: (
                (1 if 20 <= f < 40 else 4 if 40 <= f < 60 else 0) | (16 if f % 6 == 0 else 0),
                (4 if f < 40 else 1 if f < 60 else 0),
            ),
            90,
        )
        self.assertTrue(cpu.running)
        self.assertGreater(sum(1 for byte in vram(cpu) if byte), 100)

    def test_elitedemo_moons_orbit_and_scene_changes(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "Elite" / "elitedemo.c"))
        run_frame(cpu, 5_000_000)
        first = bytes(vram(cpu))
        for _ in range(40):
            run_frame(cpu, 5_000_000)
        self.assertNotEqual(first, bytes(vram(cpu)))
    def test_showcase_speed_benchmark_uses_speed_register(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "SC-16 showcase" / "sc16_showcase.c"))
        run_frame(cpu, 400_000)
        for _ in range(8):
            cpu.memory[KEYS_ADDR] = 2
            run_frame(cpu, 400_000)
            cpu.memory[KEYS_ADDR] = 0
            cpu.memory[0x43002] |= 2
            run_frame(cpu, 400_000)
            run_frame(cpu, 400_000)
        estimate = vram(cpu)
        cpu.memory[KEYS_ADDR] = 32
        run_frame(cpu, 400_000)
        cpu.memory[KEYS_ADDR] = 0
        run_frame(cpu, 400_000)
        run_frame(cpu, 400_000)
        self.assertEqual(cpu.memory[0x43003], SPEED_MAX)
        for _ in range(8):
            cpu.memory[0x43010] = (cpu.memory[0x43010] + 1) % 60
            run_frame(cpu, 400_000)
        self.assertEqual(cpu.memory[0x43003], SPEED_MAX)
        measured = vram(cpu)
        estimate_chart = estimate[90 * SCREEN_WIDTH + 160:90 * SCREEN_WIDTH + 312]
        measured_chart = measured[90 * SCREEN_WIDTH + 160:90 * SCREEN_WIDTH + 312]
        self.assertNotEqual(estimate_chart, measured_chart)
        self.assertNotEqual(estimate, measured)

    def test_showcase_benchmark_reports_every_workload(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "SC-16 showcase" / "sc16_showcase.c"))
        run_frame(cpu, 400_000)
        for _ in range(7):
            cpu.memory[KEYS_ADDR] = 2
            run_frame(cpu, 400_000)
            cpu.memory[KEYS_ADDR] = 0
            cpu.memory[0x43002] |= 2
            run_frame(cpu, 400_000)
            run_frame(cpu, 400_000)
        cpu.memory[KEYS_ADDR] = 32
        run_frame(cpu, 400_000)
        cpu.memory[KEYS_ADDR] = 0
        run_frame(cpu, 400_000)
        for _ in range(32):
            cpu.frames = (cpu.frames + 29) & 0xFFFF
            run_frame(cpu, 400_000)
        bar_colors = (2, 8, 5, 3, 6, 4, 7)
        for i, color in enumerate(bar_colors):
            self.assertEqual(pixel(cpu, 150, 36 + i * 20), color)

    def test_showcase_pages_and_interrupt_counters(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "SC-16 showcase" / "sc16_showcase.c"))
        self.addCleanup(lambda: cpu.net.transport.close() if getattr(cpu, "net", None) else None)
        run_frame(cpu, 400_000)
        seen = set()
        for page in range(14):
            cpu.memory[KEYS_ADDR] = 2
            run_frame(cpu, 400_000)
            cpu.memory[KEYS_ADDR] = 0
            cpu.memory[0x43002] |= 2
            run_frame(cpu, 400_000)
            run_frame(cpu, 400_000)
            seen.add(bytes(vram(cpu)))
            if page == 1:
                self.assertEqual(pixel(cpu, 64, 45), 3)
                self.assertEqual(pixel(cpu, 112, 45), 8)
                self.assertEqual(pixel(cpu, 136, 45), 5)
                self.assertEqual(pixel(cpu, 167, 45), 7)
                self.assertEqual(pixel(cpu, 176, 45), 6)
                self.assertEqual(pixel(cpu, 224, 45), 10)
        self.assertEqual(len(seen), 14)
        self.assertTrue(cpu.running)
        self.assertGreater(cpu.read_16(0x4340), 10)
        self.assertGreaterEqual(cpu.read_16(0x4342), 1)

    def test_showcase_3d_ship_flies_and_changes_orientation(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "SC-16 showcase" / "sc16_showcase.c"))
        self.addCleanup(lambda: cpu.net.transport.close() if getattr(cpu, "net", None) else None)
        run_frame(cpu, 400_000)
        for _ in range(12):
            cpu.memory[KEYS_ADDR] = 2
            run_frame(cpu, 400_000)
            cpu.memory[KEYS_ADDR] = 0
            cpu.memory[0x43002] |= 2
            run_frame(cpu, 400_000)
            run_frame(cpu, 400_000)
        first = pygame.image.tobytes(cpu.vram_surface, "P")
        for _ in range(32):
            run_frame(cpu, 400_000)
        last = pygame.image.tobytes(cpu.vram_surface, "P")
        ship_area = b"".join(last[y * SCREEN_WIDTH + 80:y * SCREEN_WIDTH + 240] for y in range(50, 180))
        self.assertIn(6, ship_area)
        self.assertNotEqual(first, last)
        first_cyan = [
            (x, y)
            for y in range(50, 180)
            for x in range(50, 270)
            if first[y * SCREEN_WIDTH + x] == 6
        ]
        last_cyan = [
            (x, y)
            for y in range(50, 180)
            for x in range(50, 270)
            if last[y * SCREEN_WIDTH + x] == 6
        ]
        first_center_x = sum(point[0] for point in first_cyan) // len(first_cyan)
        last_center_x = sum(point[0] for point in last_cyan) // len(last_cyan)
        self.assertGreater(abs(last_center_x - first_center_x), 8)
        self.assertTrue(cpu.running)

    def test_showcase_speed_comparison_measures_the_max_speed_loop(self):
        path = ROOT / "modules" / "SC-16 showcase" / "sc16_showcase.c"
        source = path.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n{")] + (
            "\nint main(void) {\n"
            "  gfx_speed(SPEED_FAST);\n"
            "  gfx_vsync(1);\n"
            "  bench_max();\n"
            "  cmp_mips_tenths = kops_to_mips_tenths(cmp_kops);\n"
            "  *((unsigned int *)0x10000000) = cmp_kops;\n"
            "  *((unsigned int *)0x10000004) = cmp_mips_tenths;\n"
            "  *((unsigned char *)0x10000008) = *((unsigned char *)0x43003);\n"
            "  *((unsigned char *)0x10000009) = *((unsigned char *)0x43013);\n"
            "  *((unsigned long *)0x10000010) = 0x5343554D;\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(path)))
        self.addCleanup(lambda: cpu.net.transport.close() if getattr(cpu, "net", None) else None)
        rtc_modes = []
        original_gpu_command = cpu.gpu_command

        def observe_rtc_mode(command):
            if command == 12:
                rtc_modes.append(
                    (cpu.memory[SPEED_ADDR], cpu.memory[VSYNC_ADDR])
                )
            original_gpu_command(command)

        cpu.gpu_command = observe_rtc_mode
        for _ in range(200):
            run_frame(cpu, limit=400_000)
            if cpu.read_32(0x10000010) == 0x5343554D:
                break
        self.assertEqual(
            cpu.read_32(0x10000010),
            0x5343554D,
            f"RTC calls={len(rtc_modes)}, RTC={list(cpu.memory[0x43010:0x43013])}, "
            f"speed={cpu.memory[SPEED_ADDR]}, vsync={cpu.memory[VSYNC_ADDR]}, "
            f"pc={cpu.pc:#x}, running={cpu.running}",
        )
        self.assertTrue(rtc_modes)
        self.assertTrue(all(mode == (SPEED_MAX, 0) for mode in rtc_modes))
        self.assertGreater(cpu.read_16(0x10000000), 0)
        kops = cpu.read_16(0x10000000)
        expected_mips_tenths = (kops * 533 + 1600) // 3200
        self.assertEqual(cpu.read_16(0x10000004), expected_mips_tenths)
        self.assertEqual(cpu.memory[0x10000008], SPEED_FAST)
        self.assertEqual(cpu.memory[0x10000009], 1)

    def test_showcase_sprite_load_animates_a_continuous_worm_in_both_axes(self):
        path = ROOT / "modules" / "SC-16 showcase" / "sc16_showcase.c"
        source = path.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n{")]
        draw_call = "        gfx_sprite(x, y, 8, 8, smiley);"
        self.assertEqual(source.count(draw_call), 1)
        source = source.replace(
            draw_call,
            "        *((int *)(0x10000000 + frame * 0x100 + i * 4)) = x;\n"
            "        *((int *)(0x10000002 + frame * 0x100 + i * 4)) = y;",
        )
        source += (
            "\nint main(void) {\n"
            "  gfx_speed(SPEED_MAX);\n"
            "  gfx_vsync(0);\n"
            "  stress_sprite_count = 16;\n"
            "  stress_last_ticks = gfx_ticks();\n"
            "  stress_last_second = host_seconds();\n"
            "  frame = 0;\n"
            "  page_sprite_load();\n"
            "  frame = 8;\n"
            "  page_sprite_load();\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(path)))
        self.addCleanup(lambda: cpu.net.transport.close() if getattr(cpu, "net", None) else None)
        steps = 0
        while cpu.running and steps < 200_000:
            cpu.step()
            steps += 1
        self.assertFalse(cpu.running, "instrumented sprite page did not finish")

        frames = []
        for frame in (0, 8):
            positions = []
            for index in range(16):
                address = 0x10000000 + frame * 0x100 + index * 4
                positions.append((cpu.read_16(address), cpu.read_16(address + 2)))
            frames.append(positions)

        first_frame, later_frame = frames
        self.assertNotEqual(first_frame[0][0], later_frame[0][0])
        self.assertNotEqual(first_frame[0][1], later_frame[0][1])
        for positions in frames:
            for index, (x, y) in enumerate(positions):
                self.assertGreaterEqual(x, 8)
                self.assertLessEqual(x, 312)
                self.assertGreaterEqual(y, 50)
                self.assertLessEqual(y, 190)
                if index:
                    previous_x, previous_y = positions[index - 1]
                    self.assertGreaterEqual(x - previous_x, 5)
                    self.assertLessEqual(x - previous_x, 11)
                    self.assertLessEqual(abs(y - previous_y), 7)

    def test_showcase_sprite_load_slows_down_and_recovers_after_temporary_fps_drop(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "SC-16 showcase" / "sc16_showcase.c"))
        self.addCleanup(lambda: cpu.net.transport.close() if getattr(cpu, "net", None) else None)
        run_frame(cpu, 400_000)
        for _ in range(13):
            cpu.memory[KEYS_ADDR] = 2
            run_frame(cpu, 400_000)
            cpu.memory[KEYS_ADDR] = 0
            cpu.memory[0x43002] |= 2
            run_frame(cpu, 400_000)
            run_frame(cpu, 400_000)
        self.assertEqual(cpu.memory[0x43003], SPEED_MAX)
        self.assertEqual(cpu.memory[VSYNC_ADDR], 0)
        base_seconds = cpu.memory[0x43011] * 60 + cpu.memory[0x43010]
        elapsed_seconds = [0]
        frame_count = [0]
        frames_per_second = [30]

        def fake_update_rtc():
            frame_count[0] += 1
            if frame_count[0] % frames_per_second[0] == 0:
                elapsed_seconds[0] += 1
            total = (base_seconds + elapsed_seconds[0]) % 3600
            cpu.memory[0x43010] = total % 60
            cpu.memory[0x43011] = total // 60

        cpu.update_rtc = fake_update_rtc
        initial_count_display = b"".join(
            vram(cpu)[y * SCREEN_WIDTH + 68:y * SCREEN_WIDTH + 92] for y in range(38, 46)
        )
        for _ in range(450):
            run_frame(cpu, 400_000)
        self.assertEqual(cpu.memory[0x43003], SPEED_MAX)
        green_status = any(pixel(cpu, x, y) == 3 for x in range(8, 312) for y in range(216, 224))
        self.assertTrue(green_status)
        image = vram(cpu)
        snake_x = [
            x for x in range(SCREEN_WIDTH)
            if any(image[y * SCREEN_WIDTH + x] == 5 for y in range(45, 206))
        ]
        self.assertGreater(snake_x[-1] - snake_x[0], 100)
        snake_gaps = [snake_x[i + 1] - snake_x[i] for i in range(len(snake_x) - 1)]
        wrap_gap = max(snake_gaps)
        snake_gaps.remove(wrap_gap)
        self.assertLess(wrap_gap, 200)
        self.assertLessEqual(max(snake_gaps), 3)
        count_display = b"".join(
            vram(cpu)[y * SCREEN_WIDTH + 68:y * SCREEN_WIDTH + 92] for y in range(38, 46)
        )
        self.assertNotEqual(initial_count_display, count_display)
        frames_per_second[0] = 20
        frame_count[0] = 0
        for _ in range(20):
            run_frame(cpu, 400_000)
        self.assertEqual(cpu.memory[0x43003], SPEED_MAX)
        red_status = any(pixel(cpu, x, y) == 2 for x in range(8, 312) for y in range(216, 224))
        self.assertFalse(red_status)
        self.assertEqual(
            count_display,
            b"".join(vram(cpu)[y * SCREEN_WIDTH + 68:y * SCREEN_WIDTH + 92] for y in range(38, 46)),
        )
        frames_per_second[0] = 30
        frame_count[0] = 0
        for _ in range(30):
            run_frame(cpu, 400_000)
        self.assertEqual(
            count_display,
            b"".join(vram(cpu)[y * SCREEN_WIDTH + 68:y * SCREEN_WIDTH + 92] for y in range(38, 46)),
        )
        frames_per_second[0] = 31
        frame_count[0] = 0
        for _ in range(31):
            run_frame(cpu, 400_000)
        self.assertNotEqual(
            count_display,
            b"".join(vram(cpu)[y * SCREEN_WIDTH + 68:y * SCREEN_WIDTH + 92] for y in range(38, 46)),
        )
        cpu.memory[KEYS_ADDR] = 2
        run_frame(cpu, 400_000)
        self.assertEqual(cpu.memory[0x43003], SPEED_MAX)
        self.assertEqual(cpu.memory[VSYNC_ADDR], 1)

    def test_showcase_sprite_load_uses_ten_then_one_sprite_per_second(self):
        path = ROOT / "modules" / "SC-16 showcase" / "sc16_showcase.c"
        source = path.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n{")] + (
            "\nint main(void) {\n"
            "  gfx_speed(SPEED_MAX);\n"
            "  gfx_vsync(0);\n"
            "  stress_sprite_count = 199;\n"
            "  stress_slow_mode = 0;\n"
            "  stress_last_ticks = gfx_ticks() - 1;\n"
            "  stress_last_second = host_seconds();\n"
            "  while (1) {\n"
            "    page_sprite_load();\n"
            "    *((unsigned int *)0x10000000) = stress_sprite_count;\n"
            "    *((unsigned int *)0x10000002) = stress_slow_mode;\n"
            "    gfx_present();\n"
            "  }\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(path)))
        self.addCleanup(lambda: cpu.net.transport.close() if getattr(cpu, "net", None) else None)
        elapsed_seconds = [0]
        frame_count = [0]
        frames_per_second = [31]

        def fake_update_rtc():
            frame_count[0] += 1
            if frame_count[0] % frames_per_second[0] == 0:
                elapsed_seconds[0] += 1
            cpu.memory[0x43010] = elapsed_seconds[0] % 60
            cpu.memory[0x43011] = elapsed_seconds[0] // 60

        cpu.update_rtc = fake_update_rtc
        for _ in range(62):
            run_frame(cpu, 400_000)
        self.assertEqual(cpu.read_16(0x10000000), 219)
        self.assertEqual(cpu.read_16(0x10000002), 0)

        frames_per_second[0] = 20
        frame_count[0] = 0
        for _ in range(40):
            run_frame(cpu, 400_000)
        self.assertEqual(cpu.read_16(0x10000000), 219)
        self.assertEqual(cpu.read_16(0x10000002), 1)

        frames_per_second[0] = 30
        frame_count[0] = 0
        for _ in range(30):
            run_frame(cpu, 400_000)
        self.assertEqual(cpu.read_16(0x10000000), 219)

        frames_per_second[0] = 31
        frame_count[0] = 0
        for _ in range(31):
            run_frame(cpu, 400_000)
        self.assertEqual(cpu.read_16(0x10000000), 220)

    def test_lunar_lander_menu_flight_sound_and_crash(self):
        cpu = self.play("lunarlander.c", lambda f: 32 if f == 3 else 0, 8)
        self.assertIn(3, vram(cpu))
        cpu.sound_log.clear()
        cpu = self.play("lunarlander.c", lambda f: 32 if f == 3 else (16 if 8 <= f < 30 else 0), 30)
        self.assertTrue(any(call[0] == 0 and call[1] > 0 for call in cpu.sound_log))
        for frame in range(400):
            cpu.memory[KEYS_ADDR] = 0
            run_frame(cpu)
        self.assertTrue(any(call[0] == 2 and call[3] == 1 for call in cpu.sound_log))
        self.assertIn(2, vram(cpu))

    def test_games_emit_sound_effects(self):
        cases = [
            ("asteroids.c", lambda f: 4 | 16, 40, (0, 2)),
            ("space_invaders.c", lambda f: 16, 60, (0, 2)),
            ("pacman.c", lambda f: 0, 90, (0, 2)),
            ("sc16_showcase.c", lambda f: 2 if f == 3 else 0, 8, (2,)),
        ]
        for name, keys, frames, channels in cases:
            cpu = self.play(name, keys, frames)
            used = {call[0] for call in cpu.sound_log if call[1]}
            for channel in channels:
                self.assertIn(channel, used, name)

    def test_elitedemo_engine_and_laser_sounds(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "Elite" / "elitedemo.c"))
        for frame in range(12):
            cpu.memory[KEYS_ADDR] = 16
            cpu.memory[KEYS_ADDR + 1] = 4
            run_frame(cpu)
        used = {call[0] for call in cpu.sound_log if call[1]}
        self.assertIn(0, used)
        self.assertIn(2, used)

    def test_mouse_registers_clicks_double_click_and_drag(self):
        cpu = run_c("int main(void) { gfx_rect(gfx_mouse_x(), gfx_mouse_y(), 2, 2, gfx_mouse_buttons() + 3);"
                    " gfx_present(); return 0; }", frames=0)
        cpu.set_mouse_position(40, 30)
        cpu.mouse_button(0, True, 40, 30, 1000)
        self.assertEqual(cpu.memory[MOUSE_ADDR + 4], 1)
        self.assertEqual(cpu.memory[MOUSE_ADDR + 5], 1)
        cpu.memory[MOUSE_ADDR + 5] = 0
        cpu.mouse_button(0, False, 40, 30, 1100)
        self.assertEqual(cpu.memory[MOUSE_ADDR + 5], 16)
        cpu.memory[MOUSE_ADDR + 5] = 0
        cpu.mouse_button(0, True, 41, 30, 1200)
        self.assertEqual(cpu.memory[MOUSE_ADDR + 5], 1 | 4)
        cpu.memory[MOUSE_ADDR + 5] = 0
        cpu.mouse_button(1, True, 41, 30, 1250)
        self.assertEqual(cpu.memory[MOUSE_ADDR + 5], 2)
        cpu.set_mouse_position(500, -5)
        self.assertEqual(bytes(cpu.memory[MOUSE_ADDR:MOUSE_ADDR + 4]), bytes((1, 63, 0, 0)))
        cpu.set_mouse_position(40, 30)
        run_frame(cpu)
        self.assertEqual(back(cpu, 40, 30), 6)

    def test_mouse_wheel_builtin_reports_signed_frame_delta(self):
        cpu = run_c(
            "int main(void) {\n"
            "  gfx_rect(0, 0, gfx_mouse_wheel() + 2, 1, WHITE);\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n",
            frames=0,
        )
        cpu.set_mouse_wheel(-1)
        self.assertEqual(cpu.read_16(MOUSE_ADDR + 6), 0xFFFF)
        run_frame(cpu)
        self.assertEqual(back(cpu, 0, 0), 1)
        self.assertEqual(back(cpu, 1, 0), 0)
        cpu.end_mouse_frame()
        self.assertEqual(cpu.read_16(MOUSE_ADDR + 6), 0)

    def test_paint_draws_with_mouse_drag_and_shapes(self):
        cpu = make_cpu(compile_file(ROOT / "modules" / "Paint" / "paint.c"))
        clock = [0]

        def step(x, y, down=None, up=None):
            cpu.set_mouse_position(x, y)
            if down is not None:
                clock[0] += 1000
                cpu.mouse_button(down, True, x, y, clock[0])
            if up is not None:
                cpu.mouse_button(up, False, x, y, clock[0])
            run_frame(cpu)
            cpu.memory[MOUSE_ADDR + 5] = 0

        step(100, 100)
        self.assertEqual(pixel(cpu, 100, 100), 1)
        step(100, 100, down=0)
        for i in range(1, 20):
            step(100 + i * 3, 100 + i)
        step(160, 120, up=0)
        self.assertEqual(pixel(cpu, 130, 110), 0)
        step(30, 36, down=0)
        step(30, 36, up=0)
        step(60, 150, down=0)
        for i in range(1, 5):
            step(60 + i * 20, 150 + i * 10)
        step(140, 190, up=0)
        self.assertEqual(pixel(cpu, 100, 170), 0)
        step(150, 150)
        self.assertEqual(pixel(cpu, 150, 150), 1)
        step(10, 200, down=0)
        step(10, 200, up=0)
        cpu.set_mouse_position(10, 200)
        cpu.mouse_button(0, True, 10, 200, clock[0] + 100)
        run_frame(cpu)
        self.assertEqual(pixel(cpu, 250, 20), 10)

    def test_gfx_sound_and_static_layer_commands(self):
        cpu = run_c("int main(void) { gfx_sound(1, 440, 10, WAVE_TRIANGLE, 50); gfx_rect(0,0,4,4,3); gfx_save();"
                    " gfx_clear(0); gfx_restore(); gfx_present(); return 0; }")
        self.assertIn((1, 440, 10, 2, 50), cpu.sound_log)
        self.assertEqual(back(cpu, 1, 1), 3)


if __name__ == "__main__":
    unittest.main()
