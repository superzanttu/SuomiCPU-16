import unittest

from gfx_harness import ROOT, compile_file, make_cpu, run_frame
from src.SuomiCPU import GPU_BASE, KEYS_ADDR
from c_compiler import compile_source

SOURCE = ROOT / "modules" / "Hertta-3D" / "hertta3d.c"


class Hertta3DTests(unittest.TestCase):
    def test_module_starts_and_submits_a_valid_gpu_scene(self):
        cpu = make_cpu(compile_file(SOURCE))
        steps = run_frame(cpu)
        self.assertLess(steps, 30_000)

        cpu.memory[KEYS_ADDR] = 32
        run_frame(cpu)
        self.assertEqual(cpu.read_16(GPU_BASE + 14), 0)

    def test_throwing_a_treat_in_front_of_hertta_counts_a_snack(self):
        source = SOURCE.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n")]
        source += (
            "\nint main(void) {\n"
            "  int i;\n"
            "  level = 1;\n"
            "  reset_level();\n"
            "  player_angle = 128;\n"
            "  dog_x = player_x;\n"
            "  dog_y = player_y + 300;\n"
            "  throw_treat();\n"
            "  for (i = 0; i < 10; i++) update_treats();\n"
            "  *((int *)0x10000000) = treats_eaten;\n"
            "  *((int *)0x10000002) = state;\n"
            "  treats_eaten = treats_needed;\n"
            "  state = ST_HAPPY;\n"
            "  dog_x = player_x + 10;\n"
            "  dog_y = player_y;\n"
            "  update_dog();\n"
            "  *((int *)0x10000004) = state;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(SOURCE)))
        steps = 0
        while cpu.running and steps < 100_000:
            cpu.step()
            steps += 1

        self.assertFalse(cpu.running, "instrumented treat simulation did not finish")
        self.assertEqual(cpu.read_16(0x10000000), 1)
        self.assertEqual(cpu.read_16(0x10000002), 1)
        self.assertEqual(cpu.read_16(0x10000004), 3)

    def test_forward_input_moves_player_and_later_mazes_add_obstacles(self):
        source = SOURCE.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n")]
        source += (
            "\nint main(void) {\n"
            "  int i;\n"
            "  int easy_walls;\n"
            "  int hard_walls;\n"
            "  level = 1;\n"
            "  make_maze();\n"
            "  easy_walls = 0;\n"
            "  for (i = 0; i < MW * MH; i++) easy_walls += maze[i];\n"
            "  level = 5;\n"
            "  make_maze();\n"
            "  hard_walls = 0;\n"
            "  for (i = 0; i < MW * MH; i++) hard_walls += maze[i];\n"
            "  level = 1;\n"
            "  reset_level();\n"
            "  player_angle = 0;\n"
            "  *((unsigned char *)0x43004) = KEY_UP;\n"
            "  update_game();\n"
            "  *((int *)0x10000000) = player_y;\n"
            "  *((int *)0x10000002) = easy_walls;\n"
            "  *((int *)0x10000004) = hard_walls;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(SOURCE)))
        steps = 0
        while cpu.running and steps < 100_000:
            cpu.step()
            steps += 1

        self.assertFalse(cpu.running, "instrumented movement/level test did not finish")
        self.assertLess(cpu.read_16(0x10000000), 384)
        self.assertGreater(cpu.read_16(0x10000004), cpu.read_16(0x10000002))


if __name__ == "__main__":
    unittest.main()
