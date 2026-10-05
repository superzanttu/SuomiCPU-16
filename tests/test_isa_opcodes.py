"""Exhaustive per-opcode tests: assembler encoding and CPU execution.

Every opcode is checked with every register-field combination (R0-R7 for
rd, rs1 and rs2) and with boundary values for immediates, targets and flags.
"""

import itertools
import math
import struct
import unittest

import isa
from assembler import AssemblyError, assemble
from SuomiCPU import ICR_ADDR, MEM_SIZE, SuomiCompute8

REGS = range(8)
CODE = 0x100
EDGE_BYTES = (0, 1, 2, 0x7F, 0x80, 0xFE, 0xFF)
EDGE_PAIRS = list(itertools.product((0, 1, 0x7F, 0x80, 0xFF), repeat=2))


def word(opcode, rd=0, rs1=0, rs2=0, imm=None):
    if imm is not None:
        return (opcode << 11) | (rd << 8) | imm
    return (opcode << 11) | (rd << 8) | (rs1 << 5) | (rs2 << 2)


def be(*words):
    return b"".join(w.to_bytes(2, "big") for w in words)


def encode(text):
    image = assemble(text)
    assert len(image.segments) == 1
    return bytes(image.segments[0].data)


def make_cpu(program=b"", registers=None, pc=CODE, sp=0x2FFFF, flags=None):
    cpu = SuomiCompute8.__new__(SuomiCompute8)
    cpu.memory = bytearray(MEM_SIZE)
    cpu.memory[pc:pc + len(program)] = program
    cpu.registers = list(registers) if registers else [0] * 8
    cpu.flags = {"Z": 0, "C": 0}
    if flags:
        cpu.flags.update(flags)
    cpu.running = True
    cpu.sp = sp
    cpu.pc = pc
    return cpu


def distinct_registers():
    return [0x50 + 0x11 * index for index in REGS]


def half(value):
    return struct.unpack(">e", value.to_bytes(2, "big"))[0]


def bits(value):
    return int.from_bytes(struct.pack(">e", value), "big")


class OpcodeTableTests(unittest.TestCase):
    def test_every_opcode_constant_is_unique_and_five_bits(self):
        constants = {name: getattr(isa, name) for name in dir(isa) if name.startswith("OP_")}
        self.assertEqual(set(constants.values()), set(range(32)))
        aliases = {n for n, v in constants.items() if v == 31}
        self.assertEqual(
            aliases,
            {"OP_SHR", "OP_MOVSP", "OP_FADD", "OP_FSUB", "OP_FMUL", "OP_FDIV",
             "OP_FCMP", "OP_ITOF", "OP_FTOI"},
        )
        self.assertEqual(len(constants) - len(aliases), 31)

    def test_mnemonic_table_matches_opcode_constants(self):
        expected = {
            "HALT": 0x00, "LDI": 0x01, "LD": 0x02, "ST": 0x03, "MOV": 0x04,
            "ADD": 0x05, "SUB": 0x06, "AND": 0x07, "OR": 0x08, "XOR": 0x09,
            "NOT": 0x0A, "JMP": 0x0B, "JZ": 0x0C, "INC": 0x0D, "DEC": 0x0E,
            "CMP": 0x0F, "LDI_H": 0x10, "EI": 0x11, "DI": 0x12, "RTI": 0x13,
            "PUSH": 0x14, "POP": 0x15, "CALL": 0x16, "RET": 0x17, "BZ": 0x18,
            "LDW": 0x19, "STW": 0x1A, "LDWS": 0x1B, "STWS": 0x1C,
            "ADJSP": 0x1D, "BC": 0x1E, "SHR": 0x1F,
        }
        for mnemonic, opcode in expected.items():
            with self.subTest(mnemonic=mnemonic):
                self.assertEqual(isa.OPCODES[mnemonic], opcode)
        for mnemonic in ("MOVSP", "FADD", "FSUB", "FMUL", "FDIV", "FCMP", "ITOF", "FTOI"):
            self.assertEqual(isa.OPCODES[mnemonic], isa.OP_SHR)


