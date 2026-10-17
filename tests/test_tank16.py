from pathlib import Path
import math
import json
import re
import runpy
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

import pygame

GAME_DIR = Path(__file__).resolve().parents[1] / "modules" / "Tank-16"
if str(GAME_DIR) not in sys.path:
    sys.path.insert(0, str(GAME_DIR))

from c_compiler import GLOBAL_BASE, compile_source
from gfx_harness import ROOT, layer_pixel, make_cpu, run_frame
from src.SuomiCPU import DISK_STATUS, KEYS_ADDR, load_program_file
from src.SuomiCPU import RTC_START
from src.lib_disk import DiskDevice
from tank16_map_loader import (
    RUNTIME_TILE_DATA_PATH, build_map_header, build_runtime_assets_header,
    discover_round_maps, load_map_assets, map_tile_pixel_size,
)


class Tank16Tests(unittest.TestCase):
    def game_source(self, source_path, tile_size=64):
        source = source_path.read_text(encoding="utf-8")
        return re.sub(r"#define MAP_TILE_PIXEL_SIZE \d+",
                      f"#define MAP_TILE_PIXEL_SIZE {tile_size}", source).replace(
                          "int main(void)", "int tank16_game_entry(void)", 1)

    def scenario_results(self, body, count, tile_size=64):
        path = ROOT / "modules" / "Tank-16" / "main.c"
        source = (f"int results[{count}];\n" + self.game_source(path, tile_size) +
                  "int main(void) {\n" + body + "\nreturn 0;\n}\n")
        cpu = make_cpu(compile_source(source, str(path)))
        for _ in range(100):
            run_frame(cpu)
            if not cpu.running:
                break
        self.assertFalse(cpu.running, "Scenario exceeded 40 million instructions")
        values = [cpu.read_16(GLOBAL_BASE + i * 2) for i in range(count)]
        return [value - 65536 if value >= 32768 else value for value in values]

    def test_throttle_coasts_brakes_reverses_and_stationary_turning_is_doubled(self):
        results = self.scenario_results(
            "int i;\n"
            "start_round();\n"
            "tanks[0].x = 100; tanks[0].y = 100; tanks[0].direction = 8;\n"
            "tanks[1].x = 500; tanks[1].y = 400;\n"
            "update_tank(0, 4, 0);\n"
            "results[0] = tanks[0].speed; results[1] = tanks[0].x;\n"
            "for (i = 1; i < 32; i++) update_tank(0, 4, 0);\n"
            "results[2] = tanks[0].speed; results[3] = tanks[0].x;\n"
            "for (i = 0; i < 8; i++) update_tank(0, 0, 0);\n"
            "results[4] = tanks[0].speed; results[5] = tanks[0].x;\n"
            "for (i = 0; i < 32; i++) update_tank(0, 8, 0);\n"
            "results[6] = tanks[0].speed;\n"
            "for (i = 0; i < 16; i++) update_tank(0, 8, 0);\n"
            "results[7] = tanks[0].speed; results[8] = tanks[0].x;\n"
            "tanks[0].speed = 0; tanks[0].throttle = 0; tanks[0].direction = 8;\n"
            "for (i = 0; i < 8; i++) update_tank(0, 2, 0);\n"
            "results[9] = tanks[0].direction;\n"
            "tanks[0].speed = 32; tanks[0].throttle = 32; tanks[0].direction = 8;\n"
            "for (i = 0; i < 8; i++) update_tank(0, 2, 0);\n"
            "results[10] = tanks[0].direction;\n"
            "for (i = 0; i < 64; i++) update_tank(1, 0, KEYX2_P2_FORWARD);\n"
            "results[11] = tanks[1].speed;\n"
            "for (i = 0; i < 64; i++) update_tank(1, 0, KEYX2_P2_BACKWARD);\n"
            "results[12] = tanks[1].speed;\n", 13)
        self.assertEqual(results[:3], [1, 100, 32])
        self.assertEqual(results[3:7], [133, 32, 149, 0])
        self.assertEqual(results[7], -16)
        self.assertLess(results[8], 180)
        self.assertEqual(results[9:], [12, 10, 32, -16])

    def test_handbrake_overrides_throttle_stops_both_players_and_turning_tracks_speed(self):
        results = self.scenario_results(
            "int i; int speed;\n"
            "start_round();\n"
            "tanks[0].x = 100; tanks[0].y = 100; tanks[0].direction = 8;\n"
            "tanks[1].x = 500; tanks[1].y = 400;\n"
            "tanks[0].speed = 32; tanks[0].throttle = 32;\n"
            "update_tank(0, 7, 0); results[0] = tanks[0].speed;\n"
            "for (i = 0; i < 3; i++) update_tank(0, 7, 0);\n"
            "results[1] = tanks[0].speed; results[2] = tanks[0].throttle;\n"
            "results[3] = tanks[0].direction; results[4] = tanks[0].x;\n"
            "update_tank(0, 0, 0); results[5] = tanks[0].speed;\n"
            "results[6] = tanks[0].handbrake;\n"
            "tanks[1].speed = -16; tanks[1].throttle = -16;\n"
            "for (i = 0; i < 2; i++) update_tank(1, 0, KEYX2_P2_LEFT | KEYX2_P2_RIGHT | KEYX2_P2_BACKWARD);\n"
            "results[7] = tanks[1].speed; results[8] = tanks[1].throttle;\n"
            "results[9] = tanks[1].direction; results[10] = tanks[1].handbrake;\n"
            "for (speed = 0; speed < 3; speed++) {\n"
            "  tanks[0].x = 100; tanks[0].y = 100; tanks[0].direction = 8;\n"
            "  tanks[0].speed = speed * 16; tanks[0].throttle = speed * 16;\n"
            "  tanks[0].turn_tick = 0;\n"
            "  for (i = 0; i < 12; i++) update_tank(0, 2, 0);\n"
            "  results[11 + speed] = tanks[0].direction;\n"
            "}\n", 14)
        self.assertEqual(results, [24, 0, 0, 8, 102, 0, 0, 0, 0, 24, 1, 14, 12, 11])

    def test_ammo_and_tank_fires_use_distinct_real_time_lifetimes(self):
        results = self.scenario_results(
            "start_round();\n"
            "tanks[0].x = 100; tanks[0].y = 100;\n"
            "tanks[1].x = 140; tanks[1].y = 100;\n"
            "shots[0].x = 132; shots[0].y = 100; shots[0].direction = 8;\n"
            "shots[0].life = 120; update_shots();\n"
            "change_health(1, -100);\n"
            "results[0] = burn_count; results[1] = burn_sites[0].mask;\n"
            "results[2] = burn_sites[1].mask;\n"
            "results[3] = burn_sites[0].remaining_seconds;\n"
            "results[4] = burn_sites[1].remaining_seconds;\n"
            "results[5] = burn_owner[1]; results[6] = burn_kind[1];\n"
            "age_burn_sites(10);\n"
            "results[7] = burn_sites[0].remaining_seconds;\n"
            "results[8] = burn_sites[1].remaining_seconds;\n"
            "age_burn_sites(20);\n"
            "results[9] = burn_sites[1].remaining_seconds;\n"
            "reset_battlefield(); results[10] = burn_count;\n", 11)
        self.assertEqual(results, [
            2, -1, 24, 10, 30, 1, 1, 0, 20, 0, 0,
        ])

    def test_burn_site_lifetimes_follow_wall_clock_seconds(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        source = self.game_source(source_path).replace(
            "int main(void)", "int tank16_game_entry(void)", 1)
        test_source = (
            "int main(void) {\n"
            "  add_ammo_burn_site(10, 20, 0);\n"
            "  add_tank_burn_site(30, 40, 0, 1);\n"
            "  update_burn_site_lifetimes();\n"
            "  gfx_present();\n"
            "  update_burn_site_lifetimes();\n"
            "  results[0] = burn_sites[0].remaining_seconds;\n"
            "  results[1] = burn_sites[1].remaining_seconds;\n"
            "  gfx_present();\n"
            "  update_burn_site_lifetimes();\n"
            "  results[2] = burn_sites[1].remaining_seconds;\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            "int results[4];\nint main(void);\n" + source + test_source,
            str(source_path)))
        run_frame(cpu)
        current_time = (
            cpu.memory[RTC_START + 1] * 60 + cpu.memory[RTC_START])

        def advance_clock(seconds):
            nonlocal current_time
            current_time += seconds
            cpu.memory[RTC_START] = current_time % 60
            cpu.memory[RTC_START + 1] = current_time // 60 % 60
            cpu.frame_yield = False
            while cpu.running and not cpu.frame_yield:
                cpu.step()

        advance_clock(10)
        advance_clock(20)
        values = [
            cpu.read_16(GLOBAL_BASE + index * 2)
            for index in range(3)
        ]
        self.assertEqual(values, [0, 20, 0])

    def test_burn_site_rendering_is_bounded_and_round_robin(self):
        results = self.scenario_results(
            "gfx_layer_select(LAYER_3D);\n"
            "burn_count = MAX_BURN_SITES;\n"
            "burn_active_count = MAX_BURN_SITES;\n"
            "draw_burn_sites(); results[0] = burn_render_cursor;\n"
            "draw_burn_sites(); results[1] = burn_render_cursor;\n"
            "draw_burn_sites(); results[2] = burn_render_cursor;\n", 3)
        self.assertEqual(results, [8, 16, 0])

    def test_ammo_and_destroyed_tank_fires_are_capped_per_player(self):
        results = self.scenario_results(
            "int i;\n"
            "int ammo[2]; int tanks_count[2];\n"
            "ammo[0] = 0; ammo[1] = 0;\n"
            "tanks_count[0] = 0; tanks_count[1] = 0;\n"
            "for (i = 0; i < 12; i++) {\n"
            "  add_ammo_burn_site(i, 100, 0);\n"
            "  add_ammo_burn_site(i, 200, 1);\n"
            "}\n"
            "for (i = 0; i < 4; i++) {\n"
            "  add_tank_burn_site(i, 300, 0, 0);\n"
            "  add_tank_burn_site(i, 400, 0, 1);\n"
            "}\n"
            "for (i = 0; i < MAX_BURN_SITES; i++) {\n"
            "  if (burn_sites[i].remaining_seconds > 0) {\n"
            "    if (burn_kind[i] == BURN_KIND_AMMO)\n"
            "      ammo[burn_owner[i]]++;\n"
            "    else\n"
            "      tanks_count[burn_owner[i]]++;\n"
            "  }\n"
            "}\n"
            "results[0] = ammo[0]; results[1] = ammo[1];\n"
            "results[2] = tanks_count[0]; results[3] = tanks_count[1];\n"
            "results[4] = burn_count; results[5] = burn_capacity_error;\n", 6)
        self.assertEqual(results, [10, 10, 2, 2, 24, 0])

    def test_recoil_hit_and_blast_impulses_respect_obstacles_and_decay(self):
        results = self.scenario_results(
            "int i;\n"
            "start_round();\n"
            "tanks[0].x = 100; tanks[0].y = 100; tanks[0].direction = 8;\n"
            "tanks[1].x = 140; tanks[1].y = 100;\n"
            "fire_shot(0); results[0] = tanks[0].impulse_x;\n"
            "results[1] = tanks[0].shake;\n"
            "for (i = 0; i < 5; i++) update_shots();\n"
            "results[2] = tanks[1].impulse_x;\n"
            "results[3] = tanks[1].health;\n"
            "for (i = 0; i < 12; i++) update_tank(0, 0, 0);\n"
            "results[4] = tanks[0].impulse_x; results[5] = tanks[0].x;\n"
            "tanks[0].speed = 32; tanks[0].throttle = 32;\n"
            "blast_reaction(tanks[0].x - 8, 100, 40);\n"
            "results[6] = tanks[0].impulse_x; results[7] = tanks[0].speed;\n"
            "tanks[0].x = 16; tanks[0].speed = 0; tanks[0].throttle = 0;\n"
            "tank_impulse(0, -48, 0, 8);\n"
            "for (i = 0; i < 16; i++) update_tank(0, 0, 0);\n"
            "results[8] = tanks[0].x;\n", 9)
        self.assertEqual(results[:5], [-12, 3, 24, 90, 0])
        self.assertLess(results[5], 100)
        self.assertGreater(results[6], 0)
        self.assertEqual(results[7:], [16, 16])

    def test_particles_move_expire_wrap_and_reset(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        scenario = (
            "int main(void) {\n"
            "  int i;\n"
            "  gfx_mode(GFX_MODE_640_480); start_round();\n"
            "  tanks[0].x = 100; tanks[0].y = 100;\n"
            "  tanks[1].x = 140; tanks[1].y = 100;\n"
            "  emit_particle(150, 160, 2, -3, PARTICLE_SPARK);\n"
            "  emit_particle(148, 160, -1, -1, PARTICLE_SMOKE);\n"
            "  results[0] = next_particle;\n"
            "  update_particles();\n"
            "  results[1] = particles[0].y;\n"
            "  results[2] = particles[1].y;\n"
            "  results[3] = particles[0].life;\n"
            "  gfx_split_camera(0, 0, 0, 0); draw_hud(); gfx_present();\n"
            "  for (i = 0; i < 64; i++) update_particles();\n"
            "  for (i = 0; i < MAX_PARTICLES; i++) results[4] += particles[i].life;\n"
            "  emit_particle(0, 0, -1, -1, 0); update_particles();\n"
            "  results[5] = particles[2].life;\n"
            "  start_round();\n"
            "  for (i = 0; i < MAX_PARTICLES + 3; i++) emit_particle(i, 100, 1, 0, 0);\n"
            "  results[6] = next_particle;\n"
            "  results[7] = particles[0].x;\n"
            "  start_round();\n"
            "  results[8] = next_particle;\n"
            "  for (i = 0; i < MAX_PARTICLES; i++) results[9] += particles[i].life;\n"
            "  tanks[1].x = 140; tanks[1].y = 100;\n"
            "  change_health(1, -100); round_over = 1; round_delay = 60;\n"
            "  for (i = 0; i < 3; i++) update_game(0, 0);\n"
            "  results[10] = burn_count;\n"
            "  results[11] = burn_sites[0].remaining_seconds;\n"
            "  results[12] = burn_sites[0].mask;\n"
            "  results[13] = round_delay;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            "int results[14];\n" + self.game_source(source_path) + scenario,
            str(source_path)))
        self.assertLess(run_frame(cpu), 400_000)
        self.assertTrue(cpu.frame_yield)
        for offset in (0, 320):
            self.assertEqual(layer_pixel(cpu, 4, offset + 152, 157), 1)
            self.assertTrue(any(layer_pixel(cpu, 4, offset + x, y) == 10
                                for x in range(143, 152) for y in range(156, 163)))
        while cpu.running:
            cpu.step()
        results = [cpu.read_16(GLOBAL_BASE + i * 2) for i in range(14)]
        self.assertEqual(results[:3], [2, 157, 159])
        self.assertGreaterEqual(results[3], 17)
        self.assertLessEqual(results[3], 35)
        self.assertEqual(results[4:], [0, 0, 3, 80, 0, 0, 1, 30, 24, 57])

    def test_pixel_collisions_rotation_thin_obstacles_and_crossing_ammo(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        scenario = (
            "int main(void) {\n"
            "  int i;\n"
            "  prepare_collision_assets();\n"
            "  for (i = 0; i < 1024; i++) sprite_work[i] = 0;\n"
            "  sprite_work[528] = 1;\n"
            "  gfx_sprite_upload(8, 32, 32, sprite_work);\n"
            "  gfx_sprite_upload(24, 32, 32, sprite_work);\n"
            "  sprite_work[528] = 0; sprite_work[529] = 1;\n"
            "  gfx_sprite_upload(9, 32, 32, sprite_work);\n"
            "  for (i = 0; i < 4096; i++) terrain_tiles[1][i] = 0;\n"
            "  terrain_tiles[1][16 * 32 + 16] = 1;\n"
            "  gfx_tile_upload(1, terrain_tiles[1]);\n"
            "  map_obstacles[2 * MAP_COLUMNS + 3] = 1;\n"
            "  tanks[0].x = 111; tanks[0].y = 80; tanks[0].direction = 8;\n"
            "  tanks[1].x = 200; tanks[1].y = 100; tanks[1].direction = 24;\n"
            "  tanks[0].health = 100; tanks[1].health = 100;\n"
            "  results[0] = blocked(111, 80, 0);\n"
            "  results[1] = blocked(112, 80, 0);\n"
            "  update_tank(0, 2, 0); update_tank(0, 2, 0); results[2] = tanks[0].direction;\n"
            "  results[3] = tank_collision(199, 100, 0);\n"
            "  results[4] = tank_collision(200, 100, 0);\n"
            "  shots[0].x = 106; shots[0].y = 80; shots[0].life = 120;\n"
            "  shots[0].direction = 8; update_shots();\n"
            "  results[5] = shots[0].life;\n"
            "  results[6] = shots[0].x;\n"
            "  shots[0].x = 100; shots[0].y = 120; shots[0].life = 120;\n"
            "  shots[1].x = 110; shots[1].y = 120; shots[1].life = 120;\n"
            "  shots[1].direction = 24; update_shots();\n"
            "  results[7] = shots[0].life; results[8] = shots[1].life;\n"
            "  shots[0].x = 194; shots[0].y = 100; shots[0].life = 120;\n"
            "  update_shots(); results[9] = tanks[1].health;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            "int results[10];\n" + self.game_source(source_path, 32) + scenario,
            str(source_path)))
        while cpu.running:
            cpu.step()
        self.assertEqual([cpu.read_16(GLOBAL_BASE + i * 2) for i in range(10)],
                         [0, 1, 8, 0, 1, 0, 109, 0, 0, 90])

    def test_objects_require_opaque_contact_and_react_to_ammunition(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        editor = runpy.run_path(str(source_path.with_name("map_editor.py")))
        data = editor["new_map"]("PIXELS", 10, 8)
        for column, removal in enumerate(("explosion", "fixed", "disappear"), 1):
            data["objects"][1][column] = {
                "tile": [1, 0], "hp_change": -25, "removal": removal,
            }
        with tempfile.TemporaryDirectory() as directory, patch(
                "tank16_map_loader.map_tile_pixel_size", return_value=32):
            path = editor["save_map"](data, Path(directory))
            header = build_map_header(path)
        source = self.game_source(source_path, 32).replace(
            '#include "map_assets.h"', header, 1)
        scenario = (
            "int main(void) {\n"
            "  int i;\n"
            "  start_round();\n"
            "  for (i = 0; i < 1024; i++) sprite_work[i] = 0;\n"
            "  sprite_work[528] = 1;\n"
            "  gfx_sprite_upload(8, 32, 32, sprite_work);\n"
            "  for (i = 0; i < 1024; i++) terrain_tiles[MAP_TERRAIN_TILE_COUNT][i] = 0;\n"
            "  terrain_tiles[MAP_TERRAIN_TILE_COUNT][528] = 1;\n"
            "  gfx_tile_upload(MAP_TERRAIN_TILE_COUNT, terrain_tiles[MAP_TERRAIN_TILE_COUNT]);\n"
            "  tanks[0].x = 47; tanks[0].y = 48;\n"
            "  tanks[1].x = 240; tanks[1].y = 180;\n"
            "  update_objects(); results[0] = tanks[0].health;\n"
            "  tanks[0].x = 48; update_objects(); results[1] = tanks[0].health;\n"
            "  start_round(); tanks[1].x = 240; tanks[1].y = 180;\n"
            "  shots[0].x = 42; shots[0].y = 48;\n"
            "  shots[0].direction = 8; shots[0].life = 120; update_shots();\n"
            "  results[2] = object_active[0]; results[3] = object_explosion[0];\n"
            "  results[4] = next_particle;\n"
            "  shots[0].x = 74; shots[0].life = 120; update_shots();\n"
            "  results[5] = object_active[1]; results[6] = shots[0].life;\n"
            "  shots[0].x = 106; shots[0].life = 120; update_shots();\n"
            "  results[7] = object_active[2]; results[8] = object_explosion[2];\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source("int results[9];\n" + source + scenario,
                                     str(source_path)))
        while cpu.running:
            cpu.step()
        self.assertEqual([cpu.read_16(GLOBAL_BASE + i * 2) for i in range(9)],
                         [100, 75, 0, 18, 48, 1, 0, 0, 0])

    def test_full_tracks_and_impact_pool_render_within_frame_budget(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        source = self.game_source(source_path)
        scenario = (
            "int main(void) {\n"
            "  int i;\n"
            "  gfx_mode(GFX_MODE_640_480);\n"
            "  start_round(); gfx_present();\n"
            "  tanks[0].x = 160; tanks[0].y = 240; tanks[0].health = 65;\n"
            "  tanks[1].x = 480; tanks[1].y = 240; tanks[1].health = 20;\n"
            "  tank_sprite_slots[0] = -1; tank_sprite_slots[1] = -1;\n"
            "  for (i = 0; i < MAX_TRACK_MARKS; i++) {\n"
            "    track_x[i] = 40 + (i % 96) * 5; track_y[i] = 210 + i / 96 * 4;\n"
            "    track_direction[i] = 8;\n"
            "    track_offset_y[i] = -10;\n"
            "  }\n"
            "  track_count = MAX_TRACK_MARKS;\n"
            "  for (i = 0; i < MAX_IMPACTS; i++) {\n"
            "    add_impact(70 + i * 40, 180 + i * 12, 12);\n"
            "    impact_life[i] = 2;\n"
            "  }\n"
            "  fire_shot(0); fire_shot(1);\n"
            "  hit_flash[1] = HIT_FLASH_FRAMES;\n"
            "  for (i = 0; i < 100; i++) add_burn_site(40 + i * 5, 250, -1);\n"
            "  for (i = 0; i < MAX_SHOTS; i++) {\n"
            "    shots[i].life = 120; shots[i].x = 40 + i * 40;\n"
            "    shots[i].y = 160; shots[i].direction = 0;\n"
            "  }\n"
            "  gfx_present();\n"
            "  update_game(0, 0);\n"
            "  draw_game(); gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source + scenario, str(source_path)))
        self.assertLess(run_frame(cpu), 400_000)
        self.assertTrue(cpu.frame_yield)
        run_frame(cpu, 2_000_000)
        self.assertTrue(cpu.frame_yield)
        self.assertLess(run_frame(cpu), 400_000)
        self.assertTrue(cpu.frame_yield)

    def test_combat_feedback_renders_expires_and_resets(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        source = self.game_source(source_path)
        test_source = (
            "int main(void) {\n"
            "  int i;\n"
            "  gfx_mode(GFX_MODE_640_480);\n"
            "  tank_sprite_slots[0] = -1; tank_sprite_slots[1] = -1;\n"
            "  start_round();\n"
            "  tanks[0].x = 100; tanks[0].y = 100; tanks[0].direction = 8;\n"
            "  tanks[1].x = 140; tanks[1].y = 100;\n"
            "  fire_shot(0); fire_shot(0);\n"
            "  results[0] = muzzle_flash[0];\n"
            "  results[1] = tanks[0].cooldown;\n"
            "  results[2] = shots[1].life;\n"
            "  for (i = 0; i < 5; i++) update_shots();\n"
            "  results[3] = tanks[1].health;\n"
            "  results[4] = hit_flash[1];\n"
            "  results[5] = impact_life[0];\n"
            "  gfx_split_camera(0, 0, 0, 0);\n"
            "  draw_hud(); gfx_present();\n"
            "  change_health(1, 5);\n"
            "  change_health(1, -100);\n"
            "  results[6] = tanks[1].health;\n"
            "  results[7] = impact_size[1];\n"
            "  draw_tank(1);\n"
            "  fire_shot(1);\n"
            "  results[8] = muzzle_flash[1];\n"
            "  round_over = 1; round_delay = 60;\n"
            "  for (i = 0; i < IMPACT_FRAMES; i++) update_game(0, 0);\n"
            "  results[9] = hit_flash[1];\n"
            "  results[10] = impact_life[1];\n"
            "  results[11] = muzzle_flash[0];\n"
            "  draw_game(); gfx_present();\n"
            "  start_round();\n"
            "  results[12] = next_impact;\n"
            "  results[13] = hit_flash[1];\n"
            "  results[14] = tanks[1].health;\n"
            "  change_health(1, 25);\n"
            "  results[15] = tanks[1].health;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            "int results[16];\nint main(void);\n" + source + test_source,
            str(source_path),
        ))
        self.assertLess(run_frame(cpu), 400_000)
        self.assertTrue(cpu.frame_yield)
        self.assertEqual(layer_pixel(cpu, 4, 120, 21), 0)
        self.assertEqual(layer_pixel(cpu, 4, 68, 21), 0)
        self.assertEqual(layer_pixel(cpu, 4, 528, 7), 1)
        self.assertEqual(layer_pixel(cpu, 4, 319, 100), 10)
        self.assertEqual(layer_pixel(cpu, 4, 320, 100), 10)
        self.assertEqual(layer_pixel(cpu, 4, 118, 100), 1)
        self.assertEqual(layer_pixel(cpu, 4, 438, 100), 1)
        self.assertLess(run_frame(cpu), 400_000)
        self.assertTrue(cpu.frame_yield)
        self.assertEqual(cpu.sprites[24]["flags"], 0)
        for offset in (0, 320):
            self.assertEqual(layer_pixel(cpu, 4, offset + 32, 412), 0)
            self.assertEqual(layer_pixel(cpu, 4, offset + 34, 396),
                             3 if offset == 0 else 2)
            self.assertTrue(any(
                layer_pixel(cpu, 4, x, y) != 0
                for y in range(412, 419)
                for x in range(offset + 130, offset + 190)
            ))
        while cpu.running:
            cpu.step()
        results = [cpu.read_16(GLOBAL_BASE + i * 2) for i in range(16)]
        self.assertEqual(results, [
            3, 18, 0, 90, 8, 10, 0, 28, 0, 0, 0, 0, 0, 0, 100, 100,
        ])
        self.assertEqual(cpu.sound_log, [
            (0, 230, 3, 1, 22),
            (2, 160, 4, 1, 25),
            (3, 660, 6, 2, 25),
            (2, 65, 16, 1, 45),
        ])

    def test_object_tiles_scale_smoothly_with_configured_tile_size(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        editor = runpy.run_path(str(source_path.with_name("map_editor.py")))
        data = editor["new_map"]("SCALED", 10, 8)
        data["obstacles"][0][0] = [0, 0]
        atlas, _, _ = editor["load_tileset"](
            editor["OBJECT_TILESET_PATH"], pad_edges=True)
        source_tile = atlas.subsurface((0, 0, 64, 64))
        with tempfile.TemporaryDirectory() as directory:
            path = editor["save_map"](data, Path(directory))
            for size in (24, 32, 64):
                with self.subTest(size=size), patch(
                        "tank16_map_loader.map_tile_pixel_size",
                        return_value=size):
                    _, tiles, palette, dimensions = load_map_assets(path)
                self.assertEqual(dimensions[2], size)
                object_tile_id = dimensions[0] * dimensions[1]
                self.assertEqual(len(tiles), object_tile_id + 4)
                self.assertEqual(len(tiles[object_tile_id]), size * size)
                expected = pygame.transform.smoothscale(source_tile, (size, size))
                for y in range(size):
                    for x in range(size):
                        color = expected.get_at((x, y))
                        pixel = tiles[object_tile_id][y * size + x]
                        if color.a < 128:
                            self.assertEqual(pixel, 0)
                        else:
                            closest = min(
                                range(1, len(palette)),
                                key=lambda index: sum(
                                    (color[channel] - palette[index][channel]) ** 2
                                    for channel in range(3)),
                            )
                            self.assertEqual(pixel, closest + 31)

    def test_map_objects_obstacles_and_tracks_work_in_game(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        editor = runpy.run_path(str(source_path.with_name("map_editor.py")))
        data = editor["new_map"]("LAYER", 10, 8)
        data["obstacles"][2][3] = [0, 0]
        for column, hp, removal in (
            (1, -25, "explosion"), (2, 30, "disappear"),
            (4, -10, "fixed"), (5, -100, "disappear"),
        ):
            data["objects"][1][column] = {
                "tile": [1, 0], "hp_change": hp, "removal": removal,
            }
        with tempfile.TemporaryDirectory() as directory, patch(
                "tank16_map_loader.map_tile_pixel_size", return_value=64):
            path = editor["save_map"](data, Path(directory))
            header = build_map_header(path)
            _, tiles, _, dimensions = load_map_assets(path)
        game_source = self.game_source(source_path).replace(
            '#include "map_assets.h"', header, 1
        )
        test_source = (
            "int main(void) {\n"
            "  int i;\n"
            "  start_round();\n"
            "  tanks[1].x = 600; tanks[1].y = 440;\n"
            "  tanks[0].x = 96; tanks[0].y = 96;\n"
            "  update_objects();\n"
            "  results[0] = tanks[0].health;\n"
            "  results[1] = object_active[0];\n"
            "  results[2] = object_explosion[0];\n"
            "  update_objects();\n"
            "  results[3] = tanks[0].health;\n"
            "  tanks[0].x = 160; tanks[0].health = 90;\n"
            "  update_objects();\n"
            "  results[4] = tanks[0].health;\n"
            "  results[5] = object_active[1];\n"
            "  results[6] = object_explosion[1];\n"
            "  tanks[0].x = 288;\n"
            "  update_objects(); update_objects();\n"
            "  results[7] = tanks[0].health;\n"
            "  results[8] = object_active[2];\n"
            "  tanks[0].y = 260; update_objects();\n"
            "  tanks[0].y = 96; update_objects();\n"
            "  results[9] = tanks[0].health;\n"
            "  tanks[0].x = 352; update_objects();\n"
            "  results[10] = tanks[0].health;\n"
            "  start_round();\n"
            "  results[11] = object_active[0];\n"
            "  results[12] = object_explosion[0];\n"
            "  results[13] = blocked(208, 150, 0);\n"
            "  results[14] = blocked(176, 150, 0);\n"
            "  tanks[0].x = 176; tanks[0].y = 150;\n"
            "  tanks[0].direction = 8;\n"
            "  fire_shot(0); update_shots();\n"
            "  results[15] = shots[0].life;\n"
            "  tanks[0].x = 100; tanks[0].y = 260;\n"
            "  tanks[0].speed = MAX_FORWARD_SPEED; tanks[0].throttle = MAX_FORWARD_SPEED;\n"
            "  tanks[0].impulse_x = 0; tanks[0].impulse_y = 0;\n"
            "  last_track_x[0] = 100; last_track_y[0] = 260;\n"
            "  for (i = 0; i < 10; i++) update_tank(0, 4, 0);\n"
            "  results[16] = track_count;\n"
            "  update_tank(0, 0, 0);\n"
            "  results[17] = track_count;\n"
            "  gfx_mode(GFX_MODE_640_480);\n"
            "  gfx_tile_size(MAP_TILE_PIXEL_SIZE);\n"
            "  upload_terrain_tiles();\n"
            "  gfx_split_camera(0, 0, 320, 0);\n"
            "  reset_battlefield();\n"
            "  draw_ground();\n"
            "  start_round();\n"
            "  results[18] = track_count;\n"
            "  tanks[0].x = 100; tanks[0].y = 260;\n"
            "  tanks[0].direction = 8;\n"
            "  tanks[0].speed = MAX_FORWARD_SPEED; tanks[0].throttle = MAX_FORWARD_SPEED;\n"
            "  tanks[1].x = 600; tanks[1].y = 440;\n"
            "  last_track_x[0] = 100; last_track_y[0] = 260;\n"
            "  for (i = 0; i < 200; i++) update_tank(0, 4, 0);\n"
            "  results[19] = track_count;\n"
            "  results[20] = next_track;\n"
            "  object_explosion[0] = EXPLOSION_FRAMES;\n"
            "  round_over = 1; round_delay = 90;\n"
            "  for (i = 0; i < EXPLOSION_FRAMES; i++) update_game(0, 0);\n"
            "  results[21] = object_explosion[0];\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(
            "int results[22];\nint main(void);\n" + game_source + test_source,
            str(source_path),
        ))
        while cpu.running:
            cpu.step()
        results = [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(22)]
        self.assertEqual(results, [
            75, 0, 18, 75, 100, 0, 0, 90, 1, 80, 0,
            1, 0, 1, 0, 0, 5, 5, 0, 100, 100, 0,
        ])
        obstacle_tile = tiles[dimensions[0] * dimensions[1]]
        for y in range(64):
            for x in range(64):
                value = obstacle_tile[y * 64 + x]
                self.assertEqual(layer_pixel(cpu, 3, 192 + x, 128 + y), value)
        self.assertEqual(layer_pixel(cpu, 3, 101, 249), 12)

    def test_map_bounds_and_camera_clamping_follow_tile_pixel_size(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        for tile_size, centered_camera, edge_camera in (
            (24, -40, -40),
            (32, 0, 0),
            (64, 160, 320),
        ):
            with self.subTest(tile_size=tile_size):
                game_source = self.game_source(source_path, tile_size)
                test_source = (
                    "int main(void) {\n"
                    "  int map_width;\n"
                    "  tanks[1].x = -1000; tanks[1].y = -1000;\n"
                    "  map_width = MAP_COLUMNS * MAP_TILE_PIXEL_SIZE;\n"
                    "  bounds_results[0] = blocked(16, 16, 0);\n"
                    "  bounds_results[1] = blocked(map_width - 16, 16, 0);\n"
                    "  bounds_results[2] = blocked(map_width - 15, 16, 0);\n"
                    "  bounds_results[3] = camera_origin(map_width / 2, 320, map_width);\n"
                    "  bounds_results[4] = camera_origin(map_width - 16, 320, map_width);\n"
                    "  return 0;\n"
                    "}\n"
                )
                cpu = make_cpu(compile_source(
                    "int bounds_results[5];\n"
                    "int main(void);\n" + game_source + test_source,
                    str(source_path),
                ))
                while cpu.running:
                    cpu.step()
                results = [
                    cpu.read_16(GLOBAL_BASE + index * 2)
                    for index in range(5)
                ]
                results = [
                    value - 0x10000 if value & 0x8000 else value
                    for value in results
                ]
                expected = [0, 0, 1, centered_camera, edge_camera]
                self.assertEqual(results, expected)

    def test_tank_slides_along_obstacle_when_diagonal_path_is_blocked(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        game_source = self.game_source(source_path)
        test_source = (
            "int main(void) {\n"
            "  int i;\n"
            "  prepare_collision_assets();\n"
            "  for (i = 0; i < 4096; i++) terrain_tiles[1][i] = 1;\n"
            "  gfx_tile_upload(1, terrain_tiles[1]);\n"
            "  map_obstacles[2 * MAP_COLUMNS + 3] = 1;\n"
            "  tanks[0].x = 177; tanks[0].y = 140;\n"
            "  tanks[0].direction = 12;\n"
            "  tanks[0].health = 100;\n"
            "  tanks[0].speed = MAX_FORWARD_SPEED; tanks[0].throttle = MAX_FORWARD_SPEED;\n"
            "  tanks[1].x = 560; tanks[1].y = 440;\n"
            "  update_tank(0, 4, 0);\n"
            "  slide_results[0] = tanks[0].x;\n"
            "  slide_results[1] = tanks[0].y;\n"
            "  return 0;\n"
            "}\n"
        )
        filename = ROOT / "modules" / "Tank-16" / "slide_test.c"
        cpu = make_cpu(compile_source(
            "int slide_results[2];\nint main(void);\n" +
            game_source + test_source,
            str(filename),
        ))
        while cpu.running:
            cpu.step()

        results = [
            cpu.read_16(GLOBAL_BASE + index * 2)
            for index in range(2)
        ]
        self.assertEqual(results, [177, 141])

    def test_main_game_compiles_and_renders_a_frame(self):
        source = ROOT / "modules" / "Tank-16" / "main.c"
        map_path = ROOT / "modules" / "Tank-16" / "disks" / "maps" / "PERUS-1.json"
        _, _, _, dimensions = load_map_assets(map_path)
        cpu = make_cpu(load_program_file(source))
        with tempfile.TemporaryDirectory() as directory:
            module_disks = Path(directory) / "module" / "disks"
            maps_dir = module_disks / "maps"
            maps_dir.mkdir(parents=True)
            map_document = json.loads(map_path.read_text(encoding="utf-8"))
            map_document = {
                "name": map_document["name"],
                "format_version": 1,
                "tile_size": 64,
                "width": map_document["width"],
                "height": map_document["height"],
                "tileset": "tiles_terrain.png",
            }
            map_document["tiles"] = [
                [[1, 0] for _ in range(map_document["width"])]
                for _ in range(map_document["height"])
            ]
            map_document["obstacles"] = [
                [None for _ in range(map_document["width"])]
                for _ in range(map_document["height"])
            ]
            map_document["objects"] = [
                [None for _ in range(map_document["width"])]
                for _ in range(map_document["height"])
            ]
            editor = runpy.run_path(str(source.with_name("map_editor.py")))
            (maps_dir / "FINLAND.json").write_bytes(
                editor["serialize_runtime_map"](
                    editor["encode_runtime_map"](map_document)
                ).encode("ascii"))
            shutil.copytree(
                source.parent / "disks" / "assets", module_disks / "assets")
            cpu.disk = DiskDevice(Path(directory) / "virtual-disks")
            cpu.disk.set_file_root(module_disks)
            for _ in range(400):
                run_frame(cpu)
                if cpu.frame_yield and cpu.split_cameras:
                    break

        self.assertTrue(cpu.frame_yield)
        self.assertTrue(cpu.split_cameras)
        self.assertEqual(cpu.memory[DISK_STATUS], 0)
        self.assertEqual(cpu.tilemap_width, map_document["width"])
        self.assertEqual(cpu.tilemap_height, map_document["height"])
        self.assertEqual(cpu.tile_pixel_size, map_tile_pixel_size())
        self.assertEqual(dimensions[2], cpu.tile_pixel_size)
        tile_pixels = cpu.tile_pixel_size ** 2
        tile_one = cpu.tile_memory[tile_pixels:tile_pixels * 2]
        expected_layer = bytearray(640 * 480)
        for screen_y in range(480):
            for screen_x in range(640):
                viewport = 0 if screen_x < 320 else 1
                camera_x, camera_y = cpu.split_cameras[viewport]
                world_x = screen_x - viewport * 320 + camera_x
                world_y = screen_y + camera_y
                if not (0 <= world_x < 640 and 0 <= world_y < 512):
                    continue
                tile_offset = (
                    (world_y % cpu.tile_pixel_size) * cpu.tile_pixel_size +
                    world_x % cpu.tile_pixel_size
                )
                expected_layer[screen_y * 640 + screen_x] = tile_one[tile_offset]
        actual_layer = cpu.gpu_layers[1]
        mismatches = [
            (index, actual_layer[index], expected_layer[index])
            for index in range(len(expected_layer))
            if actual_layer[index] != expected_layer[index]
        ]
        self.assertFalse(mismatches)
        initial_sprites = [
            (slot, sprite)
            for slot, sprite in enumerate(cpu.sprites)
            if sprite["flags"] & 1
        ]
        player_one_sprite = next(
            sprite for slot, sprite in initial_sprites if slot >= 32
        )
        player_two_sprite = next(
            sprite for slot, sprite in initial_sprites if slot < 32
        )
        player_one_camera = cpu.split_cameras[0]
        player_two_camera = cpu.split_cameras[1]
        player_one_x = player_one_sprite["x"] - player_one_camera[0]
        player_one_y = player_one_sprite["y"] - player_one_camera[1]
        player_two_x = player_two_sprite["x"] - player_two_camera[0] + 320
        player_two_y = player_two_sprite["y"] - player_two_camera[1]
        player_one = [
            layer_pixel(cpu, 2, x, y)
            for y in range(player_one_y, player_one_y + 32)
            for x in range(player_one_x, player_one_x + 32)
        ]
        player_two = [
            layer_pixel(cpu, 2, x, y)
            for y in range(player_two_y, player_two_y + 32)
            for x in range(player_two_x, player_two_x + 32)
        ]
        self.assertGreater(sum(value != 0 for value in player_one), 100)
        self.assertGreater(sum(value != 0 for value in player_two), 100)
        self.assertIn(3, player_one)
        self.assertIn(2, player_two)
        for slot in range(64):
            width, height, pixels = cpu.sprite_memory[slot]
            self.assertEqual((width, height), (32, 32))
            for position in range(32):
                self.assertEqual(pixels[position], 0)
                self.assertEqual(pixels[31 * 32 + position], 0)
                self.assertEqual(pixels[position * 32], 0)
                self.assertEqual(pixels[position * 32 + 31], 0)
        self.assertNotEqual(cpu.sprite_memory[0][2], cpu.sprite_memory[8][2])
        self.assertNotEqual(cpu.sprite_memory[0][2], cpu.sprite_memory[16][2])
        self.assertNotEqual(cpu.sprite_memory[0][2], cpu.sprite_memory[24][2])
        cpu.memory[KEYS_ADDR] = 1
        cpu.memory[KEYS_ADDR + 2] = 2
        for _ in range(4):
            run_frame(cpu)
        visible_slots = [
            slot for slot, sprite in enumerate(cpu.sprites)
            if sprite["flags"] & 1
        ]
        self.assertEqual(visible_slots, [22, 38])
        map_width = map_document["width"] * cpu.tile_pixel_size
        map_height = map_document["height"] * cpu.tile_pixel_size
        player_two_x = cpu.sprites[22]["x"] + 16
        player_two_y = cpu.sprites[22]["y"] + 16
        player_one_x = cpu.sprites[38]["x"] + 16
        player_one_y = cpu.sprites[38]["y"] + 16
        self.assertLess(player_one_x, map_width // 2)
        self.assertGreaterEqual(player_two_x, map_width // 2)
        self.assertLess(player_two_x, map_width)
        self.assertLess(player_one_y, map_height)
        self.assertLess(player_two_y, map_height)

    def test_round_start_switches_between_bundled_compatible_maps(self):
        game_dir = ROOT / "modules" / "Tank-16"
        first_map = game_dir / "disks" / "maps" / "FINLAND.json"
        second_map = game_dir / "disks" / "maps" / "100.json"
        self.assertIn(second_map, discover_round_maps(first_map))
        header = build_runtime_assets_header()
        self.assertIn("#define POWERUP_HEALTH_TILE", header)
        self.assertIn("#define POWERUP_DAMAGE_TILE", header)
        self.assertIn("#define POWERUP_SPEED_TILE", header)
        self.assertIn("#define RUNTIME_MAP_LOADING 1", header)
        self.assertIn(
            '#define RUNTIME_TILE_DATA_PATH "assets/TANK16_TILES.json"',
            header)
        self.assertIn(
            '#define RUNTIME_TILE_BINARY_PATH "assets/TANK16_TILES.bin"',
            header)
        self.assertIn("volatile unsigned char map_json_chunk[4096];",
                      header)
        self.assertNotIn("terrain_tiles[", header)
        self.assertNotIn("map_palette[", header)
        self.assertLess(first_map.stat().st_size, 65_535)
        self.assertLess(second_map.stat().st_size, 65_535)
        self.assertTrue(RUNTIME_TILE_DATA_PATH.is_file())
        tileset = json.loads(RUNTIME_TILE_DATA_PATH.read_text(encoding="utf-8"))
        self.assertEqual(tileset["format"], "SC16-TANK16-TILES")
        self.assertEqual(tileset["version"], 1)
        self.assertEqual(tileset["pixel_encoding"], "raw-index-v1")
        self.assertEqual(tileset["tile_size"], 64)
        self.assertEqual(
            (tileset["terrain_columns"], tileset["terrain_rows"]), (10, 9))
        self.assertEqual(
            (tileset["object_columns"], tileset["object_rows"]), (6, 8))
        self.assertEqual(len(tileset["palette"]), 224)
        self.assertEqual(tileset["tile_count"], 138)
        binary_path = RUNTIME_TILE_DATA_PATH.with_suffix(".bin")
        self.assertEqual(
            binary_path.stat().st_size,
            tileset["tile_count"] * tileset["tile_size"] ** 2,
        )
        tile_files = sorted(
            (RUNTIME_TILE_DATA_PATH.parent / "tiles").glob("tile_*.json"))
        self.assertEqual(len(tile_files), tileset["tile_count"])

    def test_runtime_round_selection_randomly_loads_disk_map_files(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        game_source = self.game_source(source_path).replace(
            '#include "map_assets.h"', build_runtime_assets_header(), 1
        )
        test_source = (
            "int main(void) {\n"
            "  int i;\n"
            "  int original_map;\n"
            "  int found_map_change;\n"
            "  runtime_terrain_columns = 10; runtime_terrain_rows = 9;\n"
            "  runtime_object_columns = 6; runtime_object_rows = 8;\n"
            "  runtime_tile_size = 64; runtime_tile_count = 138;\n"
            "  runtime_palette_count = 225;\n"
            "  results[8] = load_map_path(\"maps/B.json\");\n"
            "  results[9] = map_columns;\n"
            "  results[10] = load_map_path(\"maps/A.json\");\n"
            "  results[11] = runtime_map_lazy * 100 + runtime_map_rows_loaded;\n"
            "  results[12] = map_tile_at(2, 3);\n"
            "  results[13] = map_obstacle_at(1, 2) - OBJECT_TILE_BASE;\n"
            "  results[14] = map_obstacle_at(0, 0);\n"
            "  results[15] = runtime_map_rows_loaded;\n"
            "  for (i = 0; i < 2; i++) {\n"
            "    effect_random = i;\n"
            "    round_map_selection_started = 0;\n"
            "    select_runtime_round_map();\n"
            "    results[i * 2] = map_columns;\n"
            "    results[i * 2 + 1] = map_rows;\n"
            "    if (map_load_error) return 1;\n"
            "  }\n"
            "  effect_random = 17; select_runtime_round_map();\n"
            "  original_map = map_selected_path[5];\n"
            "  add_ammo_burn_site(10, 20, 0);\n"
            "  effect_random = 17; select_runtime_round_map();\n"
            "  results[4] = burn_count;\n"
            "  effect_random = 0; select_runtime_round_map();\n"
            "  found_map_change = map_selected_path[5] != original_map;\n"
            "  results[5] = burn_count;\n"
            "  results[6] = found_map_change;\n"
            "  return 0;\n"
            "}\n"
        )
        with tempfile.TemporaryDirectory() as directory:
            module_disks = Path(directory) / "module" / "disks"
            maps_dir = module_disks / "maps"
            maps_dir.mkdir(parents=True)
            editor = runpy.run_path(str(source_path.with_name("map_editor.py")))
            for name, width, height, tile_x in (
                    ("A.json", 4, 3, 1), ("B.json", 5, 4, 2)):
                map_document = {
                    "width": width,
                    "height": height,
                    "tileset": "tiles_terrain.png",
                    "tiles": [
                        [[tile_x, 0] for _ in range(width)]
                        for _ in range(height)
                    ],
                }
                if name == "B.json":
                    map_document["runtime_encoding"] = "flat-v1"
                    map_document["tiles"] = [tile_x] * (width * height)
                    map_document["obstacles"] = []
                    map_document["objects"] = []
                    (maps_dir / name).write_text(
                        json.dumps(map_document), encoding="utf-8")
                    continue
                map_document["name"] = "A"
                map_document["format_version"] = 1
                map_document["tile_size"] = 64
                map_document["tiles"][2][3] = [5, 0]
                map_document["obstacles"] = [
                    [None] * width for _ in range(height)]
                map_document["obstacles"][1][2] = [4, 1]
                map_document["objects"] = [
                    [None] * width for _ in range(height)]
                (maps_dir / name).write_bytes(
                    editor["serialize_runtime_map"](
                        editor["encode_runtime_map"](map_document)
                    ).encode("ascii"))
            image = compile_source(
                "int results[16];\n" + game_source + test_source,
                str(source_path),
            )
            cpu = make_cpu(image)
            cpu.disk = DiskDevice(Path(directory) / "virtual-disks")
            cpu.disk.set_file_root(module_disks)
            for _ in range(40):
                run_frame(cpu)
                if not cpu.running:
                    break

        self.assertFalse(cpu.running)
        results = [
            cpu.read_16(GLOBAL_BASE + index * 2)
            for index in range(64)
        ]
        selected_dimensions = {
            tuple(results[index:index + 2])
            for index in range(0, 4, 2)
        }
        self.assertEqual(
            selected_dimensions, {(4, 3), (5, 4)},
            cpu.console_device.output.decode("ascii", errors="replace"))
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + index * 2) for index in (8, 9)],
            [1, 5],
            cpu.console_device.output.decode("ascii", errors="replace"))
        self.assertEqual([
            cpu.read_16(GLOBAL_BASE + index * 2)
            for index in range(4, 7)
        ], [1, 0, 1])
        runtime_log = cpu.console_device.output.decode("ascii")
        self.assertIn("MAP FILE: maps/A.json", runtime_log)
        self.assertIn(
            "MAP COORDINATES: x=0..3 y=0..2 cells=12", runtime_log)
        self.assertIn(
            "MAP TERRAIN TILE: id=1 source=(1,0) cells=11", runtime_log)
        self.assertIn("MAP INDEX: rows=3 stride=20", runtime_log)
        self.assertIn("MAP ROWS: y=0..2 loaded (60 bytes, 3/3 rows)",
                      runtime_log)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(10, 16)],
            [1, 100, 5, 1 * 6 + 4, 0, 3], runtime_log)
        self.assertIn("MAP FILE: maps/B.json", runtime_log)
        self.assertIn(
            "MAP TERRAIN TILE: id=2 source=(2,0) cells=20", runtime_log)

    def test_runtime_assets_exclude_maps_and_disk_maps_include_infinite_world(self):
        game_dir = ROOT / "modules" / "Tank-16"
        header = build_runtime_assets_header()
        self.assertIn("#define RUNTIME_MAP_LOADING 1", header)
        self.assertIn("#define MAP_COLUMNS map_columns", header)
        self.assertIn("unsigned char *map_data;", header)
        self.assertIn("unsigned char *map_obstacles;", header)
        self.assertNotIn("map_data[", header)
        self.assertNotIn("map_obstacles[", header)
        self.assertNotIn("terrain_tiles[", header)
        self.assertNotIn("map_palette[", header)

        disk_maps = game_dir / "disks" / "maps"
        finland = json.loads(
            (disk_maps / "FINLAND.json").read_text(encoding="utf-8")
        )
        infinite = json.loads(
            (disk_maps / "INFINITE.json").read_text(encoding="utf-8")
        )
        self.assertEqual((finland["width"], finland["height"]), (100, 100))
        self.assertTrue(infinite["infinite"])
        self.assertTrue(infinite["terrain_tiles"])
        self.assertTrue(infinite["obstacle_tiles"])

    def test_runtime_tile_json_is_loaded_and_uploaded_from_module_disk(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        header = build_runtime_assets_header()
        tileset = json.loads(RUNTIME_TILE_DATA_PATH.read_text(encoding="utf-8"))
        first_tile = json.loads(
            (RUNTIME_TILE_DATA_PATH.parent / "tiles" / "tile_000.json")
            .read_text(encoding="utf-8"))
        binary_tiles = RUNTIME_TILE_DATA_PATH.with_suffix(".bin").read_bytes()
        game_source = self.game_source(source_path).replace(
            '#include "map_assets.h"', header, 1
        )
        source = (
            "int results[4];\n" + game_source +
            "int main(void) {\n"
            "  gfx_tile_size(MAP_TILE_PIXEL_SIZE);\n"
            "  results[0] = load_runtime_tile_data();\n"
            "  results[1] = ensure_runtime_tile_loaded(0);\n"
            "  results[2] = ensure_runtime_tile_loaded(90);\n"
            "  runtime_tile_binary = 0; runtime_tile_loaded[0] = 0;\n"
            "  results[3] = ensure_runtime_tile_loaded(0);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(source_path)))
        with tempfile.TemporaryDirectory() as directory:
            cpu.disk = DiskDevice(Path(directory))
            cpu.disk.set_file_root(ROOT / "modules" / "Tank-16" / "disks")
            for _ in range(40):
                run_frame(cpu)
                if not cpu.running:
                    break

        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(4)],
            [1, 1, 1, 1],
            cpu.console_device.output.decode("ascii", errors="replace"))
        actual_tile = bytes(cpu.tile_memory[:64 * 64])
        expected_tile = bytes.fromhex(first_tile["pixels"])
        self.assertEqual(binary_tiles[:64 * 64], expected_tile)
        first_difference = next(
            (index for index, pixels in enumerate(
                zip(actual_tile, expected_tile)) if pixels[0] != pixels[1]),
            None,
        )
        self.assertEqual(actual_tile, expected_tile, first_difference)
        palette_rgb = tuple(tileset["palette"][0])
        loaded_palette_indices = [
            index for index, color in enumerate(cpu.palette)
            if tuple(color)[:3] == palette_rgb
        ]
        self.assertIn(32, loaded_palette_indices, loaded_palette_indices)
        runtime_log = cpu.console_device.output.decode("ascii")
        self.assertIn("TILE DATA: assets/TANK16_TILES.json", runtime_log)
        self.assertIn("TILE 0 terrain source=(0,0) loaded", runtime_log)
        self.assertIn("TILE 90 object source=(0,0) loaded", runtime_log)
        self.assertIn("TILE 0 terrain source=(0,0) loaded (4096 bytes)",
                      runtime_log)
        self.assertIn("TILE 0 terrain source=(0,0) loaded (8206 bytes)",
                      runtime_log)
        self.assertIn("#define POWERUP_HEALTH_TILE", header)
        self.assertIn("#define POWERUP_DAMAGE_TILE", header)
        self.assertIn("#define POWERUP_SPEED_TILE", header)

    def test_runtime_loading_screen_shows_progress_and_rotates_spinner(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        game_source = self.game_source(source_path).replace(
            '#include "map_assets.h"', build_runtime_assets_header(), 1
        )
        source = (
            game_source +
            "int main(void) {\n"
            "  gfx_mode(GFX_MODE_640_480);\n"
            "  gfx_layer_reset(1 << LAYER_HUD);\n"
            "  gfx_layer_transparency(LAYER_HUD, 15);\n"
            "  draw_runtime_loading_screen(\"LOADING TILES\", 50, 100);\n"
            "  draw_runtime_loading_screen(\"LOADING TILES\", 50, 100);\n"
            "  draw_runtime_loading_screen(\"LOADING TILES\", 100, 100);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(source_path)))

        run_frame(cpu)
        first_spinner_frame = bytes(cpu.gpu_layers[4])
        run_frame(cpu)
        second_spinner_frame = bytes(cpu.gpu_layers[4])
        self.assertNotEqual(first_spinner_frame, second_spinner_frame)
        self.assertEqual(layer_pixel(cpu, 4, 150, 278), 3)
        self.assertEqual(layer_pixel(cpu, 4, 400, 278), 10)
        run_frame(cpu)
        self.assertEqual(layer_pixel(cpu, 4, 500, 278), 3)
        run_frame(cpu)
        self.assertFalse(cpu.running)

    def test_finite_runtime_terrain_uses_split_screen_tilemap_renderer(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        game_source = self.game_source(source_path).replace(
            '#include "map_assets.h"', build_runtime_assets_header(), 1
        )
        source = (
            "unsigned char test_tile_pixels[4096];\n" +
            game_source +
            "int main(void) {\n"
            "  int i;\n"
            "  gfx_mode(GFX_MODE_640_480);\n"
            "  gfx_layer_reset(1 << LAYER_TILES);\n"
            "  gfx_layer_transparency(LAYER_TILES, 15);\n"
            "  gfx_tile_size(MAP_TILE_PIXEL_SIZE);\n"
            "  for (i = 0; i < 4096; i++) test_tile_pixels[i] = 7;\n"
            "  gfx_tile_upload(0, test_tile_pixels);\n"
            "  for (i = 0; i < 4096; i++) test_tile_pixels[i] = 8;\n"
            "  gfx_tile_upload(1, test_tile_pixels);\n"
            "  map_data = runtime_map_data;\n"
            "  map_data[0] = 0; map_data[1] = 1;\n"
            "  map_columns = 2; map_rows = 1; map_infinite = 0;\n"
            "  configure_runtime_tilemap();\n"
            "  gfx_split_camera_wide(0, 0, 0, 0);\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(source_path)))

        run_frame(cpu)

        self.assertEqual(layer_pixel(cpu, 1, 20, 20), 7)
        self.assertEqual(layer_pixel(cpu, 1, 80, 20), 8)
        self.assertEqual(layer_pixel(cpu, 1, 340, 20), 7)
        self.assertEqual(layer_pixel(cpu, 1, 400, 20), 8)

    def test_timed_powerups_heal_boost_speed_and_increase_shot_damage(self):
        results = self.scenario_results(
            "int i;\n"
            "start_round();\n"
            "tanks[0].health = 50;\n"
            "powerup.type = POWERUP_HEALTH;\n"
            "powerup.x = tanks[0].x; powerup.y = tanks[0].y;\n"
            "powerup.life = 10; update_powerups();\n"
            "results[0] = tanks[0].health;\n"
            "powerup.type = POWERUP_SPEED;\n"
            "powerup.x = tanks[0].x; powerup.y = tanks[0].y;\n"
            "powerup.life = 10; update_powerups();\n"
            "results[1] = tanks[0].speed_boost_frames;\n"
            "for (i = 0; i < 55; i++) update_tank(0, 4, 0);\n"
            "results[2] = tanks[0].speed;\n"
            "for (i = 0; i < 300; i++) update_powerups();\n"
            "results[3] = tanks[0].speed_boost_frames;\n"
            "results[4] = tanks[0].speed;\n"
            "powerup.type = POWERUP_DAMAGE;\n"
            "powerup.x = tanks[0].x; powerup.y = tanks[0].y;\n"
            "powerup.life = 10; update_powerups();\n"
            "fire_shot(0); results[5] = shots[0].damage;\n",
            6,
        )
        self.assertEqual(results, [75, 300, 48, 0, 32, 20])

    def test_tank_movement_emits_periodic_engine_sound(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        game_source = self.game_source(source_path)
        source = (
            game_source +
            "int main(void) {\n"
            "  start_round();\n"
            "  tanks[0].speed = 16; tanks[0].throttle = 16;\n"
            "  update_tank(0, 0, 0);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(source_path)))
        while cpu.running:
            cpu.step()
        self.assertIn((0, 120, 5, 2, 12), cpu.sound_log)

    def test_json_map_is_data_not_an_executable_program(self):
        map_path = ROOT / "modules" / "Tank-16" / "disks" / "maps" / "PERUS-1.json"
        map_data, tiles, palette, dimensions = load_map_assets(map_path)
        self.assertEqual((map_data["width"], map_data["height"]), (10, 8))
        self.assertEqual(dimensions[2], map_tile_pixel_size())
        self.assertEqual(len(tiles[0]), dimensions[2] ** 2)
        self.assertTrue(any(pixel > 15 for pixel in tiles[0]))
        self.assertTrue(palette)
        with self.assertRaisesRegex(ValueError, "JSON files are data"):
            load_program_file(map_path)

    def test_maximum_map_renders_and_collides_beyond_signed_word_range(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        map_header = (
            "#define MAP_COLUMNS 1024\n"
            "#define MAP_ROWS 1024\n"
            "#define MAP_CELLS (MAP_COLUMNS * MAP_ROWS)\n"
            "#define TILE_COUNT 2\n"
            "#define TILE_PIXEL_COUNT 4096\n"
            "#define MAP_PALETTE_COUNT 1\n"
            "#define MAP_PALETTE_BASE 32\n"
            "#define MAP_OBJECT_COUNT 0\n"
            "#define MAP_OBJECT_CAPACITY 1\n"
            "#define OBJECT_FIXED 0\n"
            "#define OBJECT_EXPLOSION 1\n"
            "#define OBJECT_DISAPPEAR 2\n"
            "unsigned char map_palette[MAP_PALETTE_COUNT][3];\n"
            "unsigned char terrain_tiles[TILE_COUNT][TILE_PIXEL_COUNT];\n"
            "void upload_terrain_tiles(void) {\n"
            "  gfx_tile_upload(0, terrain_tiles[0]);\n"
            "  gfx_tile_upload(1, terrain_tiles[1]);\n"
            "}\n"
            + "".join(
                f"unsigned char map_data_{page}[32768];\n"
                for page in range(32)
            )
            + "#define map_data map_data_0\n"
            + "".join(
                f"unsigned char map_obstacles_{page}[32768];\n"
                for page in range(32)
            )
            + "int map_object_x[MAP_OBJECT_CAPACITY];\n"
            "int map_object_y[MAP_OBJECT_CAPACITY];\n"
            "int map_object_tile[MAP_OBJECT_CAPACITY];\n"
            "int map_object_hp[MAP_OBJECT_CAPACITY];\n"
            "int map_object_removal[MAP_OBJECT_CAPACITY];\n"
        )
        game_source = self.game_source(source_path).replace(
            '#include "map_assets.h"', map_header, 1
        )
        test_source = (
            "int main(void) {\n"
            "  int i;\n"
            "  gfx_mode(GFX_MODE_640_480); gfx_tile_size(64);\n"
            "  for (i = 0; i < 4096; i++) {\n"
            "    terrain_tiles[0][i] = 3; terrain_tiles[1][i] = 1;\n"
            "  }\n"
            "  upload_terrain_tiles();\n"
            "  map_obstacles_2[0] = 1;\n"
            "  map_obstacles_31[32767] = 1;\n"
            "  results[0] = map_obstacle_at(64, 0);\n"
            "  results[1] = map_obstacle_at(1023, 1023);\n"
            "  tanks[0].health = 100; tanks[0].direction = 8;\n"
            "  results[2] = blocked(32, 4128, 0);\n"
            "  results[3] = blocked(65504, 65504, 0);\n"
            "  results[4] = blocked(65400, 65000, 0);\n"
            "  start_round();\n"
            "  tanks[0].x = 65504; tanks[0].y = 65504;\n"
            "  tanks[1].x = 64000; tanks[1].y = 64000;\n"
            "  gfx_layer_select(LAYER_TILES);\n"
            "  gfx_tilemap(map_data, MAP_COLUMNS, MAP_ROWS, 0, 0);\n"
            "  draw_game(); gfx_present(); return 0;\n"
            "}\n"
        )
        image = compile_source(
            "long results[8];\nint main(void);\n" +
            game_source + test_source,
            str(source_path),
        )
        cpu = make_cpu(image)
        steps = 0
        for _ in range(40):
            if not cpu.running:
                break
            cpu.frame_yield = False
            cpu.frames = (getattr(cpu, "frames", 0) + 1) & 0xFFFF
            cpu.update_rtc()
            frame_steps = 0
            while cpu.running and not cpu.frame_yield and frame_steps < 400_000:
                cpu.step()
                frame_steps += 1
            steps += frame_steps
        self.assertFalse(cpu.running, "Maximum-map startup exceeded 40 frames")
        results = [
            cpu.read_32(GLOBAL_BASE + index * 4)
            for index in range(8)
        ]
        results = [
            value - 0x100000000 if value & 0x80000000 else value
            for value in results
        ]
        self.assertEqual(
            results[:5], [1, 1, 1, 1, 0],
        )
        self.assertEqual((cpu.tilemap_width, cpu.tilemap_height), (1024, 1024))
        self.assertEqual(
            cpu.split_cameras, ((65216, 65056), (63840, 63760))
        )
        self.assertEqual(cpu.sprites[40]["x"], 65488)
        self.assertEqual(cpu.sprites[40]["y"], 65488)
        self.assertEqual(layer_pixel(cpu, 1, 0, 0), 3)
        self.assertEqual(layer_pixel(cpu, 3, 256, 416), 1)

    def test_tank_speed_and_sprite_heading_match_all_directions(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        game_source = self.game_source(source_path)
        test_source = (
            "int main(void) {\n"
            "  int direction;\n"
            "  int frame;\n"
            "  tank_sprite_slots[0] = -1;\n"
            "  for (direction = 0; direction < 32; direction++) {\n"
            "    start_round();\n"
            "    tanks[0].x = 100; tanks[0].y = 260;\n"
            "    tanks[0].direction = direction;\n"
            "    tanks[0].speed = MAX_FORWARD_SPEED; tanks[0].throttle = MAX_FORWARD_SPEED;\n"
            "    tanks[1].x = 560; tanks[1].y = 440;\n"
            "    for (frame = 0; frame < 16; frame++)\n"
            "      update_tank(0, 4, 0);\n"
            "    movement_results[direction * 3] = tanks[0].x - 100;\n"
            "    movement_results[direction * 3 + 1] = tanks[0].y - 260;\n"
            "    draw_tank(0);\n"
            "    movement_results[direction * 3 + 2] = tank_sprite_slots[0];\n"
            "  }\n"
            "  return 0;\n"
            "}\n"
        )
        filename = ROOT / "modules" / "Tank-16" / "movement_test.c"
        cpu = make_cpu(compile_source(
            "int movement_results[96];\nint main(void);\n" +
            game_source + test_source,
            str(filename),
        ))
        while cpu.running:
            cpu.step()

        def signed_word(value):
            return value - 0x10000 if value & 0x8000 else value

        results = [
            signed_word(cpu.read_16(GLOBAL_BASE + index * 2))
            for index in range(96)
        ]
        for direction in range(32):
            dx, dy, sprite_slot = results[direction * 3:direction * 3 + 3]
            self.assertGreaterEqual(dx * dx + dy * dy, 900)
            self.assertLessEqual(dx * dx + dy * dy, 1156)
            actual_angle = math.atan2(dx, -dy) % (math.pi * 2)
            expected_angle = direction * math.pi / 16
            angle_error = abs(actual_angle - expected_angle)
            angle_error = min(angle_error, math.pi * 2 - angle_error)
            self.assertLessEqual(angle_error, 0.05)
            self.assertEqual(sprite_slot, direction + 32)

    def test_shot_speed_and_aim_match_all_tank_directions(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        game_source = self.game_source(source_path)
        test_source = (
            "int main(void) {\n"
            "  int direction;\n"
            "  int frame;\n"
            "  int i;\n"
            "  for (direction = 0; direction < 32; direction++) {\n"
            "    start_round();\n"
            "    tanks[0].x = 320; tanks[0].y = 240;\n"
            "    tanks[0].direction = direction;\n"
            "    tanks[1].x = 600; tanks[1].y = 440;\n"
            "    fire_shot(0);\n"
            "    shot_results[direction * 4] = shots[0].x;\n"
            "    shot_results[direction * 4 + 1] = shots[0].y;\n"
            "    shot_results[direction * 4 + 2] = shots[0].direction;\n"
            "    for (frame = 0; frame < 10; frame++) update_shots();\n"
            "    shot_results[direction * 4 + 3] = shots[0].life;\n"
            "    shot_results[128 + direction * 2] = shots[0].x;\n"
            "    shot_results[129 + direction * 2] = shots[0].y;\n"
            "  }\n"
            "  return 0;\n"
            "}\n"
        )
        filename = ROOT / "modules" / "Tank-16" / "shot_direction_test.c"
        cpu = make_cpu(compile_source(
            "int shot_results[192];\nint main(void);\n" +
            game_source + test_source,
            str(filename),
        ))
        while cpu.running:
            cpu.step()

        def signed_word(value):
            return value - 0x10000 if value & 0x8000 else value

        results = [
            signed_word(cpu.read_16(GLOBAL_BASE + index * 2))
            for index in range(192)
        ]
        for direction in range(32):
            start_x, start_y, shot_direction, life = results[
                direction * 4:direction * 4 + 4
            ]
            end_x, end_y = results[128 + direction * 2:
                                   130 + direction * 2]
            dx = end_x - start_x
            dy = end_y - start_y
            distance_squared = dx * dx + dy * dy
            self.assertEqual(shot_direction, direction)
            self.assertEqual(life, 110)
            self.assertGreaterEqual(distance_squared, 2304)
            self.assertLessEqual(distance_squared, 2704)
            actual_angle = math.atan2(dx, -dy) % (math.pi * 2)
            expected_angle = direction * math.pi / 16
            angle_error = abs(actual_angle - expected_angle)
            angle_error = min(angle_error, math.pi * 2 - angle_error)
            self.assertLessEqual(angle_error, 0.05)

    def test_c_program_can_read_third_held_key_byte(self):
        source = (
            '#include "suomi_gfx.h"\n'
            "unsigned int result;\n"
            "int main(void) {\n"
            "  result = gfx_keys_ext2();\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )
        filename = ROOT / "modules" / "Tank-16" / "key_bank_test.c"
        cpu = make_cpu(compile_source(source, str(filename)))
        cpu.memory[KEYS_ADDR + 2] = 0x2D
        run_frame(cpu)

        self.assertEqual(cpu.read_16(GLOBAL_BASE), 0x2D)

    def test_combat_damage_round_wins_and_restart(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        game_source = source_path.read_text(encoding="utf-8")
        self.assertEqual(game_source.count("int main(void)"), 1)
        game_source = game_source.replace(
            "int main(void)", "int tank16_game_entry(void)", 1)
        test_source = (
            "int main(void) {\n"
            "  int i;\n"
            "  start_round();\n"
            "  tanks[0].x = 100; tanks[0].y = 100; tanks[0].direction = 8;\n"
            "  tanks[1].x = 140; tanks[1].y = 100;\n"
            "  fire_shot(0);\n"
            "  for (i = 0; i < 10; i++) update_shots();\n"
            "  results[0] = tanks[1].health;\n"
            "  start_round();\n"
            "  tanks[0].x = 136; tanks[0].y = 184;\n"
            "  update_objects();\n"
            "  results[1] = tanks[0].health;\n"
            "  start_round();\n"
            "  tanks[0].x = 100; tanks[0].y = 100;\n"
            "  tanks[0].direction = 8;\n"
            "  tanks[1].x = 120; tanks[1].y = 100;\n"
            "  tanks[0].speed = MAX_FORWARD_SPEED; tanks[0].throttle = MAX_FORWARD_SPEED;\n"
            "  update_tank(0, 4, 0);\n"
            "  results[2] = tanks[0].health;\n"
            "  results[3] = tanks[1].health;\n"
            "  start_round();\n"
            "  tanks[0].health = 80; tanks[1].health = 50;\n"
            "  round_frames = 1;\n"
            "  update_game(0, 0);\n"
            "  results[4] = wins[0];\n"
            "  results[5] = round_winner;\n"
            "  for (i = 0; i < 91; i++) update_game(0, 0);\n"
            "  results[6] = tanks[0].health;\n"
            "  results[7] = wins[0];\n"
            "  update_game(32, 0);\n"
            "  results[8] = wins[0];\n"
            "  results[9] = wins[1];\n"
            "  results[10] = tanks[0].health;\n"
            "  update_tank(1, 0, KEYX2_P2_LEFT | KEYX2_P2_FORWARD);\n"
            "  results[11] = tanks[1].direction;\n"
            "  return 0;\n"
            "}\n"
        )
        filename = ROOT / "modules" / "Tank-16" / "combat_test.c"
        cpu = make_cpu(compile_source(
            "int results[12];\n" + game_source + test_source,
            str(filename),
        ))
        while cpu.running:
            cpu.step()

        results = [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(12)]
        self.assertEqual(results, [90, 100, 95, 95, 1, 0, 100, 1, 0, 0, 100, 24])


if __name__ == "__main__":
    unittest.main()
