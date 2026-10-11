import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / "src"), str(ROOT)]

from SuomiCPU import CONSOLE_CONTROL, CONSOLE_DATA, CONSOLE_STATUS, MEM_SIZE, SuomiCompute16  # noqa: E402
from src.console_device import ConsoleDevice, ConsoleWindow  # noqa: E402


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

    def test_console_is_an_80_by_25_text_buffer(self):
        console = self.make_cpu()._console_device()
        console.write_text("SC-16 emulator started")

        snapshot = console.text_snapshot()
        self.assertEqual(len(snapshot), 25)
        self.assertTrue(all(len(line) == 80 for line in snapshot))
        self.assertEqual(snapshot[0][:22], "SC-16 emulator started")

    def test_console_scrollback_retains_the_latest_1000_rows(self):
        console = self.make_cpu()._console_device()
        for index in range(1100):
            console.write_text(f"line-{index}")

        snapshot = console.scrollback_snapshot()
        self.assertEqual(len(console.scrollback), 1000)
        self.assertEqual(len(snapshot), 1025)
        self.assertFalse(any(line.startswith("line-0 ") for line in snapshot))
        self.assertTrue(any(line.startswith("line-1099") for line in snapshot))

    def test_host_lifecycle_messages_share_the_console_output(self):
        cpu = self.make_cpu()
        cpu.module_name = "asteroids"
        cpu.disk = type("Disk", (), {"current_disk": "SAVE0001"})()

        cpu._log_startup("SC-16 emulator started")

        text = "\n".join(cpu._console_device().text_snapshot())
        self.assertIn("SC-16 emulator started", text)
        self.assertIn("Loaded module: asteroids", text)
        self.assertIn("Loaded data disk: SAVE0001", text)

    def test_cpu_reset_preserves_console_history_and_resets_input(self):
        cpu = self.make_cpu()
        cpu.entry_point = 0
        console = cpu._console_device()
        console.write_text("C compilation complete; SC-16 image ready")
        console.receive(b"pending")

        cpu.reset()

        text = "\n".join(console.scrollback_snapshot())
        self.assertIn("C compilation complete; SC-16 image ready", text)
        self.assertEqual(tuple(console.receive_buffer), ())
        self.assertEqual(console.control, 0x03)

    def test_console_window_has_controls_and_fixed_80_by_25_text_area(self):
        tk = Mock()
        root = Mock()
        text = Mock()
        status_var = Mock()
        buttons = [Mock(), Mock(), Mock(), Mock()]
        tk.Tk.return_value = root
        tk.Frame.return_value = Mock()
        tk.Button.side_effect = buttons
        tk.StringVar.return_value = status_var
        tk.Label.return_value = Mock()
        tk.Text.return_value = text
        cpu = Mock()
        cpu.program_loaded = True
        cpu._console_device.return_value = ConsoleDevice()

        with patch.dict(sys.modules, {"tkinter": tk}):
            window = ConsoleWindow(cpu)
            window.refresh("asteroids", "SAVE0001", 0, False)

        button_labels = [call.kwargs["text"] for call in tk.Button.call_args_list]
        self.assertEqual(button_labels, ["REBOOT", "RESET", "COPY", "PAUSE"])
        text_options = tk.Text.call_args.kwargs
        self.assertEqual((text_options["width"], text_options["height"]), (80, 25))
        self.assertIn("DISK SAVE0001", status_var.set.call_args.args[0])
        self.assertIn("MODULE asteroids", status_var.set.call_args.args[0])
        commands = [call.kwargs["command"] for call in tk.Button.call_args_list]
        commands[0]()
        commands[1]()
        commands[3]()
        cpu.request_reboot.assert_called_once_with()
        cpu.request_bios_reset.assert_called_once_with()
        cpu.toggle_pause.assert_called_once_with()
        text.tag_ranges.return_value = ()
        text.get.return_value = "console line   \n\n"
        commands[2]()
        root.clipboard_clear.assert_called_once_with()
        root.clipboard_append.assert_called_once_with("console line")
        window.close()
        root.destroy.assert_called_once_with()

    def test_control_c_copies_selected_console_text(self):
        tk = Mock()
        root = Mock()
        text = Mock()
        tk.Tk.return_value = root
        tk.Frame.return_value = Mock()
        tk.Button.side_effect = [Mock(), Mock(), Mock(), Mock()]
        tk.StringVar.return_value = Mock()
        tk.Label.return_value = Mock()
        tk.Text.return_value = text
        cpu = Mock()
        console = ConsoleDevice()
        console.receive(b"x")
        cpu._console_device.return_value = console

        with patch.dict(sys.modules, {"tkinter": tk}):
            window = ConsoleWindow(cpu)
            text.tag_ranges.return_value = ("1.0", "1.5")
            text.get.return_value = "hello"
            result = window._on_key_press(
                type("KeyEvent", (), {"state": 0x04, "keysym": "c"})()
            )

        self.assertEqual(result, "break")
        root.clipboard_clear.assert_called_once_with()
        root.clipboard_append.assert_called_once_with("hello")
        self.assertEqual(tuple(console.receive_buffer), (ord("x"),))
        window.close()

    def test_mouse_wheel_scrolls_console_history(self):
        tk = Mock()
        root = Mock()
        text = Mock()
        tk.Tk.return_value = root
        tk.Frame.return_value = Mock()
        tk.Button.side_effect = [Mock(), Mock(), Mock(), Mock()]
        tk.StringVar.return_value = Mock()
        tk.Label.return_value = Mock()
        tk.Text.return_value = text
        cpu = Mock()
        cpu.program_loaded = True
        cpu._console_device.return_value.scrollback_snapshot.return_value = tuple(
            f"line-{index}".ljust(80) for index in range(30)
        )

        with patch.dict(sys.modules, {"tkinter": tk}):
            window = ConsoleWindow(cpu)
            window.refresh("module", None, 0, False)
            window._on_mousewheel(type("WheelEvent", (), {"delta": 120})())
            window.refresh("module", None, 0, False)

        rendered = text.insert.call_args.args[1]
        self.assertEqual(window.scroll_offset, 1)
        self.assertIn("line-4", rendered)
        self.assertIn("line-28", rendered)
        self.assertNotIn("line-29", rendered)
        window.close()

    def test_program_load_records_image_and_memory_usage(self):
        from tools.assembler import AssemblyImage, MemorySegment

        cpu = self.make_cpu()
        image = AssemblyImage(
            (MemorySegment(0x100, b"\x00\x01"), MemorySegment(0x200, b"\x02")),
            0x100,
        )

        cpu.load_program(image)

        self.assertEqual(cpu.program_image_bytes, 3)
        self.assertEqual(cpu.program_image_span, 0x101)
        self.assertEqual(cpu.program_memory_remaining, MEM_SIZE - 3)
        output = "\n".join(cpu._console_device().text_snapshot())
        self.assertIn("3 image bytes used", output)


if __name__ == "__main__":
    unittest.main()
