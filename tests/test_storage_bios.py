import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / "src"), str(ROOT)]

from SuomiCPU import (  # noqa: E402
    BIOS_BASE,
    DISK_BASE,
    RAM_START,
    SuomiCompute16,
    create_memory,
)
from src.bios import BIOSError, initialize_bios, mount_data_disk  # noqa: E402
from src.lib_disk import DiskDevice, DiskDeviceError  # noqa: E402


class StorageBIOSIntegrationTests(unittest.TestCase):
    def test_module_file_api_reads_chunks_and_lists_only_its_module_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            module_disks = root / "modules" / "tank16" / "disks"
            other_disks = root / "modules" / "other" / "disks"
            (module_disks / "maps").mkdir(parents=True)
            other_disks.mkdir(parents=True)
            payload = bytes(range(256)) * 300
            (module_disks / "maps" / "world.json").write_bytes(payload)
            (other_disks / "secret.json").write_text("hidden", encoding="ascii")
            cpu = SuomiCompute16.__new__(SuomiCompute16)
            cpu.memory = create_memory()
            cpu.disk = DiskDevice(root / "virtual-disks")
            cpu.disk.set_file_root(module_disks)

            path_offset = 0x100
            buffer_offset = 0x200
            path_address = RAM_START + path_offset
            buffer_address = RAM_START + buffer_offset
            for index, value in enumerate(b"maps/world.json\0"):
                cpu.write(path_address + index, value)
            cpu.memory[DISK_BASE + 22:DISK_BASE + 26] = path_address.to_bytes(
                4, "big"
            )
            cpu.memory[DISK_BASE + 26:DISK_BASE + 30] = buffer_address.to_bytes(
                4, "big"
            )
            cpu.write_16(DISK_BASE + 30, 32)
            cpu.memory[DISK_BASE + 18:DISK_BASE + 22] = (65530).to_bytes(
                4, "big"
            )
            cpu.write(DISK_BASE, 0x40)

            self.assertEqual(cpu.read_16(DISK_BASE + 14), 32)
            self.assertEqual(
                bytes(cpu.memory[RAM_START + buffer_offset:
                                 RAM_START + buffer_offset + 32]),
                payload[65530:65562],
            )
            self.assertEqual(
                int.from_bytes(cpu.memory[DISK_BASE + 32:DISK_BASE + 36], "big"),
                len(payload),
            )

            output_address = RAM_START + 0x400
            cpu.write_16(DISK_BASE + 10, 0)
            cpu.memory[DISK_BASE + 26:DISK_BASE + 30] = output_address.to_bytes(
                4, "big"
            )
            cpu.write_16(DISK_BASE + 30, 129)
            cpu.write(DISK_BASE, 0x80)
            self.assertEqual(cpu.read_16(DISK_BASE + 14), 1)
            self.assertEqual(
                bytes(cpu.memory[output_address:output_address + 16]),
                b"maps/world.json\0",
            )

    def test_module_file_api_rejects_paths_outside_its_sandbox(self):
        with tempfile.TemporaryDirectory() as directory:
            module_disks = Path(directory) / "disks"
            module_disks.mkdir()
            disk = DiskDevice(Path(directory) / "virtual-disks")
            disk.set_file_root(module_disks)
            with self.assertRaises(DiskDeviceError):
                disk.read_file("../outside.json", 0, 16)

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
