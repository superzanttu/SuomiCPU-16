import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / "src"), str(ROOT)]

from SuomiCPU import BIOS_BASE, DISK_BASE, SuomiCompute16, create_memory  # noqa: E402
from src.bios import BIOSError, initialize_bios, mount_data_disk  # noqa: E402
from src.lib_disk import DiskDevice, DiskDeviceError  # noqa: E402


class StorageBIOSIntegrationTests(unittest.TestCase):
    def test_disk_blocks_persist_as_data_not_programs(self):
        with tempfile.TemporaryDirectory() as directory:
            disk = DiskDevice(directory)
            disk.load_disk("SAVE0001")
            disk.write_block(7, "save data")

            reopened = DiskDevice(directory)
            reopened.load_disk("SAVE0001")
            self.assertEqual(reopened.read_block(7)[:9], "save data")
            self.assertEqual(reopened.read_block(0), " " * 256)

    def test_disk_id_rejects_path_components(self):
        with tempfile.TemporaryDirectory() as directory:
            disk = DiskDevice(directory)
            with self.assertRaises(DiskDeviceError):
                disk.load_disk("../evil")

    def test_bios_selects_data_disk_without_loading_program_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            disk = DiskDevice(directory)
            disk.load_disk("DATA0001")
            disk.write_block(0, "application save")

            cpu = SuomiCompute16.__new__(SuomiCompute16)
            cpu.memory = create_memory()
            cpu.disk = DiskDevice(directory)
            cpu.entry_point = 0x1234
            initialize_bios(cpu)
            mount_data_disk(cpu, "DATA0001")

            self.assertEqual(cpu.memory[0x1234], 0)
            self.assertEqual(cpu.entry_point, 0x1234)
            self.assertEqual(cpu.disk.read_block(0)[:16], "application save")
            self.assertEqual(cpu.memory[BIOS_BASE:BIOS_BASE + 4], b"SC16")

    def test_disk_command_errors_are_reported_in_status_register(self):
        with tempfile.TemporaryDirectory() as directory:
            cpu = SuomiCompute16.__new__(SuomiCompute16)
            cpu.memory = create_memory()
            cpu.disk = DiskDevice(directory)
            cpu.write(DISK_BASE, 0x04)
            self.assertEqual(cpu.read(DISK_BASE + 16), 1)
            self.assertIn(
                "Disk operation failed",
                bytes(cpu._console_device().output).decode(
                    "ascii",
                    errors="replace",
                ),
            )

    def test_bios_reports_missing_data_disk(self):
        with tempfile.TemporaryDirectory() as directory:
            cpu = SuomiCompute16.__new__(SuomiCompute16)
            cpu.memory = create_memory()
            cpu.disk = DiskDevice(directory)
            with self.assertRaises(BIOSError):
                mount_data_disk(cpu, "bad/id!!")


if __name__ == "__main__":
    unittest.main()
