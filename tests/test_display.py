import unittest
from unittest.mock import Mock, patch

import pygame

from assembler import assemble
from SuomiCPU import MEM_SIZE, VRAM_START, SuomiCompute16


class DisplayTests(unittest.TestCase):
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

    def test_halt_keeps_display_open_until_keypress(self):
        emulator = SuomiCompute16.__new__(SuomiCompute16)
        emulator.clock = Mock()
        emulator.reset = lambda: setattr(emulator, "running", True)
        emulator.execute_frame = lambda: setattr(emulator, "running", False)
        emulator.update_rtc = Mock()
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
        emulator.clock.tick.assert_called_once()


if __name__ == "__main__":
    unittest.main()
