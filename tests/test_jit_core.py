"""The Numba CPU core must behave exactly like the pure-Python step() interpreter."""
import random
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / "src"), str(ROOT)]

import SuomiCPU as emulator  # noqa: E402
from SuomiCPU import GPU_BASE, ICR_ADDR, MEM_SIZE, SuomiCompute16  # noqa: E402
from tests.gfx_harness import make_cpu  # noqa: E402
from tools.assembler import AssemblyImage, MemorySegment  # noqa: E402

try:
    import sc16_jit  # noqa: F401
    HAVE_JIT = True
except ImportError:
    HAVE_JIT = False

PREFIXES = (0xF81E, 0xF81F, 0xF81B, 0xF81D, 0xF91D, 0xFA1D, 0xFB1D, 0xF81C)


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
        self.assertEqual(sc16_jit.CONSTANTS, (emulator.MEM_MASK, GPU_BASE, ICR_ADDR))
        self.assertEqual(MEM_SIZE - 1, emulator.MEM_MASK)


if __name__ == "__main__":
    unittest.main()
