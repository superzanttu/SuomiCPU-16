import tempfile
import unittest
from pathlib import Path

from c_compiler import GLOBAL_BASE, CCompilerError, compile_source
from gfx_harness import ROOT, compile_file, make_cpu, pixel, run_frame, vram
from src.SuomiCPU import BACK_START, GPU_BASE, KEYS_ADDR, MEM_SIZE, MOUSE_ADDR, SCREEN_WIDTH, SuomiCompute16

HEADER = '#include "suomi_gfx.h"\n'


def run_c(body, frames=1):
    cpu = make_cpu(compile_source(HEADER + body, str(ROOT / "inline.c")))
    for _ in range(frames):
        run_frame(cpu)
    return cpu


def bare_cpu():
    cpu = SuomiCompute16.__new__(SuomiCompute16)
    cpu.memory = bytearray(MEM_SIZE)
    cpu.frame_yield = False
    return cpu


def gpu(cpu, command, color=0, x=0, y=0, w=0, h=0, source=0):
    for offset, value in ((2, x), (4, y), (6, w), (8, h)):
        cpu.write_16(GPU_BASE + offset, value & 0xFFFF)
    cpu.write(GPU_BASE + 1, color)
    cpu.write_16(GPU_BASE + 10, source & 0xFFFF)
    cpu.write(GPU_BASE + 12, source >> 16)
    cpu.write(GPU_BASE, command)


def back(cpu, x, y):
    return cpu.memory[BACK_START + y * SCREEN_WIDTH + x]


class GpuTests(unittest.TestCase):
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
        self.assertEqual(cpu.memory[BACK_START], 0)


def vram_pixel(cpu, x, y):
    from src.SuomiCPU import VRAM_START
    return cpu.memory[VRAM_START + y * SCREEN_WIDTH + x]


class CGraphicsTests(unittest.TestCase):
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
        self.assertEqual((cpu.memory[0xE803] << 8) | cpu.memory[0xE804], 4)
        self.assertEqual((cpu.memory[0xE805] << 8) | cpu.memory[0xE806], 255)
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

    def test_preprocessor_errors(self):
        cases = [
            "#define F(x) x\nint main(void) { return 0; }\n",
            "#define\nint main(void) { return 0; }\n",
            '#include "missing_file.h"\nint main(void) { return 0; }\n',
            "#include nothing\nint main(void) { return 0; }\n",
            "#pragma once\nint main(void) { return 0; }\n",
        ]
        for source in cases:
            with self.subTest(source=source), self.assertRaises(CCompilerError):
                compile_source(source)

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
        cpu = make_cpu(compile_file(ROOT / "examples" / name))
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
        cpu = make_cpu(compile_file(ROOT / "examples" / "anaclock.c"))
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
        cpu = make_cpu(compile_file(ROOT / "examples" / "elitedemo.c"))
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
        cpu = make_cpu(compile_file(ROOT / "examples" / "elitedemo.c"))
        run_frame(cpu, 5_000_000)
        first = bytes(vram(cpu))
        for _ in range(40):
            run_frame(cpu, 5_000_000)
        self.assertNotEqual(first, bytes(vram(cpu)))
    def test_showcase_pages_and_interrupt_counters(self):
        cpu = make_cpu(compile_file(ROOT / "examples" / "sc16_showcase.c"))
        run_frame(cpu, 400_000)
        seen = set()
        for _ in range(8):
            cpu.memory[KEYS_ADDR] = 2
            run_frame(cpu, 400_000)
            cpu.memory[KEYS_ADDR] = 0
            cpu.memory[0x43002] |= 2
            run_frame(cpu, 400_000)
            run_frame(cpu, 400_000)
            seen.add(bytes(vram(cpu)))
        self.assertEqual(len(seen), 8)
        self.assertTrue(cpu.running)
        self.assertGreater(cpu.read_16(0x4340), 10)
        self.assertGreaterEqual(cpu.read_16(0x4342), 1)

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
        cpu = make_cpu(compile_file(ROOT / "examples" / "elitedemo.c"))
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

    def test_paint_draws_with_mouse_drag_and_shapes(self):
        cpu = make_cpu(compile_file(ROOT / "examples" / "paint.c"))
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
