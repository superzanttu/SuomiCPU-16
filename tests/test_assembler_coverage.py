"""Directive, parsing, include and error-path tests for the assembler."""

import tempfile
import unittest
from pathlib import Path

from assembler import MEMORY_SIZE, AssemblyError, assemble, assemble_file


def err(source, **kwargs):
    with unittest.TestCase().assertRaises(AssemblyError) as ctx:
        assemble(source, **kwargs)
    return str(ctx.exception)


class DataDirectiveTests(unittest.TestCase):
    def data(self, text):
        return bytes(assemble(text).segments[0].data)

    def test_formats(self):
        self.assertEqual(self.data(".data hex 0x1F, ab 0X0"), b"\x1f\xab\x00")
        self.assertEqual(self.data(".data oct 0o17 7 0O1"), b"\x0f\x07\x01")
        self.assertEqual(self.data(".data bin 0b101 11 0B0"), b"\x05\x03\x00")
        self.assertEqual(self.data(".DATA HEX ff"), b"\xff")

    def test_strings(self):
        self.assertEqual(self.data('.data string "Hi", \'x\' "a\\n"'), b"Hixa\n")
        self.assertEqual(self.data('.data string "ä"'), "ä".encode())
        self.assertEqual(self.data('.data string "a;b" ; comment'), b"a;b")
        self.assertEqual(self.data('.data string "q\\"q"'), b'q"q')

    def test_data_errors(self):
        for text in [
            ".data", ".data hex", ".data xyz 1", ".data hex 100", ".data hex zz",
            ".data oct 8", ".data bin 2", ".data string", ".data string abc",
            '.data string "abc', '.data string "a""b"', ".data string 'a\\'",
            '.data string "\\x"', ".data string b'a'", ".datax hex 1",
            '.data string "a"b',
        ]:
            with self.subTest(text=text):
                with self.assertRaises(AssemblyError):
                    assemble(text)

    def test_empty_value_lists_and_trailing_separator(self):
        self.assertEqual(bytes(assemble('.data string "a",').segments[0].data), b"a")
        for text in [".data string ,", ".data hex ,", ".data bin , ,"]:
            with self.subTest(text=text):
                with self.assertRaises(AssemblyError):
                    assemble(text)

    def test_empty_string_data_is_valid_but_emits_nothing(self):
        image = assemble('.data string ""\nHALT')
        self.assertEqual(image.entry_point, 0)
        self.assertEqual(len(image.segments), 1)


class AddressAndSegmentTests(unittest.TestCase):
    def test_multiple_segments_and_entry_point(self):
        image = assemble(".address 0x200\nHALT\n.address 0x100\nINC R0\n")
        self.assertEqual([(s.address, s.data) for s in image.segments],
                         [(0x100, b"\x68\x00"), (0x200, b"\x00\x00")])
        self.assertEqual(image.entry_point, 0x200)

    def test_adjacent_output_merges_into_one_segment(self):
        image = assemble(".address 0x10\nHALT\n.data hex 01\nHALT")
        self.assertEqual(len(image.segments), 1)

    def test_entry_point_defaults(self):
        self.assertEqual(assemble("").entry_point, 0)
        self.assertEqual(assemble("", origin=0x40).entry_point, 0x40)
        self.assertEqual(assemble(".address 5\n.data hex 1").entry_point, 5)
        self.assertEqual(assemble(".data hex 1\nHALT").entry_point, 1)
        self.assertEqual(assemble("HALT", origin=0x30).segments[0].address, 0x30)

    def test_address_forms(self):
        self.assertEqual(assemble(".ADDRESS 0x20\nHALT").segments[0].address, 0x20)
        for text in [".address", ".address x", ".address 0x80000", ".address -1",
                     ".addressx 1", ".address 1 2"]:
            with self.subTest(text=text):
                with self.assertRaises(AssemblyError):
                    assemble(text)

    def test_invalid_origin(self):
        for origin in (-1, MEMORY_SIZE):
            with self.assertRaises(AssemblyError):
                assemble("HALT", origin=origin)

    def test_overlap_and_overflow(self):
        with self.assertRaises(AssemblyError):
            assemble(".address 0\nHALT\n.address 1\nHALT")
        with self.assertRaises(AssemblyError):
            assemble(f".address {MEMORY_SIZE - 1}\nHALT")
        with self.assertRaises(AssemblyError):
            assemble(f".address {MEMORY_SIZE - 1}\n.data hex 1 2")
        with self.assertRaises(AssemblyError):
            assemble(f".address {MEMORY_SIZE - 2}\nJMPX 0")
        # Last byte of memory is addressable.
        image = assemble(f".address {MEMORY_SIZE - 1}\n.data hex 7")
        self.assertEqual(image.segments[0].address, MEMORY_SIZE - 1)


