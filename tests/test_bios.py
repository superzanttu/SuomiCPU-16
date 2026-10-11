import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import pygame

from src import bios
from src.lib_disk import DiskDevice
from src.SuomiCPU import main as emulator_main
from tools.assembler import AssemblyImage, MemorySegment


class BIOSBootTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        pygame.init()
        pygame.display.set_mode((1, 1))

    @classmethod
    def tearDownClass(cls):
        pygame.quit()

    def test_discovers_folder_modules_and_selects_single_c_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            alpha = root / "Alpha"
            alpha.mkdir()
            (alpha / "alpha.c").write_text("/* Alpha program */\n")
            zeta = root / "zeta"
            zeta.mkdir()
            (zeta / "zeta.c").write_text("// Zeta program\n")
            (root / "ignored.c").write_text("Not a module\n")

            modules = bios.discover_boot_modules(root)

        self.assertEqual(
            [(module.name, module.path.name, module.description) for module in modules],
            [("Alpha", "alpha.c", "Alpha program"), ("zeta", "zeta.c", "Zeta program")],
        )

    def test_multiple_c_files_require_main_c_and_empty_folders_are_modules(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            broken = root / "broken"
            broken.mkdir()
            (broken / "first.c").write_text("// First\n")
            (broken / "second.c").write_text("// Second\n")
            empty = root / "empty"
            empty.mkdir()
            valid = root / "valid"
            valid.mkdir()
            main_path = valid / "main.c"
            main_path.write_text("// Main program\n")
            (valid / "helper.c").write_text("// Helper\n")

            modules = bios.discover_boot_modules(root)

        self.assertEqual(
            [module.name for module in modules],
            ["broken", "empty", "valid"],
        )
        self.assertIsNone(modules[0].path)
        self.assertEqual(modules[0].error, "Multiple .c files found; expected main.c.")
        self.assertEqual(modules[1].error, "No .c files found in module folder.")
        self.assertEqual(modules[2].path, main_path)
        self.assertIsNone(modules[2].error)

    def test_loading_module_without_main_c_displays_error(self):
        with tempfile.TemporaryDirectory() as directory:
            modules_dir = Path(directory)
            module_dir = modules_dir / "broken"
            module_dir.mkdir()
            (module_dir / "first.c").write_text("// First\n")
            (module_dir / "second.c").write_text("// Second\n")
            cpu = type("TestCPU", (), {})()
            cpu.disk = DiskDevice(modules_dir.parent / "disks")
            cpu.screen = pygame.Surface((960, 400))
            cpu.messages = []
            cpu.console_log = cpu.messages.append
            rendered_text = []

            class CaptureFont:
                def render(self, text, _antialias, _color):
                    rendered_text.append(text)
                    return pygame.Surface((1, 1))

            with patch("pygame.font.Font", side_effect=lambda *_args: CaptureFont()):
                with patch(
                    "pygame.event.get",
                    side_effect=[
                        [pygame.event.Event(pygame.KEYDOWN, key=pygame.K_RETURN)],
                        [pygame.event.Event(pygame.KEYDOWN, key=pygame.K_ESCAPE)],
                    ],
                ):
                    selection = bios.select_boot_program(cpu, modules_dir)

        self.assertIsNone(selection.image)
        self.assertIn(
            "ERROR loading broken: Multiple .c files found; expected main.c.",
            cpu.messages,
        )
        self.assertIn("broken: Multiple .c files found; expected main.c.", rendered_text)

    def test_bios_compiles_selected_module_and_keeps_data_disk_optional(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            modules_dir = root / "modules"
            modules_dir.mkdir()
            module_dir = modules_dir / "sample"
            module_dir.mkdir()
            module_path = module_dir / "sample.c"
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
        self.assertEqual(selection.module_name, "sample")
        self.assertIsNotNone(selection.image)
        self.assertEqual(selection.disk_id, "DATA0001")

    def test_mouse_selects_and_double_clicks_module_and_cycles_data_disk(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            modules_dir = root / "modules"
            modules_dir.mkdir()
            alpha_dir = modules_dir / "alpha"
            alpha_dir.mkdir()
            (alpha_dir / "alpha.c").write_text("// Alpha\n")
            beta_dir = modules_dir / "beta"
            beta_dir.mkdir()
            beta_path = beta_dir / "beta.c"
            beta_path.write_text("// Beta\n")
            disk = DiskDevice(root / "disks")
            disk.load_disk("DATA0001")
            cpu = type("TestCPU", (), {})()
            cpu.disk = disk
            cpu.screen = pygame.Surface((960, 400))
            image = object()
            module_position = (100, 184 + 44 + 5)
            with patch(
                "pygame.event.get",
                side_effect=[
                    [pygame.event.Event(pygame.MOUSEBUTTONDOWN, pos=(100, 135), button=1)],
                    [pygame.event.Event(pygame.MOUSEMOTION, pos=module_position)],
                    [pygame.event.Event(pygame.MOUSEBUTTONDOWN, pos=module_position, button=1)],
                    [pygame.event.Event(pygame.MOUSEBUTTONDOWN, pos=module_position, button=1)],
                ],
            ):
                with patch("pygame.time.get_ticks", side_effect=[100, 200]):
                    with patch("src.SuomiCPU.load_program_file", return_value=image):
                        selection = bios.select_boot_program(cpu, modules_dir)

        self.assertEqual(selection.module_path, beta_path)
        self.assertIs(selection.image, image)
        self.assertEqual(selection.disk_id, "DATA0001")

    def test_mouse_wheel_scrolls_modules_and_indicates_hidden_rows(self):
        with tempfile.TemporaryDirectory() as directory:
            modules_dir = Path(directory)
            for index in range(8):
                module_dir = modules_dir / f"module{index}"
                module_dir.mkdir()
                (module_dir / f"module{index}.c").write_text(
                    f"// Module {index}\n",
                    encoding="utf-8",
                )
            cpu = type("TestCPU", (), {})()
            cpu.disk = DiskDevice(modules_dir.parent / "disks")
            cpu.screen = pygame.Surface((960, 400))
            rendered_text = []

            class CaptureFont:
                def render(self, text, _antialias, _color):
                    rendered_text.append(text)
                    return pygame.Surface((1, 1))

            image = object()
            with patch("pygame.font.Font", side_effect=lambda *_args: CaptureFont()):
                with patch(
                    "pygame.event.get",
                    side_effect=[
                        [
                            pygame.event.Event(
                                pygame.MOUSEWHEEL,
                                y=-1,
                                mouse_x=100,
                                mouse_y=200,
                            )
                        ],
                        [pygame.event.Event(pygame.KEYDOWN, key=pygame.K_RETURN)],
                    ],
                ):
                    with patch("src.SuomiCPU.load_program_file", return_value=image):
                        selection = bios.select_boot_program(cpu, modules_dir)

        self.assertEqual(selection.module_path.name, "module3.c")
        self.assertEqual(selection.module_name, "module3")
        self.assertIn("^ 3 more module(s) above", rendered_text)
        self.assertIn("v 3 more module(s) below", rendered_text)
        self.assertIn("Wheel: scroll modules", rendered_text[-1])

    def test_start_without_program_runs_bios_selected_image(self):
        image = AssemblyImage((MemorySegment(0, b"\x00\x01"),), 0)
        selection = bios.BIOSSelection(Path("modules/sample.c"), image, "DATA0001")
        with patch("sys.argv", ["main.py"]):
            with patch("src.SuomiCPU.SuomiCompute16") as cpu_class:
                cpu = cpu_class.return_value
                with patch("src.SuomiCPU.ConsoleWindow"):
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

    def test_reset_after_program_returns_to_bios_without_relaunching_program(self):
        image = object()
        selection = bios.BIOSSelection(Path("modules/sample.c"), image, None)
        with patch("sys.argv", ["main.py"]):
            with patch("src.SuomiCPU.SuomiCompute16") as cpu_class:
                cpu = cpu_class.return_value
                cpu.run.side_effect = lambda: setattr(cpu, "bios_reset_requested", True)
                with patch("src.SuomiCPU.ConsoleWindow"):
                    with patch(
                        "src.bios.select_boot_program",
                        side_effect=[selection, bios.BIOSSelection(None, None, None)],
                    ) as select_program:
                        with patch("pygame.quit"):
                            emulator_main()

        self.assertEqual(select_program.call_count, 2)
        cpu.load_program.assert_called_once_with(image)
        cpu.run.assert_called_once_with()
        self.assertIsNone(cpu.module_name)
        self.assertFalse(cpu.program_loaded)
        self.assertGreaterEqual(cpu.reset.call_count, 2)

    def test_program_loader_reports_compile_and_assembly_image_status(self):
        from src.SuomiCPU import CCompilerError, load_program_file

        with tempfile.TemporaryDirectory() as directory:
            source_path = Path(directory) / "sample.c"
            source_path.write_text("int main(void) { return 0; }\n")
            image = AssemblyImage((MemorySegment(0, b"\x00\x01"),), 0)
            messages = []
            with patch(
                "tools.c_compiler.compile_file_with_assembly",
                return_value=(image, "HALT\n"),
            ):
                loaded = load_program_file(source_path, messages.append)

        self.assertIs(loaded, image)
        self.assertEqual(messages[0], "Compiling C source: sample.c")
        self.assertIn("C source file size:", messages[1])
        self.assertEqual(messages[2], "Generated assembly size: 5 bytes")
        self.assertEqual(messages[3], "C compilation complete; SC-16 image ready")
        self.assertIn("Program image:", messages[4])

        messages.clear()
        with tempfile.TemporaryDirectory() as directory:
            source_path = Path(directory) / "broken.c"
            source_path.write_text("not valid C\n")
            with patch(
                "tools.c_compiler.compile_file_with_assembly",
                side_effect=CCompilerError("invalid C source"),
            ):
                with self.assertRaises(CCompilerError):
                    load_program_file(source_path, messages.append)

        self.assertIn("ERROR: C compilation failed: invalid C source", messages)

        messages.clear()
        with tempfile.TemporaryDirectory() as directory:
            asm_path = Path(directory) / "sample.asm"
            asm_path.write_text("HALT\n")
            assembly_size = asm_path.stat().st_size
            image = AssemblyImage((MemorySegment(0, b"\x00\x01"),), 0)
            with patch("src.SuomiCPU.assemble_file", return_value=image):
                loaded = load_program_file(asm_path, messages.append)

        self.assertIs(loaded, image)
        self.assertEqual(
            messages,
            [
                "Assembling source: sample.asm",
                f"Assembly source file size: {assembly_size} bytes",
                "Assembly complete; SC-16 image ready",
                "Program image: 2 bytes across 1 segments; address span 2 bytes; "
                "memory capacity 524288 bytes",
            ],
        )


if __name__ == "__main__":
    unittest.main()
