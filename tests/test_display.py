import unittest
from unittest.mock import Mock, patch

import pygame

from assembler import assemble
from src.SuomiCPU import (
    KEY_BITS,
    SPEED_MAX,
    SPEED_ADDR,
    VSYNC_ADDR,
    VRAM_START,
    SuomiCompute16,
    create_memory,
    _screen_to_logical,
    _set_display_mode,
)


class DisplayTests(unittest.TestCase):
    def test_resuming_pause_resets_instruction_budget_timestamp(self):
        emulator = SuomiCompute16.__new__(SuomiCompute16)
        emulator.paused = True
        emulator.console_log = Mock()

        with patch("src.SuomiCPU.time.monotonic", return_value=123.0):
            emulator.toggle_pause()

        self.assertFalse(emulator.paused)
        self.assertEqual(emulator._speed_time, 123.0)

    def test_tab_key_has_a_dedicated_held_key_bit(self):
        self.assertEqual(KEY_BITS[pygame.K_TAB], 64)

    def test_wasd_keys_map_to_directional_controls(self):
        self.assertEqual(
            [KEY_BITS[pygame.K_a], KEY_BITS[pygame.K_d],
             KEY_BITS[pygame.K_w], KEY_BITS[pygame.K_s]],
            [1, 2, 4, 8],
        )

    def test_cpu_vram_writes_are_visible_in_palette_surface(self):
        emulator = SuomiCompute16.__new__(SuomiCompute16)
        emulator.memory = create_memory()
        emulator.registers = [0] * 8
        emulator.flags = {"Z": 0, "C": 0}
        emulator.running = True
        emulator.sp = 0x2FFFF
        emulator.palette = [pygame.Color(i, i, i) for i in range(256)]
        emulator.palette[0] = (0, 0, 0)
        emulator.palette[1] = (255, 255, 255)
        emulator.palette[25] = (0, 0, 255)
        emulator.vram_surface = emulator._create_vram_surface()

        emulator.load_program(
            assemble(
                "LDI R0, 0xFFFF\n"
                "INC R0\n"
                "MOV R1, R0\n"
                "ADD R0, R1\n"
                "ADD R0, R1\n"
                "ADD R0, R1\n"
                "ADD R0, R1\n"
                "LDI R2, 25\n"
                "ST R0, R2\n"
                "HALT\n"
            )
        )
        emulator.reset()
        while emulator.running:
            emulator.step()

        pixel_address = VRAM_START
        self.assertEqual(emulator.read(pixel_address), 25)
        self.assertEqual(
            tuple(emulator.vram_surface.get_at((0, 0)))[:3],
            (0, 0, 255),
        )

        emulator.write(pixel_address, 1)
        self.assertEqual(
            tuple(emulator.vram_surface.get_at((0, 0)))[:3],
            (255, 255, 255),
        )

    def test_extended_keys_use_second_key_byte(self):
        self.assertEqual(KEY_BITS[pygame.K_q], 0x100)
        self.assertEqual(KEY_BITS[pygame.K_e], 0x200)
        self.assertEqual(KEY_BITS[pygame.K_x], 0x400)
        self.assertEqual(KEY_BITS[pygame.K_z], 0x800)

    def test_fullscreen_viewport_preserves_aspect_ratio_for_mouse(self):
        self.assertEqual(_screen_to_logical((240, 0), (1920, 1080)), (0, 0))
        self.assertEqual(_screen_to_logical((960, 540), (1920, 1080)), (160, 120))
        self.assertEqual(_screen_to_logical((1680, 1080), (1920, 1080)), (319, 239))
        self.assertEqual(
            _screen_to_logical((480, 360), (960, 720)),
            (160, 120),
        )

    def test_display_mode_can_be_selected(self):
        with patch("pygame.display.set_mode") as set_mode:
            _set_display_mode(True)
            set_mode.assert_called_once_with((0, 0), pygame.FULLSCREEN)

        with patch("pygame.display.set_mode") as set_mode:
            _set_display_mode(False)
            set_mode.assert_called_once_with((960, 720))

    def test_display_updates_game_window_without_composing_console(self):
        emulator = SuomiCompute16.__new__(SuomiCompute16)
        emulator.vram_surface = object()
        emulator.screen = Mock()
        emulator._composite_layers = Mock()

        with patch("src.SuomiCPU._present_scaled") as present:
            emulator.update_display()
        emulator._composite_layers.assert_called_once_with()
        present.assert_called_once_with(emulator.vram_surface, emulator.screen)

    def test_halt_keeps_display_open_until_keypress(self):
        emulator = SuomiCompute16.__new__(SuomiCompute16)
        emulator.clock = Mock()
        emulator.paused = False
        emulator.reboot_requested = False
        emulator.module_name = "test"
        emulator.reset = lambda **_kwargs: setattr(emulator, "running", True)
        emulator.execute_frame = lambda: setattr(emulator, "running", False)
        emulator.update_rtc = Mock()
        emulator.begin_mouse_frame = Mock()
        emulator.end_mouse_frame = Mock()
        emulator.update_display = Mock()
        keypress = pygame.event.Event(
            pygame.KEYDOWN,
            key=pygame.K_SPACE,
            unicode=" ",
        )

        with patch.object(pygame.event, "get", side_effect=[[], [keypress]]):
            with patch("src.SuomiCPU.ConsoleWindow") as console_window:
                emulator.run()

        self.assertFalse(emulator.running)
        self.assertFalse(emulator.window_open)
        emulator.update_display.assert_called_once_with()
        emulator.clock.tick.assert_called_once_with(30)
        console_window.return_value.close.assert_called_once_with()

    def test_max_speed_keeps_display_updates_at_display_rate(self):
        emulator = SuomiCompute16.__new__(SuomiCompute16)
        emulator.memory = create_memory()
        emulator.memory[SPEED_ADDR] = SPEED_MAX
        emulator.clock = Mock()
        emulator.paused = False
        emulator.reboot_requested = False
        emulator.bios_reset_requested = False
        emulator.module_name = "test"
        emulator.boot_time = 0.0
        emulator._prepared_for_run = True
        emulator.running = True
        emulator.reset = Mock()
        emulator.execute_frame = Mock()
        emulator.update_rtc = Mock()
        emulator.begin_mouse_frame = Mock()
        emulator.end_mouse_frame = Mock()
        emulator.update_display = Mock()
        quit_event = pygame.event.Event(pygame.QUIT)
        event_batches = [[] for _ in range(6)] + [[quit_event]]
        monotonic_calls = 0

        def monotonic():
            nonlocal monotonic_calls
            monotonic_calls += 1
            return 10 + monotonic_calls * 0.017

        with patch.object(pygame.event, "get", side_effect=event_batches):
            with patch("src.SuomiCPU.time.monotonic", side_effect=monotonic):
                with patch("src.SuomiCPU.ConsoleWindow"):
                    emulator.run()

        self.assertEqual(emulator.update_display.call_count, 6)
        emulator.clock.tick.assert_not_called()

    def test_vsync_throttles_max_speed_on_presented_frames(self):
        emulator = SuomiCompute16.__new__(SuomiCompute16)
        emulator.memory = create_memory()
        emulator.memory[SPEED_ADDR] = SPEED_MAX
        emulator.memory[VSYNC_ADDR] = 1
        emulator.clock = Mock()
        emulator.paused = False
        emulator.reboot_requested = False
        emulator.bios_reset_requested = False
        emulator.module_name = "test"
        emulator.boot_time = 0.0
        emulator._prepared_for_run = True
        emulator.running = True
        emulator.reset = Mock()
        emulator.execute_frame = lambda: setattr(emulator, "frame_yield", True)
        emulator.update_rtc = Mock()
        emulator.begin_mouse_frame = Mock()
        emulator.end_mouse_frame = Mock()
        emulator.update_display = Mock()
        quit_event = pygame.event.Event(pygame.QUIT)

        with patch.object(pygame.event, "get", side_effect=[[], [], [quit_event]]):
            with patch("src.SuomiCPU.time.monotonic", return_value=10.0):
                with patch("src.SuomiCPU.ConsoleWindow"):
                    emulator.run()

        self.assertEqual(emulator.update_display.call_count, 2)
        self.assertEqual(emulator.clock.tick.call_count, 2)
        emulator.clock.tick.assert_called_with(30)

    def test_f11_toggles_fullscreen_during_emulation(self):
        emulator = SuomiCompute16.__new__(SuomiCompute16)
        emulator.fullscreen = False
        emulator.clock = Mock()
        emulator.paused = False
        emulator.reboot_requested = False
        emulator.module_name = "test"
        emulator.reset = lambda **_kwargs: setattr(emulator, "running", True)
        emulator.execute_frame = lambda: setattr(emulator, "running", False)
        emulator.update_rtc = Mock()
        emulator.begin_mouse_frame = Mock()
        emulator.end_mouse_frame = Mock()
        emulator.update_display = Mock()
        fullscreen_key = pygame.event.Event(pygame.KEYDOWN, key=pygame.K_F11, unicode="")
        close_key = pygame.event.Event(pygame.KEYDOWN, key=pygame.K_SPACE, unicode=" ")

        with patch.object(pygame.event, "get", side_effect=[[fullscreen_key], [close_key]]):
            with patch("src.SuomiCPU.ConsoleWindow"):
                with patch("src.SuomiCPU._set_display_mode", return_value=object()) as set_mode:
                    with patch("pygame.display.set_caption"):
                        emulator.run()

        self.assertTrue(emulator.fullscreen)
        set_mode.assert_called_once_with(True)
        emulator.clock.tick.assert_called_once_with(30)

    def test_f2_toggles_separate_console_window(self):
        emulator = SuomiCompute16.__new__(SuomiCompute16)
        emulator.clock = Mock()
        emulator.paused = False
        emulator.reboot_requested = False
        emulator.module_name = "test"
        emulator.reset = lambda **_kwargs: setattr(emulator, "running", True)
        emulator.execute_frame = lambda: setattr(emulator, "running", False)
        emulator.update_rtc = Mock()
        emulator.begin_mouse_frame = Mock()
        emulator.end_mouse_frame = Mock()
        emulator.update_display = Mock()
        console_key = pygame.event.Event(pygame.KEYDOWN, key=pygame.K_F2, unicode="")
        close_key = pygame.event.Event(pygame.KEYDOWN, key=pygame.K_SPACE, unicode=" ")

        with patch.object(pygame.event, "get", side_effect=[[console_key], [close_key]]):
            with patch("src.SuomiCPU.ConsoleWindow") as console_window:
                emulator.run()

        console_window.return_value.toggle.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
