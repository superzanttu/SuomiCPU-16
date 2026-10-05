"""Headless helpers for running compiled SC-8 programs frame by frame."""
from pathlib import Path

from SuomiCPU import BACK_START, KEYS_ADDR, MEM_SIZE, SCREEN_HEIGHT, SCREEN_WIDTH, VRAM_START, SuomiCompute8
from c_compiler import compile_source

ROOT = Path(__file__).resolve().parent.parent


def make_cpu(image):
    cpu = SuomiCompute8.__new__(SuomiCompute8)
    cpu.memory = bytearray(MEM_SIZE)
    cpu.registers = [0] * 8
    cpu.flags = {"Z": 0, "C": 0}
    cpu.running = True
    cpu.sp = 0x2FFFF
    cpu.frame_yield = False
    cpu.load_program(image)
    cpu.reset()
    return cpu


def compile_file(path):
    path = Path(path)
    return compile_source(path.read_text(encoding="utf-8"), str(path))


def run_frame(cpu, limit=400_000):
    """Run until the program presents a frame; returns executed instructions."""
    cpu.frame_yield = False
    cpu.frames = (getattr(cpu, "frames", 0) + 1) & 0xFFFF
    cpu.update_rtc()
    steps = 0
    while cpu.running and not cpu.frame_yield and steps < limit:
        cpu.step()
        steps += 1
    return steps


def vram(cpu):
    return bytes(cpu.memory[VRAM_START:VRAM_START + SCREEN_WIDTH * SCREEN_HEIGHT])


def pixel(cpu, x, y):
    return cpu.memory[VRAM_START + y * SCREEN_WIDTH + x]


def save_png(cpu, path):
    import pygame
    from SuomiCPU import PALETTE_COLORS
    palette = [(i, i, i) for i in range(256)]
    palette[:len(PALETTE_COLORS)] = PALETTE_COLORS
    surface = pygame.image.frombuffer(vram(cpu), (SCREEN_WIDTH, SCREEN_HEIGHT), "P")
    surface.set_palette(palette)
    pygame.image.save(pygame.transform.scale(surface, (640, 480)), str(path))