class SyntaxTests(unittest.TestCase):
    def test_labels(self):
        image = assemble("a:\nb: HALT\nJMP a")
        self.assertEqual(bytes(image.segments[0].data)[2:], b"\x58\x00")
        for text in ["a:\na:", "1a: HALT", "a b: HALT", "HALT: x"]:
            with self.subTest(text=text):
                with self.assertRaises(AssemblyError):
                    assemble(text)

    def test_label_on_directive_line(self):
        image = assemble(".address 8\nx: .data hex 1\nLDA R0, x")
        self.assertEqual(bytes(image.segments[0].data)[1:], b"\x08\x08\x80\x00")

    def test_comments_and_whitespace(self):
        src = "  ; full\n# hash\nLDI R0, 1 ; c\n\tHALT # c\n"
        self.assertEqual(bytes(assemble(src).segments[0].data), b"\x08\x01\x00\x00")

    def test_numeric_forms(self):
        for literal, value in (("0x10", 16), ("0b11", 3), ("0o7", 7), ("12", 12)):
            self.assertEqual(bytes(assemble(f"LDI R0, {literal}").segments[0].data)[1], value)

    def test_error_messages_carry_line_numbers(self):
        self.assertIn("line 3", err("HALT\n\nFOO"))
        self.assertIn("unknown instruction", err("FOO"))
        self.assertIn("invalid register", err("INC Q"))
        self.assertIn("expected a numeric value", err("LDI R0, xyz"))
        self.assertIn("duplicate label", err("a:\na:"))

    def test_operand_count_errors_for_every_form(self):
        for text in [
            "LDA R0", "LDI R0", "LDI_H R0", "LD R0", "LDW R0", "ST R0", "STW R0",
            "LDWS R0", "STWS R0", "ADJSP", "ADJSP 1, 2", "MOV R0", "ADD R0",
            "NOT", "NOT R0, R1, R2", "INC", "JMPX", "JMPX 1, 2", "CALLX", "BZX",
            "BCX", "JZ R0", "JZ", "ITOF", "FTOI", "MOVSP", "MOVSP R0, R1",
            "FADD R0", "HALT 1", "EI 1", "DI 1", "RTI 1", "RET 1", "JMP 1, 2",
            "LDA R0, 1, 2", "LDI R0, 1, 2", "SHR", "PUSH", "POP",
        ]:
            with self.subTest(text=text):
                with self.assertRaises(AssemblyError):
                    assemble(text)

    def test_undefined_label_errors(self):
        for text in ["JMP nowhere", "JZ R0, nowhere", "LDA R0, nowhere", "CALLX nowhere"]:
            with self.subTest(text=text):
                with self.assertRaises(AssemblyError):
                    assemble(text)

    def test_range_errors(self):
        for text in [
            "JZ R0, -1", "JMP 0x800", "LDA R0, -1", "LDA R0, 0x80000",
            "JMPX -1", "LDI R0, -1", "LDI R0, 0x10000", "LDI_H R0, -1",
        ]:
            with self.subTest(text=text):
                with self.assertRaises(AssemblyError):
                    assemble(text)

    def test_forward_label_far_and_lda(self):
        image = assemble("LDA R2, end\nJMPX end\n.address 0x12345\nend: HALT")
        self.assertEqual(image.entry_point, 0)
        self.assertEqual(bytes(image.segments[0].data)[:2], b"\x0a\x45")
        self.assertEqual(bytes(image.segments[0].data)[4:], b"\xf8\x1d\x23\x45\x00\x01")


class IncludeTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name)

    def write(self, name, text, encoding="utf-8"):
        path = self.dir / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(text.encode(encoding))
        return path

    def test_include_relative_nested_and_bom(self):
        self.write("sub/inner.asm", "INC R1\n")
        self.write("lib.asm", '.include "sub/inner.asm"\nINC R2\n', "utf-8-sig")
        main = self.write("main.asm", '.include "lib.asm"\nHALT\n')
        image = assemble_file(main)
        self.assertEqual(bytes(image.segments[0].data), b"\x69\x00\x6a\x00\x00\x00")

    def test_include_via_base_directory(self):
        self.write("lib.asm", "HALT")
        image = assemble('.INCLUDE "lib.asm"', base_directory=self.dir)
        self.assertEqual(bytes(image.segments[0].data), b"\x00\x00")
        image = assemble('.include "lib.asm"', base_directory=str(self.dir))
        self.assertEqual(len(image.segments), 1)

    def test_include_defaults_to_cwd(self):
        with self.assertRaises(AssemblyError):
            assemble('.include "definitely_missing_file.asm"')

    def test_include_errors(self):
        self.write("a.asm", '.include "b.asm"')
        self.write("b.asm", '.include "a.asm"')
        self.write("bad.asm", "\xff\xfe", "latin-1")
        for text in [
            ".include", ".include lib", '.include ""', ".include 5",
            '.include "x" "y"', '.include "missing.asm"', '.include "a.asm"',
            '.include "bad.asm"', '.include "."',
        ]:
            with self.subTest(text=text):
                with self.assertRaises(AssemblyError):
                    assemble(text, base_directory=self.dir)

    def test_circular_message(self):
        self.write("a.asm", '.include "b.asm"')
        self.write("b.asm", '.include "a.asm"')
        with self.assertRaisesRegex(AssemblyError, "circular"):
            assemble('.include "a.asm"', base_directory=self.dir)

    def test_assemble_file_errors(self):
        with self.assertRaises(AssemblyError):
            assemble_file(self.dir / "missing.asm")
        bad = self.write("bad.asm", "FOO")
        with self.assertRaisesRegex(AssemblyError, "bad.asm"):
            assemble_file(bad)
        latin = self.write("latin.asm", "\xe4", "latin-1")
        with self.assertRaises(AssemblyError):
            assemble_file(latin)

    def test_assemble_file_origin(self):
        path = self.write("o.asm", "HALT")
        self.assertEqual(assemble_file(path, origin=0x10).segments[0].address, 0x10)


if __name__ == "__main__":
    unittest.main()
