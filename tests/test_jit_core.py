"""The Numba CPU core must behave exactly like the pure-Python step() interpreter."""
import random
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / "src"), str(ROOT)]

import SuomiCPU as emulator  # noqa: E402
from SuomiCPU import (  # noqa: E402
    CONSOLE_CONTROL,
    CONSOLE_DATA,
    CONSOLE_STATUS,
    DISK_BASE,
    GPU_BASE,
    ICR_ADDR,
    MEM_SIZE,
    SuomiCompute16,
)
from tests.gfx_harness import make_cpu  # noqa: E402
from tools.assembler import AssemblyImage, MemorySegment, assemble  # noqa: E402

try:
    import sc16_jit  # noqa: F401
    HAVE_JIT = True
except ImportError:
    HAVE_JIT = False

PREFIXES = (0xF81E, 0xF81F, 0xF80D, 0xF83E, 0xF81C)


def random_cpu(seed, use_jit):
    rng = random.Random(seed)
    code = bytearray()
    common = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 13, 14, 15, 16, 19, 20, 21, 24, 25, 26, 27, 28, 29, 30, 12, 17, 18]
    for _ in range(3000):
        kind = rng.random()
        if kind < 0.30:
            word = rng.choice(PREFIXES) | (rng.randrange(8) << 8) | (rng.randrange(8) << 5 if rng.random() < 0.5 else 0)
        elif kind < 0.33:
            word = 0xF800 | rng.randrange(0x800)
        elif kind < 0.35:
            word = rng.choice((11, 22, 23)) << 11 | rng.randrange(0x800)
        else:
            word = rng.choice(common) << 11 | rng.randrange(0x800)
        code += word.to_bytes(2, "big")
    code = bytes(code) * (0x40000 // len(code) + 1)  # random jumps still land on valid code
    cpu = make_cpu(AssemblyImage((MemorySegment(0x0, code[:0x40000]),), 0x40))
    cpu.use_jit = use_jit
    cpu.gpu_log = []
    cpu.gpu_command = lambda command, log=cpu.gpu_log: log.append(command)
    cpu.registers = [rng.choice((0, 1, 0x7FFF, 0x8000, 0xFFFF, rng.randrange(0x10000), 0x40, 0x2FF00)) for _ in range(8)]
    cpu.flags = {"Z": rng.randrange(2), "C": rng.randrange(2), "N": rng.randrange(2), "V": rng.randrange(2)}
    cpu.sp = rng.choice((0x2FFFF, 0x2F000, 0x43020, 0x4301E))
    cpu.memory[0x20000:0x20400] = bytes(rng.randrange(256) for _ in range(0x400))
    cpu.memory[ICR_ADDR] = rng.choice((0, 1, 1 | 2, 1 | 8))
    cpu.memory[0x0002:0x0006] = bytes((0x01, 0x00, 0x01, 0x80))
    cpu.frame_yield = False
    return cpu


def state(cpu):
    return (bytes(cpu.memory), list(cpu.registers), dict(cpu.flags), cpu.pc, cpu.sp, cpu.running,
            list(cpu.gpu_log))


def reference_run(cpu, count):
    for _ in range(count):
        if not cpu.running or cpu.frame_yield:
            break
        cpu.step()


@unittest.skipUnless(HAVE_JIT, "numba is not installed")
class JitEquivalenceTests(unittest.TestCase):
    def test_console_mmio_reads_and_writes_fall_back_to_interpreter(self):
        input_program = assemble("LD R0, R1\nHALT")
        slow = make_cpu(input_program)
        fast = make_cpu(input_program)
        slow.use_jit = False
        fast.use_jit = True
        for cpu in (slow, fast):
            cpu.registers[1] = CONSOLE_DATA
            cpu._console_device().receive(b"A")

        slow.step()
        fast._execute_jit(1)

        self.assertEqual(slow.registers, fast.registers)
        self.assertEqual(slow._console_device().status, fast._console_device().status)
        self.assertEqual(slow.console_device.output, fast.console_device.output)

        output_program = assemble("ST R1, R0\nHALT")
        slow = make_cpu(output_program)
        fast = make_cpu(output_program)
        slow.use_jit = False
        fast.use_jit = True
        for cpu in (slow, fast):
            cpu.registers[0] = ord("Z")
            cpu.registers[1] = CONSOLE_DATA

        slow.step()
        fast._execute_jit(1)

        self.assertEqual(slow._console_device().output, b"Z")
        self.assertEqual(slow._console_device().output, fast._console_device().output)

    def test_32_bit_pair_instructions_fall_back_to_interpreter(self):
        program = assemble(
            "LDI R0, 0xFFFF\nLDI R1, 0xFFFF\n"
            "LDI R2, 0\nLDI R3, 1\nADD32 R0, R2\n"
            "LDI R4, 0x8000\nLDI R5, 0\nNEG32 R4\n"
            "LDI R6, 0\nLDI R7, 3\nSHL32 R6, 1\n"
            "LDI R2, 0\nLDI R3, 0x100\nST32 R0, R2\nLD32 R4, R2\nHALT"
        )
        slow = make_cpu(program)
        fast = make_cpu(program)
        slow.use_jit = False
        fast.use_jit = True
        slow.gpu_log = []
        fast.gpu_log = []
        slow.flags = {"Z": 0, "C": 0, "N": 0, "V": 0}
        fast.flags = {"Z": 0, "C": 0, "N": 0, "V": 0}
        slow.gpu_command = lambda command: slow.gpu_log.append(command)
        fast.gpu_command = lambda command: fast.gpu_log.append(command)
        reference_run(slow, 100)
        fast._execute_jit(100)
        self.assertEqual(state(slow), state(fast))

    def test_19_bit_immediate_load_falls_back_to_interpreter(self):
        program = assemble("LDA R0, 0x51234\nHALT")
        slow = make_cpu(program)
        fast = make_cpu(program)
        slow.use_jit = False
        fast.use_jit = True
        slow.gpu_log = []
        fast.gpu_log = []
        slow.flags = {"Z": 0, "C": 0, "N": 0, "V": 0}
        fast.flags = {"Z": 0, "C": 0, "N": 0, "V": 0}
        slow.step()
        fast._execute_jit(1)

        self.assertEqual(state(slow), state(fast))
        self.assertEqual(fast.registers[0], 0x51234)

    def test_stack_pointer_high_bits_instruction_matches_interpreter(self):
        program = assemble("LDI R0, 5\nSETSPH R0\nHALT")
        slow = make_cpu(program)
        fast = make_cpu(program)
        slow.use_jit = False
        fast.use_jit = True
        slow.gpu_log = []
        fast.gpu_log = []
        slow.flags = {"Z": 0, "C": 0, "N": 0, "V": 0}
        fast.flags = {"Z": 0, "C": 0, "N": 0, "V": 0}

        reference_run(slow, 10)
        fast._execute_jit(10)

        self.assertEqual(state(slow), state(fast))
        self.assertEqual(fast.sp, 0x5FFFF)

    def test_19_bit_return_addresses_match_for_calls_above_0xffff(self):
        program = assemble(
            ".address 0x20000\n"
            "entry: CALLX far_routine\n"
            "LDI R0, 0x400\n"
            "LDI R1, 0\n"
            "CALLRP R0, R1\n"
            "HALT\n"
            ".address 0x30000\n"
            "far_routine: RET\n"
            ".address 0x400\n"
            "indirect_routine: RET\n"
        )
        slow = make_cpu(program)
        fast = make_cpu(program)
        slow.use_jit = False
        fast.use_jit = True
        slow.gpu_log = []
        fast.gpu_log = []
        slow.flags = {"Z": 0, "C": 0, "N": 0, "V": 0}
        fast.flags = {"Z": 0, "C": 0, "N": 0, "V": 0}

        reference_run(slow, 20)
        fast._execute_jit(20)

        self.assertEqual(state(slow), state(fast))
        self.assertEqual(slow.pc, 0x20010)
        self.assertEqual(slow.sp, 0x2FFFF)

    def test_19_bit_indirect_jump_matches_interpreter(self):
        program = assemble(
            ".address 0x20000\n"
            "LDI R0, 0\n"
            "LDI R1, 1\n"
            "JMPRP R0, R1\n"
            ".address 0x10000\n"
            "HALT\n"
        )
        slow = make_cpu(program)
        fast = make_cpu(program)
        slow.use_jit = False
        fast.use_jit = True
        slow.flags = {"Z": 0, "C": 0, "N": 0, "V": 0}
        fast.flags = {"Z": 0, "C": 0, "N": 0, "V": 0}
        slow.gpu_log = []
        fast.gpu_log = []

        reference_run(slow, 10)
        fast._execute_jit(10)

        self.assertEqual(state(slow), state(fast))
        self.assertEqual(slow.pc, 0x10002)

    def test_random_programs_match_step(self):
        compared = 0
        for seed in range(250):
            slow = random_cpu(seed, False)
            fast = random_cpu(seed, True)
            for _ in range(2):
                try:
                    reference_run(slow, 2500)
                except (ValueError, OverflowError):
                    break  # pre-existing float-conversion error in step(); not comparable further
                compared += 1
                fast._execute_jit(2500)
                self.assertTrue(state(slow) == state(fast), f"seed {seed} diverged")
        self.assertGreater(compared, 200)
    def test_constants_match(self):
        self.assertEqual(
            sc16_jit.CONSTANTS,
            (
                emulator.MEM_MASK,
                GPU_BASE,
                ICR_ADDR,
                DISK_BASE,
                CONSOLE_DATA,
                CONSOLE_STATUS,
                CONSOLE_CONTROL,
            ),
        )
        self.assertEqual(MEM_SIZE - 1, emulator.MEM_MASK)


if __name__ == "__main__":
    unittest.main()
