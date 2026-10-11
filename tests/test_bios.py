import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import pygame

from src import bios
from src.lib_disk import DiskDevice
from src.SuomiCPU import main as emulator_main


class BIOSBootTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        pygame.init()
        pygame.display.set_mode((1, 1))

    @classmethod
    def tearDownClass(cls):
        pygame.quit()

    def test_discovers_c_modules_in_filename_order(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "zeta.c").write_text("// Zeta program\n")
            (root / "Alpha.c").write_text("/* Alpha program */\n")
            (root / "ignore.txt").write_text("Not a program\n")

            modules = bios.discover_boot_modules(root)

        self.assertEqual(
            [(module.name, module.description) for module in modules],
            [("Alpha", "Alpha program"), ("zeta", "Zeta program")],
        )

    def test_bios_compiles_selected_module_and_keeps_data_disk_optional(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            modules_dir = root / "modules"
            modules_dir.mkdir()
            module_path = modules_dir / "sample.c"
            module_path.write_text(
                "// BIOS program\nint main(void) { return 0; }\n",
                encoding="utf-8",
            )
            disk = DiskDevice(root / "disks")
            disk.load_disk("DATA0001")
            cpu = type("TestCPU", (), {})()
            cpu.disk = disk
            cpu.screen = pygame.Surface((960, 400))
            with patch(
                "pygame.event.get",
                side_effect=[
                    [pygame.event.Event(pygame.KEYDOWN, key=pygame.K_d)],
                    [pygame.event.Event(pygame.KEYDOWN, key=pygame.K_RETURN)],
                ],
            ):
                selection = bios.select_boot_program(cpu, modules_dir)

        self.assertEqual(selection.module_path, module_path)
        self.assertIsNotNone(selection.image)
        self.assertEqual(selection.disk_id, "DATA0001")

    def test_start_without_program_runs_bios_selected_image(self):
        image = object()
        selection = bios.BIOSSelection(Path("modules/sample.c"), image, "DATA0001")
        with patch("sys.argv", ["main.py"]):
            with patch("src.SuomiCPU.SuomiCompute16") as cpu_class:
                cpu = cpu_class.return_value
                with patch(
                    "src.bios.select_boot_program",
                    return_value=selection,
                ):
                    with patch("src.bios.mount_data_disk") as mount_disk:
                        with patch("pygame.quit"):
                            emulator_main()

        mount_disk.assert_called_once_with(cpu, "DATA0001")
        cpu.load_program.assert_called_once_with(image)
        cpu.run.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
