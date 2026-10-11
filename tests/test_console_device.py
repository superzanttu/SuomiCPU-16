import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / "src"), str(ROOT)]

from SuomiCPU import CONSOLE_CONTROL, CONSOLE_DATA, CONSOLE_STATUS, MEM_SIZE, SuomiCompute16  # noqa: E402


class ConsoleDeviceTests(unittest.TestCase):
    def make_cpu(self):
        cpu = SuomiCompute16.__new__(SuomiCompute16)
        cpu.memory = bytearray(MEM_SIZE)
        return cpu

    def test_console_fifo_status_and_echo(self):
        cpu = self.make_cpu()
        cpu.write(CONSOLE_CONTROL, 0x03)
        cpu.console_device.receive(b"A")

        self.assertEqual(cpu.read(CONSOLE_STATUS) & 1, 1)
        self.assertEqual(cpu.read(CONSOLE_DATA), ord("A"))
        self.assertEqual(cpu.read(CONSOLE_STATUS) & 1, 0)
        self.assertEqual(cpu.console_device.output, b"A")

    def test_console_input_can_disable_echo(self):
        cpu = self.make_cpu()
        cpu.write(CONSOLE_CONTROL, 0x02)
        cpu.console_device.receive(b"B")

        self.assertEqual(cpu.read(CONSOLE_DATA), ord("B"))
        self.assertEqual(cpu.console_device.output, b"")

    def test_empty_input_poll_yields_to_host_event_loop(self):
        cpu = self.make_cpu()
        cpu.running = True
        cpu.frame_yield = False

        self.assertFalse(cpu.read(CONSOLE_STATUS) & 1)
        self.assertTrue(cpu.frame_yield)

    def test_console_input_fifo_reports_overrun(self):
        cpu = self.make_cpu()
        console = cpu._console_device()
        console.receive(b"x" * (console.RX_CAPACITY + 1))

        self.assertTrue(cpu.read(CONSOLE_STATUS) & 0x04)
        cpu.write(CONSOLE_STATUS, 0x04)
        self.assertFalse(cpu.read(CONSOLE_STATUS) & 0x04)


if __name__ == "__main__":
    unittest.main()
