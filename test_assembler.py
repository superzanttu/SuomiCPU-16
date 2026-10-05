import unittest
import tempfile
from pathlib import Path

from assembler import AssemblyError, assemble, assemble_file
from isa import OPCODES


class AssemblerTests(unittest.TestCase):
    def test_bitmap_font_contains_ascii_and_finnish_glyphs(self):
        font_path = Path(__file__).parent / "fonts" / "font5x7.asm"
        image = assemble_file(font_path)
        font = next(segment for segment in image.segments if segment.address == 0x4400)

        self.assertEqual(len(font.data), 101 * 8)
        self.assertEqual(font.data[0:8], bytes(8))
        self.assertEqual(font.data[8:16], bytes((0x20, 0x20, 0x20, 0x20, 0x20, 0, 0x20, 0)))

    def test_loads_example_assembly(self):
        image = assemble_file("examples/example.asm")

        self.assertEqual(image.entry_point, 0)
        self.assertEqual(len(image.segments), 1)
        self.assertEqual(len(image.segments[0].data), 94)
        self.assertEqual(image.segments[0].data[:2], b"\x08\x0a")
        self.assertIn(b"\x80\x30", image.segments[0].data)
        self.assertEqual(image.segments[0].data[-2:], b"\x00\x00")

    def test_command_tour_covers_every_opcode(self):
        source = Path(__file__).parent / "examples" / "commands.asm"
        mnemonics = {
            line.split(";", 1)[0].strip().split()[0].upper()
            for line in source.read_text(encoding="utf-8").splitlines()
            if line.split(";", 1)[0].strip()
            and line.split(";", 1)[0].strip().split()[0].upper() in OPCODES
        }

        self.assertEqual(mnemonics, set(OPCODES))

    def test_expands_wide_ldi_and_resolves_forward_labels(self):
        image = assemble(
            "JMP finish\n"
            "LDI R1, 315\n"
            "finish: HALT\n"
        )

        self.assertEqual(image.entry_point, 0)
        self.assertEqual(
            image.segments[0].data,
            b"\x58\x06\x09\x3b\x81\x01\x00\x00",
        )

    def test_multiple_address_directives_place_distinct_segments(self):
        image = assemble(
            ".address 0x100\n"
            "start: JMP end\n"
            ".address 0x20\n"
            "end: HALT\n"
        )

        self.assertEqual(image.entry_point, 0x100)
        self.assertEqual(
            [(segment.address, segment.data) for segment in image.segments],
            [(0x20, b"\x00\x00"), (0x100, b"\x58\x20")],
        )

    def test_address_directive_accepts_full_cpu_memory_map(self):
        image = assemble(".address 0x30000\n.data hex 19")

        self.assertEqual(image.segments[0].address, 0x30000)
        self.assertEqual(image.segments[0].data, b"\x19")

    def test_data_directive_emits_hex_octal_binary_and_utf8_strings(self):
        image = assemble(
            '.data hex 0x41, 42\n'
            '.data oct 101 102\n'
            '.data bin 01000011, 01000100\n'
            '.data string "Hi\\n"\n'
        )

        self.assertEqual(image.segments[0].address, 0)
        self.assertEqual(image.segments[0].data, b"ABABCDHi\n")

    def test_reports_overlapping_output(self):
        with self.assertRaisesRegex(AssemblyError, "overlaps"):
            assemble("HALT\n.address 0\n.data hex 01")

    def test_include_resolves_from_including_file_and_inlines_statements(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "parts").mkdir()
            (root / "parts" / "font.asm").write_text(
                ".data hex 41, 42\n", encoding="utf-8"
            )
            (root / "main.asm").write_text(
                '.address 0x20\n.include "parts/font.asm"\nHALT\n',
                encoding="utf-8",
            )

            image = assemble_file(root / "main.asm")

        self.assertEqual(
            [(segment.address, segment.data) for segment in image.segments],
            [(0x20, b"AB\x00\x00")],
        )
        self.assertEqual(image.entry_point, 0x22)

    def test_reports_circular_include(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            source = Path(temp_dir) / "cycle.asm"
            source.write_text('.include "cycle.asm"\n', encoding="utf-8")

            with self.assertRaisesRegex(AssemblyError, "circular .include"):
                assemble_file(source)

    def test_encodes_store_address_and_value_registers(self):
        self.assertEqual(assemble("ST R0, R1").segments[0].data, b"\x19\x00")

    def test_encodes_call_return_and_zero_flag_branch(self):
        image = assemble(
            "CALL routine\n"
            "HALT\n"
            ".address 0x400\n"
            "routine: RET\n"
            "branch: BZ routine\n"
        )

        segments = {segment.address: segment.data for segment in image.segments}
        self.assertEqual(segments[0], b"\xB4\x00\x00\x00")
        self.assertEqual(segments[0x400], b"\xB8\x00\xC4\x00")

    def test_encodes_far_control_flow_targets(self):
        image = assemble(
            "JMPX 0x12345\n"
            "CALLX 0x12345\n"
            "BZX 0x12345\n"
            "BCX 0x12345\n"
        )

        self.assertEqual(
            image.segments[0].data,
            b"\xF8\x1D\x23\x45\x00\x01"
            b"\xF9\x1D\x23\x45\x00\x01"
            b"\xFA\x1D\x23\x45\x00\x01"
            b"\xFB\x1D\x23\x45\x00\x01",
        )

    def test_encodes_word_stack_and_carry_instructions(self):
        image = assemble(
            "LDW R2, R3\n"
            "STW R2, R3\n"
            "LDWS R2, 4\n"
            "STWS R2, 4\n"
            "ADJSP -2\n"
            "BC 0x100\n"
            "SHR R3\n"
        )

        self.assertEqual(
            image.segments[0].data,
            b"\xCA\x60\xD3\x40\xDA\x04\xE2\x04"
            b"\xE8\xFE\xF1\x00\xFB\x00",
        )

    def test_reports_invalid_register(self):
        with self.assertRaisesRegex(AssemblyError, "invalid register"):
            assemble("INC R8")

    def test_reports_unknown_instruction(self):
        with self.assertRaisesRegex(AssemblyError, "unknown instruction"):
            assemble("NOPE")


if __name__ == "__main__":
    unittest.main()
