from pathlib import Path
import math
import unittest

from c_compiler import GLOBAL_BASE, compile_source
from gfx_harness import ROOT, compile_file, layer_pixel, make_cpu, run_frame
from src.SuomiCPU import KEYS_ADDR


class Tank16Tests(unittest.TestCase):
    def test_main_game_compiles_and_renders_a_frame(self):
        source = ROOT / "modules" / "Tank-16" / "main.c"
        cpu = make_cpu(compile_file(source))
        for _ in range(8):
            run_frame(cpu)
            if cpu.frame_yield:
                break

        self.assertTrue(cpu.frame_yield)
        self.assertEqual(cpu.tilemap_width, 40)
        self.assertEqual(cpu.tilemap_height, 30)
        player_one = [
            layer_pixel(cpu, 2, x, y)
            for y in range(244, 276)
            for x in range(60, 92)
        ]
        player_two = [
            layer_pixel(cpu, 2, x, y)
            for y in range(244, 276)
            for x in range(548, 580)
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
        self.assertEqual(visible_slots, [20, 36])
        self.assertEqual(cpu.sprites[20]["x"], 548)
        self.assertEqual(cpu.sprites[20]["y"], 244)
        self.assertEqual(cpu.sprites[36]["x"], 60)
        self.assertEqual(cpu.sprites[36]["y"], 244)

    def test_tank_speed_and_sprite_heading_match_all_directions(self):
        source_path = ROOT / "modules" / "Tank-16" / "main.c"
        game_source = source_path.read_text(encoding="utf-8").replace(
            "int main(void)", "int tank16_game_entry(void)", 1)
        test_source = (
            "int main(void) {\n"
            "  int direction;\n"
            "  int frame;\n"
            "  setup_map();\n"
            "  tank_sprite_slots[0] = -1;\n"
            "  for (direction = 0; direction < 32; direction++) {\n"
            "    start_round();\n"
            "    tanks[0].x = 100; tanks[0].y = 260;\n"
            "    tanks[0].direction = direction;\n"
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
        game_source = source_path.read_text(encoding="utf-8").replace(
            "int main(void)", "int tank16_game_entry(void)", 1)
        test_source = (
            "int main(void) {\n"
            "  int direction;\n"
            "  int frame;\n"
            "  int i;\n"
            "  setup_map();\n"
            "  for (i = 0; i < 7; i++) obstacles[i].x = 1000;\n"
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
            "  setup_map();\n"
            "  start_round();\n"
            "  tanks[0].x = 100; tanks[0].y = 100; tanks[0].direction = 8;\n"
            "  tanks[1].x = 140; tanks[1].y = 100;\n"
            "  fire_shot(0);\n"
            "  for (i = 0; i < 10; i++) update_shots();\n"
            "  results[0] = tanks[1].health;\n"
            "  start_round();\n"
            "  tanks[0].x = 136; tanks[0].y = 184;\n"
            "  update_mines();\n"
            "  results[1] = tanks[0].health;\n"
            "  start_round();\n"
            "  tanks[0].x = 100; tanks[0].y = 100;\n"
            "  tanks[0].direction = 8;\n"
            "  tanks[1].x = 120; tanks[1].y = 100;\n"
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
        self.assertEqual(results, [90, 75, 95, 95, 1, 0, 100, 1, 0, 0, 100, 23])


if __name__ == "__main__":
    unittest.main()
