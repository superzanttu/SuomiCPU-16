import tempfile
import unittest
from pathlib import Path

from assembler import assemble_file
from src.SuomiCPU import (
    KBD_ADDR, MEM_MASK, MEM_SIZE, SCREEN_WIDTH, VRAM_START, SuomiCompute16,
    create_memory, load_program_file,
)
from c_compiler import GLOBAL_BASE, CCompilerError, compile_source, main as compiler_main


def run_image(image, maximum_steps=100_000, console_input=b""):
    cpu = SuomiCompute16.__new__(SuomiCompute16)
    cpu.memory = create_memory()
    cpu.registers = [0] * 8
    cpu.flags = {"Z": 0, "C": 0}
    cpu.running = True
    cpu.sp = MEM_MASK
    cpu.load_program(image)
    cpu.reset()
    if console_input:
        cpu._console_device().receive(console_input)
    steps = 0
    while cpu.running and steps < maximum_steps:
        cpu.step()
        steps += 1
    return cpu, steps


class CCompilerTests(unittest.TestCase):
    def test_text_demo_uses_standard_console_input_and_output(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "modules"
            / "Text Demo"
            / "text_demo.c"
        )
        image = compile_source(path.read_text(encoding="utf-8"), str(path))
        cpu, _ = run_image(image, console_input=b"Kaika Korhonen\n")
        self.assertEqual(
            cpu.console_device.output.decode("ascii"),
            "Kuka olet? Kaika Korhonen\n"
            "Huomenta! Kohta kouluun, Kaika Korhonen!\n",
        )

    def test_c90_stdio_limits_meet_required_minimums(self):
        image = compile_source(
            "#include <stdio.h>\n"
            "int main(void) {\n"
            "  return BUFSIZ < 256 || FOPEN_MAX < 8 || TMP_MAX < 25;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertEqual(cpu.registers[0], 0)

    def test_c90_setjmp_and_longjmp_restore_the_calling_context(self):
        image = compile_source(
            "#include <setjmp.h>\n"
            "jmp_buf environment;\n"
            "int main(void) {\n"
            "    int result;\n"
            "    result = setjmp(environment);\n"
            "    if (result == 0) longjmp(environment, 7);\n"
            "    return result != 7;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertEqual(cpu.registers[0], 0)

    def test_c90_longjmp_converts_zero_to_one(self):
        image = compile_source(
            "#include <setjmp.h>\n"
            "jmp_buf environment;\n"
            "int main(void) {\n"
            "    int result;\n"
            "    result = setjmp(environment);\n"
            "    if (result == 0) longjmp(environment, 0);\n"
            "    return result != 1;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertEqual(cpu.registers[0], 0)

    def test_c90_longjmp_unwinds_a_nested_call_with_a_local_buffer(self):
        image = compile_source(
            "#include <setjmp.h>\n"
            "void transfer(unsigned char *environment) {\n"
            "    longjmp(environment, 9);\n"
            "}\n"
            "int main(void) {\n"
            "    jmp_buf environment;\n"
            "    int result;\n"
            "    result = setjmp(environment);\n"
            "    if (result == 0) transfer(environment);\n"
            "    return result != 9;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertEqual(cpu.registers[0], 0)

    def test_c90_minimum_sized_global_object_is_addressable(self):
        image = compile_source(
            "unsigned char object[65535];\n"
            "int main(void) {\n"
            "    object[65534] = 42;\n"
            "    return object[65534] != 42;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertEqual(cpu.registers[0], 0)
        self.assertEqual(cpu.memory[GLOBAL_BASE + 65534], 42)

    def test_c90_stdio_printf_writes_to_the_console_device(self):
        image = compile_source(
            '#include <stdio.h>\n'
            'int main(void) {\n'
            '    int count;\n'
            '    count = printf("N=%d U=%u L=%ld H=%04x S=%s %%", '
            '-12, 7U, 70000L, 42U, "ok");\n'
            '    return count != 31;\n'
            '}\n'
        )
        cpu, _ = run_image(image)
        self.assertEqual(
            cpu.console_device.output.decode("ascii"),
            "N=-12 U=7 L=70000 H=002a S=ok %",
        )
        self.assertFalse(getattr(cpu, "frame_yield", False))

    def test_c90_stdio_scanf_reads_console_input(self):
        image = compile_source(
            "#include <stdio.h>\n"
            "#include <string.h>\n"
            "int result;\n"
            "int main(void) {\n"
            "  int value;\n"
            "  char word[8];\n"
            "  result = scanf(\"%d %7s\", &value, word);\n"
            "  return result != 2 || value != 42 || strcmp(word, \"hi\") != 0;\n"
            "}\n"
        )
        cpu = SuomiCompute16.__new__(SuomiCompute16)
        cpu.memory = create_memory()
        cpu.registers = [0] * 8
        cpu.flags = {"Z": 0, "C": 0}
        cpu.running = True
        cpu.sp = 0x2FFFF
        cpu.load_program(image)
        cpu.reset()
        cpu.console_device.receive(b"42 hi\n")
        steps = 0
        while cpu.running and steps < 100_000:
            cpu.step()
            steps += 1
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 2)

    def test_c90_stdio_gets_reads_until_newline_and_discards_it(self):
        image = compile_source(
            "#include <stdio.h>\n"
            "#include <string.h>\n"
            "char line[16];\n"
            "int main(void) {\n"
            "  if (gets(line) == NULL) return 1;\n"
            "  return strcmp(line, \"hello\") != 0;\n"
            "}\n"
        )
        cpu = SuomiCompute16.__new__(SuomiCompute16)
        cpu.memory = create_memory()
        cpu.registers = [0] * 8
        cpu.flags = {"Z": 0, "C": 0}
        cpu.running = True
        cpu.sp = 0x2FFFF
        cpu.load_program(image)
        cpu.reset()
        cpu.console_device.receive(b"hello\n")

        steps = 0
        while cpu.running and steps < 100_000:
            cpu.step()
            steps += 1

        self.assertFalse(cpu.running)
        self.assertEqual(cpu.registers[0], 0)
        self.assertEqual(bytes(cpu.memory[GLOBAL_BASE:GLOBAL_BASE + 6]), b"hello\0")

    def test_c90_getchar_poll_yields_to_the_emulator_event_loop(self):
        image = compile_source(
            "#include <stdio.h>\n"
            "int main(void) { return getchar(); }\n"
        )
        cpu = SuomiCompute16.__new__(SuomiCompute16)
        cpu.memory = create_memory()
        cpu.registers = [0] * 8
        cpu.flags = {"Z": 0, "C": 0}
        cpu.running = True
        cpu.sp = 0x2FFFF
        cpu.load_program(image)
        cpu.reset()
        cpu.use_jit = False

        cpu.execute_frame()

        self.assertTrue(cpu.frame_yield)
        self.assertTrue(cpu.running)

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

    def test_c90_pointer_arithmetic_accepts_long_offsets(self):
        image = compile_source(
            "int result;\n"
            "int main(void) {\n"
            "  int values[5];\n"
            "  int *pointer = &values[0];\n"
            "  result = (pointer + 3L == &values[3])\n"
            "    && (2UL + pointer == &values[2])\n"
            "    && (pointer + 4L - 1UL == &values[3]);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 1)

    def test_c90_ptrdiff_t_uses_signed_32_bit_pointer_differences(self):
        image = compile_source(
            "#include <stddef.h>\n"
            "ptrdiff_t forward;\n"
            "ptrdiff_t reverse;\n"
            "int main(void) {\n"
            "  int values[5];\n"
            "  forward = &values[4] - &values[1];\n"
            "  reverse = &values[1] - &values[4];\n"
            "  return sizeof(ptrdiff_t) != 4\n"
            "    || forward != 3L || reverse != -3L;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 0)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 3)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 0xFFFF)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 6), 0xFFFD)

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
            "return sizeof(pointer) != 4; }\n"
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

    def test_c90_const_objects_are_initialized_and_readable(self):
        image = compile_source(
            "const int global_value = 12;\n"
            "typedef const int Immutable;\n"
            "int result;\n"
            "struct Pair { const int first; int second; };\n"
            "int main(void) {\n"
            "  const int local_value = 5;\n"
            "  Immutable aliased_value = 2;\n"
            "  const int values[2] = {3, 4};\n"
            "  const struct Pair pair = {6, 7};\n"
            "  result = global_value + local_value + aliased_value + "
            "values[1] + pair.first;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 29)

    def test_c90_const_qualifier_survives_inline_record_definition(self):
        with self.assertRaisesRegex(
            CCompilerError, "const-qualified objects are not modifiable"
        ):
            compile_source(
                "const struct Pair { int first; } pair = {1};\n"
                "int main(void) { pair.first = 2; return 0; }\n"
            )

    def test_c90_const_objects_cannot_be_modified(self):
        invalid_sources = (
            "int main(void) { const int value = 1; value = 2; return 0; }\n",
            "int main(void) { const int value = 1; value++; return 0; }\n",
            "typedef const int Immutable; int main(void) { "
            "Immutable value = 1; value = 2; return 0; }\n",
            "const int value = 1; int main(void) { value = 2; return 0; }\n",
            "int main(void) { const int values[2] = {1, 2}; "
            "values[0] = 3; return 0; }\n",
            "struct Pair { const int first; int second; }; "
            "int main(void) { struct Pair pair = {1, 2}; "
            "pair.first = 3; return 0; }\n",
            "struct Pair { int first; int second; }; "
            "int main(void) { const struct Pair pair = {1, 2}; "
            "pair.second = 3; return 0; }\n",
        )
        for source in invalid_sources:
            with self.subTest(source=source):
                with self.assertRaises(CCompilerError):
                    compile_source(source)

    def test_c90_const_pointer_targets_are_read_only(self):
        invalid_sources = (
            "int main(void) { const int value = 1; "
            "const int *pointer = &value; *pointer = 2; return 0; }\n",
            "int main(void) { const int values[2] = {1, 2}; "
            "const int *pointer = values; pointer[0] = 3; return 0; }\n",
            "int update(const int *pointer) { *pointer = 4; return 0; }\n"
            "int main(void) { int value = 1; return update(&value); }\n",
            "typedef const int *ConstIntPointer; "
            "int main(void) { int value = 1; ConstIntPointer pointer = &value; "
            "*pointer = 2; return 0; }\n",
            "struct Holder { const int *pointer; }; "
            "int main(void) { int value = 1; struct Holder holder; "
            "holder.pointer = &value; *holder.pointer = 2; return 0; }\n",
            "int main(void) { const int value = 1; const int *source = &value; "
            "int *target; target = source; return 0; }\n",
            "int main(void) { const int value = 1; int *target; "
            "target = (const int *)&value; return 0; }\n",
        )
        for source in invalid_sources:
            with self.subTest(source=source):
                with self.assertRaises(CCompilerError):
                    compile_source(source)

    def test_c90_const_pointer_parameters_and_members_remain_readable(self):
        image = compile_source(
            "struct Point { int x; int y; };\n"
            "struct Point point;\n"
            "int read_point(const struct Point *value) {\n"
            "  return value->x + value->y;\n"
            "}\n"
            "int main(void) {\n"
            "  point.x = 3;\n"
            "  point.y = 4;\n"
            "  return read_point(&point) != 7;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_const_pointer_target_does_not_const_qualify_pointer_object(self):
        image = compile_source(
            "int first;\n"
            "int second;\n"
            "int main(void) {\n"
            "  const int *pointer;\n"
            "  first = 5;\n"
            "  second = 8;\n"
            "  pointer = &first;\n"
            "  pointer = &second;\n"
            "  return *pointer != 8;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_nested_pointer_const_qualification_is_rejected_explicitly(self):
        for source in (
            "int main(void) { const int **pointer; return 0; }\n",
            "int function(const int **pointer) { return 0; }\n"
            "int main(void) { return function(0); }\n",
            "struct Holder { const int **pointer; }; "
            "int main(void) { return 0; }\n",
        ):
            with self.subTest(source=source):
                with self.assertRaisesRegex(
                    CCompilerError,
                    "const qualification through multiple pointer levels",
                ):
                    compile_source(source)

    def test_c90_pointer_assignment_can_add_target_const_qualification(self):
        image = compile_source(
            "int value;\n"
            "int main(void) {\n"
            "  int *source;\n"
            "  const int *target;\n"
            "  value = 6;\n"
            "  source = &value;\n"
            "  target = source;\n"
            "  return *target != 6;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_global_redeclarations_preserve_const_qualification(self):
        for source in (
            "const int value; int value; int main(void) { return 0; }\n",
            "int value; const int value; int main(void) { return 0; }\n",
        ):
            with self.subTest(source=source):
                with self.assertRaisesRegex(
                    CCompilerError, "conflicting declaration"
                ):
                    compile_source(source)

    def test_c90_register_objects_cannot_have_their_address_taken(self):
        invalid_sources = (
            "int main(void) { register int value; &value; return 0; }\n",
            "struct Pair { int first; int second; }; "
            "int main(void) { register struct Pair pair; "
            "&pair.first; return 0; }\n",
        )
        for source in invalid_sources:
            with self.subTest(source=source):
                with self.assertRaisesRegex(
                    CCompilerError, "address-of operator cannot be applied"
                ):
                    compile_source(source)

    def test_c90_register_parameters_cannot_have_their_address_taken(self):
        with self.assertRaisesRegex(
            CCompilerError, "address-of operator cannot be applied"
        ):
            compile_source(
                "int read_value(register int value) { &value; return value; }\n"
                "int main(void) { return read_value(4) != 4; }\n"
            )

    def test_c90_register_parameters_remain_readable_and_assignable(self):
        image = compile_source(
            "int update(register int value) {\n"
            "  value++;\n"
            "  return value;\n"
            "}\n"
            "int main(void) { return update(4) != 5; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_parameter_storage_classes_only_allow_register(self):
        for storage_class in ("auto", "extern", "static"):
            for declaration in ("int f({0} int value);", "int f({0} int value) {{ return value; }}"):
                source = (
                    declaration.format(storage_class)
                    + "\nint main(void) { return 0; }\n"
                )
                with self.subTest(storage_class=storage_class, declaration=declaration):
                    with self.assertRaisesRegex(
                        CCompilerError,
                        f"{storage_class} is not valid in a parameter declaration",
                    ):
                        compile_source(source)

    def test_c90_register_locals_remain_readable_and_assignable(self):
        image = compile_source(
            "int result;\n"
            "int main(void) {\n"
            "  register int value = 4;\n"
            "  value++;\n"
            "  result = value;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 5)

    def test_c90_const_member_blocks_whole_record_assignment(self):
        with self.assertRaisesRegex(
            CCompilerError,
            "record objects with const-qualified members are not modifiable",
        ):
            compile_source(
                "struct Pair { const int first; int second; };\n"
                "int main(void) {\n"
                "  struct Pair source = {1, 2};\n"
                "  struct Pair target = {3, 4};\n"
                "  target = source;\n"
                "  return 0;\n"
                "}\n"
            )

    def test_c90_const_member_does_not_block_mutable_sibling_assignment(self):
        image = compile_source(
            "struct Pair { const int first; int second; };\n"
            "int result;\n"
            "int main(void) {\n"
            "  struct Pair value = {1, 2};\n"
            "  value.second = 8;\n"
            "  result = value.first + value.second;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 9)

    def test_c90_auto_and_register_are_rejected_at_file_scope(self):
        for storage_class in ("auto", "register"):
            with self.subTest(storage_class=storage_class):
                with self.assertRaisesRegex(CCompilerError, "not valid at file scope"):
                    compile_source(
                        f"{storage_class} int value; "
                        "int main(void) { return 0; }\n"
                    )

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
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 6), 14)

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
        self.assertEqual(cpu.sp, MEM_MASK)

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

    def test_c90_function_arguments_preserve_pointer_target_const(self):
        image = compile_source(
            "int read_value(const int *value);\n"
            "int read_value(const int *value) { return *value; }\n"
            "int main(void) {\n"
            "  const int immutable = 8;\n"
            "  int mutable = 3;\n"
            "  return read_value(&immutable) + read_value(&mutable) == 11 ? 0 : 1;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_function_arguments_reject_discarding_pointer_target_const(self):
        with self.assertRaisesRegex(
            CCompilerError, "argument 1 .* is incompatible with its parameter type"
        ):
            compile_source(
                "int update(int *value) { *value = 2; return 0; }\n"
                "int main(void) {\n"
                "  const int immutable = 1;\n"
                "  return update(&immutable);\n"
                "}\n"
            )

    def test_c90_prototypes_conflict_on_pointer_target_const(self):
        with self.assertRaisesRegex(CCompilerError, "conflicting declaration"):
            compile_source(
                "int read_value(const int *value);\n"
                "int read_value(int *value);\n"
                "int main(void) { return 0; }\n"
            )

    def test_c90_const_pointer_return_preserves_target_qualification(self):
        image = compile_source(
            "const int *read_value(const int *value);\n"
            "const int *read_value(const int *value) { return value; }\n"
            "int main(void) {\n"
            "  const int immutable = 8;\n"
            "  return *read_value(&immutable) == 8 ? 0 : 1;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_const_pointer_return_rejects_qualifier_loss(self):
        with self.assertRaisesRegex(
            CCompilerError, "return expression is incompatible with function type"
        ):
            compile_source(
                "const int value = 1;\n"
                "int *get_value(void) { return &value; }\n"
                "int main(void) { return 0; }\n"
            )

    def test_c90_const_pointer_return_is_read_only_at_call_site(self):
        with self.assertRaisesRegex(
            CCompilerError, "const-qualified objects are not modifiable lvalues"
        ):
            compile_source(
                "const int value = 1;\n"
                "const int *get_value(void) { return &value; }\n"
                "int main(void) { *get_value() = 2; return 0; }\n"
            )

    def test_c90_function_prototypes_conflict_on_return_target_const(self):
        with self.assertRaisesRegex(CCompilerError, "conflicting declaration"):
            compile_source(
                "const int *get_value(void);\n"
                "int *get_value(void);\n"
                "int main(void) { return 0; }\n"
            )

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
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 7)

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
        self.assertEqual(cpu.sp, MEM_MASK)

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
        self.assertEqual(cpu.sp, MEM_MASK)

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
        self.assertEqual(cpu.sp, MEM_MASK)

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

    def test_c90_array_typedef_declarations(self):
        image = compile_source(
            "#include <stddef.h>\n"
            "typedef unsigned int Vector[3];\n"
            "typedef Vector VectorAlias;\n"
            "VectorAlias global_vector;\n"
            "int result;\n"
            "int third(VectorAlias values) { return values[2]; }\n"
            "int main(void) {\n"
            "  typedef char Text[4];\n"
            "  Text local_text;\n"
            "  global_vector[2] = 19;\n"
            "  local_text[1] = 'x';\n"
            "  result = (sizeof(VectorAlias) == 6)\n"
            "    && (sizeof(local_text) == 4)\n"
            "    && (global_vector[2] == 19)\n"
            "    && (third(global_vector) == 19)\n"
            "    && (local_text[1] == 'x');\n"
            "  return result != 1;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 6), 1)

    def test_c90_multidimensional_arrays_index_initialize_and_pass_rows(self):
        image = compile_source(
            "typedef int Row[3];\n"
            "typedef Row Grid[2];\n"
            "Grid matrix = { {1, 2, 3}, {4, 5, 6} };\n"
            "int result;\n"
            "int inferred[][3] = { {14, 15, 16} };\n"
            "int row_value(int row[3]) { return row[1]; }\n"
            "int matrix_value(int values[][3]) { return values[1][2]; }\n"
            "int main(void) {\n"
            "  int local[2][3] = { {7, 8, 9}, {10, 11, 12} };\n"
            "  local[1][2] = 13;\n"
            "  result = sizeof(matrix) == 12\n"
            "    && sizeof(matrix[0]) == 6\n"
            "    && sizeof(inferred) == 6\n"
            "    && matrix[1][0] == 4\n"
            "    && row_value(matrix[1]) == 5\n"
            "    && matrix_value(matrix) == 6\n"
            "    && local[0][2] == 9\n"
            "    && local[1][2] == 13;\n"
            "  return result != 1;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 12), 1)

    def test_c90_indexed_array_rows_decay_to_pointers_in_assignments(self):
        image = compile_source(
            "unsigned char matrix[2][8];\n"
            "unsigned char *row;\n"
            "int main(void) {\n"
            "  row = matrix[1];\n"
            "  row[3] = 42;\n"
            "  return matrix[1][3] != 42;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.registers[0], 0)

    def test_c90_three_dimensional_array_extents_keep_declarator_order(self):
        image = compile_source(
            "int cube[2][3][4];\n"
            "int size_cube;\n"
            "int size_plane;\n"
            "int size_row;\n"
            "int indexed_value;\n"
            "int read_element(int values[][3][4]) {\n"
            "  return values[1][2][3];\n"
            "}\n"
            "int main(void) {\n"
            "  cube[1][2][3] = 27;\n"
            "  size_cube = sizeof(cube);\n"
            "  size_plane = sizeof(cube[0]);\n"
            "  size_row = sizeof(cube[0][0]);\n"
            "  indexed_value = read_element(cube);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 48), 48)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 50), 24)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 52), 8)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 54), 27)

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

    def test_c90_struct_arguments_are_copied_by_value(self):
        image = compile_source(
            "struct Pair { int first; unsigned char second; };\n"
            "struct Pair original = { 40, 2 };\n"
            "int result;\n"
            "int update_copy(struct Pair pair) {\n"
            "  pair.first = 99;\n"
            "  return pair.first + pair.second;\n"
            "}\n"
            "int main(void) {\n"
            "  result = update_copy(original);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 40)
        self.assertEqual(cpu.memory[GLOBAL_BASE + 2], 2)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 101)

    def test_c90_record_assignment_copies_complete_value_and_expression(self):
        image = compile_source(
            "struct Pair { int first; unsigned char second; };\n"
            "struct Pair original = { 40, 2 };\n"
            "struct Pair copy;\n"
            "struct Pair chained;\n"
            "int result;\n"
            "int main(void) {\n"
            "  copy = original;\n"
            "  chained = (copy = original);\n"
            "  result = copy.first + copy.second + chained.first + chained.second;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 40)
        self.assertEqual(cpu.memory[GLOBAL_BASE + 2], 2)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 40)
        self.assertEqual(cpu.memory[GLOBAL_BASE + 6], 2)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 8), 40)
        self.assertEqual(cpu.memory[GLOBAL_BASE + 10], 2)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 12), 84)

    def test_c90_union_assignment_copies_complete_representation(self):
        image = compile_source(
            "union Number { unsigned long wide; unsigned char bytes[4]; };\n"
            "union Number original;\n"
            "union Number copy;\n"
            "unsigned long result;\n"
            "int main(void) {\n"
            "  original.wide = 0x12345678UL;\n"
            "  copy = original;\n"
            "  result = copy.wide;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 0x1234)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 0x5678)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 0x1234)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 6), 0x5678)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 8), 0x1234)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 10), 0x5678)

    def test_c90_struct_returns_use_caller_owned_storage(self):
        image = compile_source(
            "struct Pair { int first; int second; };\n"
            "struct Pair make_pair(int value) {\n"
            "  struct Pair result;\n"
            "  result.first = value;\n"
            "  result.second = value + 1;\n"
            "  return result;\n"
            "}\n"
            "struct Pair pass_pair(struct Pair value) { return value; }\n"
            "int main(void) {\n"
            "  struct Pair result;\n"
            "  result = pass_pair(make_pair(30));\n"
            "  return result.first != 30 || result.second != 31 || "
            "make_pair(40).first != 40;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_recursive_struct_returns_keep_temporaries_reentrant(self):
        image = compile_source(
            "struct Value { int value; };\n"
            "struct Value descend(int count) {\n"
            "  struct Value result;\n"
            "  if (count) return descend(count - 1);\n"
            "  result.value = 7;\n"
            "  return result;\n"
            "}\n"
            "int main(void) {\n"
            "  struct Value result;\n"
            "  result = descend(4);\n"
            "  return result.value != 7;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_record_return_member_is_not_a_modifiable_lvalue(self):
        with self.assertRaisesRegex(CCompilerError, "not a modifiable lvalue"):
            compile_source(
                "struct Value { int value; };\n"
                "struct Value make_value(void) { struct Value result; "
                "return result; }\n"
                "int main(void) { make_value().value = 3; return 0; }\n"
            )

    def test_c90_record_return_temporaries_count_toward_frame_limit(self):
        with self.assertRaisesRegex(CCompilerError, "exceed"):
            compile_source(
                "struct Block { char bytes[128]; };\n"
                "struct Block make_block(void) {\n"
                "  struct Block value;\n"
                "  return value;\n"
                "}\n"
                "struct Block choose(struct Block left, struct Block right) {\n"
                "  return left;\n"
                "}\n"
                "int main(void) {\n"
                "  choose(make_block(), make_block());\n"
                "  return 0;\n"
                "}\n"
            )

    def test_c90_union_returns_preserve_complete_representation(self):
        image = compile_source(
            "union Value { int words[2]; long number; };\n"
            "union Value make_value(void) {\n"
            "  union Value result;\n"
            "  result.words[0] = 0x1234;\n"
            "  result.words[1] = 0x5678;\n"
            "  return result;\n"
            "}\n"
            "int main(void) {\n"
            "  union Value result;\n"
            "  result = make_value();\n"
            "  return result.words[0] != 0x1234 || result.words[1] != 0x5678;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c90_postfix_increment_accepts_indirect_and_indexed_lvalues(self):
        image = compile_source(
            "int values[2];\n"
            "int result;\n"
            "int after;\n"
            "int main(void) {\n"
            "  int *pointer;\n"
            "  values[0] = 10;\n"
            "  values[1] = 20;\n"
            "  pointer = &values[0];\n"
            "  result = (*pointer)++;\n"
            "  after = values[1]--;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 11)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 19)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 10)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 6), 20)

    def test_c90_increment_accepts_record_member_lvalues(self):
        image = compile_source(
            "struct Pair { int first; int second; };\n"
            "struct Pair pair;\n"
            "int previous;\n"
            "int current;\n"
            "int main(void) {\n"
            "  pair.first = 4;\n"
            "  pair.second = 8;\n"
            "  previous = pair.first++;\n"
            "  current = ++pair.second;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 5)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 9)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 4)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 6), 9)

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
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 25)
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

    def test_c90_variadic_function_prototype_and_definition(self):
        image = compile_source(
            "int first_argument(int fixed, ...);\n"
            "int first_argument(int fixed, ...) { return fixed; }\n"
            "int result;\n"
            "char character;\n"
            "int main(void) {\n"
            "  character = 'x';\n"
            "  result = first_argument(23, character, 70000L, \"tail\");\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 23)

    def test_c90_variadic_calls_require_fixed_parameters(self):
        with self.assertRaisesRegex(CCompilerError, "expects 1 argument"):
            compile_source(
                "int first_argument(int fixed, ...);\n"
                "int first_argument(int fixed, ...) { return fixed; }\n"
                "int main(void) { return first_argument(); }\n"
            )

    def test_c90_variadic_and_nonvariadic_declarations_conflict(self):
        with self.assertRaisesRegex(CCompilerError, "conflicting declaration"):
            compile_source(
                "int first_argument(int fixed, ...);\n"
                "int first_argument(int fixed);\n"
                "int first_argument(int fixed) { return fixed; }\n"
                "int main(void) { return 0; }\n"
            )

    def test_c90_stdarg_reads_promoted_and_wide_arguments(self):
        image = compile_source(
            "#include <stdarg.h>\n"
            "long result;\n"
            "long sum_values(long count, ...) {\n"
            "  va_list arguments;\n"
            "  int promoted;\n"
            "  long first;\n"
            "  long second;\n"
            "  va_start(arguments, count);\n"
            "  promoted = va_arg(arguments, int);\n"
            "  first = va_arg(arguments, long);\n"
            "  second = va_arg(arguments, long);\n"
            "  va_end(arguments);\n"
            "  return (long)promoted + first + second;\n"
            "}\n"
            "int main(void) {\n"
            "  char character;\n"
            "  character = 7;\n"
            "  result = sum_values(3L, character, 70000L, 90000L);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 2)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 28935)

    def test_c90_stdarg_starts_after_narrow_fixed_parameter(self):
        image = compile_source(
            "#include <stdarg.h>\n"
            "int result;\n"
            "int first_variadic(int fixed, ...) {\n"
            "  va_list arguments;\n"
            "  va_start(arguments, fixed);\n"
            "  return va_arg(arguments, int);\n"
            "}\n"
            "int main(void) {\n"
            "  char promoted;\n"
            "  promoted = 9;\n"
            "  result = first_variadic(1, promoted, 12);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 9)

    def test_c90_stdarg_reads_pointer_and_double_arguments(self):
        image = compile_source(
            "#include <stdarg.h>\n"
            "int results[2];\n"
            "int read_arguments(int count, ...) {\n"
            "  va_list arguments;\n"
            "  char *text;\n"
            "  double value;\n"
            "  va_start(arguments, count);\n"
            "  text = va_arg(arguments, char *);\n"
            "  value = va_arg(arguments, double);\n"
            "  va_end(arguments);\n"
            "  results[0] = text[0];\n"
            "  results[1] = value == 3.5;\n"
            "  return 0;\n"
            "}\n"
            "int main(void) {\n"
            "  read_arguments(2, \"ok\", 3.5f);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), ord("o"))
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 1)

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
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 0)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 3)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), -1 & 0xFFFF)
        self.assertEqual(cpu.memory[GLOBAL_BASE + 6:GLOBAL_BASE + 14], b"aabcab!!")

    def test_c90_size_t_uses_32_bit_unsigned_storage(self):
        image = compile_source(
            "#include <stddef.h>\n"
            "size_t value;\n"
            "int main(void) {\n"
            "  value = 70000UL;\n"
            "  return sizeof(size_t) != 4 || value != 70000UL;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 1)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 70000 & 0xFFFF)

    def test_c90_target_abi_pointer_encoding_stores_low_word_before_high_word(self):
        image = compile_source(
            "int *pointer = 0x12345UL;\n"
            "int main(void) { return 0; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(bytes(cpu.memory[GLOBAL_BASE:GLOBAL_BASE + 4]), b"\x23\x45\x00\x01")

    def test_c90_pointer_accesses_read_and_write_across_30_bit_address_space(self):
        image = compile_source(
            "int *word = 0x10FFFFUL;\n"
            "unsigned char *bytes = 0x212345UL;\n"
            "long *wide = 0x312347UL;\n"
            "int observed;\n"
            "unsigned char observed_byte;\n"
            "long observed_long;\n"
            "int main(void) {\n"
            "  word[0] = 0x1234;\n"
            "  word[1] = 0x5678;\n"
            "  observed = word[0] + word[1];\n"
            "  bytes[0] = 0xAB;\n"
            "  observed_byte = bytes[0];\n"
            "  *wide = 0x12345678L;\n"
            "  observed_long = *wide;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 12), 0x68AC)
        self.assertEqual(cpu.memory[GLOBAL_BASE + 14], 0xAB)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 15), 0x1234)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 17), 0x5678)
        self.assertEqual(cpu.read_16(0x10FFFF), 0x1234)
        self.assertEqual(cpu.read_16(0x110001), 0x5678)
        self.assertEqual(cpu.memory[0x212345], 0xAB)
        self.assertEqual(bytes(cpu.memory[0x312347:0x31234B]), b"\x12\x34\x56\x78")

    def test_c90_pointer_integer_casts_preserve_defined_address_bits(self):
        image = compile_source(
            "int observed;\n"
            "unsigned long pointer_integer;\n"
            "unsigned int pointer_low;\n"
            "unsigned char *identity(unsigned char *pointer) { return pointer; }\n"
            "int main(void) {\n"
            "  unsigned char *bytes = identity((unsigned char *)0x812345UL);\n"
            "  long *wide = (long *)0x812347UL;\n"
            "  bytes[0] = 0xAB;\n"
            "  *wide = 0x12345678L;\n"
            "  pointer_integer = (unsigned long)bytes;\n"
            "  pointer_low = (unsigned int)bytes;\n"
            "  observed = bytes[0] == 0xAB && *wide == 0x12345678L;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 1)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 0x0081)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 0x2345)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 6), 0x2345)
        self.assertEqual(cpu.memory[0x812345], 0xAB)
        self.assertEqual(bytes(cpu.memory[0x812347:0x81234B]), b"\x12\x34\x56\x78")

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
            "int results[10];\n"
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
            "#if LONG_MIN != -2147483648 || LONG_MAX != 2147483647 || ULONG_MAX != 4294967295\n"
            "#error incorrect target long limits\n"
            "#endif\n"
            "#if WCHAR_MIN != -32768 || WCHAR_MAX != 32767\n"
            "#error incorrect wide character limits\n"
            "#endif\n"
            "int main(void) { return 0; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_c99_stdint_header_matches_available_target_integer_types(self):
        image = compile_source(
            "#include <stdint.h>\n"
            "#include <stdint.h>\n"
            "#if INT8_MIN != -128 || INT8_MAX != 127 || UINT8_MAX != 255\n"
            "#error incorrect 8-bit limits\n"
            "#endif\n"
            "#if INT16_MIN != -32768 || INT16_MAX != 32767 || UINT16_MAX != 65535\n"
            "#error incorrect 16-bit limits\n"
            "#endif\n"
            "#if INT32_MIN != -2147483648 || INT32_MAX != 2147483647 || "
            "UINT32_MAX != 4294967295\n"
            "#error incorrect 32-bit limits\n"
            "#endif\n"
            "#if INTPTR_MAX != LONG_MAX || UINTPTR_MAX != ULONG_MAX || "
            "SIZE_MAX != ULONG_MAX\n"
            "#error incorrect pointer or size limits\n"
            "#endif\n"
            "#if PTRDIFF_MIN != LONG_MIN || PTRDIFF_MAX != LONG_MAX || "
            "SIG_ATOMIC_MIN != INT_MIN || SIG_ATOMIC_MAX != INT_MAX\n"
            "#error incorrect auxiliary integer limits\n"
            "#endif\n"
            "#if WCHAR_MIN != -32768 || WCHAR_MAX != 32767 || "
            "WINT_MIN != 0 || WINT_MAX != UINT_MAX\n"
            "#error incorrect wide character limits\n"
            "#endif\n"
            "int main(void) {\n"
            "  int8_t i8;\n"
            "  uint8_t u8;\n"
            "  int16_t i16;\n"
            "  uint16_t u16;\n"
            "  int32_t i32;\n"
            "  uint32_t u32;\n"
            "  intmax_t imax;\n"
            "  uintptr_t address;\n"
            "  i8 = -INT8_C(128);\n"
            "  u8 = UINT8_C(255);\n"
            "  i16 = INT16_MIN;\n"
            "  u16 = UINT16_MAX;\n"
            "  i32 = INT32_MIN;\n"
            "  u32 = UINT32_C(4294967295);\n"
            "  imax = INTMAX_C(2147483647);\n"
            "  address = (uintptr_t)0x7FFFF;\n"
            "  return sizeof(i8) != 1 || sizeof(u8) != 1 ||\n"
            "    sizeof(i16) != 2 || sizeof(u16) != 2 ||\n"
            "    sizeof(i32) != 4 || sizeof(u32) != 4 ||\n"
            "    sizeof(intptr_t) != 4 || sizeof(uintptr_t) != 4 ||\n"
            "    i8 != INT8_MIN || u8 != UINT8_MAX ||\n"
            "    i16 != INT16_MIN || u16 != UINT16_MAX ||\n"
            "    i32 != INT32_MIN || u32 != UINT32_MAX ||\n"
            "    imax != INTMAX_MAX || address != 0x7FFFF;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)

    def test_long_values_use_32_bit_storage_and_calling_convention(self):
        image = compile_source(
            "long wide_global = 0x12345678L;\n"
            "unsigned long unsigned_global = 4294967295UL;\n"
            "long observed;\n"
            "long negative_quotient;\n"
            "int failed;\n"
            "long add_long(long left, long right) { return left + right; }\n"
            "int main(void) {\n"
            "  long local;\n"
            "  local = wide_global;\n"
            "  observed = add_long(local, 1L);\n"
            "  negative_quotient = -3L / 2L;\n"
            "  if (observed != 0x12345679L) failed = 1;\n"
            "  if (unsigned_global != 4294967295UL) failed = 1;\n"
            "  if (0x80000000UL <= 0x7FFFFFFFUL) failed = 1;\n"
            "  if (negative_quotient != -1L) failed = 1;\n"
            "  if (sizeof(long) != 4) failed = 1;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            (cpu.read_16(GLOBAL_BASE + 4) << 16) | cpu.read_16(GLOBAL_BASE + 6),
            0xFFFFFFFF,
        )
        self.assertEqual(
            (cpu.read_16(GLOBAL_BASE + 8) << 16) | cpu.read_16(GLOBAL_BASE + 10),
            0x12345679,
        )
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 16), 0)
        self.assertEqual(
            (cpu.read_16(GLOBAL_BASE + 12) << 16) | cpu.read_16(GLOBAL_BASE + 14),
            0xFFFFFFFF,
        )

    def test_long_indirect_lvalues_cover_arrays_members_and_pointers(self):
        image = compile_source(
            "struct Holder { long value; } holder;\n"
            "long values[2];\n"
            "long *pointer;\n"
            "long result;\n"
            "int main(void) {\n"
            "  holder.value = 0x12345678L;\n"
            "  values[1] = holder.value;\n"
            "  pointer = &values[1];\n"
            "  *pointer = *pointer + 1L;\n"
            "  result = values[1];\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        holder_address = GLOBAL_BASE
        values_address = holder_address + 4
        result_address = values_address + 8 + 4
        self.assertEqual(
            (cpu.read_16(holder_address) << 16) | cpu.read_16(holder_address + 2),
            0x12345678,
        )
        self.assertEqual(
            (cpu.read_16(values_address + 4) << 16)
            | cpu.read_16(values_address + 6),
            0x12345679,
        )
        self.assertEqual(
            (cpu.read_16(result_address) << 16) | cpu.read_16(result_address + 2),
            0x12345679,
        )

    def test_switch_accepts_32_bit_long_control_and_case_values(self):
        image = compile_source(
            "long input;\n"
            "int result;\n"
            "int main(void) {\n"
            "  input = 0x12345678L;\n"
            "  switch (input) {\n"
            "    case 0x12345677L: result = 1; break;\n"
            "    case 0x12345678L: result = 2; break;\n"
            "    default: result = 3;\n"
            "  }\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 2)

    def test_integer_literal_types_follow_c90_candidate_ranges(self):
        image = compile_source(
            "int results[4];\n"
            "long signed_cast = (int)0x8000UL;\n"
            "unsigned int unsigned_cast = (unsigned int)0xFFFFFFFFUL;\n"
            "long address_sized_integer = 0x43101 + 1;\n"
            "int main(void) {\n"
            "  results[0] = 0xFFFF > 0;\n"
            "  results[1] = 32768 > 0;\n"
            "  results[2] = 65535U > 32767;\n"
            "  results[3] = 2147483648 > 0;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + offset) for offset in (0, 2, 4, 6)],
            [1, 1, 1, 1],
        )
        self.assertEqual(
            (cpu.read_16(GLOBAL_BASE + 8) << 16)
            | cpu.read_16(GLOBAL_BASE + 10),
            0xFFFF8000,
        )
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 12), 0xFFFF)
        self.assertEqual(
            (cpu.read_16(GLOBAL_BASE + 14) << 16)
            | cpu.read_16(GLOBAL_BASE + 16),
            0x43102,
        )

    def test_long_compound_updates_shifts_and_postfix_preserve_pair_values(self):
        image = compile_source(
            "unsigned long value;\n"
            "unsigned long result;\n"
            "int main(void) {\n"
            "  value = 0xFFFFFFFFUL;\n"
            "  result = value++;\n"
            "  if (value != 0UL) return 1;\n"
            "  if (result != 0xFFFFFFFFUL) return 2;\n"
            "  --value;\n"
            "  value += 3UL;\n"
            "  value <<= 2;\n"
            "  if (value != 8UL) return 3;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            (cpu.read_16(GLOBAL_BASE) << 16) | cpu.read_16(GLOBAL_BASE + 2),
            8,
        )
        self.assertEqual(
            (cpu.read_16(GLOBAL_BASE + 4) << 16)
            | cpu.read_16(GLOBAL_BASE + 6),
            0xFFFFFFFF,
        )

    def test_long_conversions_and_signed_comparison_boundaries(self):
        image = compile_source(
            "long signed_value;\n"
            "unsigned long unsigned_value;\n"
            "long extended_int;\n"
            "int narrowed_long;\n"
            "int checks[5];\n"
            "int main(void) {\n"
            "  signed_value = -32769L;\n"
            "  unsigned_value = 65535U;\n"
            "  extended_int = -32768;\n"
            "  narrowed_long = (int)0x12348000UL;\n"
            "  checks[0] = signed_value < -32768L;\n"
            "  checks[1] = signed_value > -32770L;\n"
            "  checks[2] = unsigned_value == 65535UL;\n"
            "  checks[3] = extended_int == -32768L;\n"
            "  checks[4] = narrowed_long == 0x8000;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + 14 + index * 2) for index in range(5)],
            [1, 1, 1, 1, 1],
        )

    def test_wide_integer_constant_expressions_preserve_unsigned_semantics(self):
        image = compile_source(
            "unsigned long quotient = 4294967295UL / 2UL;\n"
            "unsigned long remainder = 4294967295UL % 10UL;\n"
            "unsigned int shifted = 0x8000U >> 1;\n"
            "long signed_quotient = -7L / 3L;\n"
            "int main(void) { return 0; }\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            (cpu.read_16(GLOBAL_BASE) << 16) | cpu.read_16(GLOBAL_BASE + 2),
            0x7FFFFFFF,
        )
        self.assertEqual(
            (cpu.read_16(GLOBAL_BASE + 4) << 16)
            | cpu.read_16(GLOBAL_BASE + 6),
            5,
        )
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 8), 0x4000)
        self.assertEqual(
            (cpu.read_16(GLOBAL_BASE + 10) << 16)
            | cpu.read_16(GLOBAL_BASE + 12),
            0xFFFFFFFE,
        )

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
        self.assertEqual(cpu.read_16(GLOBAL_BASE), 0)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 2), 2)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 4), 0)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 6), 4)

    def test_c90_float_header_matches_ieee_target(self):
        image = compile_source(
            "#include <float.h>\n"
            "#if FLT_RADIX != 2 || FLT_MANT_DIG != 24 || FLT_DIG != 6\n"
            "#error incorrect float characteristics\n"
            "#endif\n"
            "#if DBL_MANT_DIG != 53 || DBL_DIG != 15\n"
            "#error incorrect double characteristics\n"
            "#endif\n"
            "#if LDBL_MANT_DIG != 53 || LDBL_DIG != 15\n"
            "#error incorrect long double characteristics\n"
            "#endif\n"
            "#if FLT_MIN_EXP != -125 || FLT_MAX_EXP != 128\n"
            "#error incorrect float exponent range\n"
            "#endif\n"
            "#if DBL_MIN_EXP != -1021 || DBL_MAX_EXP != 1024\n"
            "#error incorrect double exponent range\n"
            "#endif\n"
            "#if LDBL_MIN_EXP != -1021 || LDBL_MAX_EXP != 1024\n"
            "#error incorrect long double exponent range\n"
            "#endif\n"
            "int results[7];\n"
            "int main(void) {\n"
            "  results[0] = sizeof(float) == 4;\n"
            "  results[1] = sizeof(double) == 8;\n"
            "  results[2] = sizeof(long double) == 8;\n"
            "  results[3] = FLT_MAX > 3.0e38;\n"
            "  results[4] = FLT_EPSILON == 1.1920928955078125e-7;\n"
            "  results[5] = DBL_EPSILON == 2.2204460492503131e-16;\n"
            "  results[6] = LDBL_MAX == DBL_MAX;\n"
            "  results[7] = FLT_MIN > 1.1e-38 && FLT_MIN < 1.2e-38;\n"
            "  results[8] = DBL_MAX > 1.7e308;\n"
            "  results[9] = DBL_MIN > 2.2e-308 && DBL_MIN < 2.3e-308;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(10)],
            [1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        )

    def test_c90_ieee_float_layout_literals_and_execution(self):
        image = compile_source(
            "#include <stddef.h>\n"
            "float stored_float = 1.0f;\n"
            "double stored_double = 1.0;\n"
            "long double stored_long_double = 1.0L;\n"
            "float smallest_subnormal = 1.401298464324817e-45f;\n"
            "float negative_zero = -0.0f;\n"
            "struct Layout { char tag; float value; double wide; };\n"
            "struct Layout layout = { 'x', 2.0f, 3.0 };\n"
            "float rounded_float_constant = 16777216.0f + 1.0f;\n"
            "double rounded_double_constant = 9007199254740992.0 + 1.0;\n"
            "int converted_static_float = (int)2.75;\n"
            "float bump_float(float value) { return value + 1.0f; }\n"
            "double bump_double(double value) { return value + 0.5; }\n"
            "int main(void) {\n"
            "  float f;\n"
            "  double d;\n"
            "  long double ld;\n"
            "  f = bump_float(2.0f);\n"
            "  d = bump_double(2.0);\n"
            "  ld = 3.0L;\n"
            "  layout.value += 1.0f;\n"
            "  return sizeof(struct Layout) != 14\n"
            "      || offsetof(struct Layout, value) != 2\n"
            "      || offsetof(struct Layout, wide) != 6\n"
            "      || f != 3.0f || d != 2.5 || ld != 3.0L\n"
            "      || layout.value != 3.0f\n"
            "      || rounded_float_constant != 16777216.0f\n"
            "      || rounded_double_constant != 9007199254740992.0\n"
            "      || converted_static_float != 2\n"
            "      || (9007199254740992.0 + 1.0) != 9007199254740992.0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.registers[0], 0)
        self.assertEqual(
            bytes(cpu.memory[GLOBAL_BASE:GLOBAL_BASE + 28]),
            bytes.fromhex(
                "3f800000"
                "3ff0000000000000"
                "3ff0000000000000"
                "00000001"
                "80000000"
            ),
        )
        self.assertEqual(bytes(cpu.memory[GLOBAL_BASE + 30:GLOBAL_BASE + 34]), bytes.fromhex("40400000"))
        self.assertEqual(bytes(cpu.memory[GLOBAL_BASE + 34:GLOBAL_BASE + 42]), bytes.fromhex("4008000000000000"))
        self.assertEqual(bytes(cpu.memory[GLOBAL_BASE + 42:GLOBAL_BASE + 46]), bytes.fromhex("4b800000"))
        self.assertEqual(bytes(cpu.memory[GLOBAL_BASE + 46:GLOBAL_BASE + 54]), bytes.fromhex("4340000000000000"))

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

    def test_c90_stdlib_heap_and_floating_conversion(self):
        image = compile_source(
            "#include <stdlib.h>\n"
            "#include <errno.h>\n"
            "int results[10];\n"
            "int main(void) {\n"
            "  char *memory;\n"
            "  char *end;\n"
            "  double number;\n"
            "  memory = (char *)malloc(8UL);\n"
            "  results[0] = memory != NULL;\n"
            "  memory[0] = 'A'; memory[1] = 'B';\n"
            "  memory = (char *)realloc(memory, 20UL);\n"
            "  results[1] = memory != NULL && memory[0] == 'A' && "
            "memory[1] == 'B';\n"
            "  free(memory);\n"
            "  memory = (char *)calloc(4UL, 2UL);\n"
            "  results[2] = memory != NULL && memory[0] == 0 && "
            "memory[7] == 0;\n"
            "  free(memory);\n"
            "  memory = (char *)malloc(1024UL);\n"
            "  results[3] = memory != NULL;\n"
            "  free(memory);\n"
            "  memory = (char *)malloc(65536UL);\n"
            "  results[4] = memory == NULL && errno == ENOMEM;\n"
            "  number = strtod(\" -12.5e1tail\", &end);\n"
            "  results[5] = number == -125.0;\n"
            "  results[6] = *end == 't';\n"
            "  number = strtod(\"invalid\", &end);\n"
            "  results[7] = number == 0.0 && *end == 'i';\n"
            "  results[8] = div(-17, 5).quot == -3 && "
            "div(-17, 5).rem == -2;\n"
            "  errno = 0;\n"
            "  number = strtod(\"0.000030517578125e9\", &end);\n"
            "  results[9] = number > 25000.0 && number < 40000.0 && "
            "errno == 0 && *end == '\\0';\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image, maximum_steps=1_000_000)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + 2 + index * 2) for index in range(10)],
            [1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        )

    def test_c90_math_library_core_functions(self):
        image = compile_source(
            "#include <math.h>\n"
            "int results[8];\n"
            "int main(void) {\n"
            "  results[0] = (int)sqrt(9.0);\n"
            "  results[1] = (int)fabs(-2.5);\n"
            "  results[2] = (int)floor(-2.2);\n"
            "  results[3] = (int)ceil(2.2);\n"
            "  results[4] = (int)fmod(7.0, 4.0);\n"
            "  results[5] = (int)pow(2.0, 3.0);\n"
            "  results[6] = sin(0.0) == 0.0;\n"
            "  results[7] = log(exp(1.0)) > 0.5 && log(exp(1.0)) < 1.5;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + 2 + index * 2) for index in range(8)],
            [3, 2, -3 & 0xFFFF, 3, 3, 8, 1, 1],
        )

    def test_c90_math_range_and_angle_helpers(self):
        image = compile_source(
            "#include <math.h>\n"
            "int results[13];\n"
            "int main(void) {\n"
            "  int exponent;\n"
            "  double integral;\n"
            "  results[0] = (int)acos(1.0);\n"
            "  results[1] = (int)asin(0.0);\n"
            "  results[2] = (int)atan(0.0);\n"
            "  results[3] = (int)atan2(0.0, 1.0);\n"
            "  results[4] = (int)cos(0.0);\n"
            "  results[5] = (int)tan(0.0);\n"
            "  results[6] = (int)cosh(0.0);\n"
            "  results[7] = (int)sinh(0.0);\n"
            "  results[8] = (int)tanh(0.0);\n"
            "  results[9] = (int)(frexp(8.0, &exponent) * 10.0);\n"
            "  results[10] = exponent;\n"
            "  results[11] = (int)ldexp(0.5, 4);\n"
            "  results[12] = (int)(modf(2.5, &integral) * 10.0) + (int)integral;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + 2 + index * 2) for index in range(13)],
            [0, 0, 0, 0, 1, 0, 1, 0, 0, 5, 4, 8, 7],
        )

    def test_c90_strto_integer_conversions(self):
        image = compile_source(
            "#include <stdlib.h>\n"
            "#include <errno.h>\n"
            "#include <string.h>\n"
            "int results[16];\n"
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
                -47 & 0xFFFF, 7, 0, 63, 3, 32768, 5, 0, 0, 1,
                65535, 2, 0, 0, 0, 0,
            ],
        )

    def test_strto_functions_reject_unsupported_bases(self):
        image = compile_source(
            "#include <stdlib.h>\n"
            "#include <errno.h>\n"
            "int results[8];\n"
            "int main(void) {\n"
            "  char *end;\n"
            "  char text[2];\n"
            "  text[0] = '7';\n"
            "  text[1] = '\\0';\n"
            "  errno = 0;\n"
            "  results[0] = strtol(text, &end, 1);\n"
            "  results[1] = end == text;\n"
            "  results[2] = errno;\n"
            "  errno = 0;\n"
            "  results[3] = strtoul(text, &end, 37);\n"
            "  results[4] = end == text;\n"
            "  results[5] = errno;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + 2 + index * 2) for index in range(6)],
            [0, 1, 22, 0, 1, 22],
        )

    def test_strto_functions_cover_full_32_bit_long_ranges(self):
        image = compile_source(
            "#include <stdlib.h>\n"
            "#include <errno.h>\n"
            "long signed_values[4];\n"
            "unsigned long unsigned_values[4];\n"
            "int errors[4];\n"
            "int main(void) {\n"
            "  char *end;\n"
            "  signed_values[0] = strtol(\"2147483647\", &end, 10);\n"
            "  errors[0] = errno;\n"
            "  signed_values[1] = strtol(\"-2147483648\", &end, 10);\n"
            "  errors[1] = errno;\n"
            "  signed_values[2] = strtol(\"2147483648\", &end, 10);\n"
            "  errors[2] = errno;\n"
            "  signed_values[3] = strtol(\"-2147483649\", &end, 10);\n"
            "  errors[3] = errno;\n"
            "  unsigned_values[0] = strtoul(\"4294967295\", &end, 10);\n"
            "  unsigned_values[1] = strtoul(\"4294967296\", &end, 10);\n"
            "  unsigned_values[2] = strtoul(\"-1\", &end, 10);\n"
            "  unsigned_values[3] = strtoul(\"-4294967295\", &end, 10);\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image)
        self.assertFalse(cpu.running)
        signed_values = [
            (cpu.read_16(GLOBAL_BASE + 2 + index * 4) << 16)
            | cpu.read_16(GLOBAL_BASE + 4 + index * 4)
            for index in range(4)
        ]
        unsigned_base = GLOBAL_BASE + 18
        unsigned_values = [
            (cpu.read_16(unsigned_base + index * 4) << 16)
            | cpu.read_16(unsigned_base + index * 4 + 2)
            for index in range(4)
        ]
        self.assertEqual(
            signed_values,
            [0x7FFFFFFF, 0x80000000, 0x7FFFFFFF, 0x80000000],
        )
        self.assertEqual(unsigned_values, [0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 1])
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + 34 + index * 2) for index in range(4)],
            [0, 0, 34, 34],
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

    def test_c90_strerror_covers_all_defined_errno_values(self):
        cases = (
            ("E2BIG", "argument list too long"),
            ("EACCES", "permission denied"),
            ("EBADF", "bad file descriptor"),
            ("EDOM", "domain error"),
            ("EEXIST", "file exists"),
            ("EILSEQ", "illegal byte sequence"),
            ("EINVAL", "invalid argument"),
            ("EIO", "I/O error"),
            ("ENOENT", "no such file"),
            ("ENOMEM", "out of memory"),
            ("ENOSPC", "no space left"),
            ("ENOSYS", "operation unavailable"),
            ("EPERM", "operation not permitted"),
            ("ERANGE", "range error"),
            ("ESPIPE", "stream is not seekable"),
            ("EROFS", "read-only filesystem"),
            ("-1", "unknown error"),
        )
        for error_code, message in cases:
            with self.subTest(error_code=error_code):
                image = compile_source(
                    "#include <errno.h>\n"
                    "#include <string.h>\n"
                    "int main(void) {\n"
                    f'  return strcmp(strerror({error_code}), "{message}") != 0;\n'
                    "}\n"
                )
                cpu, _ = run_image(image)
                self.assertEqual(cpu.registers[0], 0)

    def test_c90_assert_macro_passes_and_aborts(self):
        success_image = compile_source(
            "#include <assert.h>\n"
            "int result;\n"
            "int main(void) {\n"
            "  assert(++result);\n"
            "  assert(2 + 2 == 4);\n"
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

    def test_c90_assert_expands_to_void_expression(self):
        with self.assertRaises(CCompilerError):
            compile_source(
                "#include <assert.h>\n"
                "int main(void) {\n"
                "  int value;\n"
                "  value = assert(1);\n"
                "  return value;\n"
                "}\n"
            )

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

    def test_c90_time_calendar_conversion_and_formatting(self):
        image = compile_source(
            "#include <time.h>\n"
            "#include <string.h>\n"
            "int results[7];\n"
            "char formatted[32];\n"
            "int main(void) {\n"
            "  time_t epoch;\n"
            "  time_t round_trip;\n"
            "  struct tm *calendar;\n"
            "  struct tm input;\n"
            "  epoch = 951782400L;\n"
            "  calendar = gmtime(&epoch);\n"
            "  results[0] = calendar->tm_year == 100;\n"
            "  results[1] = calendar->tm_mon == 1 && calendar->tm_mday == 29;\n"
            "  results[2] = calendar->tm_yday == 59 && calendar->tm_wday == 2;\n"
            "  results[3] = strftime(formatted, sizeof(formatted), "
            "\"%Y-%m-%d %a\", calendar) == 14;\n"
            "  results[4] = strcmp(formatted, \"2000-02-29 Tue\") == 0;\n"
            "  input = *calendar;\n"
            "  round_trip = mktime(&input);\n"
            "  results[5] = round_trip == epoch;\n"
            "  results[6] = strcmp(asctime(calendar), "
            "\"Tue Feb 29 00:00:00 2000\\n\") == 0;\n"
            "  return 0;\n"
            "}\n"
        )
        cpu, _ = run_image(image, maximum_steps=500_000)
        self.assertFalse(cpu.running)
        self.assertEqual(
            [cpu.read_16(GLOBAL_BASE + index * 2) for index in range(7)],
            [1, 1, 1, 1, 1, 1, 1],
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
        self.assertEqual(cpu.sp, MEM_MASK)

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
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x14), 0x4040)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x16), 0)
        self.assertEqual(cpu.read_16(GLOBAL_BASE + 0x18), 1)
        self.assertEqual(cpu.sp, MEM_MASK)

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
        self.assertEqual(cpu.read(VRAM_START + 8 * 320 + 13), 1)
        self.assertEqual(cpu.read(VRAM_START + 8 * 320 + 14), 1)
        self.assertEqual(cpu.read(VRAM_START + 8 * 320 + 15), 1)
        self.assertEqual(cpu.read(VRAM_START + 8 * 320 + 12), 0)
        self.assertEqual(cpu.sp, MEM_MASK)

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
        self.assertEqual(cpu.sp, MEM_MASK)

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
        cpu.memory = create_memory()
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
        self.assertEqual(cpu.sp, MEM_MASK)


if __name__ == "__main__":
    unittest.main()
