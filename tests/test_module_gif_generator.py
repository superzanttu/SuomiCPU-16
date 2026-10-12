import random
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import pygame

from tools.generate_module_gifs import (
    Module,
    capture_module,
    discover_modules,
    update_readme,
    write_gif,
)
from src.SuomiCPU import PALETTE_COLORS, SCREEN_HEIGHT, SCREEN_WIDTH


class ModuleGifGeneratorTests(unittest.TestCase):
    def test_discovers_bios_compatible_module_entries(self):
        with tempfile.TemporaryDirectory() as directory:
            modules_dir = Path(directory)
            single = modules_dir / "single"
            single.mkdir()
            (single / "game.c").write_text("int main(void) { return 0; }\n")
            multiple = modules_dir / "multiple"
            multiple.mkdir()
            (multiple / "main.c").write_text("int main(void) { return 0; }\n")
            (multiple / "helper.c").write_text("")
            broken = modules_dir / "broken"
            broken.mkdir()
            (broken / "one.c").write_text("")
            (broken / "two.c").write_text("")
            empty = modules_dir / "empty"
            empty.mkdir()

            modules = discover_modules(modules_dir)

        self.assertEqual(
            [(module.name, module.source.name if module.source else None)
             for module in modules],
            [
                ("broken", None),
                ("empty", None),
                ("multiple", "main.c"),
                ("single", "game.c"),
            ],
        )
        self.assertIn("expected main.c", modules[0].error)
        self.assertIn("No C source", modules[1].error)

    def test_gif_is_decodable_and_repeats_for_requested_duration(self):
        first_frame = bytes((0, 1, 2, 3, 4, 5))
        second_frame = bytes((5, 4, 3, 2, 1, 0))
        with tempfile.TemporaryDirectory() as directory:
            gif_path = Path(directory) / "sample.gif"
            write_gif(gif_path, [first_frame, second_frame], 3, 2, 0.15)

            image = pygame.image.load(str(gif_path))
            gif_bytes = gif_path.read_bytes()

        self.assertEqual(gif_bytes[:6], b"GIF89a")
        self.assertEqual(image.get_size(), (3, 2))
        delays = []
        offset = 0
        while True:
            offset = gif_bytes.find(b"!\xF9\x04\x00", offset)
            if offset == -1:
                break
            delays.append(int.from_bytes(gif_bytes[offset + 4:offset + 6], "little"))
            offset += 6
        self.assertEqual(len(delays), 2)
        self.assertEqual(sum(delays), 15)
        self.assertIn(b"NETSCAPE2.0", gif_bytes)
        self.assertEqual(gif_bytes[-1], 0x3B)

    def test_large_full_palette_frame_decodes_without_corruption(self):
        width, height = SCREEN_WIDTH, SCREEN_HEIGHT
        pixels = random.Random(42).randbytes(width * height)
        palette = [(value, value, value) for value in range(256)]
        palette[:len(PALETTE_COLORS)] = PALETTE_COLORS
        expected = bytes(channel for index in pixels for channel in palette[index])

        with tempfile.TemporaryDirectory() as directory:
            gif_path = Path(directory) / "full-screen.gif"
            write_gif(gif_path, [pixels], width, height, 15.0)
            decoded = pygame.image.tostring(pygame.image.load(str(gif_path)), "RGB")

        self.assertEqual(decoded, expected)

    def test_capture_loop_samples_multiple_frames_and_keeps_gif_duration(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "demo.c"
            source.write_text("int main(void) { return 0; }\n")
            module = Module("Demo", root, source)
            clock = [0.0]

            class FakeConsole:
                def close(self):
                    pass

            class FakeCpu:
                def update_rtc(self):
                    pass

                def execute_frame(self):
                    clock[0] += 1 / 30

                def _console_device(self):
                    return FakeConsole()

            fake_cpu = FakeCpu()
            with patch("tools.generate_module_gifs._make_cpu", return_value=fake_cpu):
                with patch("tools.generate_module_gifs.time.perf_counter",
                           side_effect=lambda: clock[0]):
                    with patch("tools.generate_module_gifs.time.sleep",
                               side_effect=lambda delay: clock.__setitem__(
                                   0, clock[0] + delay
                               )):
                        with patch(
                            "tools.generate_module_gifs._capture_frame",
                            return_value=bytes(SCREEN_WIDTH * SCREEN_HEIGHT),
                        ):
                            with patch("tools.generate_module_gifs.write_gif") as write:
                                count = capture_module(
                                    module,
                                    root / "demo.gif",
                                    0.5,
                                    15.0,
                                    30,
                                    10,
                                    "SC-16",
                                )

        self.assertGreaterEqual(count, 5)
        self.assertEqual(write.call_args.args[4], 15.0)

    def test_readme_gallery_update_is_idempotent(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            readme = root / "README.md"
            readme.write_text("# Project\n")
            module = Module("Demo", root / "modules" / "Demo", root / "Demo.c")
            gif_path = root / "images" / "demo.gif"
            results = [(module, gif_path, None)]

            update_readme(readme, results)
            first_contents = readme.read_text()
            update_readme(readme, results)
            updated_contents = readme.read_text()

        self.assertEqual(updated_contents, first_contents)
        self.assertIn("Module gallery", first_contents)
        self.assertIn("images/demo.gif", first_contents)


if __name__ == "__main__":
    unittest.main()
