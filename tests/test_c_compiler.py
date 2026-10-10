import tempfile
import unittest
from pathlib import Path

from assembler import assemble_file
from src.SuomiCPU import KBD_ADDR, MEM_SIZE, SCREEN_WIDTH, VRAM_START, SuomiCompute16, load_program_file
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

    def test_typedef_and_enum_declarations(self):
        image = compile_source(
            "enum Color { RED = 2, GREEN, BLUE = RED + 4 };\n"
            "typedef unsigned int counter;\n"
            "counter result;\n"
            "int main(void) {\n"
            "  typedef int local_type;\n"
            "  local_type value;\n"
            "  value = BLUE;\n"
            "  result = value + GREEN;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 9)

    def test_struct_union_member_access_and_aggregate_initializers(self):
        image = compile_source(
            "struct Point { unsigned char x; unsigned char y; int value; };\n"
            "struct Point point = { 7, 8, 3 };\n"
            "union Number { int word; unsigned char byte; } number = { 5 };\n"
            "int result;\n"
            "int main(void) {\n"
            "  struct Point *pointer;\n"
            "  pointer = &point;\n"
            "  pointer->value = pointer->x + pointer->y + number.word;\n"
            "  result = point.value;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 20)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 6), 20)

    def test_c90_casts_conditional_comma_and_sizeof(self):
        image = compile_source(
            "int result, first, second;\n"
            "int main(void) {\n"
            "  char text[5];\n"
            "  first = 1;\n"
            "  second = 2;\n"
            "  result = (first = 7, second = 8, first + second);\n"
            "  result += second ? (int)sizeof(char) : sizeof(long *);\n"
            "  result += sizeof(text) + sizeof(int *);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 24)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 7)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 8)

    def test_c90_string_concatenation_and_inferred_array_bounds(self):
        image = compile_source(
            'int data[] = { 1, 2, 3 };\n'
            'char text[] = "ab" "cd";\n'
            'int result;\n'
            'int main(void) {\n'
            '  char local[] = "x" "y";\n'
            '  result = sizeof(data) + sizeof(text) + sizeof(local)\n'
            '      + data[2] + text[1] + local[1];\n'
            '  return 0;\n'
            '}\n'
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 11), 236)

    def test_unsized_local_array_requires_initializer(self):
        with self.assertRaisesRegex(CCompilerError, "requires an initializer"):
            compile_source("int main(void) { int data[]; return 0; }")

    def test_c90_goto_and_labeled_statements(self):
        image = compile_source(
            "int result;\n"
            "int main(void) {\n"
            "  result = 1;\n"
            "  goto done;\n"
            "  result = 2;\n"
            "done:\n"
            "  result += 4;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 5)

    def test_c90_goto_requires_a_defined_label(self):
        with self.assertRaisesRegex(CCompilerError, "undefined label"):
            compile_source("int main(void) { goto missing; return 0; }")

    def test_c90_array_parameters_adjust_to_pointers(self):
        image = compile_source(
            "int sum(int values[3]);\n"
            "int result;\n"
            "int sum(int values[]) { return values[0] + values[1] + values[2]; }\n"
            "int main(void) { int items[3]; items[0] = 2; items[1] = 3; items[2] = 4; "
            "result = sum(items); return 0; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 9)

    def test_c90_standard_string_and_memory_library(self):
        image = compile_source(
            "#include <string.h>\n"
            "size_t result;\n"
            "int comparison;\n"
            "char buffer[12];\n"
            "int main(void) {\n"
            "  char first[8];\n"
            "  char second[8];\n"
            "  strcpy(first, \"abc\");\n"
            "  strcpy(second, \"abd\");\n"
            "  result = strlen(first);\n"
            "  comparison = strcmp(first, second);\n"
            "  strcat(buffer, first);\n"
            "  memcpy(buffer + 3, second, 3);\n"
            "  buffer[6] = '\\0';\n"
            "  comparison += memcmp(buffer, \"abcabd\", 6);\n"
            "  memmove(buffer + 1, buffer, 5);\n"
            "  memset(buffer + 6, '!', 2);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 3)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), -1 & 0xFFFF)
        self.assertEqual(cpu.memory[GLOBAL_BASE + 4:GLOBAL_BASE + 12], b"aabcab!!")

    def test_c90_string_search_and_bounded_operations(self):
        image = compile_source(
            "#include <string.h>\n"
            "char text[16];\n"
            "char copy[16];\n"
            "int results[10];\n"
            "int main(void) {\n"
            "  strcpy(text, \"abc\");\n"
            "  results[0] = strcspn(text, \"x\");\n"
            "  results[1] = strspn(text, \"ab\");\n"
            "  results[2] = strncmp(text, \"abd\", 2);\n"
            "  results[3] = strchr(text, 'b') == text + 1;\n"
            "  results[4] = strrchr(text, 'a') == text;\n"
            "  results[5] = strpbrk(text, \"xb\") == text + 1;\n"
            "  results[6] = strstr(text, \"bc\") == text + 1;\n"
            "  results[7] = strcoll(text, \"abd\") < 0;\n"
            "  results[8] = strxfrm(copy, text, sizeof(copy));\n"
            "  strncpy(copy, \"xy\", 4);\n"
            "  strncat(text, \"de\", 1);\n"
            "  results[9] = strcmp(text, \"abcd\") == 0;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + 32 + index * 2) for index in range(10)],
            [3, 2, 0, 1, 1, 1, 1, 1, 3, 1],
        )
        self.assertEqual(cpu.memory[GLOBAL_BASE + 16:GLOBAL_BASE + 20], b"xy\0\0")

    def test_c90_ctype_library(self):
        image = compile_source(
            "#include <ctype.h>\n"
            "int results[14];\n"
            "int main(void) {\n"
            "  results[0] = isalnum('z');\n"
            "  results[1] = isalpha('5');\n"
            "  results[2] = iscntrl(127);\n"
            "  results[3] = isdigit('4');\n"
            "  results[4] = isgraph(' ');\n"
            "  results[5] = islower('m');\n"
            "  results[6] = isprint('~');\n"
            "  results[7] = ispunct('!');\n"
            "  results[8] = isspace('\\n');\n"
            "  results[9] = isupper('A');\n"
            "  results[10] = isxdigit('f');\n"
            "  results[11] = tolower('Q') == 'q';\n"
            "  results[12] = toupper('x') == 'X';\n"
            "  results[13] = tolower('5') == '5';\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(14)],
            [1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        )

    def test_c90_limits_header_matches_target_types(self):
        image = compile_source(
            "#include <limits.h>\n"
            "#if CHAR_BIT != 8 || SCHAR_MIN != -128 || UCHAR_MAX != 255\n"
            "#error incorrect character limits\n"
            "#endif\n"
            "#if INT_MIN != -32768 || INT_MAX != 32767 || UINT_MAX != 65535\n"
            "#error incorrect int limits\n"
            "#endif\n"
            "#if LONG_MIN != -32768 || LONG_MAX != 32767 || ULONG_MAX != 65535\n"
            "#error incorrect target long limits\n"
            "#endif\n"
            "#if WCHAR_MIN != -32768 || WCHAR_MAX != 32767\n"
            "#error incorrect wide character limits\n"
            "#endif\n"
            "int main(void) { return 0; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_offsetof_macro(self):
        image = compile_source(
            "#include <stddef.h>\n"
            "struct Pair { char first; int second; };\n"
            "size_t result = offsetof(struct Pair, second);\n"
            "int main(void) {\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 2)

    def test_c90_float_header_matches_binary16_target(self):
        image = compile_source(
            "#include <float.h>\n"
            "#if FLT_RADIX != 2 || FLT_MANT_DIG != 11 || FLT_DIG != 3\n"
            "#error incorrect float characteristics\n"
            "#endif\n"
            "#if DBL_MANT_DIG != 11 || DBL_DIG != 3\n"
            "#error incorrect double characteristics\n"
            "#endif\n"
            "#if FLT_MIN_EXP != -13 || FLT_MAX_EXP != 16\n"
            "#error incorrect float exponent range\n"
            "#endif\n"
            "int results[6];\n"
            "int main(void) {\n"
            "  results[0] = FLT_MAX == 65504.0;\n"
            "  results[1] = FLT_EPSILON == 0.0009765625;\n"
            "  results[2] = FLT_MIN == 0.00006103515625;\n"
            "  results[3] = DBL_MAX == FLT_MAX;\n"
            "  results[4] = DBL_EPSILON == FLT_EPSILON;\n"
            "  results[5] = DBL_MIN == FLT_MIN;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(6)],
            [1, 1, 1, 1, 1, 1],
        )

    def test_c90_basic_stdlib_functions(self):
        image = compile_source(
            "#include <stdlib.h>\n"
            "int results[7];\n"
            "int main(void) {\n"
            "  results[0] = abs(-12);\n"
            "  results[1] = labs(-13L);\n"
            "  results[2] = atoi(\"  -123tail\");\n"
            "  results[3] = atol(\"+321\");\n"
            "  srand(7);\n"
            "  results[4] = rand();\n"
            "  srand(7);\n"
            "  results[5] = rand();\n"
            "  results[6] = rand() <= RAND_MAX;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(7)],
            [12, 13, -123 & 0xFFFF, 321, 26220, 26220, 1],
        )

    def test_c90_errno_header(self):
        image = compile_source(
            "#include <errno.h>\n"
            "int main(void) {\n"
            "  errno = EDOM;\n"
            "  if (errno != 33) return 1;\n"
            "  errno = ERANGE;\n"
            "  return errno != 34;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

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
