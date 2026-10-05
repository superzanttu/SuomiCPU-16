"""Behavioural tests for the extended ALU, misc and conditional-branch opcodes."""
import unittest

from assembler import assemble
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
