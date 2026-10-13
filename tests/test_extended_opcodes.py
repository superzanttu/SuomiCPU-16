"""Behavioural tests for the extended ALU, misc and conditional-branch opcodes."""
import unittest

from assembler import assemble
from src.SuomiCPU import MEM_MASK
from tests.gfx_harness import make_cpu


def run(source, limit=2000):
    cpu = make_cpu(assemble(source))
    for _ in range(limit):
        if not cpu.running:
            break
        cpu.step()
    assert not cpu.running, "program did not halt"
    return cpu


def regs(body, **_):
    return run(body + "\nHALT\n").registers


class ExtendedOpcodeTests(unittest.TestCase):
    def test_shifts_and_rotates(self):
        r = regs("LDI R0, 1\nSHL R0, 4\nLDI R1, 0x8000\nLSR R1, 3\n"
                 "LDI R2, 0x8000\nASR R2, 3\nLDI R3, 0x8001\nROL R3, 1\n"
                 "LDI R4, 0x8001\nROR R4, 1\nLDI R5, 2\nLDI R6, 1\nSHL R6, R5")
        self.assertEqual(r[0], 16)
        self.assertEqual(r[1], 0x1000)
        self.assertEqual(r[2], 0xF000)
        self.assertEqual(r[3], 0x0003)
        self.assertEqual(r[4], 0xC000)
        self.assertEqual(r[6], 4)

    def test_adc_sbc_chain_builds_32_bit_arithmetic(self):
        r = regs("LDI R0, 0xFFFF\nLDI R1, 1\nADD R0, R1\nLDI R2, 0\nLDI R3, 0\nADC R2, R3")
        self.assertEqual((r[0] & 0xFFFF, r[2]), (0, 1))
        r = regs("LDI R0, 0\nLDI R1, 1\nSUB R0, R1\nLDI R2, 5\nLDI R3, 0\nSBC R2, R3")
        self.assertEqual(r[2], 4)

    def test_32_bit_register_pair_arithmetic_and_shifts(self):
        r = regs(
            "LDI R0, 0xFFFF\nLDI R1, 0xFFFF\n"
            "LDI R2, 0\nLDI R3, 1\nADD32 R0, R2\n"
            "LDI R4, 0x8000\nLDI R5, 0\nNEG32 R4\n"
            "LDI R6, 0\nLDI R7, 3\nSHL32 R6, 1"
        )
        self.assertEqual((r[0], r[1]), (0, 0))
        self.assertEqual((r[4], r[5]), (0x8000, 0))
        self.assertEqual((r[6], r[7]), (0, 6))

    def test_32_bit_compare_and_pair_validation(self):
        cpu = run(
            "LDI R0, 1\nLDI R1, 0\nLDI R2, 1\nLDI R3, 0\nCMP32 R0, R2\nHALT"
        )
        self.assertEqual(cpu.flags["Z"], 1)
        cpu = run(
            "LDI R0, 0x7FFF\nLDI R1, 0xFFFF\n"
            "LDI R2, 0\nLDI R3, 1\nADD32 R0, R2\nHALT"
        )
        self.assertEqual((cpu.registers[0], cpu.registers[1]), (0x8000, 0))
        self.assertEqual((cpu.flags["N"], cpu.flags["V"], cpu.flags["C"]), (1, 1, 0))
        for source in (
            "ADD32 R1, R2",
            "ADD32 R0, R3",
            "NEG32 R3",
            "SHL32 R0, 32",
            "LD32 R0, R3",
            "ST32 R0, R1",
        ):
            with self.subTest(source=source):
                with self.assertRaises(ValueError):
                    assemble(source)

    def test_movsph_reads_high_bits_of_stack_pointer(self):
        cpu = run("MOVSPH R0\nHALT")
        self.assertEqual(cpu.registers[0], MEM_MASK >> 16)

    def test_setsph_updates_only_the_high_stack_pointer_bits(self):
        cpu = run(
            "LDI R1, 0x1234\n"
            "SETSP R1\n"
            "SETC\n"
            "LDI R0, 0xFFFF\n"
            "SETSPH R0\n"
            "MOVSP R1\n"
            "MOVSPH R2\n"
            "HALT"
        )
        self.assertEqual(cpu.sp, 0x3FFF1234)
        self.assertEqual((cpu.registers[1], cpu.registers[2]), (0x3FFF1234, 0x3FFF))
        self.assertEqual(cpu.flags["C"], 1)

    def test_lda_loads_a_full_19_bit_address(self):
        cpu = run("LDA R0, 0x51234\nHALT")
        self.assertEqual(cpu.registers[0], 0x51234)

    def test_32_bit_memory_access_is_big_endian_and_wraps(self):
        cpu = run(
            "LDI R0, 0xFFFF\nLDI R1, 0xFFFF\n"
            "LDI R2, 0x1234\nLDI R3, 0x5678\n"
            "ST32 R2, R0\nLD32 R4, R0\nHALT"
        )
        self.assertEqual((cpu.registers[4], cpu.registers[5]), (0x1234, 0x5678))
        self.assertEqual(bytes(cpu.memory[:3]), b"\x34\x56\x78")
        self.assertEqual(cpu.memory[MEM_MASK], 0x12)

    def test_30_bit_pointer_load_and_store_wrap_at_memory_end(self):
        cpu = run(
            "LDI30 R0, 0x12345678\n"
            "LDI30 R1, 0x3FFFFFFD\n"
            "STP R0, R1\n"
            "LDP R2, R1\n"
            "HALT"
        )
        self.assertEqual(cpu.registers[2], 0x12345678)
        self.assertEqual(bytes(cpu.memory[MEM_MASK - 1:MEM_MASK + 1]), b"\x78\x12")
        self.assertEqual(bytes(cpu.memory[:1]), b"\x34")

    def test_32_bit_multiply_divide_and_remainder(self):
        cpu = run(
            "LDI R0, 0\nLDI R1, 100\nLDI R2, 0\nLDI R3, 9\n"
            "MUL32 R0, R2\nLDI R4, 0\nLDI R5, 10\n"
            "DIV32 R0, R4\nHALT"
        )
        self.assertEqual((cpu.registers[0], cpu.registers[1]), (0, 90))
        cpu = run(
            "LDI R0, 0xFFFF\nLDI R1, 0xFF9C\n"
            "LDI R2, 0\nLDI R3, 7\nDIV32S R0, R2\nHALT"
        )
        self.assertEqual((cpu.registers[0], cpu.registers[1]), (0xFFFF, 0xFFF2))
        cpu = run(
            "LDI R0, 0xFFFF\nLDI R1, 0xFF9C\n"
            "LDI R2, 0\nLDI R3, 7\nMOD32S R0, R2\nHALT"
        )
        self.assertEqual((cpu.registers[0], cpu.registers[1]), (0xFFFF, 0xFFFE))

    def test_32_bit_subtract_logic_copy_and_shift(self):
        cpu = run(
            "LDI R0, 0\nLDI R1, 10\nLDI R2, 0\nLDI R3, 3\n"
            "SUB32 R0, R2\nMOV32 R4, R0\nAND32 R0, R2\nOR32 R0, R2\n"
            "XOR32 R0, R2\nLDI R6, 0x8000\nLDI R7, 0\n"
            "ASR32 R6, 1\nHALT"
        )
        self.assertEqual((cpu.registers[0], cpu.registers[1]), (0, 0))
        self.assertEqual((cpu.registers[4], cpu.registers[5]), (0, 7))
        self.assertEqual((cpu.registers[6], cpu.registers[7]), (0xC000, 0))
        self.assertEqual(cpu.flags["N"], 1)

    def test_neg_swap_sext_clz(self):
        r = regs("LDI R0, 1\nNEG R0\nLDI R1, 0x1234\nSWAP R1\nLDI R2, 0x80\nSEXT R2\nLDI R4, 1\nCLZ R4")
        self.assertEqual(r[0], 0xFFFF)
        self.assertEqual(r[1], 0x3412)
        self.assertEqual(r[2], 0xFF80)
        self.assertEqual(r[4], 15)

    def test_immediate_alu_forms(self):
        r = regs("LDI R0, 100\nADD R0, 1000\nLDI R1, 0xFFFF\nAND R1, 0x0F0F\nLDI R2, 5\nSUB R2, 2")
        self.assertEqual((r[0], r[1], r[2]), (1100, 0x0F0F, 3))

    def test_test_sets_zero_without_writing(self):
        cpu = run("LDI R0, 0xF0\nTEST R0, 0x0F\nHALT\n")
        self.assertEqual(cpu.registers[0], 0xF0)
        self.assertEqual(cpu.flags["Z"], 1)

    def test_signed_and_unsigned_branches(self):
        template = ("LDI R0, {a}\nLDI R1, {b}\nCMP R0, R1\n{br} yes\nLDI R7, 0\nHALT\nyes: LDI R7, 1\nHALT\n")
        cases = [("BLT", 0xFFFF, 1, 1), ("BLT", 1, 0xFFFF, 0), ("BGE", 5, 5, 1),
                 ("BGT", 5, 5, 0), ("BLE", 5, 5, 1), ("BHI", 0xFFFF, 1, 1),
                 ("BHI", 1, 0xFFFF, 0), ("BLS", 1, 0xFFFF, 1), ("BLT", 0x7FFF, 0x8000, 0)]
        for br, a, b, taken in cases:
            with self.subTest(br=br, a=a, b=b):
                self.assertEqual(run(template.format(a=a, b=b, br=br)).registers[7], taken)

    def test_register_jump_call_and_flag_stack(self):
        r = regs("LDA R1, target\nJMPR R1\nLDI R0, 1\nHALT\ntarget: LDI R0, 7")
        self.assertEqual(r[0], 7)
        r = regs("LDA R1, fn\nCALLR R1\nHALT\nfn: LDI R0, 9\nRET")
        self.assertEqual(r[0], 9)
        cpu = run("SETC\nPUSHF\nCLC\nPOPF\nHALT\n")
        self.assertEqual(cpu.flags["C"], 1)


if __name__ == "__main__":
    unittest.main()
