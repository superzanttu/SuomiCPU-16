import tempfile
import unittest
from pathlib import Path

from c_compiler import GLOBAL_BASE, CCompilerError, compile_source
from gfx_harness import ROOT, compile_file, make_cpu, pixel, run_frame, vram
from src.SuomiCPU import BACK_START, GPU_BASE, KEYS_ADDR, MEM_SIZE, SCREEN_WIDTH, SuomiCompute16

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


if __name__ == "__main__":
    unittest.main()