class AssemblerEncodingTests(unittest.TestCase):
    def test_no_operand_instructions(self):
        for name in ("HALT", "EI", "DI", "RTI", "RET"):
            with self.subTest(name=name):
                self.assertEqual(encode(name), be(isa.OPCODES[name] << 11))

    def test_single_register_instructions(self):
        for name in ("INC", "DEC", "PUSH", "POP", "SHR"):
            for rd in REGS:
                with self.subTest(name=name, rd=rd):
                    self.assertEqual(encode(f"{name} R{rd}"), be(word(isa.OPCODES[name], rd)))

    def test_movsp_every_register(self):
        for rd in REGS:
            with self.subTest(rd=rd):
                self.assertEqual(encode(f"MOVSP R{rd}"), be(word(isa.OP_SHR, rd, 0, 7)))

    def test_ldi_every_register_and_value_boundary(self):
        for rd in REGS:
            for value in EDGE_BYTES:
                with self.subTest(rd=rd, value=value):
                    self.assertEqual(encode(f"LDI R{rd}, {value}"), be(word(isa.OP_LDI, rd, imm=value)))
            for value in (0x100, 0x1234, 0xFFFF):
                with self.subTest(rd=rd, wide=value):
                    self.assertEqual(
                        encode(f"LDI R{rd}, {value}"),
                        be(word(isa.OP_LDI, rd, imm=value & 0xFF), word(isa.OP_LDI_H, rd, imm=value >> 8)),
                    )

    def test_ldi_h_every_register_and_value(self):
        for rd in REGS:
            for value in EDGE_BYTES:
                with self.subTest(rd=rd, value=value):
                    self.assertEqual(encode(f"LDI_H R{rd}, {value}"), be(word(isa.OP_LDI_H, rd, imm=value)))

    def test_lda_every_register(self):
        for rd in REGS:
            for address in (0, 0xFF, 0x100, 0x2FFFF, MEM_SIZE - 1):
                with self.subTest(rd=rd, address=address):
                    data = encode(f"LDA R{rd}, {address}")
                    self.assertEqual(
                        data,
                        be(word(isa.OP_LDI, rd, imm=address & 0xFF), word(isa.OP_LDI_H, rd, imm=address >> 8)),
                    )

    def test_two_register_forms(self):
        for rd, rs in itertools.product(REGS, REGS):
            with self.subTest(rd=rd, rs=rs):
                self.assertEqual(encode(f"MOV R{rd}, R{rs}"), be(word(isa.OP_MOV, rd, rs)))
                self.assertEqual(encode(f"LD R{rd}, R{rs}"), be(word(isa.OP_LD, rd, rs)))
                self.assertEqual(encode(f"LDW R{rd}, R{rs}"), be(word(isa.OP_LDW, rd, rs)))
                # ST/STW take the address register first and the value second.
                self.assertEqual(encode(f"ST R{rd}, R{rs}"), be(word(isa.OP_ST, rs, rd)))
                self.assertEqual(encode(f"STW R{rd}, R{rs}"), be(word(isa.OP_STW, rs, rd)))
                self.assertEqual(encode(f"NOT R{rd}, R{rs}"), be(word(isa.OP_NOT, rd, rs)))
                for name in ("ADD", "SUB", "AND", "OR", "XOR", "CMP"):
                    self.assertEqual(encode(f"{name} R{rd}, R{rs}"), be(word(isa.OPCODES[name], rd, rd, rs)))
                for name, ext in (("FADD", 1), ("FSUB", 2), ("FMUL", 3), ("FDIV", 4), ("FCMP", 5)):
                    self.assertEqual(encode(f"{name} R{rd}, R{rs}"), be(word(isa.OP_SHR, rd, ext, rs)))

    def test_not_single_operand_form(self):
        for rd in REGS:
            with self.subTest(rd=rd):
                self.assertEqual(encode(f"NOT R{rd}"), be(word(isa.OP_NOT, rd, rd)))

    def test_float_conversions(self):
        for rd in REGS:
            with self.subTest(rd=rd):
                self.assertEqual(encode(f"ITOF R{rd}"), be(word(isa.OP_SHR, rd, 6)))
                self.assertEqual(encode(f"FTOI R{rd}"), be(word(isa.OP_SHR, rd, 7)))

    def test_stack_relative_offsets(self):
        for name in ("LDWS", "STWS"):
            for rd, offset in itertools.product(REGS, EDGE_BYTES):
                with self.subTest(name=name, rd=rd, offset=offset):
                    self.assertEqual(encode(f"{name} R{rd}, {offset}"), be(word(isa.OPCODES[name], rd, imm=offset)))

    def test_adjsp_signed_range(self):
        for offset in range(-128, 128):
            with self.subTest(offset=offset):
                self.assertEqual(encode(f"ADJSP {offset}"), be(word(isa.OP_ADJSP, imm=offset & 0xFF)))

    def test_short_branches_boundary_targets(self):
        for name in ("JMP", "CALL", "BZ", "BC"):
            for target in (0, 1, 0xFF, 0x100, 0x400, 0x7FE, 0x7FF):
                with self.subTest(name=name, target=target):
                    self.assertEqual(encode(f"{name} {target}"), be((isa.OPCODES[name] << 11) | target))

    def test_jz_every_register_and_target(self):
        for rd, target in itertools.product(REGS, EDGE_BYTES):
            with self.subTest(rd=rd, target=target):
                self.assertEqual(encode(f"JZ R{rd}, {target}"), be(word(isa.OP_JZ, rd, imm=target)))

    def test_far_control_boundary_targets(self):
        sentinels = {"JMPX": 0xF81D, "CALLX": 0xF91D, "BZX": 0xFA1D, "BCX": 0xFB1D}
        for name, sentinel in sentinels.items():
            for target in (0, 1, 0xFFFF, 0x10000, 0x12345, MEM_SIZE - 1):
                with self.subTest(name=name, target=target):
                    self.assertEqual(
                        encode(f"{name} {target}"),
                        be(sentinel, target & 0xFFFF, target >> 16),
                    )

    def test_labels_resolve_for_every_branch(self):
        for name in ("JMP", "CALL", "BZ", "BC"):
            data = encode(f".address 0x10\n{name} end\nHALT\nend:\nHALT")
            self.assertEqual(data[:2], be((isa.OPCODES[name] << 11) | 0x14))
        data = encode("JZ R3, end\nHALT\nend:\nHALT")
        self.assertEqual(data[:2], be(word(isa.OP_JZ, 3, imm=4)))

    def test_case_insensitive_mnemonics_and_registers(self):
        self.assertEqual(encode("add r1, r2"), encode("ADD R1, R2"))

    def test_rejected_operands(self):
        bad = [
            "MOV R8, R0", "MOV R0, R8", "MOV R0, X", "INC R8", "ST R0, R9",
            "LDI R0, 65536", "LDI R0, -1", "LDI_H R0, 256", "LDWS R0, 256",
            "STWS R0, -1", "ADJSP 128", "ADJSP -129", "ADJSP x",
            "JMP 0x800", "CALL 0x800", "BZ 0x800", "BC 0x800", "JMP -1",
            "JZ R0, 256", "JZ R8, 0", "JMPX 0x80000", "CALLX -1", "BZX 0x80000",
            "BCX 0x80000", "LDA R0, 0x80000", "FOO R0", "HALT R0", "MOV R0",
            "ADD R0", "JMP", "RET R0", "PUSH", "FADD R0", "ITOF", "MOVSP",
        ]
        for text in bad:
            with self.subTest(text=text):
                with self.assertRaises(AssemblyError):
                    assemble(text)


