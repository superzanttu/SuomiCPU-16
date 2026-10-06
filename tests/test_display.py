import unittest
from unittest.mock import Mock, patch

import pygame

from assembler import assemble
from src.SuomiCPU import KEY_BITS, MEM_SIZE, VRAM_START, SuomiCompute16


class DisplayTests(unittest.TestCase):
    def test_tab_key_has_a_dedicated_held_key_bit(self):
        self.assertEqual(KEY_BITS[pygame.K_TAB], 64)

    def test_cpu_vram_writes_are_visible_in_palette_surface(self):
        emulator = SuomiCompute16.__new__(SuomiCompute16)
        emulator.memory = bytearray(MEM_SIZE)
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

    def test_halt_keeps_display_open_until_keypress(self):
        emulator = SuomiCompute16.__new__(SuomiCompute16)
        emulator.clock = Mock()
        emulator.reset = lambda: setattr(emulator, "running", True)
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
            emulator.run()

        self.assertFalse(emulator.running)
        self.assertFalse(emulator.window_open)
        emulator.update_display.assert_called_once_with()
        emulator.clock.tick.assert_called_once_with(30)


if __name__ == "__main__":
    unittest.main()
