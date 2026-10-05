import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import pygame

from src import sc_launcher
from src.SuomiCPU import main as emulator_main
from src.sc_launcher import SCLauncher, discover_modules


class ModuleDiscoveryTests(unittest.TestCase):
    def test_modules_use_filename_and_first_line_description(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            (folder / "zeta.c").write_text("// Zeta module\nint main(void) { return 0; }\n")
            (folder / "Alpha.c").write_text("/* Alpha module */\nint main(void) { return 0; }\n")
            (folder / "ignore.txt").write_text("// Not a module\n")

            modules = discover_modules(folder)

        self.assertEqual(
            [(module.name, module.description) for module in modules],
            [("Alpha", "Alpha module"), ("zeta", "Zeta module")],
        )

    def test_empty_first_line_has_fallback_description(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "blank.c"
            path.write_text("\nint main(void) { return 0; }\n")

            modules = discover_modules(directory)

        self.assertEqual(modules[0].description, "No description")

    def test_repository_modules_have_first_line_descriptions(self):
        modules_dir = Path(__file__).resolve().parents[1] / "modules"
        modules = discover_modules(modules_dir)

        self.assertTrue(modules)
        self.assertTrue(all(module.description != "No description" for module in modules))


class LauncherInteractionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        pygame.init()
        pygame.display.set_mode((1, 1))

    @classmethod
    def tearDownClass(cls):
        pygame.quit()

    def test_mouse_selects_module_and_load_button_launches_selection(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            (folder / "one.c").write_text("// First\n")
            (folder / "two.c").write_text("// Second\n")
            launcher = SCLauncher(folder)
            launcher._draw()
            launcher._handle_click((30 * 3, 95 * 3))
            self.assertEqual(launcher.selected, 1)

            with patch.object(launcher, "_launch_selected") as launch:
                launcher._handle_click((250 * 3, 220 * 3))

        launch.assert_called_once_with()

    def test_mouse_wheel_scrolls_module_list(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            for index in range(6):
                (folder / f"module{index}.c").write_text(f"// Module {index}\n")
            launcher = SCLauncher(folder)
            wheel = pygame.event.Event(pygame.MOUSEWHEEL, y=-1)
            quit_event = pygame.event.Event(pygame.QUIT)
            with patch("pygame.event.get", side_effect=[[wheel], [quit_event]]):
                with patch.object(launcher, "_draw"):
                    launcher.run()

        self.assertEqual(launcher.scroll, 1)
        self.assertEqual(launcher.selected, 1)

    def test_start_without_program_opens_launcher(self):
        with patch("sys.argv", ["main.py"]):
            with patch.object(sc_launcher.SCLauncher, "run") as run:
                with patch("pygame.quit"):
                    emulator_main()

        run.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