class CpuExecutionTests(unittest.TestCase):
    def run_one(self, instruction, registers=None, **kwargs):
        cpu = make_cpu(be(*instruction) if isinstance(instruction, tuple) else be(instruction),
                       registers or distinct_registers(), **kwargs)
        cpu.step()
        return cpu

    def test_halt(self):
        cpu = self.run_one(0)
        self.assertFalse(cpu.running)
        self.assertEqual(cpu.pc, CODE + 2)

    def test_step_does_nothing_when_stopped(self):
        cpu = make_cpu(be(word(isa.OP_INC, 1)))
        cpu.running = False
        cpu.step()
        self.assertEqual((cpu.pc, cpu.registers[1]), (CODE, 0))

    def test_ldi_and_ldi_h(self):
        for rd, value in itertools.product(REGS, EDGE_BYTES):
            with self.subTest(rd=rd, value=value):
                cpu = self.run_one(word(isa.OP_LDI, rd, imm=value))
                expected = distinct_registers()
                expected[rd] = value
                self.assertEqual(cpu.registers, expected)
                cpu = self.run_one(word(isa.OP_LDI_H, rd, imm=value))
                expected = distinct_registers()
                expected[rd] = (value << 8) | expected[rd]
                self.assertEqual(cpu.registers, expected)

    def test_wide_ldi_sequence_every_register(self):
        for rd in REGS:
            for value in (0x100, 0x1234, 0xABCD, 0xFFFF):
                with self.subTest(rd=rd, value=value):
                    cpu = make_cpu(encode(f"LDI R{rd}, {value}") + be(0))
                    cpu.step()
                    cpu.step()
                    self.assertEqual(cpu.registers[rd], value)

    def test_mov_every_combination(self):
        for rd, rs in itertools.product(REGS, REGS):
            with self.subTest(rd=rd, rs=rs):
                cpu = self.run_one(word(isa.OP_MOV, rd, rs))
                expected = distinct_registers()
                expected[rd] = expected[rs]
                self.assertEqual(cpu.registers, expected)

    def test_logic_every_combination(self):
        ops = {
            isa.OP_AND: lambda a, b: a & b,
            isa.OP_OR: lambda a, b: a | b,
            isa.OP_XOR: lambda a, b: a ^ b,
        }
        for opcode, fn in ops.items():
            for rd, rs1, rs2 in itertools.product(REGS, REGS, REGS):
                with self.subTest(opcode=opcode, rd=rd, rs1=rs1, rs2=rs2):
                    regs = distinct_registers()
                    cpu = self.run_one(word(opcode, rd, rs1, rs2), regs, flags={"Z": 1, "C": 1})
                    expected = list(regs)
                    expected[rd] = fn(regs[rs1], regs[rs2])
                    self.assertEqual(cpu.registers, expected)
                    self.assertEqual(cpu.flags, {"Z": 1, "C": 1})

    def test_not_every_combination(self):
        for rd, rs in itertools.product(REGS, REGS):
            for value in EDGE_BYTES:
                with self.subTest(rd=rd, rs=rs, value=value):
                    regs = distinct_registers()
                    regs[rs] = value
                    cpu = self.run_one(word(isa.OP_NOT, rd, rs), regs)
                    expected = list(regs)
                    expected[rd] = (~value) & 0xFF
                    self.assertEqual(cpu.registers, expected)

    def test_inc_dec_every_register(self):
        for rd, value in itertools.product(REGS, EDGE_BYTES):
            for opcode, delta in ((isa.OP_INC, 1), (isa.OP_DEC, -1)):
                with self.subTest(rd=rd, value=value, delta=delta):
                    regs = distinct_registers()
                    regs[rd] = value
                    cpu = self.run_one(word(opcode, rd), regs, flags={"Z": 1, "C": 1})
                    expected = list(regs)
                    expected[rd] = value + delta
                    self.assertEqual(cpu.registers, expected)
                    self.assertEqual(cpu.flags, {"Z": 1, "C": 1})

    def test_add_sub_cmp_every_register_combination_and_flags(self):
        for opcode in (isa.OP_ADD, isa.OP_SUB, isa.OP_CMP):
            for rd, rs1, rs2 in itertools.product(REGS, REGS, REGS):
                for a, b in ((0x12, 0x34), (0x80, 0x80)):
                    with self.subTest(opcode=opcode, rd=rd, rs1=rs1, rs2=rs2, a=a, b=b):
                        regs = [0x55] * 8
                        regs[rs1] = a
                        regs[rs2] = b if rs2 != rs1 else a
                        left, right = regs[rs1], regs[rs2]
                        cpu = self.run_one(word(opcode, rd, rs1, rs2), regs)
                        result = left + right if opcode == isa.OP_ADD else left - right
                        expected = list(regs)
                        if opcode != isa.OP_CMP:
                            expected[rd] = result
                        self.assertEqual(cpu.registers, expected)
                        self.assertEqual(cpu.flags["Z"], int(
                            (result if opcode != isa.OP_CMP else left - right) == 0))
                        carry = result > 0xFF if opcode == isa.OP_ADD else result < 0
                        self.assertEqual(cpu.flags["C"], int(carry))

    def test_add_sub_cmp_edge_value_flags(self):
        for opcode in (isa.OP_ADD, isa.OP_SUB, isa.OP_CMP):
            for a, b in EDGE_PAIRS:
                with self.subTest(opcode=opcode, a=a, b=b):
                    regs = [0] * 8
                    regs[1], regs[2] = a, b
                    cpu = self.run_one(word(opcode, 3, 1, 2), regs)
                    result = a + b if opcode == isa.OP_ADD else a - b
                    self.assertEqual(cpu.flags["Z"], int(result == 0))
                    self.assertEqual(cpu.flags["C"], int(result > 0xFF if opcode == isa.OP_ADD else result < 0))
                    self.assertEqual(cpu.registers[3], 0 if opcode == isa.OP_CMP else result)

    def test_shr_every_register_and_value(self):
        for rd, value in itertools.product(REGS, (0, 1, 2, 0x7F, 0x80, 0xFF, 0x100, 0x8001, 0xFFFF)):
            with self.subTest(rd=rd, value=value):
                regs = distinct_registers()
                regs[rd] = value
                cpu = self.run_one(word(isa.OP_SHR, rd), regs)
                expected = list(regs)
                expected[rd] = value >> 1
                self.assertEqual(cpu.registers, expected)

    def test_movsp_every_register(self):
        for rd, sp in itertools.product(REGS, (0x2FFFF, 0x1234, 0)):
            with self.subTest(rd=rd, sp=sp):
                cpu = self.run_one(word(isa.OP_SHR, rd, 0, 7), sp=sp)
                self.assertEqual(cpu.registers[rd], sp)

    def test_ld_st_every_combination(self):
        for rd, rs in itertools.product(REGS, REGS):
            with self.subTest(rd=rd, rs=rs):
                regs = [0x40 + index for index in REGS]
                cpu = make_cpu(be(word(isa.OP_LD, rd, rs)), regs)
                cpu.memory[regs[rs]] = 0xA5
                cpu.step()
                expected = list(regs)
                expected[rd] = 0xA5
                self.assertEqual(cpu.registers, expected)

                # In ST, rd is the value register and rs1 the address register.
                cpu = make_cpu(be(word(isa.OP_ST, rd, rs)), regs)
                cpu.step()
                self.assertEqual(cpu.memory[regs[rs]], regs[rd])

    def test_st_masks_value_to_a_byte_and_ld_high_addresses(self):
        regs = [0] * 8
        regs[0], regs[1] = 0x1ABC, 0x400
        cpu = make_cpu(be(word(isa.OP_ST, 0, 1)), regs)
        cpu.step()
        self.assertEqual(cpu.memory[0x400], 0xBC)

    def test_ldw_stw_every_combination(self):
        for rd, rs in itertools.product(REGS, REGS):
            with self.subTest(rd=rd, rs=rs):
                regs = [0x200 + 4 * index for index in REGS]
                cpu = make_cpu(be(word(isa.OP_LDW, rd, rs)), regs)
                cpu.memory[regs[rs]:regs[rs] + 2] = b"\x12\x34"
                cpu.step()
                self.assertEqual(cpu.registers[rd], 0x1234)

                cpu = make_cpu(be(word(isa.OP_STW, rd, rs)), regs)
                cpu.step()
                value = regs[rd] & 0xFFFF
                self.assertEqual(bytes(cpu.memory[regs[rs]:regs[rs] + 2]), value.to_bytes(2, "big"))

    def test_ldws_stws_every_register_and_offset(self):
        sp = 0x20000
        for rd, offset in itertools.product(REGS, EDGE_BYTES):
            with self.subTest(rd=rd, offset=offset):
                regs = distinct_registers()
                cpu = make_cpu(be(word(isa.OP_LDWS, rd, imm=offset)), regs, sp=sp)
                cpu.memory[sp + offset:sp + offset + 2] = b"\xBE\xEF"
                cpu.step()
                self.assertEqual(cpu.registers[rd], 0xBEEF)

                cpu = make_cpu(be(word(isa.OP_STWS, rd, imm=offset)), regs, sp=sp)
                cpu.step()
                self.assertEqual(bytes(cpu.memory[sp + offset:sp + offset + 2]), regs[rd].to_bytes(2, "big"))

    def test_adjsp_every_signed_offset(self):
        for offset in range(-128, 128):
            with self.subTest(offset=offset):
                cpu = self.run_one(word(isa.OP_ADJSP, imm=offset & 0xFF), sp=0x20000)
                self.assertEqual(cpu.sp, 0x20000 + offset)

    def test_adjsp_wraps_within_memory(self):
        cpu = self.run_one(word(isa.OP_ADJSP, imm=0xFF), sp=0)
        self.assertEqual(cpu.sp, MEM_SIZE - 1)
        cpu = self.run_one(word(isa.OP_ADJSP, imm=0x01), sp=MEM_SIZE - 1)
        self.assertEqual(cpu.sp, 0)

    def test_push_pop_every_register(self):
        for rd, other in itertools.product(REGS, REGS):
            with self.subTest(rd=rd, other=other):
                regs = distinct_registers()
                cpu = make_cpu(be(word(isa.OP_PUSH, rd), word(isa.OP_POP, other)), regs)
                cpu.step()
                self.assertEqual(cpu.sp, 0x2FFFD)
                self.assertEqual(bytes(cpu.memory[0x2FFFD:0x2FFFF]), regs[rd].to_bytes(2, "big"))
                cpu.step()
                self.assertEqual(cpu.sp, 0x2FFFF)
                self.assertEqual(cpu.registers[other], regs[rd])

    def test_pop_masks_to_a_byte(self):
        cpu = make_cpu(be(word(isa.OP_POP, 2)), sp=0x1000)
        cpu.memory[0x1000:0x1002] = b"\x12\x34"
        cpu.step()
        self.assertEqual(cpu.registers[2], 0x34)

    def test_jmp_every_target_boundary(self):
        for target in (0, 2, 0xFF, 0x100, 0x7FE, 0x7FF):
            with self.subTest(target=target):
                cpu = self.run_one((isa.OP_JMP << 11) | target)
                self.assertEqual(cpu.pc, target)

    def test_jz_every_register_taken_and_not_taken(self):
        for rd, target in itertools.product(REGS, (0, 0x40, 0xFF)):
            for value, taken in ((0, True), (1, False), (0xFF, False)):
                with self.subTest(rd=rd, target=target, value=value):
                    regs = distinct_registers()
                    regs[rd] = value
                    cpu = self.run_one(word(isa.OP_JZ, rd, imm=target), regs)
                    self.assertEqual(cpu.pc, target if taken else CODE + 2)

    def test_conditional_branches_taken_and_not_taken(self):
        for opcode, flag in ((isa.OP_BZ, "Z"), (isa.OP_BC, "C")):
            for z, c in itertools.product((0, 1), repeat=2):
                for target in (0, 0x7FF):
                    with self.subTest(opcode=opcode, z=z, c=c, target=target):
                        cpu = self.run_one((opcode << 11) | target, flags={"Z": z, "C": c})
                        taken = {"Z": z, "C": c}[flag]
                        self.assertEqual(cpu.pc, target if taken else CODE + 2)
                        self.assertEqual(cpu.flags, {"Z": z, "C": c})

    def test_call_and_ret(self):
        for target in (0, 0x200, 0x7FF):
            with self.subTest(target=target):
                cpu = self.run_one((isa.OP_CALL << 11) | target)
                self.assertEqual(cpu.pc, target)
                self.assertEqual(cpu.sp, 0x2FFFD)
                self.assertEqual(cpu.read_16(cpu.sp), CODE + 2)
                cpu.memory[target:target + 2] = be(isa.OP_RET << 11)
                cpu.step()
                self.assertEqual((cpu.pc, cpu.sp), (CODE + 2, 0x2FFFF))

    def test_far_control_taken_and_not_taken(self):
        sentinels = {"JMPX": 0xF81D, "CALLX": 0xF91D, "BZX": 0xFA1D, "BCX": 0xFB1D}
        for name, sentinel in sentinels.items():
            for z, c in itertools.product((0, 1), repeat=2):
                for target in (0, 0x12345, MEM_SIZE - 1):
                    with self.subTest(name=name, z=z, c=c, target=target):
                        program = be(sentinel, target & 0xFFFF, target >> 16)
                        cpu = make_cpu(program, flags={"Z": z, "C": c})
                        cpu.step()
                        taken = {"JMPX": True, "CALLX": True, "BZX": z, "BCX": c}[name]
                        self.assertEqual(cpu.pc, target if taken else CODE + 6)
                        if name == "CALLX":
                            self.assertEqual(cpu.sp, 0x2FFFD)
                            self.assertEqual(cpu.read_16(cpu.sp), CODE + 6)
                        else:
                            self.assertEqual(cpu.sp, 0x2FFFF)

    def test_far_return_via_ret_truncates_to_16_bit_pc(self):
        cpu = make_cpu(encode("CALLX 0x20000"))
        cpu.step()
        self.assertEqual(cpu.pc, 0x20000)

    def test_ei_di_toggle_only_enable_bit(self):
        for initial in range(256):
            with self.subTest(initial=initial):
                cpu = make_cpu(be(isa.OP_EI << 11, isa.OP_DI << 11))
                cpu.memory[ICR_ADDR] = initial & ~0x0A
                cpu.step()
                self.assertEqual(cpu.memory[ICR_ADDR], (initial & ~0x0A) | 0x01)
                cpu.step()
                self.assertEqual(cpu.memory[ICR_ADDR], (initial & ~0x0A) & ~0x01 & 0xFF)

    def test_interrupt_entry_and_rti_for_both_sources(self):
        for pending, vector in ((0x02, 0x0002), (0x08, 0x0004)):
            for z in (0, 1):
                with self.subTest(pending=pending, z=z):
                    cpu = make_cpu(be(isa.OP_EI << 11), flags={"Z": z})
                    cpu.memory[vector:vector + 2] = (0x300).to_bytes(2, "big")
                    cpu.memory[0x300:0x302] = be(isa.OP_RTI << 11)
                    cpu.memory[ICR_ADDR] = pending
                    cpu.step()
                    self.assertEqual(cpu.pc, 0x300)
                    self.assertEqual(cpu.sp, 0x2FFFF - 4)
                    self.assertEqual(cpu.read_16(cpu.sp), z)
                    self.assertEqual(cpu.read_16(cpu.sp + 2), CODE + 2)
                    self.assertEqual(cpu.memory[ICR_ADDR] & 0x0B, 0)
                    cpu.flags["Z"] = 1 - z
                    cpu.step()
                    self.assertEqual((cpu.pc, cpu.sp, cpu.flags["Z"]), (CODE + 2, 0x2FFFF, z))

    def test_interrupt_ignored_while_disabled(self):
        cpu = make_cpu(be(word(isa.OP_INC, 0)))
        cpu.memory[ICR_ADDR] = 0x0A
        cpu.step()
        self.assertEqual(cpu.pc, CODE + 2)
        self.assertEqual(cpu.memory[ICR_ADDR], 0x0A)

    def test_keyboard_interrupt_has_priority_over_rtc(self):
        cpu = make_cpu(be(isa.OP_EI << 11))
        cpu.memory[0x0002:0x0004] = (0x300).to_bytes(2, "big")
        cpu.memory[0x0004:0x0006] = (0x400).to_bytes(2, "big")
        cpu.memory[ICR_ADDR] = 0x0A
        cpu.step()
        self.assertEqual(cpu.pc, 0x300)


