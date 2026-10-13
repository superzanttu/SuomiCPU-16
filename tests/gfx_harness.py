"""Headless helpers for running compiled SC-16 programs frame by frame."""
from pathlib import Path

from src.SuomiCPU import (
    KEYS_ADDR, MEM_MASK, SCREEN_HEIGHT, SCREEN_WIDTH, VRAM_START,
    SuomiCompute16, create_memory,
)
from c_compiler import compile_source

ROOT = Path(__file__).resolve().parent.parent


def make_cpu(image):
    cpu = SuomiCompute16.__new__(SuomiCompute16)
    cpu.memory = create_memory()
    cpu.registers = [0] * 8
    cpu.flags = {"Z": 0, "C": 0, "N": 0, "V": 0}
    cpu.running = True
    cpu.sp = MEM_MASK
    cpu.frame_yield = False
    cpu.audio_enabled = False
    cpu.sound_log = []
    cpu._last_click = [(-10**9, 0, 0), (-10**9, 0, 0)]
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

def layer_pixel(cpu, layer, x, y):
    width, _ = cpu._screen_size()
    if layer == 0:
        return cpu._decode_framebuffer(VRAM_START)[y * width + x]
    return cpu.gpu_layers[layer][y * width + x]


def pixel(cpu, x, y):
    surface = getattr(cpu, "vram_surface", None)
    if surface is not None:
        return surface.map_rgb(surface.get_at((x, y))) & 0xFF
    return layer_pixel(cpu, 0, x, y)


def save_png(cpu, path):
    import pygame
    from src.SuomiCPU import PALETTE_COLORS
    palette = [(i, i, i) for i in range(256)]
    palette[:len(PALETTE_COLORS)] = PALETTE_COLORS
    surface = pygame.image.frombuffer(vram(cpu), (SCREEN_WIDTH, SCREEN_HEIGHT), "P")
    surface.set_palette(palette)
    pygame.image.save(pygame.transform.scale(surface, (640, 480)), str(path))
