import unittest

from gfx_harness import ROOT, compile_file, make_cpu, run_frame
from src.SuomiCPU import (
    GPU_BASE, KEYS_ADDR, SPEED_ADDR, SPEED_MAX,
)
from c_compiler import compile_source

SOURCE = ROOT / "modules" / "Hertta-3D" / "hertta3d.c"


class Hertta3DTests(unittest.TestCase):
    def test_module_starts_and_submits_a_valid_gpu_scene(self):
        cpu = make_cpu(compile_file(SOURCE))
        steps = run_frame(cpu)
        self.assertLess(steps, 65_000)
        self.assertEqual(cpu.memory[SPEED_ADDR], SPEED_MAX)

        cpu.memory[KEYS_ADDR] = 32
        run_frame(cpu)
        self.assertEqual(cpu.read_16(GPU_BASE + 14), 0)

    def test_gameplay_frame_renders_maze_walls_with_the_gpu(self):
        source = SOURCE.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n")]
        source += (
            "\nint main(void) {\n"
            "  level = 1;\n"
            "  reset_level();\n"
            "  render_game();\n"
            "  gfx_present();\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(SOURCE)))
        steps = 0
        while cpu.running and steps < 300_000:
            cpu.step()
            steps += 1

        self.assertFalse(cpu.running, "instrumented gameplay render did not finish")
        self.assertEqual(cpu.read_16(GPU_BASE + 14), 0)
        surface = cpu.vram_surface

        def color_at(x, y):
            return surface.map_rgb(surface.get_at((x, y))) & 0xFF

        self.assertEqual(color_at(160, 30), 4)
        self.assertIn(color_at(140, 120), (8, 9, 14))
        self.assertEqual(color_at(160, 220), 10)
        self.assertGreater(
            sum(color_at(x, y) == 8 for y in range(240) for x in range(320)),
            1_000,
        )
        self.assertGreater(
            sum(color_at(x, y) == 9 for y in range(240) for x in range(320)),
            1_000,
        )
        self.assertEqual(color_at(255, 28), 9)
        self.assertEqual(color_at(260, 33), 3)
        self.assertEqual(color_at(296, 69), 8)
        edge_area = [
            color_at(x, y)
            for y in range(18, 108)
            for x in range(250)
        ]
        self.assertGreater(sum(pixel == 10 for pixel in edge_area), 100)

    def test_adjacent_rays_that_hit_one_wall_cell_share_one_surface(self):
        source = SOURCE.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n")]
        source += (
            "\nint main(void) {\n"
            "  int i;\n"
            "  level = 1;\n"
            "  for (i = 0; i < MW * MH; i++) maze[i] = 0;\n"
            "  for (i = 0; i < MW; i++) {\n"
            "    maze[i] = 1;\n"
            "    maze[(MH - 1) * MW + i] = 1;\n"
            "    maze[i * MW] = 1;\n"
            "    maze[i * MW + MW - 1] = 1;\n"
            "  }\n"
            "  player_x = FIX + FIX / 2;\n"
            "  player_y = FIX + FIX / 2;\n"
            "  player_angle = 0;\n"
            "  dog_x = player_x;\n"
            "  dog_y = player_y + 4 * FIX;\n"
            "  build_view();\n"
            "  *((int *)0x10000000) = vertex_count;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(SOURCE)))
        steps = 0
        while cpu.running and steps < 300_000:
            cpu.step()
            steps += 1

        self.assertFalse(cpu.running, "instrumented wall-surface build did not finish")
        self.assertLess(cpu.read_16(0x10000000), 4 * 24)

    def test_throwing_a_treat_in_front_of_hertta_counts_a_snack(self):
        source = SOURCE.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n")]
        source += (
            "\nint main(void) {\n"
            "  int i;\n"
            "  level = 1;\n"
            "  reset_level();\n"
            "  for (i = 0; i < MW * MH; i++) maze[i] = 0;\n"
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

    def test_forward_input_moves_player_and_later_levels_need_more_treats(self):
        source = SOURCE.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n")]
        source += (
            "\nint main(void) {\n"
            "  int start_y;\n"
            "  int easy_treats;\n"
            "  int hard_treats;\n"
            "  level = 1;\n"
            "  reset_level();\n"
            "  player_angle = 0;\n"
            "  start_y = player_y;\n"
            "  *((unsigned char *)0x43004) = KEY_UP;\n"
            "  update_game();\n"
            "  *((int *)0x10000000) = player_y;\n"
            "  *((int *)0x10000002) = start_y;\n"
            "  easy_treats = treats_needed;\n"
            "  level = 5;\n"
            "  reset_level();\n"
            "  hard_treats = treats_needed;\n"
            "  *((int *)0x10000004) = easy_treats;\n"
            "  *((int *)0x10000006) = hard_treats;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(SOURCE)))
        steps = 0
        while cpu.running and steps < 100_000:
            cpu.step()
            steps += 1

        self.assertFalse(cpu.running, "instrumented movement/level test did not finish")
        self.assertLess(cpu.read_16(0x10000000), cpu.read_16(0x10000002))
        self.assertGreater(cpu.read_16(0x10000006), cpu.read_16(0x10000004))

    def test_generated_mazes_are_connected_and_keep_player_and_hertta_reachable(self):
        source = SOURCE.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n")]
        source += (
            "\nint main(void) {\n"
            "  int level_index;\n"
            "  int cell;\n"
            "  for (level_index = 1; level_index <= 10; level_index++) {\n"
            "    level = level_index;\n"
            "    make_maze();\n"
            "    for (cell = 0; cell < MW * MH; cell++)\n"
            "      *((unsigned char *)(0x10000000 + (level_index - 1) * MW * MH + cell)) = maze[cell];\n"
            "  }\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(SOURCE)))
        steps = 0
        while cpu.running and steps < 1_000_000:
            cpu.step()
            steps += 1

        self.assertFalse(cpu.running, "instrumented maze generation did not finish")
        for level in range(10):
            base = 0x10000000 + level * 100
            maze = list(cpu.memory[base:base + 100])
            open_cells = {cell for cell, wall in enumerate(maze) if not wall}
            self.assertIn(11, open_cells)
            self.assertIn(77, open_cells)
            self.assertEqual(len(open_cells), 31)
            reached = {11}
            queue = [11]
            while queue:
                cell = queue.pop(0)
                x = cell % 10
                y = cell // 10
                for neighbor in (
                    cell - 10 if y > 0 else -1,
                    cell + 1 if x < 9 else -1,
                    cell + 10 if y < 9 else -1,
                    cell - 1 if x > 0 else -1,
                ):
                    if neighbor in open_cells and neighbor not in reached:
                        reached.add(neighbor)
                        queue.append(neighbor)
            self.assertEqual(reached, open_cells, f"level {level + 1} contains disconnected corridors")

    def test_hertta_wanders_and_uses_the_maze_path_to_reach_the_player(self):
        source = SOURCE.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n")]
        source += (
            "\nint main(void) {\n"
            "  int i;\n"
            "  int start_x;\n"
            "  int start_y;\n"
            "  level = 1;\n"
            "  reset_level();\n"
            "  start_x = dog_x;\n"
            "  start_y = dog_y;\n"
            "  for (i = 0; i < 200; i++) update_dog();\n"
            "  *((int *)0x10000000) = dog_x;\n"
            "  *((int *)0x10000002) = dog_y;\n"
            "  *((int *)0x10000004) = start_x;\n"
            "  *((int *)0x10000006) = start_y;\n"
            "  dog_x = start_x;\n"
            "  dog_y = start_y;\n"
            "  state = ST_HAPPY;\n"
            "  *((int *)0x10000008) = path_direction(dog_x, dog_y, player_x, player_y);\n"
            "  update_dog();\n"
            "  *((int *)0x1000000A) = dog_x;\n"
            "  *((int *)0x1000000C) = dog_y;\n"
            "  *((int *)0x1000000E) = dog_angle;\n"
            "  player_x = dog_x;\n"
            "  player_y = dog_y;\n"
            "  update_dog();\n"
            "  *((int *)0x10000010) = state;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(SOURCE)))
        steps = 0
        while cpu.running and steps < 500_000:
            cpu.step()
            steps += 1

        self.assertFalse(cpu.running, "instrumented Hertta movement did not finish")
        self.assertNotEqual(
            (cpu.read_16(0x10000000), cpu.read_16(0x10000002)),
            (cpu.read_16(0x10000004), cpu.read_16(0x10000006)),
        )
        direction = cpu.read_16(0x10000008)
        self.assertIn(direction, range(4))
        self.assertEqual(cpu.read_16(0x1000000E), direction * 64)
        self.assertNotEqual(
            (cpu.read_16(0x1000000A), cpu.read_16(0x1000000C)),
            (cpu.read_16(0x10000004), cpu.read_16(0x10000006)),
        )
        self.assertEqual(cpu.read_16(0x10000010), 3)

    def test_wasd_controls_move_forward_and_turn_left_and_right(self):
        source = SOURCE.read_text(encoding="utf-8")
        source = source[:source.index("\nint main(void)\n")]
        source += (
            "\nint main(void) {\n"
            "  int start_y;\n"
            "  level = 1;\n"
            "  reset_level();\n"
            "  player_angle = 0;\n"
            "  start_y = player_y;\n"
            "  *((unsigned char *)0x43004) = KEY_UP;\n"
            "  update_game();\n"
            "  *((int *)0x10000000) = player_y;\n"
            "  *((unsigned char *)0x43004) = KEY_DOWN;\n"
            "  update_game();\n"
            "  *((int *)0x10000002) = player_y;\n"
            "  player_angle = 128;\n"
            "  *((unsigned char *)0x43004) = KEY_LEFT;\n"
            "  update_game();\n"
            "  *((int *)0x10000004) = player_angle;\n"
            "  *((unsigned char *)0x43004) = KEY_RIGHT;\n"
            "  update_game();\n"
            "  *((int *)0x10000006) = player_angle;\n"
            "  *((int *)0x10000008) = start_y;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = make_cpu(compile_source(source, str(SOURCE)))
        steps = 0
        while cpu.running and steps < 100_000:
            cpu.step()
            steps += 1

        self.assertFalse(cpu.running, "instrumented WASD control test did not finish")
        self.assertLess(cpu.read_16(0x10000000), cpu.read_16(0x10000008))
        self.assertEqual(cpu.read_16(0x10000002), cpu.read_16(0x10000008))
        self.assertEqual(cpu.read_16(0x10000004), 125)
        self.assertEqual(cpu.read_16(0x10000006), 128)


if __name__ == "__main__":
    unittest.main()