class FloatExecutionTests(unittest.TestCase):
    VALUES = (0.0, 1.0, -1.0, 2.5, -3.25, 100.0, 0.5)

    def run_one(self, instruction, registers):
        cpu = make_cpu(be(instruction), registers)
        cpu.step()
        return cpu

    def test_binary_float_ops_every_register_pair(self):
        ops = {
            1: lambda a, b: a + b,
            2: lambda a, b: a - b,
            3: lambda a, b: a * b,
            4: lambda a, b: a / b,
        }
        for ext, fn in ops.items():
            for rd, rs in itertools.product(REGS, REGS):
                with self.subTest(ext=ext, rd=rd, rs=rs):
                    regs = [bits(self.VALUES[index % len(self.VALUES)] + 0.0) for index in REGS]
                    regs[rd], regs[rs] = bits(6.0), bits(1.5) if rd != rs else bits(6.0)
                    cpu = self.run_one(word(isa.OP_SHR, rd, ext, rs), regs)
                    left, right = half(regs[rd]), half(regs[rs])
                    expected = list(regs)
                    expected[rd] = bits(fn(left, right))
                    self.assertEqual(cpu.registers, expected)

    def test_fdiv_by_zero_cases(self):
        zero, pos, neg = bits(0.0), bits(2.0), bits(-2.0)
        cases = ((pos, zero, 0x7C00), (neg, zero, 0xFC00), (pos, bits(-0.0), 0xFC00))
        for left, right, expected in cases:
            with self.subTest(left=left, right=right):
                regs = [0] * 8
                regs[1], regs[2] = left, right
                cpu = self.run_one(word(isa.OP_SHR, 1, 4, 2), regs)
                self.assertEqual(cpu.registers[1], expected)
        regs = [0] * 8
        cpu = self.run_one(word(isa.OP_SHR, 1, 4, 2), regs)
        self.assertTrue(math.isnan(half(cpu.registers[1])))

    def test_float_overflow_saturates_to_infinity(self):
        regs = [0] * 8
        regs[1] = regs[2] = bits(60000.0)
        cpu = self.run_one(word(isa.OP_SHR, 1, 3, 2), regs)
        self.assertEqual(cpu.registers[1], 0x7C00)
        regs[2] = bits(-60000.0)
        cpu = self.run_one(word(isa.OP_SHR, 1, 3, 2), regs)
        self.assertEqual(cpu.registers[1], 0xFC00)

    def test_fcmp_every_register_pair_and_ordering(self):
        for rd, rs in itertools.product(REGS, REGS):
            for a, b in itertools.product((-2.0, 0.0, 1.5), repeat=2):
                with self.subTest(rd=rd, rs=rs, a=a, b=b):
                    regs = [0] * 8
                    regs[rd] = bits(a)
                    regs[rs] = bits(b) if rs != rd else bits(a)
                    left, right = half(regs[rd]), half(regs[rs])
                    cpu = self.run_one(word(isa.OP_SHR, rd, 5, rs), regs)
                    self.assertEqual(cpu.registers, regs)
                    self.assertEqual(cpu.flags, {"Z": int(left == right), "C": int(left < right)})

    def test_itof_every_register_and_value(self):
        for rd, value in itertools.product(REGS, (0, 1, 7, 255, 2048, 0xFFFF, 0x8000)):
            with self.subTest(rd=rd, value=value):
                regs = distinct_registers()
                regs[rd] = value
                cpu = self.run_one(word(isa.OP_SHR, rd, 6), regs)
                signed = value - 0x10000 if value & 0x8000 else value
                self.assertEqual(cpu.registers[rd], bits(float(signed)))

    def test_ftoi_every_register_and_value(self):
        for rd, value in itertools.product(REGS, (0.0, 1.0, 1.9, -1.9, 100.0, -100.0, 2.5)):
            with self.subTest(rd=rd, value=value):
                regs = distinct_registers()
                regs[rd] = bits(value)
                cpu = self.run_one(word(isa.OP_SHR, rd, 7), regs)
                self.assertEqual(cpu.registers[rd], int(half(regs[rd])) & 0xFFFF)


