from pathlib import Path
import unittest

from c_compiler import GLOBAL_BASE, compile_source
from gfx_harness import ROOT, compile_file, layer_pixel, make_cpu, run_frame
from src.SuomiCPU import KEYS_ADDR


class Tank16Tests(unittest.TestCase):
    def test_main_game_compiles_and_renders_a_frame(self):
        source = ROOT / "modules" / "Tank-16" / "main.c"
        cpu = make_cpu(compile_file(source))
        run_frame(cpu)
        run_frame(cpu)

        self.assertEqual(cpu.tilemap_width, 40)
        self.assertEqual(cpu.tilemap_height, 30)
        self.assertEqual(layer_pixel(cpu, 2, 76, 260), 3)
        self.assertEqual(layer_pixel(cpu, 2, 564, 260), 2)

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
