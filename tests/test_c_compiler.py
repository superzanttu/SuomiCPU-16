import tempfile
import unittest
from pathlib import Path

from assembler import assemble_file
from SuomiCPU import KBD_ADDR, MEM_SIZE, SCREEN_WIDTH, VRAM_START, SuomiCompute16, load_program_file
from c_compiler import GLOBAL_BASE, CCompilerError, compile_source, main as compiler_main


def run_image(image, maximum_steps=100_000):
    cpu = SuomiCompute16.__new__(SuomiCompute16)
    cpu.memory = bytearray(MEM_SIZE)
    cpu.registers = [0] * 8
    cpu.flags = {"Z": 0, "C": 0}
    cpu.running = True
    cpu.sp = 0x2FFFF
    cpu.load_program(image)
    cpu.reset()
    steps = 0
    while cpu.running and steps < maximum_steps:
        cpu.step()
        steps += 1
    return cpu, steps


class CCompilerTests(unittest.TestCase):
    def test_compiles_functions_locals_and_loops(self):
        image = compile_source(
            "int sum(int, int);\n"
            "int result;\n"
            "int main(void) {\n"
            "  int i;\n"
            "  i = 0;\n"
            "  while (i < 5) { i++; }\n"
            "  result = sum(i, 7);\n"
            "  return 0;\n"
            "}\n"
            "int sum(int x, int y) { int z; z = x + y; return z; }\n"
        )

        cpu, _ = run_image(image)

        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 12)
        self.assertEqual(cpu.sp, 0x2FFFF)

    def test_compiles_arithmetic_and_comparisons(self):
        image = compile_source(
            "int a,b,c,d,e,f,g,h,i,j,k,l,m,n;\n"
            "int main(void) {\n"
            "  a = 7 * 6;\n"
            "  b = -7 / 3;\n"
            "  c = -7 % 3;\n"
            "  d = -7 * 6;\n"
            "  e = 7 * -6;\n"
            "  f = 3 != 4;\n"
            "  g = -3 < -2;\n"
            "  h = -2 >= -3;\n"
            "  i = 3 <= 3;\n"
            "  j = 7 / -3;\n"
            "  k = 7 % -3;\n"
            "  l = -7 / -3;\n"
            "  m = -7 % -3;\n"
            "  n = 0 || 5;\n"
            "  return 0;\n"
            "}\n"
        )

        cpu, _ = run_image(image)
        actual = [cpu.read_16(GLOBAL_BASE + 2 * index) for index in range(14)]

        self.assertFalse(cpu.running)
        self.assertEqual(
            actual,
            [42, 0xFFFE, 0xFFFF, 0xFFD6, 0xFFD6, 1, 1, 1, 1, 0xFFFE, 1, 2, 0xFFFF, 1],
        )
        self.assertEqual(cpu.sp, 0x2FFFF)

    def test_compiles_for_do_while_break_and_continue(self):
        image = compile_source(
            "int result;\n"
            "int main(void) {\n"
            "  int i;\n"
            "  result = 0;\n"
            "  for (i = 0; i < 10; i++) {\n"
            "    if (i == 3) continue;\n"
            "    result += i;\n"
            "    if (i == 7) break;\n"
            "  }\n"
            "  do { result--; } while (result > 20);\n"
            "  return 0;\n"
            "}\n"
        )

        cpu, _ = run_image(image)

        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 20)
        self.assertEqual(cpu.sp, 0x2FFFF)

    def test_logical_operators_short_circuit(self):
        image = compile_source(
            "int side, left, right;\n"
            "int main(void) {\n"
            "  left = 0 && side++;\n"
            "  right = 5 || side++;\n"
            "  return 0;\n"
            "}\n"
        )

        cpu, _ = run_image(image)

        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 0)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x02), 0)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x04), 1)
        self.assertEqual(cpu.sp, 0x2FFFF)

    def test_reports_unsupported_c_constructs(self):
        with self.assertRaisesRegex(CCompilerError, "expected int or void type"):
            compile_source("struct Item { int value; }; int main(void) { return 0; }")

    def test_executes_recursive_factorial_example(self):
        image = compile_source(
            "int result;\n"
            "int factorial(int n) {\n"
            "  if (n <= 1) return 1;\n"
            "  return n * factorial(n - 1);\n"
            "}\n"
            "int main(void) { result = factorial(5); return 0; }\n"
        )

        cpu, _ = run_image(image)

        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 120)
        self.assertEqual(cpu.sp, 0x2FFFF)

    def test_loads_c_file_and_initializes_constant_globals(self):
        with tempfile.TemporaryDirectory() as directory:
            source_path = Path(directory) / "constants.c"
            source_path.write_text(
                "int first = 0x2A; int second = -7 / 3; "
                "int main(void) { return 0; }\n",
                encoding="utf-8",
            )

            image = load_program_file(source_path)

        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 42)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x02), 0xFFFE)

    def test_compiler_cli_binary_can_be_loaded_and_executed(self):
        with tempfile.TemporaryDirectory() as directory:
            source_path = Path(directory) / "small.c"
            binary_path = Path(directory) / "small.bin"
            assembly_path = Path(directory) / "small.asm"
            source_path.write_text(
                "int result; int main(void) { result = 6 * 7; return 0; }\n",
                encoding="utf-8",
            )

            self.assertEqual(
                compiler_main([str(source_path), "-o", str(binary_path)]),
                0,
            )
            self.assertEqual(
                compiler_main(
                    [str(source_path), "-S", "-o", str(assembly_path)]
                ),
                0,
            )
            image = load_program_file(binary_path)
            assembly_image = assemble_file(assembly_path)

        cpu, _ = run_image(image)
        assembly_cpu, _ = run_image(assembly_image)
        self.assertFalse(cpu.running)
        self.assertFalse(assembly_cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 42)
        self.assertEqual(assembly_cpu.read_16(GLOBAL_BASE), 42)

    def test_compiles_arrays_pointers_unsigned_float_and_switch(self):
        image = compile_source(
            'char title[4] = "Cat";\n'
            "int result, route, default_route, dereferenced, float_integer;\n"
            "unsigned int amount, high, quotient;\n"
            "float product;\n"
            "unsigned int remainder;\n"
            "int *identity(int *parameter) { return parameter; }\n"
            "int main(void) {\n"
            '  char local[4] = "dog";\n'
            "  char *text;\n"
            "  int values[2];\n"
            "  int value;\n"
            "  int *pointer;\n"
            "  text = local;\n"
            "  result = title[1];\n"
            "  values[0] = 21;\n"
            "  values[1] = values[0] + 21;\n"
            "  amount = values[1];\n"
            "  value = 40;\n"
            "  pointer = identity(&value);\n"
            "  *pointer = *pointer + 2;\n"
            "  dereferenced = *pointer;\n"
            "  switch (amount) {\n"
            "    case 42: route = 20;\n"
            "    case 43: route += 3; break;\n"
            "    default: route = 7;\n"
            "  }\n"
            "  switch (99) { case 1: default_route = 1; break;"
            " default: default_route = 7; }\n"
            "  product = 1.5 * 2.0;\n"
            "  float_integer = product;\n"
            "  high = 65535;\n"
            "  quotient = high / 2;\n"
            "  remainder = high % 2;\n"
            "  if (high > 1) default_route += 1;\n"
            "  if (text[1] != 'o') return 1;\n"
            "  return 0;\n"
            "}\n"
        )

        cpu, _ = run_image(image)

        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x04), ord("a"))
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x06), 23)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x08), 8)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x0A), 42)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x0C), 3)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x0E), 42)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x12), 32767)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x14), 0x4200)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x16), 1)
        self.assertEqual(cpu.sp, 0x2FFFF)

    def test_text_library_prints_and_tracks_cursor(self):
        image = compile_source(
            "void SCREEN_CLEAR(void);\n"
            "void SCREEN_SET_CURSOR(unsigned char, unsigned char);\n"
            "void SCREEN_PRINT(char *, unsigned char);\n"
            "int SCREEN_GET_CURSOR(void);\n"
            "int cursor;\n"
            "int main(void) {\n"
            '  char text[4] = "A";\n'
            "  SCREEN_CLEAR();\n"
            "  SCREEN_SET_CURSOR(2, 1);\n"
            "  SCREEN_PRINT(text, 1);\n"
            "  cursor = SCREEN_GET_CURSOR();\n"
            "  return 0;\n"
            "}\n"
        )

        cpu, steps = run_image(image, maximum_steps=500_000)

        self.assertFalse(cpu.running)
        self.assertLess(steps, 500_000)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 0x0103)
        self.assertEqual(cpu.read(0x30000 + 8 * 320 + 13), 1)
        self.assertEqual(cpu.read(0x30000 + 8 * 320 + 14), 1)
        self.assertEqual(cpu.read(0x30000 + 8 * 320 + 15), 1)
        self.assertEqual(cpu.read(0x30000 + 8 * 320 + 12), 0)
        self.assertEqual(cpu.sp, 0x2FFFF)

    def test_text_library_prints_ascii_punctuation_and_finnish_letters(self):
        image = compile_source(
            "void SCREEN_PRINT(char *, unsigned char);\n"
            "int SCREEN_GET_CURSOR(void);\n"
            "int cursor;\n"
            "int main(void) {\n"
            '  SCREEN_PRINT("!~öäåÖÄÅ", 1);\n'
            "  cursor = SCREEN_GET_CURSOR();\n"
            "  return 0;\n"
            "}\n"
        )

        cpu, _ = run_image(image, maximum_steps=500_000)

        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 8)
        font = next(segment.data for segment in image.segments if segment.address == 0x4400)
        glyph_indexes = [ord("!") - 32, ord("~") - 32, 95, 96, 97, 98, 99, 100]
        for cell, glyph_index in enumerate(glyph_indexes):
            for y in range(8):
                for x in range(6):
                    expected = (
                        1
                        if x < 5 and font[glyph_index * 8 + y] & (0x80 >> x)
                        else 0
                    )
                    pixel_address = VRAM_START + y * SCREEN_WIDTH + cell * 6 + x
                    self.assertEqual(
                        cpu.read(pixel_address),
                        expected,
                        f"glyph {cell} pixel ({x}, {y})",
                    )
        self.assertEqual(cpu.sp, 0x2FFFF)

    def test_text_input_reads_and_terminates_a_buffer(self):
        image = compile_source(
            "unsigned int SCREEN_INPUT(char *, unsigned int, unsigned char);\n"
            "unsigned int count;\n"
            "int second, terminator;\n"
            "int main(void) {\n"
            "  char line[8];\n"
            "  count = SCREEN_INPUT(line, 4, 1);\n"
            "  second = line[1];\n"
            "  terminator = line[3];\n"
            "  return 0;\n"
            "}\n"
        )
        cpu = SuomiCompute16.__new__(SuomiCompute16)
        cpu.memory = bytearray(MEM_SIZE)
        cpu.registers = [0] * 8
        cpu.flags = {"Z": 0, "C": 0}
        cpu.running = True
        cpu.sp = 0x2FFFF
        cpu.load_program(image)
        cpu.reset()
        characters = [ord("A"), ord("b"), ord("2")]
        sent = 0
        sent_enter = False
        steps = 0
        while cpu.running and steps < 100_000:
            if (
                cpu.read(0x430E) == 4
                and cpu.read(0x430F) == sent
                and cpu.read(KBD_ADDR) == 0
                and sent < len(characters)
            ):
                cpu.write(KBD_ADDR, characters[sent])
                sent += 1
            elif (
                sent == len(characters)
                and cpu.read(0x430F) == len(characters)
                and cpu.read(KBD_ADDR) == 0
                and not sent_enter
            ):
                cpu.write(KBD_ADDR, 13)
                sent_enter = True
            cpu.step()
            steps += 1

        self.assertFalse(cpu.running)
        self.assertEqual(sent, 3)
        self.assertTrue(sent_enter)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 3)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x02), ord("b"))
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x04), 0)
        self.assertEqual(cpu.sp, 0x2FFFF)


if __name__ == "__main__":
    unittest.main()
