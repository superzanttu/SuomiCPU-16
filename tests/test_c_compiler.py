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
    def test_c90_unary_operators_require_valid_operand_types(self):
        invalid_sources = (
            "int main(void) { int *pointer; -pointer; return 0; }\n",
            "int main(void) { int *pointer; ~pointer; return 0; }\n",
        )
        for source in invalid_sources:
            with self.subTest(source=source):
                with self.assertRaisesRegex(
                    CCompilerError, "requires an (arithmetic|integer) operand"
                ):
                    compile_source(source)

    def test_c90_logical_negation_accepts_pointer_operands(self):
        image = compile_source(
            "int main(void) { int *pointer = 0; return !pointer ? 0 : 1; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_binary_operators_reject_invalid_operand_types(self):
        invalid_sources = (
            "int main(void) { int *pointer; pointer * pointer; return 0; }\n",
            "int main(void) { int *pointer; pointer + pointer; return 0; }\n",
            "int main(void) { float value; value & value; return 0; }\n",
            "int main(void) { int *pointer; pointer < 1; return 0; }\n",
            "struct S { int value; }; int main(void) { struct S value; "
            "value && 1.0; return 0; }\n",
            "int main(void) { void *pointer; pointer + 1; return 0; }\n",
            "struct Incomplete; int main(void) { struct Incomplete *pointer; "
            "pointer + 1; return 0; }\n",
        )
        for source in invalid_sources:
            with self.subTest(source=source):
                with self.assertRaises(CCompilerError):
                    compile_source(source)

    def test_c90_pointer_arithmetic_and_comparisons_accept_valid_operands(self):
        image = compile_source(
            "int difference;\n"
            "int reverse_difference;\n"
            "int byte_difference;\n"
            "int record_difference;\n"
            "struct Pair { int first; int second; };\n"
            "int main(void) {\n"
            "  int values[2];\n"
            "  int *first = values;\n"
            "  int *second = values + 1;\n"
            "  unsigned char bytes[2];\n"
            "  struct Pair pairs[2];\n"
            "  difference = second - first;\n"
            "  reverse_difference = first - second;\n"
            "  byte_difference = &bytes[1] - &bytes[0];\n"
            "  record_difference = &pairs[1] - &pairs[0];\n"
            "  return first == 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 1)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 0xFFFF)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 1)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 6), 1)

    def test_c90_conditional_operator_checks_operand_types(self):
        invalid_sources = (
            "struct S { int value; }; int main(void) { struct S value; "
            "return value ? 1 : 0; }\n",
            "int main(void) { int *pointer; return pointer ? pointer : 1; }\n",
            "int main(void) { int *pointer; float *other; "
            "return pointer ? pointer : other; }\n",
        )
        for source in invalid_sources:
            with self.subTest(source=source):
                with self.assertRaises(CCompilerError):
                    compile_source(source)

    def test_c90_conditional_operator_accepts_arithmetic_and_null_pointer(self):
        image = compile_source(
            "int main(void) {\n"
            "  int *pointer = 0;\n"
            "  int result = 1 ? (pointer ? 4 : 0) : 6.5;\n"
            "  return result != 4;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_control_flow_conditions_must_be_scalar(self):
        invalid_sources = (
            "struct S { int value; }; int main(void) { struct S value; "
            "if (value) return 1; return 0; }\n",
            "void nothing(void) {} int main(void) { if (nothing()) return 1; "
            "return 0; }\n",
            "struct S { int value; }; int main(void) { struct S value; "
            "while (value) break; return 0; }\n",
        )
        for source in invalid_sources:
            with self.subTest(source=source):
                with self.assertRaisesRegex(CCompilerError, "condition must be scalar"):
                    compile_source(source)

    def test_c90_sizeof_requires_complete_object_types(self):
        invalid_sources = (
            "int main(void) { return sizeof(void); }\n",
            "struct Incomplete; int main(void) { "
            "return sizeof(struct Incomplete); }\n",
            "struct Incomplete; int main(void) { struct Incomplete *pointer; "
            "return sizeof(*pointer); }\n",
        )
        for source in invalid_sources:
            with self.subTest(source=source):
                with self.assertRaisesRegex(
                    CCompilerError, "sizeof requires a complete object type"
                ):
                    compile_source(source)

    def test_c90_sizeof_accepts_pointers_to_incomplete_types(self):
        image = compile_source(
            "struct Incomplete;\n"
            "int main(void) { struct Incomplete *pointer; "
            "return sizeof(pointer) != 3; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_sizeof_preserves_array_extents(self):
        image = compile_source(
            "struct S { char text[4]; };\n"
            "int main(void) {\n"
            "  struct S value;\n"
            "  return sizeof(\"abc\") != 4 || sizeof(value.text) != 4;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_float_increment_and_decrement_use_arithmetic(self):
        image = compile_source(
            "float value;\n"
            "float previous;\n"
            "int main(void) {\n"
            "  value = 1.0;\n"
            "  ++value;\n"
            "  previous = value--;\n"
            "  --value;\n"
            "  value++;\n"
            "  return previous != 2.0 || value != 1.0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_compound_assignments_enforce_operator_constraints(self):
        invalid_sources = (
            "int main(void) { int *pointer; pointer *= 2; return 0; }\n",
            "int main(void) { float value; value &= 1; return 0; }\n",
            "int main(void) { float value; value <<= 1; return 0; }\n",
            "int main(void) { int *pointer; pointer += 1.0; return 0; }\n",
        )
        for source in invalid_sources:
            with self.subTest(source=source):
                with self.assertRaises(CCompilerError):
                    compile_source(source)

    def test_c90_compound_assignment_scales_pointer_and_converts_float(self):
        image = compile_source(
            "int values[2];\n"
            "int *cursor;\n"
            "int result;\n"
            "float floating;\n"
            "int main(void) {\n"
            "  values[1] = 7;\n"
            "  cursor = values;\n"
            "  cursor += 1;\n"
            "  result = *cursor;\n"
            "  floating = 1.0;\n"
            "  floating += 1;\n"
            "  result += floating;\n"
            "  return result != 9;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_increment_rejects_non_scalar_records(self):
        with self.assertRaisesRegex(CCompilerError, "require an arithmetic or pointer"):
            compile_source(
                "struct S { int value; }; int main(void) { "
                "struct S item; item++; return 0; }\n"
            )

    def test_c90_arrays_and_enumerators_are_not_modifiable_lvalues(self):
        invalid_sources = (
            "int main(void) { int values[2]; values = 0; return 0; }\n",
            "int main(void) { int values[2]; values++; return 0; }\n",
            "struct S { char values[2]; }; int main(void) { "
            "struct S item; item.values = 0; return 0; }\n",
            "enum State { READY }; int main(void) { READY = 2; return 0; }\n",
        )
        for source in invalid_sources:
            with self.subTest(source=source):
                with self.assertRaises(CCompilerError):
                    compile_source(source)

    def test_c90_rejects_duplicate_storage_classes_and_qualifiers(self):
        invalid_sources = (
            "static extern int value; int main(void) { return 0; }\n",
            "int main(void) { static auto int value; return 0; }\n",
            "const const int value; int main(void) { return 0; }\n",
            "volatile volatile int value; int main(void) { return 0; }\n",
        )
        for source in invalid_sources:
            with self.subTest(source=source):
                with self.assertRaises(CCompilerError):
                    compile_source(source)

    def test_c90_scalar_brace_initializers(self):
        image = compile_source(
            "int global_value = {{{7}}};\n"
            "int *global_pointer = {{0}};\n"
            "int result;\n"
            "int main(void) {\n"
            "  int local_value = {{5}};\n"
            "  int *local_pointer = {0};\n"
            "  result = global_value + local_value + (global_pointer == 0) "
            "+ (local_pointer == 0);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 5), 14)

    def test_c90_scalar_brace_initializer_requires_one_element(self):
        invalid_sources = (
            "int value = {}; int main(void) { return 0; }\n",
            "int value = {1, 2}; int main(void) { return 0; }\n",
        )
        for source in invalid_sources:
            with self.subTest(source=source):
                with self.assertRaisesRegex(
                    CCompilerError, "scalar initializer braces must contain exactly one value"
                ):
                    compile_source(source)

    def test_global_pointer_initializer_uses_runtime_pointer_layout(self):
        image = compile_source(
            "unsigned char *device = 0x43101;\n"
            "int main(void) { device[0] = 0x41; return 0; }\n"
        )

        cpu, _ = run_image(image)

        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read(0x43101), 0x41)

    def test_c90_global_pointer_initializer_requires_null_or_address_constant(self):
        with self.assertRaisesRegex(
            CCompilerError, "pointer initializer must be an address constant"
        ):
            compile_source(
                "int *pointer = 1;\n"
                "int main(void) { return 0; }\n"
            )

    def test_c90_global_pointer_initializer_accepts_null_constant_expression(self):
        image = compile_source(
            "int *pointer = 1 - 1;\n"
            "int main(void) { return pointer != 0; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

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

    def test_c90_function_arguments_convert_to_parameter_types(self):
        image = compile_source(
            "int result;\n"
            "int narrow(unsigned char value) { return value; }\n"
            "float as_float(float value) { return value; }\n"
            "int main(void) {\n"
            "  result = narrow(258) + (int)as_float(7);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 9)

    def test_c90_function_arguments_reject_incompatible_pointer_and_scalar(self):
        sources = (
            "int take_pointer(int *value) { return value != 0; }\n"
            "int main(void) { return take_pointer(1); }\n",
            "int take_integer(int value) { return value; }\n"
            "int main(void) { int *pointer; return take_integer(pointer); }\n",
        )
        for source in sources:
            with self.subTest(source=source):
                with self.assertRaisesRegex(
                    CCompilerError, "argument 1 .* is incompatible"
                ):
                    compile_source(source)

    def test_c90_function_arguments_accept_null_pointer_constant_expression(self):
        image = compile_source(
            "int is_null(int *value) { return value == 0; }\n"
            "int main(void) { return is_null(1 - 1) ? 0 : 1; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_assignment_converts_arithmetic_values(self):
        image = compile_source(
            "float floating;\n"
            "int integral;\n"
            "int main(void) {\n"
            "  floating = 7;\n"
            "  integral = floating;\n"
            "  return integral != 7;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 7)

    def test_c90_assignment_rejects_incompatible_pointer_scalar_values(self):
        sources = (
            "int *pointer; int main(void) { pointer = 1; return 0; }\n",
            "int *pointer; int main(void) { int value; pointer = value; return 0; }\n",
            "int result; int main(void) { int *pointer; result = pointer; return 0; }\n",
        )
        for source in sources:
            with self.subTest(source=source):
                with self.assertRaisesRegex(
                    CCompilerError, "assignment expression is incompatible"
                ):
                    compile_source(source)

    def test_c90_assignment_accepts_null_pointer_constant_expression(self):
        image = compile_source(
            "int *pointer;\n"
            "int main(void) { pointer = 2 - 2; return pointer != 0; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_return_values_convert_to_declared_function_type(self):
        image = compile_source(
            "int result;\n"
            "int from_float(void) { return 7.75; }\n"
            "float from_integer(void) { return 7; }\n"
            "int main(void) {\n"
            "  result = from_float() + (int)from_integer();\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 14)

    def test_c90_return_rejects_incompatible_scalar_pointer_types(self):
        sources = (
            "int *pointer_result(void) { return 1; }\n",
            "int integer_result(void) { int *pointer; return pointer; }\n",
        )
        for definition in sources:
            with self.subTest(definition=definition):
                with self.assertRaisesRegex(CCompilerError, "return expression is incompatible"):
                    compile_source(
                        definition + "int main(void) { return 0; }\n"
                    )

    def test_c90_pointer_return_accepts_null_pointer_constant_expression(self):
        image = compile_source(
            "int *empty_pointer(void) { return 1 - 1; }\n"
            "int main(void) { return empty_pointer() != 0; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

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

    def test_c90_signed_and_unsigned_without_int_are_int_types(self):
        image = compile_source(
            "signed signed_value;\n"
            "unsigned unsigned_value;\n"
            "int result;\n"
            "int main(void) {\n"
            "  signed_value = -2;\n"
            "  unsigned_value = 3;\n"
            "  result = signed_value + unsigned_value;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 0xFFFE)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 3)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 1)

    def test_c90_unsigned_char_expressions_promote_to_int(self):
        image = compile_source(
            "unsigned char byte_value;\n"
            "int divided;\n"
            "int complemented;\n"
            "int main(void) {\n"
            "  byte_value = 0;\n"
            "  divided = (byte_value - 1) / 2;\n"
            "  complemented = ~byte_value;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 1), 0)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 3), 0xFFFF)

    def test_c90_rejects_repeated_and_long_long_type_specifiers(self):
        declarations = (
            "signed signed value;\n",
            "unsigned unsigned value;\n",
            "short short value;\n",
            "long long value;\n",
        )
        for declaration in declarations:
            with self.subTest(declaration=declaration):
                with self.assertRaisesRegex(CCompilerError, "duplicate type specifier"):
                    compile_source(
                        declaration + "int main(void) { return 0; }\n"
                    )

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

    def test_line_directive_updates_file_macro(self):
        image = compile_source(
            "#define CURRENT_FILE __FILE__\n"
            '#line 60 "virtual.c"\n'
            "char file_name[] = CURRENT_FILE;\n"
            "int main(void) { return 0; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            bytes(cpu.memory[GLOBAL_BASE:GLOBAL_BASE + 10]),
            b"virtual.c\0",
        )

    def test_c90_array_bounds_accept_integer_constant_expressions(self):
        image = compile_source(
            "enum { COUNT = 2 };\n"
            "int values[COUNT * 2 + 1];\n"
            "int result;\n"
            "int main(void) {\n"
            "  char local[0 ? (1 / 0) : (int)sizeof(int) + 1];\n"
            "  values[4] = 9;\n"
            "  result = sizeof(values) + sizeof(local) + values[4];\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 10), 22)

    def test_c90_array_bound_rejects_nonconstant_expression(self):
        with self.assertRaisesRegex(CCompilerError, "integer constant expression"):
            compile_source(
                "int length;\n"
                "int values[length];\n"
                "int main(void) { return 0; }\n"
            )

    def test_c90_enumerator_values_must_fit_int(self):
        with self.assertRaisesRegex(CCompilerError, "not representable as int"):
            compile_source(
                "enum Invalid { TOO_LARGE = 32768 };\n"
                "int main(void) { return 0; }\n"
            )

    def test_c90_switch_accepts_enum_and_constant_expression_cases(self):
        image = compile_source(
            "enum State { FIRST = 2, SECOND };\n"
            "int result;\n"
            "int main(void) {\n"
            "  switch (SECOND) {\n"
            "    case FIRST + 1: result = 7; break;\n"
            "    case 2 * 2: result = 9; break;\n"
            "    default: result = 0;\n"
            "  }\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 7)

    def test_c90_switch_rejects_non_integer_control_expression(self):
        for controlling_expression in ("1.0", "pointer", "record"):
            with self.subTest(controlling_expression=controlling_expression):
                declarations = {
                    "1.0": "",
                    "pointer": "int *pointer;\n",
                    "record": "struct Item { int value; } record;\n",
                }[controlling_expression]
                source = (
                    declarations
                    + "int main(void) {\n"
                    + f"  switch ({controlling_expression}) {{ case 0: break; }}\n"
                    + "  return 0;\n"
                    + "}\n"
                )
                with self.assertRaisesRegex(
                    CCompilerError, "switch expression must have integer type"
                ):
                    compile_source(source)

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

    def test_c90_old_style_function_definition(self):
        image = compile_source(
            "int combine(first, second)\n"
            "int second;\n"
            "int first;\n"
            "{ return first * 10 + second; }\n"
            "int implicit_parameter(value) { return value + 3; }\n"
            "int result;\n"
            "int main(void) {\n"
            "  result = combine(4, 7) + implicit_parameter(5);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 55)

    def test_c90_old_style_parameter_must_be_in_identifier_list(self):
        with self.assertRaisesRegex(CCompilerError, "not in the identifier list"):
            compile_source(
                "int invalid(value)\n"
                "int other;\n"
                "{ return value; }\n"
                "int main(void) { return invalid(0); }\n"
            )

    def test_c90_old_style_identifier_list_requires_definition(self):
        with self.assertRaisesRegex(CCompilerError, "only valid in definitions"):
            compile_source(
                "int invalid(value);\n"
                "int main(void) { return 0; }\n"
            )

    def test_c90_empty_function_declarator_has_no_prototype(self):
        image = compile_source(
            "int combine();\n"
            "int combine(int first, int second);\n"
            "int result;\n"
            "int main(void) {\n"
            "  result = combine(4, 7);\n"
            "  return 0;\n"
            "}\n"
            "int combine(int first, int second) { return first * 10 + second; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 47)

    def test_c90_function_declarations_require_compatible_parameter_types(self):
        sources = (
            "int convert(int value);\n"
            "int convert(float value);\n"
            "int main(void) { return 0; }\n",
            "int convert(int value);\n"
            "int convert(float value) { return (int)value; }\n"
            "int main(void) { return 0; }\n",
        )
        for source in sources:
            with self.subTest(source=source):
                with self.assertRaisesRegex(CCompilerError, "conflicting declaration"):
                    compile_source(source)

    def test_c90_void_function_prototype_rejects_arguments(self):
        with self.assertRaisesRegex(CCompilerError, "expects 0 argument"):
            compile_source(
                "int no_arguments(void);\n"
                "int no_arguments(void) { return 0; }\n"
                "int main(void) { return no_arguments(1); }\n"
            )

    def test_c90_function_definition_defaults_to_int_return_type(self):
        image = compile_source(
            "int result;\n"
            "main() { result = 42; return 0; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 42)

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
            "int results[16];\n"
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

    def test_c90_string_tokenization_and_error_strings(self):
        image = compile_source(
            "#include <string.h>\n"
            "char text[12];\n"
            "int results[7];\n"
            "int main(void) {\n"
            "  char *token;\n"
            "  strcpy(text, \"a,,b,c\");\n"
            "  token = strtok(text, \",\");\n"
            "  results[0] = token == text && token[0] == 'a';\n"
            "  token = strtok(NULL, \",\");\n"
            "  results[1] = token[0] == 'b';\n"
            "  token = strtok(NULL, \",\");\n"
            "  results[2] = token[0] == 'c';\n"
            "  results[3] = strtok(NULL, \",\") == NULL;\n"
            "  results[4] = memchr(text, 'b', 6) == text + 3;\n"
            "  results[5] = strerror(33)[0] == 'd';\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + 12 + index * 2) for index in range(6)],
            [1, 1, 1, 1, 1, 1],
        )

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
            "struct Inner { char first; int value; };\n"
            "struct Outer { char head; struct Inner inner; int tail; };\n"
            "size_t result = offsetof(struct Pair, second);\n"
            "size_t nested = offsetof(struct Outer, inner.value);\n"
            "int main(void) {\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 2)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 4)

    def test_c90_float_header_matches_binary16_target(self):
        image = compile_source(
            "#include <float.h>\n"
            "#if FLT_RADIX != 2 || FLT_MANT_DIG != 11 || FLT_DIG != 3\n"
            "#error incorrect float characteristics\n"
            "#endif\n"
            "#if DBL_MANT_DIG != 11 || DBL_DIG != 3\n"
            "#error incorrect double characteristics\n"
            "#endif\n"
            "#if LDBL_MANT_DIG != 11 || LDBL_DIG != 3\n"
            "#error incorrect long double characteristics\n"
            "#endif\n"
            "#if FLT_MIN_EXP != -13 || FLT_MAX_EXP != 16\n"
            "#error incorrect float exponent range\n"
            "#endif\n"
            "#if LDBL_MIN_EXP != -13 || LDBL_MAX_EXP != 16\n"
            "#error incorrect long double exponent range\n"
            "#endif\n"
            "int results[6];\n"
            "int main(void) {\n"
            "  results[0] = FLT_MAX == 65504.0;\n"
            "  results[1] = FLT_EPSILON == 0.0009765625;\n"
            "  results[2] = FLT_MIN == 0.00006103515625;\n"
            "  results[3] = DBL_MAX == FLT_MAX;\n"
            "  results[4] = DBL_EPSILON == FLT_EPSILON;\n"
            "  results[5] = DBL_MIN == FLT_MIN;\n"
            "  results[6] = LDBL_MAX == FLT_MAX;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(7)],
            [1, 1, 1, 1, 1, 1, 1],
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

    def test_c90_strto_integer_conversions(self):
        image = compile_source(
            "#include <stdlib.h>\n"
            "#include <errno.h>\n"
            "#include <string.h>\n"
            "int results[10];\n"
            "int main(void) {\n"
            "  char *end;\n"
            "  char text[12];\n"
            "  strcpy(text, \"  -0x2fZ\");\n"
            "  results[0] = strtol(text, &end, 0);\n"
            "  results[1] = end - text;\n"
            "  results[2] = errno;\n"
            "  strcpy(text, \"077tail\");\n"
            "  results[3] = strtoul(text, &end, 0);\n"
            "  results[4] = end - text;\n"
            "  strcpy(text, \"32768!\");\n"
            "  results[5] = strtol(text, &end, 10);\n"
            "  results[6] = end - text;\n"
            "  results[7] = errno;\n"
            "  strcpy(text, \"word\");\n"
            "  results[8] = strtol(text, &end, 10);\n"
            "  results[9] = end == text;\n"
            "  strcpy(text, \"-1\");\n"
            "  results[10] = strtoul(text, &end, 10);\n"
            "  results[11] = end - text;\n"
            "  strcpy(text, \"65536!\");\n"
            "  results[12] = strtoul(text, &end, 10);\n"
            "  results[13] = errno;\n"
            "  strcpy(text, \"-65536!\");\n"
            "  results[14] = strtoul(text, &end, 10);\n"
            "  results[15] = errno;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + 2 + index * 2) for index in range(16)],
            [
                -47 & 0xFFFF, 7, 0, 63, 3, 32767, 5, 34, 0, 1,
                65535, 2, 65535, 34, 65535, 34,
            ],
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

    def test_c90_assert_macro_passes_and_aborts(self):
        success_image = compile_source(
            "#include <assert.h>\n"
            "int result;\n"
            "int main(void) {\n"
            "  assert(2 + 2 == 4);\n"
            "  result = 1;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(success_image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 1)

        failure_image = compile_source(
            "#include <assert.h>\n"
            "int main(void) {\n"
            "  assert(0);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(failure_image)
        self.assertFalse(cpu.running)

    def test_c90_assert_is_disabled_by_ndebug(self):
        image = compile_source(
            "#define NDEBUG\n"
            "#include <assert.h>\n"
            "int result;\n"
            "int main(void) {\n"
            "  assert(++result);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 0)

    def test_c90_stdlib_environment_and_system_facilities(self):
        image = compile_source(
            "#include <stdlib.h>\n"
            "int results[3];\n"
            "int main(void) {\n"
            "  results[0] = getenv(\"PATH\") == NULL;\n"
            "  results[1] = system(NULL);\n"
            "  results[2] = system(\"not-run\") == -1;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(3)],
            [1, 0, 1],
        )

    def test_c90_exit_terminates_execution(self):
        image = compile_source(
            "#include <stdlib.h>\n"
            "int result;\n"
            "int main(void) {\n"
            "  exit(EXIT_FAILURE);\n"
            "  result = 1;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 0)

    def test_c90_c_locale_facilities(self):
        image = compile_source(
            "#include <locale.h>\n"
            "#include <limits.h>\n"
            "#include <stddef.h>\n"
            "int results[6];\n"
            "int main(void) {\n"
            "  char *name;\n"
            "  struct lconv *format;\n"
            "  name = setlocale(LC_ALL, NULL);\n"
            "  results[0] = name[0] == 'C' && name[1] == '\\0';\n"
            "  results[1] = setlocale(LC_NUMERIC, \"C\") != NULL;\n"
            "  results[2] = setlocale(LC_ALL, \"en_US\") == NULL;\n"
            "  results[3] = setlocale(99, NULL) == NULL;\n"
            "  format = localeconv();\n"
            "  results[4] = format->decimal_point[0] == '.';\n"
            "  results[5] = format->thousands_sep[0] == '\\0' && "
            "format->frac_digits == CHAR_MAX;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(6)],
            [1, 1, 1, 1, 1, 1],
        )

    def test_c90_time_facilities_report_unavailable_clock(self):
        image = compile_source(
            "#include <time.h>\n"
            "#include <stddef.h>\n"
            "int results[3];\n"
            "int main(void) {\n"
            "  time_t captured;\n"
            "  results[0] = clock() == (clock_t)-1;\n"
            "  results[1] = time(NULL) == (time_t)-1;\n"
            "  captured = 0;\n"
            "  results[2] = time(&captured) == (time_t)-1 && "
            "captured == (time_t)-1;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(3)],
            [1, 1, 1],
        )

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