class AssembledProgramTests(unittest.TestCase):
    """Assembler output executed on the CPU for instructions with labels."""

    def run_source(self, source, steps=200):
        cpu = make_cpu(b"")
        image = assemble(source)
        cpu.load_program(image)
        cpu.pc = image.entry_point
        for _ in range(steps):
            if not cpu.running:
                break
            cpu.step()
        return cpu

    def test_every_register_can_be_loaded_pushed_and_popped(self):
        lines = [f"LDI R{r}, {r + 10}" for r in REGS]
        lines += [f"PUSH R{r}" for r in REGS]
        lines += [f"POP R{r}" for r in reversed(REGS)]
        lines.append("HALT")
        cpu = self.run_source("\n".join(lines))
        self.assertEqual(cpu.registers, [r + 10 for r in REGS])
        self.assertEqual(cpu.sp, 0x2FFFF)

    def test_loop_with_jz_cmp_and_bz(self):
        cpu = self.run_source(
            """
            LDI R0, 5
            LDI R1, 0
            loop:
                JZ R0, done
                INC R1
                DEC R0
                JMP loop
            done:
                LDI R2, 5
                CMP R1, R2
                BZ equal
                LDI R3, 1
                HALT
            equal:
                LDI R3, 2
                HALT
            """
        )
        self.assertEqual(cpu.registers[3], 2)

    def test_call_ret_and_bc(self):
        cpu = self.run_source(
            """
            LDI R0, 3
            LDI R1, 5
            CMP R0, R1
            BC less
            LDI R2, 1
            HALT
            less:
                CALL sub
                HALT
            sub:
                LDI R2, 9
                RET
            """
        )
        self.assertEqual(cpu.registers[2], 9)


if __name__ == "__main__":
    unittest.main()
