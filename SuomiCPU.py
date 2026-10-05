import pygame
import datetime
import math
import sys
import argparse
import struct
import random
from pathlib import Path

from assembler import AssemblyError, AssemblyImage, MemorySegment, assemble_file
from c_compiler import CCompilerError, compile_file
from isa import (
    OP_ADD,
    OP_ADJSP,
    OP_AND,
    OP_BC,
    OP_BZ,
    OP_CALL,
    OP_CMP,
    OP_DEC,
    OP_DI,
    OP_EI,
    OP_HALT,
    OP_INC,
    OP_JMP,
    OP_JZ,
    OP_LD,
    OP_LDW,
    OP_LDWS,
    OP_LDI,
    OP_LDI_H,
    OP_MOV,
    OP_NOT,
    OP_OR,
    OP_POP,
    OP_PUSH,
    OP_RET,
    OP_RTI,
    OP_ST,
    OP_STW,
    OP_STWS,
    OP_SHR,
    OP_SUB,
    OP_XOR,
)
# ==========================================
# CONFIGURATION & MEMORY MAP
# ==========================================
MEM_SIZE = 2**19
FLASH_START = 0x00000
RAM_START   = 0x20000
VRAM_START  = 0x30000
KBD_ADDR    = 0x43000
ICR_ADDR    = 0x43002
RTC_START   = 0x43010
KEYS_ADDR   = 0x43004   # held-key bitmask: 1 left, 2 right, 4 up, 8 down, 16 fire, 32 start
GPU_BASE    = 0x43020   # graphics coprocessor registers (see user_guide.md)
BACK_START  = 0x44000   # off-screen back buffer drawn by the coprocessor
FONT_ADDR   = 0x4400    # 5x7 font table used by the coprocessor TEXT command

GPU_CLEAR, GPU_PIXEL, GPU_RECT, GPU_LINE = 1, 2, 3, 4
GPU_SPRITE, GPU_BITMAP, GPU_TEXT, GPU_PRESENT, GPU_RANDOM, GPU_POLY = 5, 6, 7, 8, 9, 10
GPU_TICKS = 11  # RESULT word (+14/+15) = emulated frame counter
GPU_RTC = 12  # RESULT byte (+14) = RTC register COLOR (0 sec, 1 min, 2 hour)

SCREEN_WIDTH = 320
SCREEN_HEIGHT = 240
WINDOW_SCALE = 3
DISPLAY_FPS = 60
INSTRUCTIONS_PER_FRAME = 30000
PALETTE_COLORS = [
    (0, 0, 0), (255, 255, 255), (255, 48, 48), (48, 220, 64),
    (64, 96, 255), (255, 224, 0), (0, 224, 224), (224, 64, 224),
    (255, 144, 0), (150, 150, 150), (80, 80, 80), (128, 0, 0),
]
KEY_BITS = {
    pygame.K_LEFT: 1, pygame.K_a: 1, pygame.K_RIGHT: 2, pygame.K_d: 2,
    pygame.K_UP: 4, pygame.K_w: 4, pygame.K_DOWN: 8, pygame.K_s: 8,
    pygame.K_SPACE: 16, pygame.K_RETURN: 32,
}


def load_program_file(path: str | Path) -> AssemblyImage:
    """Load an assembly, C source, or flat binary program image."""
    program_path = Path(path)
    if program_path.suffix.lower() == ".c":
        return compile_file(program_path)
    if program_path.suffix.lower() == ".bin":
        binary = program_path.read_bytes()
        if len(binary) > MEM_SIZE:
            raise ValueError("binary program exceeds available memory")
        return AssemblyImage((MemorySegment(0, binary),), 0)
    return assemble_file(program_path)


class SuomiCompute8:
    def __init__(self):
        self.memory = bytearray(MEM_SIZE)
        self.registers = [0] * 8
        self.pc = 0
        self.entry_point = 0
        self.sp = 0x2FFFF
        self.flags = {'Z': 0, 'C': 0}
        self.running = True
        
        pygame.init()
        self.screen = pygame.display.set_mode((SCREEN_WIDTH * WINDOW_SCALE, SCREEN_HEIGHT * WINDOW_SCALE))
        pygame.display.set_caption("SC-8 Flash-Execution Emulator")
        self.clock = pygame.time.Clock()
        self.palette = [pygame.Color(i, i, i) for i in range(256)]
        for index, color in enumerate(PALETTE_COLORS):
            self.palette[index] = color
        self.palette[25] = (0, 0, 255)
        self.frame_yield = False
        self.vram_surface = self._create_vram_surface()

    def _create_vram_surface(self):
        vram_size = SCREEN_WIDTH * SCREEN_HEIGHT
        framebuffer = memoryview(self.memory)[VRAM_START:VRAM_START + vram_size]
        surface = pygame.image.frombuffer(framebuffer, (SCREEN_WIDTH, SCREEN_HEIGHT), "P")
        surface.set_palette(self.palette)
        return surface

    def write(self, addr, val):
        addr &= MEM_SIZE - 1
        self.memory[addr] = val & 0xFF
        if addr == GPU_BASE:
            self.gpu_command(val & 0xFF)

    def _gpu_word(self, offset, signed=True):
        value = (self.memory[GPU_BASE + offset] << 8) | self.memory[GPU_BASE + offset + 1]
        return value - 0x10000 if signed and value & 0x8000 else value

    def _back_pixel(self, x, y, color):
        if 0 <= x < SCREEN_WIDTH and 0 <= y < SCREEN_HEIGHT:
            self.memory[BACK_START + y * SCREEN_WIDTH + x] = color

    def _gpu_source(self):
        return (self.memory[GPU_BASE + 12] << 16) | (self.memory[GPU_BASE + 10] << 8) | self.memory[GPU_BASE + 11]

    def gpu_command(self, command):
        """Run a graphics coprocessor command; drawing targets the back buffer."""
        color = self.memory[GPU_BASE + 1]
        x, y = self._gpu_word(2), self._gpu_word(4)
        w, h = self._gpu_word(6), self._gpu_word(8)
        if command == GPU_CLEAR:
            size = SCREEN_WIDTH * SCREEN_HEIGHT
            self.memory[BACK_START:BACK_START + size] = bytes([color]) * size
        elif command == GPU_PIXEL:
            self._back_pixel(x, y, color)
        elif command == GPU_RECT:
            left, right = max(x, 0), min(x + w, SCREEN_WIDTH)
            if right > left:
                row = bytes([color]) * (right - left)
                for row_y in range(max(y, 0), min(y + h, SCREEN_HEIGHT)):
                    start = BACK_START + row_y * SCREEN_WIDTH + left
                    self.memory[start:start + len(row)] = row
        elif command == GPU_LINE:
            self._gpu_line(x, y, w, h, color)
        elif command == GPU_POLY:
            self._gpu_polygon(x, y, w, color)
        elif command == GPU_SPRITE:
            source = self._gpu_source()
            for row in range(max(h, 0)):
                for column in range(max(w, 0)):
                    pixel = self.memory[(source + row * w + column) & (MEM_SIZE - 1)]
                    if pixel:
                        self._back_pixel(x + column, y + row, pixel)
        elif command == GPU_BITMAP:
            source = self._gpu_source()
            row_bytes = (max(w, 0) + 7) // 8
            for row in range(max(h, 0)):
                for column in range(max(w, 0)):
                    byte = self.memory[(source + row * row_bytes + column // 8) & (MEM_SIZE - 1)]
                    if byte & (0x80 >> (column % 8)):
                        self._back_pixel(x + column, y + row, color)
        elif command == GPU_TEXT:
            self._gpu_text(x, y, color)
        elif command == GPU_PRESENT:
            size = SCREEN_WIDTH * SCREEN_HEIGHT
            self.memory[VRAM_START:VRAM_START + size] = self.memory[BACK_START:BACK_START + size]
            self.frame_yield = True
        elif command == GPU_TICKS:
            ticks = getattr(self, 'frames', 0)
            self.memory[GPU_BASE + 14] = ticks >> 8
            self.memory[GPU_BASE + 15] = ticks & 0xFF
        elif command == GPU_RTC:
            index = self.memory[GPU_BASE + 1]
            self.memory[GPU_BASE + 14] = self.memory[RTC_START + index] if index < 3 else 0
        elif command == GPU_RANDOM:
            self.memory[GPU_BASE + 14] = random.randrange(256)

    def _gpu_line(self, x, y, end_x, end_y, color):
        dx, dy = abs(end_x - x), -abs(end_y - y)
        step_x, step_y = (1 if x < end_x else -1), (1 if y < end_y else -1)
        error = dx + dy
        for _ in range(2000):
            self._back_pixel(x, y, color)
            if x == end_x and y == end_y:
                break
            doubled = 2 * error
            if doubled >= dy:
                error += dy
                x += step_x
            if doubled <= dx:
                error += dx
                y += step_y

    def _gpu_polygon(self, x, y, count, color):
        """Draw a closed outline through `count` signed-byte (dx, dy) pairs at SRC."""
        source = self._gpu_source()
        points = []
        for index in range(max(min(count, 64), 0)):
            pair = [self.memory[(source + index * 2 + offset) & (MEM_SIZE - 1)] for offset in (0, 1)]
            points.append((x + pair[0] - (256 if pair[0] > 127 else 0),
                           y + pair[1] - (256 if pair[1] > 127 else 0)))
        for index, point in enumerate(points):
            if len(points) > 1:
                self._gpu_line(*point, *points[(index + 1) % len(points)], color)
            else:
                self._back_pixel(*point, color)

    def _gpu_text(self, x, y, color):
        source = self._gpu_source()
        raw = bytearray()
        while len(raw) < 255 and self.memory[(source + len(raw)) & (MEM_SIZE - 1)]:
            raw.append(self.memory[(source + len(raw)) & (MEM_SIZE - 1)])
        left = x
        for char in raw.decode("utf-8", errors="replace"):
            if char == "\n":
                x, y = left, y + 8
                continue
            if "\u0020" <= char <= "\u007e":
                glyph = ord(char) - 32
            elif char in "öäåÖÄÅ":
                glyph = 95 + "öäåÖÄÅ".index(char)
            else:
                glyph = ord("?") - 32
            for row in range(7):
                bits = self.memory[FONT_ADDR + glyph * 8 + row]
                for column in range(5):
                    if bits & (0x80 >> column):
                        self._back_pixel(x + column, y + row, color)
            x += 6

    def read(self, addr): return self.memory[addr & (MEM_SIZE - 1)]
    def read_16(self, addr):
        memory = self.memory
        return (memory[addr & (MEM_SIZE - 1)] << 8) | memory[(addr + 1) & (MEM_SIZE - 1)]

    def write_16(self, addr, val):
        self.write(addr, (val >> 8) & 0xFF)
        self.write(addr + 1, val & 0xFF)

    @staticmethod
    def _binary16(value):
        return struct.unpack(">e", (int(value) & 0xFFFF).to_bytes(2, "big"))[0]

    @staticmethod
    def _binary16_bits(value):
        try:
            return int.from_bytes(struct.pack(">e", value), "big")
        except OverflowError:
            return 0xFC00 if value < 0 else 0x7C00

    def load_program(self, image):
        for segment in image.segments:
            end = segment.address + len(segment.data)
            if segment.address < 0 or end > MEM_SIZE:
                raise ValueError("program segment does not fit in memory")
            self.memory[segment.address:end] = segment.data
        if not 0 <= image.entry_point < MEM_SIZE:
            raise ValueError("program entry point is outside memory")
        self.entry_point = image.entry_point

    def push(self, val):
        self.sp -= 2
        self.write_16(self.sp, val)

    def pop(self):
        val = self.read_16(self.sp)
        self.sp += 2
        return val

    def step(self):
        if not self.running: return
        memory = self.memory
        registers = self.registers
        pc = self.pc
        instr = (memory[pc & (MEM_SIZE - 1)] << 8) | memory[(pc + 1) & (MEM_SIZE - 1)]
        self.pc = pc + 2

        # 5-Bit Opcode Decoding
        opcode = (instr >> 11) & 0x1F
        rd = (instr >> 8) & 0x7
        rs1 = (instr >> 5) & 0x7
        rs2 = (instr >> 2) & 0x7
        imm = instr & 0xFF

        if instr >= 0xF800 and instr in (0xF81D, 0xF91D, 0xFA1D, 0xFB1D):
            target = self.read_16(self.pc) | (self.read_16(self.pc + 2) << 16)
            self.pc += 4
            if instr == 0xF91D:
                self.push(self.pc)
                self.pc = target
            elif instr == 0xF81D:
                self.pc = target
            elif instr == 0xFA1D and self.flags["Z"]:
                self.pc = target
            elif instr == 0xFB1D and self.flags["C"]:
                self.pc = target
        elif opcode == OP_HALT: self.running = False
        elif opcode == OP_LDI: registers[rd] = imm
        elif opcode == OP_LDI_H: registers[rd] = (imm << 8) | registers[rd]
        elif opcode == OP_MOV: registers[rd] = registers[rs1]
        elif opcode == OP_ADD:
            res = registers[rs1] + registers[rs2]
            registers[rd] = res
            self.flags['Z'] = 1 if (registers[rd] == 0) else 0
            self.flags['C'] = 1 if res > 0xFF else 0
        elif opcode == OP_SUB:
            res = registers[rs1] - registers[rs2]
            registers[rd] = res
            self.flags['Z'] = 1 if (registers[rd] == 0) else 0
            self.flags['C'] = 1 if res < 0 else 0
        elif opcode == OP_AND: registers[rd] = registers[rs1] & registers[rs2]
        elif opcode == OP_OR:  registers[rd] = registers[rs1] | registers[rs2]
        elif opcode == OP_XOR: registers[rd] = registers[rs1] ^ registers[rs2]
        elif opcode == OP_NOT: registers[rd] = (~registers[rs1]) & 0xFF
        elif opcode == OP_INC: registers[rd] = registers[rd] + 1
        elif opcode == OP_DEC: registers[rd] = registers[rd] - 1
        elif opcode == OP_SHR:
            extension = (instr >> 5) & 0x7
            if extension == 0:
                if instr & 0xFF == 0x1C:
                    registers[rd] = self.sp
                else:
                    registers[rd] = (registers[rd] & 0xFFFF) >> 1
            else:
                source = (instr >> 2) & 0x7
                left = self._binary16(registers[rd])
                right = self._binary16(registers[source])
                if extension == 1:
                    result = left + right
                    registers[rd] = self._binary16_bits(result)
                elif extension == 2:
                    result = left - right
                    registers[rd] = self._binary16_bits(result)
                elif extension == 3:
                    result = left * right
                    registers[rd] = self._binary16_bits(result)
                elif extension == 4:
                    if right == 0.0:
                        result = math.nan if left == 0.0 else math.copysign(math.inf, left * math.copysign(1.0, right))
                    else:
                        result = left / right
                    registers[rd] = self._binary16_bits(result)
                elif extension == 5:
                    self.flags["Z"] = int(left == right)
                    self.flags["C"] = int(left < right)
                elif extension == 6:
                    integer = registers[rd] & 0xFFFF
                    signed = integer - 0x10000 if integer & 0x8000 else integer
                    registers[rd] = self._binary16_bits(float(signed))
                elif extension == 7:
                    value = self._binary16(registers[rd])
                    registers[rd] = int(value) & 0xFFFF
        elif opcode == OP_CMP:
            res = registers[rs1] - registers[rs2]
            self.flags['Z'] = 1 if res == 0 else 0
            self.flags['C'] = 1 if res < 0 else 0
        elif opcode == OP_LD: registers[rd] = self.read(registers[rs1])
        elif opcode == OP_ST: self.write(registers[rs1], registers[rd])
        elif opcode == OP_LDW: registers[rd] = self.read_16(registers[rs1])
        elif opcode == OP_STW: self.write_16(registers[rs1], registers[rd])
        elif opcode == OP_LDWS: registers[rd] = self.read_16(self.sp + imm)
        elif opcode == OP_STWS: self.write_16(self.sp + imm, registers[rd])
        elif opcode == OP_ADJSP:
            offset = imm if imm < 0x80 else imm - 0x100
            self.sp = (self.sp + offset) & (MEM_SIZE - 1)
        elif opcode == OP_JMP: self.pc = instr & 0x7FF
        elif opcode == OP_JZ:
            if registers[rd] == 0: self.pc = imm
        elif opcode == OP_BZ:
            if self.flags['Z']: self.pc = instr & 0x7FF
        elif opcode == OP_BC:
            if self.flags['C']: self.pc = instr & 0x7FF
        elif opcode == OP_CALL:
            self.push(self.pc)
            self.pc = instr & 0x7FF
        elif opcode == OP_RET: self.pc = self.pop()
        elif opcode == OP_PUSH: self.push(registers[rd])
        elif opcode == OP_POP: registers[rd] = self.pop() & 0xFF
        elif opcode == OP_RTI:
            saved = self.pop()
            self.flags['Z'] = saved & 1
            self.flags['C'] = (saved >> 1) & 1
            self.pc = self.pop()
        elif opcode == OP_EI: self.write(ICR_ADDR, self.read(ICR_ADDR) | 0x01)
        elif opcode == OP_DI: self.write(ICR_ADDR, self.read(ICR_ADDR) & ~0x01)

        if memory[ICR_ADDR] & 0x01:
            self.check_interrupts()

    def check_interrupts(self):
        icr = self.read(ICR_ADDR)
        if (icr & 0x01) and (icr & 0x02): self.handle_interrupt(0x0002)
        elif (icr & 0x01) and (icr & 0x08): self.handle_interrupt(0x0004)

    def handle_interrupt(self, vector_addr):
        self.push(self.pc)
        self.push(self.flags['Z'] | (self.flags['C'] << 1))
        self.write(ICR_ADDR, self.read(ICR_ADDR) & ~0x01)
        self.write(ICR_ADDR, self.read(ICR_ADDR) & ~0x02 & ~0x08)
        self.pc = self.read_16(vector_addr)

    def update_rtc(self):
        now = datetime.datetime.now()
        self.write(RTC_START, now.second)
        self.write(RTC_START + 1, now.minute)
        self.write(RTC_START + 2, now.hour)
        if now.second % 1 == 0: self.write(ICR_ADDR, self.read(ICR_ADDR) | 0x08)

    def update_display(self):
        self.screen.blit(pygame.transform.scale(self.vram_surface.convert(self.screen), self.screen.get_size()), (0, 0))
        pygame.display.flip()

    def execute_frame(self):
        self.frame_yield = False
        self.frames = (getattr(self, 'frames', 0) + 1) & 0xFFFF
        for _ in range(INSTRUCTIONS_PER_FRAME):
            if not self.running or self.frame_yield:
                break
            self.step()

    def reset(self):
        print("SC-8 starting...")
        self.registers = [0]*8
        self.sp = 0x2FFFF
        self.pc = self.entry_point
        self.running = True

    def run(self):
        self.reset()
        self.window_open = True
        while self.window_open:
            for event in pygame.event.get():
                if event.type == pygame.QUIT:
                    self.window_open = False
                if event.type == pygame.KEYUP and event.key in KEY_BITS:
                    self.write(KEYS_ADDR, self.read(KEYS_ADDR) & ~KEY_BITS[event.key])
                if event.type == pygame.KEYDOWN:
                    if not self.running:
                        self.window_open = False
                        continue
                    if event.key in KEY_BITS:
                        self.write(KEYS_ADDR, self.read(KEYS_ADDR) | KEY_BITS[event.key])
                    if event.key == pygame.K_RETURN:
                        key = 13
                    elif event.key == pygame.K_BACKSPACE:
                        key = 8
                    else:
                        key = ord(event.unicode) if event.unicode else 0
                    self.write(KBD_ADDR, key)
                    self.write(ICR_ADDR, self.read(ICR_ADDR) | 0x02)

            if not self.window_open:
                break
            self.update_rtc()
            self.execute_frame()
            self.update_display()
            self.clock.tick(DISPLAY_FPS)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run the SC-8 emulator.")
    parser.add_argument(
        "program_file",
        nargs="?",
        help="optional .asm, .c, or flat .bin program to load",
    )
    args = parser.parse_args()

    try:
        program = None
        if args.program_file:
            program = load_program_file(args.program_file)
    except (AssemblyError, CCompilerError, OSError, ValueError) as exc:
        parser.error(str(exc))

    sc8 = SuomiCompute8()
    if program is not None:
        sc8.load_program(program)
    try:
        sc8.run()
    finally:
        pygame.quit()
