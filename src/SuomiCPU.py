import pygame
import datetime
import math
import sys
import argparse
import array
import struct
import random
from pathlib import Path

# Tuomme tarvittavat asiat työkaluista
from tools.assembler import AssemblyError, AssemblyImage, MemorySegment, assemble_file
from tools.c_compiler import CCompilerError, compile_file
from tools.sc16net import DEFAULT_PORT, MAX_PAYLOAD, NetNode, UdpTransport

# =============================================================================
# ISA (Instruction Set Architecture) - Prosessorin "kieli"
# Tässä määritellään, mitä numeroita prosessori käyttää eri käskyille.
# =============================================================================
OP_HALT = 0x00 # Pysäytä kone
OP_LDI = 0x01  # Laita numero rekisteriin
OP_LD = 0x02   # Lue muistista rekisteriin
OP_ST = 0x03   # Tallenna rekisteristä muistiin
OP_MOV = 0x04  # Kopioi rekisteristä toiseen
OP_ADD = 0x05  # Laske yhteen
OP_SUB = 0x06  # Vähennä
OP_AND = 0x07  # JA-operaatio (bittitason)
OP_OR = 0x08   # TAI-operaatio (bittitason)
OP_XOR = 0x09  # XOR-operaatio (bittitason)
OP_NOT = 0x0A  # Käännä bitit
OP_JMP = 0x0B  # Hyppää tiettyyn kohtaan
OP_JZ = 0x0C   # Hyppää jos nolla
OP_INC = 0x0D  # Kasvata yhdellä
OP_DEC = 0x0E  # Vähennä yhdellä
OP_CMP = 0x0F  # Vertaa kahta lukua
OP_LDI_H = 0x10 # Laita numero rekisterin yläosaan
OP_EI = 0x11   # Salli keskeytykset
OP_DI = 0x12   # Estä keskeytykset
OP_RTI = 0x13  # Palaa keskeytyksestä
OP_PUSH = 0x14 # Työnnä pinoon
OP_POP = 0x15  # Poista pinosta
OP_CALL = 0x16 # Kutsu funktiota
OP_RET = 0x17  # Palaa funktiosta
OP_BZ = 0x18   # Hyppää jos nolla (toinen versio)
OP_LDW = 0x19  # Lue 16-bittinen sana muistista
OP_STW = 0x1A  # Tallenna 16-bittinen sana muistiin
OP_LDWS = 0x1B # Lue sana pinon suhteen
OP_STWS = 0x1C # Tallenna sana pinon suhteen
OP_ADJSP = 0x1D # Muuta pinon kokoa
OP_BC = 0x1E   # Hyppää jos kantolippu on päällä
OP_SHR = 0x1F  # Siirrä bittejä oikealle

# Erikoiskäskyt, jotka käyttävät samaa koodia mutta toimivat eri tavalla
OP_MOVSP = OP_SHR
OP_FADD = OP_SHR
OP_FSUB = OP_SHR
OP_FMUL = OP_SHR
OP_FDIV = OP_SHR
OP_FCMP = OP_SHR
OP_ITOF = OP_SHR
OP_FTOI = OP_SHR

# =============================================================================
# KONFIGURAATIO JA MUISTIKARTTA
# Tässä päätetään, mihin muistiin mitäkin laitetaan.
# =============================================================================
MEM_SIZE = 2**19       # Muistin kokonaiskoko (512 KB)
MEM_MASK = MEM_SIZE - 1
FLASH_START = 0x00000 # Ohjelman alkuosa
RAM_START   = 0x20000 # Työmuistin alku
VRAM_START  = 0x30000 # Videomuistin alku (tänne piirretään ruutu)
KBD_ADDR    = 0x43000 # Näppäimistön osoite
ICR_ADDR    = 0x43002 # Keskeytysten ohjaus
RTC_START   = 0x43010 # Reaaliaikakello
KEYS_ADDR   = 0x43004 # Mitkä näppäimet on painettuna
MOUSE_ADDR  = 0x43008 # Mouse: +0 X word, +2 Y word, +4 held buttons, +5 click events (one frame)
GPU_BASE    = 0x43020 # Grafiikkapiirin ohjausrekisterit
BACK_START  = 0x44000 # "Takapuskuri" - piirretään tänne, sitten siirretään ruudulle
FONT_ADDR   = 0x4400  # Fonttien paikka

# Grafiikkapiirin komennot
GPU_CLEAR, GPU_PIXEL, GPU_RECT, GPU_LINE = 1, 2, 3, 4
GPU_SPRITE, GPU_BITMAP, GPU_TEXT, GPU_PRESENT, GPU_RANDOM, GPU_POLY = 5, 6, 7, 8, 9, 10
GPU_TICKS = 11  # Laskee kuinka monta ruutua on kulunut
GPU_RTC = 12    # Kello-tieto
GPU_SOUND = 13  # Sound: COLOR=channel 0-3, X=frequency Hz (0=silence), Y=duration frames (0=loop), W=waveform, H=volume %
GPU_SAVE, GPU_LOAD = 14, 15  # Save/restore the back buffer (static background layer)
# LAN networking: SOURCE=buffer, X=length; results in bytes +14/+15 (word)
GPU_NET_OPEN, GPU_NET_CLOSE, GPU_NET_SEND, GPU_NET_RECV, GPU_NET_INFO = 16, 17, 18, 19, 20

SCREEN_WIDTH = 320
SCREEN_HEIGHT = 240
DOUBLE_CLICK_MS = 400  # Max gap between two clicks counted as a double click
WINDOW_SCALE = 3   # Tehdään ikkunasta isompi, jotta näkyy paremmin
DISPLAY_FPS = 60
INSTRUCTIONS_PER_FRAME = 30000 # Kuinka monta käskyä suoritetaan yhden kuvan välillä

# Värit, joita kone osaa käyttää
PALETTE_COLORS = [
    (0, 0, 0), (255, 255, 255), (255, 48, 48), (48, 220, 64),
    (64, 96, 255), (255, 224, 0), (0, 224, 224), (224, 64, 224),
    (255, 144, 0), (150, 150, 150), (80, 80, 80), (128, 0, 0),
]

# Näppäimistön yhdistykset
KEY_BITS = {
    pygame.K_LEFT: 1, pygame.K_a: 1, pygame.K_RIGHT: 2, pygame.K_d: 2,
    pygame.K_UP: 4, pygame.K_w: 4, pygame.K_DOWN: 8, pygame.K_s: 8,
    pygame.K_SPACE: 16, pygame.K_RETURN: 32, pygame.K_TAB: 64,
    # Extended keys are reported in the byte after KEYS_ADDR (see gfx_keys_ext).
    pygame.K_q: 0x100, pygame.K_e: 0x200,
    pygame.K_LSHIFT: 0x400, pygame.K_RSHIFT: 0x400, pygame.K_x: 0x400,
    pygame.K_LCTRL: 0x800, pygame.K_RCTRL: 0x800, pygame.K_z: 0x800,
}

def load_program_file(path: str | Path) -> AssemblyImage:
    """Lataa ohjelman tiedostosta (.asm, .c tai .bin)."""
    program_path = Path(path)
    if program_path.suffix.lower() == ".c":
        return compile_file(program_path)
    if program_path.suffix.lower() == ".bin":
        binary = program_path.read_bytes()
        if len(binary) > MEM_SIZE:
            raise ValueError("Ohjelma on liian suuri muistiin")
        return AssemblyImage((MemorySegment(0, binary),), 0)
    return assemble_file(program_path)

class SuomiCompute16:
    """Tämä on itse tietokoneen sydän (emulaattori)."""
    def __init__(self):
        # Muisti on kuin pitkä jono numeroita
        self.memory = bytearray(MEM_SIZE)
        self._last_click = [(-10**9, 0, 0), (-10**9, 0, 0)]
        # Rekisterit ovat koneen "lyhytkestoisia muistipaikkoja"
        self.registers = [0] * 8
        # PC (Program Counter) kertoo, missä kohtaa ohjelmaa ollaan
        self.pc = 0
        self.entry_point = 0
        # Pino (Stack) on paikka, jonne tallennetaan tietoa funktioiden ajaksi
        self.sp = 0x2FFFF
        self.flags = {'Z': 0, 'C': 0, 'N': 0, 'V': 0} # Z = nolla, C = kanto (käytetään vertailuissa)
        self.running = True
        
        # Käynnistetään näyttö ja ikkuna
        pygame.init()
        self.screen = pygame.display.set_mode((SCREEN_WIDTH * WINDOW_SCALE, SCREEN_HEIGHT * WINDOW_SCALE))
        pygame.display.set_caption("SuomiCPU-16 Emulaattori")
        self.clock = pygame.time.Clock()
        
        # Luodaan väripaletti
        self.palette = [pygame.Color(i, i, i) for i in range(256)]
        for index, color in enumerate(PALETTE_COLORS):
            self.palette[index] = color
        self.palette[25] = (0, 0, 255) # Lisätään yksi sininen väri
        
        self.frame_yield = False
        self.vram_surface = self._create_vram_surface()

        self.entry_point = 0
        self.sp = 0x2FFFF
        self.flags = {'Z': 0, 'C': 0, 'N': 0, 'V': 0}
        self.running = True
        
        pygame.init()
        self.screen = pygame.display.set_mode((SCREEN_WIDTH * WINDOW_SCALE, SCREEN_HEIGHT * WINDOW_SCALE))
        pygame.display.set_caption("SuomiCPU-16 Emulator")
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
        elif command == GPU_SOUND:
            self._gpu_sound(color, self._gpu_word(2, False), self._gpu_word(4, False),
                            self._gpu_word(6, False), self._gpu_word(8, False))
        elif command == GPU_SAVE:
            size = SCREEN_WIDTH * SCREEN_HEIGHT
            self._saved_layer = bytes(self.memory[BACK_START:BACK_START + size])
        elif command == GPU_LOAD:
            size = SCREEN_WIDTH * SCREEN_HEIGHT
            saved = getattr(self, '_saved_layer', None)
            if saved is not None:
                self.memory[BACK_START:BACK_START + size] = saved
        elif command == GPU_RANDOM:
            self.memory[GPU_BASE + 14] = random.randrange(256)
        elif GPU_NET_OPEN <= command <= GPU_NET_INFO:
            self._gpu_net(command, color, x)

    def _net_node(self):
        node = getattr(self, 'net', None)
        if node is None:
            factory = getattr(self, 'net_transport_factory', None)
            try:
                transport = factory() if factory else UdpTransport(getattr(self, 'net_port', DEFAULT_PORT))
            except OSError:
                return None
            node = self.net = NetNode(transport)
        return node

    def _gpu_result(self, value):
        value &= 0xFFFF
        self.memory[GPU_BASE + 14] = value >> 8
        self.memory[GPU_BASE + 15] = value & 0xFF

    def _gpu_net(self, command, query, arg):
        """Networking commands; see tools/sc16net.py for the protocol."""
        node = self._net_node()
        if node is None:
            self._gpu_result(0)
            return
        node.poll()
        source = self._gpu_source()
        if command == GPU_NET_OPEN:
            title = bytearray()
            while len(title) < 16 and self.memory[(source + len(title)) & (MEM_SIZE - 1)]:
                title.append(self.memory[(source + len(title)) & (MEM_SIZE - 1)])
            self._gpu_result(1 if node.open(bytes(title)) else 0)
        elif command == GPU_NET_CLOSE:
            node.close()
        elif command == GPU_NET_SEND:
            length = max(0, min(arg, MAX_PAYLOAD))
            data = bytes(self.memory[(source + i) & (MEM_SIZE - 1)] for i in range(length))
            self._gpu_result(1 if length and node.send(data) else 0)
        elif command == GPU_NET_RECV:
            message = node.recv()
            if message is None:
                self._gpu_result(0)
            else:
                for i, byte in enumerate(message[1]):
                    self.memory[(source + i) & (MEM_SIZE - 1)] = byte
                self._gpu_result(len(message[1]))
        else:
            self._gpu_result({
                0: lambda: node.players(),
                1: lambda: -1 if node.slot is None else node.slot,
                2: lambda: int(node.active(arg)),
                3: lambda: int(node.full),
                4: lambda: node.last_sender,
                5: lambda: int(node.ready),
                6: lambda: int(node.is_open),
            }.get(query, lambda: 0)())

    SOUND_RATE = 22050
    SOUND_FPS = 60

    def _gpu_sound(self, channel, freq, frames, wave, volume):
        """Play a synthesized tone on a channel. wave: 0 square, 1 noise, 2 triangle.

        frames == 0 loops until the channel is changed or silenced (freq == 0).
        """
        channel &= 3
        log = getattr(self, 'sound_log', None)
        if log is not None:
            log.append((channel, freq, frames, wave, volume))
        if not getattr(self, 'audio_enabled', True):
            return
        key = (freq, frames, wave, volume)
        playing = getattr(self, '_sound_playing', None)
        if playing is None:
            playing = self._sound_playing = {}
            self._sound_cache = {}
            try:
                pygame.mixer.quit()
                pygame.mixer.init(self.SOUND_RATE, -16, 1, 512)
                pygame.mixer.set_num_channels(4)
            except pygame.error:
                self.audio_enabled = False
                return
        mixer_channel = pygame.mixer.Channel(channel)
        if freq == 0:
            mixer_channel.stop()
            playing.pop(channel, None)
            return
        if frames == 0 and playing.get(channel) == key and mixer_channel.get_busy():
            return
        sound = self._sound_cache.get(key)
        if sound is None:
            sound = self._sound_cache[key] = pygame.mixer.Sound(
                buffer=self._synthesize(freq, frames, wave, volume))
        mixer_channel.play(sound, loops=-1 if frames == 0 else 0)
        playing[channel] = key

    def _synthesize(self, freq, frames, wave, volume):
        rate = self.SOUND_RATE
        freq = max(20, min(freq, rate // 2))
        period = max(2, round(rate / freq))
        if frames:
            count = frames * rate // self.SOUND_FPS
        else:
            count = period * max(1, (rate // 4) // period) if wave != 1 else rate // 2
        amplitude = 32767 * max(1, min(volume or 100, 100)) // 100 // 2
        samples = array.array('h')
        held = 0
        for index in range(count):
            if wave == 1:
                if index % period == 0:
                    held = random.randrange(-amplitude, amplitude + 1)
                value = held
            elif wave == 2:
                phase = (index % period) / period
                value = int(amplitude * (4 * abs(phase - 0.5) - 1))
            else:
                value = amplitude if (index % period) < period // 2 else -amplitude
            if frames:
                value = value * (count - index) // count
            samples.append(value)
        return samples.tobytes()

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

    def _execute_muldiv(self, instr):
        """Run the MUL/DIV/MOD group; `instr` is the 0xF81E prefix word."""
        rd = (instr >> 8) & 0x7
        control = self.read_16(self.pc)
        self.pc += 2
        if control & 0x0800:
            right = self.read_16(self.pc)
            self.pc += 2
        else:
            right = self.registers[control & 0x7]
        left = self.registers[rd] & 0xFFFF
        right &= 0xFFFF
        signed_left = left - 0x10000 if left & 0x8000 else left
        signed_right = right - 0x10000 if right & 0x8000 else right
        operation = control >> 12
        carry = 0
        if operation == 0:
            product = left * right
            result = product & 0xFFFF
            carry = int(product > 0xFFFF)
        elif operation == 1:
            result = (left * right) >> 16
        elif operation == 2:
            result = ((signed_left * signed_right) >> 16) & 0xFFFF
        elif right == 0:
            # Division by zero: quotient 0, remainder = dividend, C flag set.
            result = left if operation in (5, 6) else 0
            carry = 1
        elif operation == 3:
            result = left // right
        elif operation == 5:
            result = left % right
        else:
            quotient = abs(signed_left) // abs(signed_right)
            if (signed_left < 0) != (signed_right < 0):
                quotient = -quotient
            if operation == 4:
                result = quotient & 0xFFFF
            else:
                result = (signed_left - quotient * signed_right) & 0xFFFF
        self.registers[rd] = result
        self.flags['Z'] = int(result == 0)
        self.flags['C'] = carry

    def _set_flags(self, result, carry, overflow=0):
        """Set Z, N, C and V from a 16-bit result."""
        result &= 0xFFFF
        self.flags['Z'] = int(result == 0)
        self.flags['N'] = result >> 15
        self.flags['C'] = int(carry)
        self.flags['V'] = int(overflow)

    def _set_nv(self, left, right, result, subtract):
        """Set N and V after the classic ADD/SUB/CMP (Z and C keep their old meaning)."""
        left &= 0xFFFF
        right &= 0xFFFF
        result &= 0xFFFF
        if subtract:
            overflow = ((left ^ right) & (left ^ result) & 0x8000) != 0
        else:
            overflow = ((left ^ result) & (right ^ result) & 0x8000) != 0
        self.flags['N'] = result >> 15
        self.flags['V'] = int(overflow)

    def _flag_condition(self, condition):
        """Evaluate a far-branch condition code (see isa.BRANCH_CONDITIONS)."""
        flags = self.flags
        zero, carry = flags['Z'], flags['C']
        negative, overflow = flags.get('N', 0), flags.get('V', 0)
        if condition == 0: return not zero
        if condition == 1: return not carry
        if condition == 2: return bool(negative)
        if condition == 3: return not negative
        if condition == 4: return bool(overflow)
        if condition == 5: return not overflow
        if condition == 6: return negative != overflow
        if condition == 7: return negative == overflow
        if condition == 8: return not zero and negative == overflow
        if condition == 9: return bool(zero) or negative != overflow
        if condition == 10: return not carry and not zero
        if condition == 11: return bool(carry) or bool(zero)
        return False

    def _execute_alu(self, instr):
        """Run the extended ALU group; `instr` is the 0xF81F prefix word."""
        rd = (instr >> 8) & 0x7
        control = self.read_16(self.pc)
        self.pc += 2
        operation = control >> 12
        left = self.registers[rd] & 0xFFFF
        if operation == 15:
            self._execute_misc(rd, left, control & 0xF)
            return
        immediate = bool(control & 0x0800)
        if operation <= 4:
            count = (control & 0xF) if immediate else self.registers[control & 0x7] & 0xF
            self._execute_shift(rd, left, operation, count)
            return
        if operation == 7:
            result = -left & 0xFFFF
            self._set_flags(result, left != 0, left == 0x8000)
            self.registers[rd] = result
            return
        if immediate:
            right = self.read_16(self.pc) & 0xFFFF
            self.pc += 2
        else:
            right = self.registers[control & 0x7] & 0xFFFF
        carry_in = self.flags['C']
        if operation in (5, 9):
            total = left + right + (carry_in if operation == 5 else 0)
            result = total & 0xFFFF
            overflow = ((left ^ result) & (right ^ result) & 0x8000) != 0
            self._set_flags(result, total > 0xFFFF, overflow)
        elif operation in (6, 10, 14):
            total = left - right - (carry_in if operation == 6 else 0)
            result = total & 0xFFFF
            overflow = ((left ^ right) & (left ^ result) & 0x8000) != 0
            self._set_flags(result, total < 0, overflow)
        else:
            if operation in (8, 11):
                result = left & right
            elif operation == 12:
                result = left | right
            else:
                result = left ^ right
            self._set_flags(result, 0, 0)
        if operation not in (8, 14):
            self.registers[rd] = result

    def _execute_shift(self, rd, value, operation, count):
        """Shift or rotate `value` by `count` bits (0-15); C gets the last bit out."""
        if count == 0:
            self._set_flags(value, self.flags['C'], 0)
            self.registers[rd] = value
            return
        if operation == 0:
            result = (value << count) & 0xFFFF
            carry = (value >> (16 - count)) & 1
        elif operation == 1:
            result = value >> count
            carry = (value >> (count - 1)) & 1
        elif operation == 2:
            signed = value - 0x10000 if value & 0x8000 else value
            result = (signed >> count) & 0xFFFF
            carry = (signed >> (count - 1)) & 1
        elif operation == 3:
            result = ((value << count) | (value >> (16 - count))) & 0xFFFF
            carry = result & 1
        else:
            result = ((value >> count) | (value << (16 - count))) & 0xFFFF
            carry = result >> 15
        self._set_flags(result, carry, 0)
        self.registers[rd] = result

    def _execute_misc(self, rd, value, selector):
        """Run the miscellaneous register instructions (ALU operation 15)."""
        if selector == 0:
            self.registers[rd] = ((value << 8) | (value >> 8)) & 0xFFFF
        elif selector == 1:
            result = (value & 0xFF) | (0xFF00 if value & 0x80 else 0)
            self.registers[rd] = result
            self._set_flags(result, self.flags['C'], 0)
        elif selector == 2:
            self.pc = value
        elif selector == 3:
            self.push(self.pc)
            self.pc = value
        elif selector == 4:
            self.push(self._pack_flags())
        elif selector == 5:
            self._restore_flags(self.pop())
        elif selector == 6:
            self.flags['C'] = 1
        elif selector == 7:
            self.flags['C'] = 0
        elif selector == 8:
            self.sp = value
        elif selector == 9:
            self.registers[rd] = 16 - value.bit_length()
            self.flags['Z'] = int(value == 0)

    def _pack_flags(self):
        flags = self.flags
        return flags['Z'] | (flags['C'] << 1) | (flags.get('N', 0) << 2) | (flags.get('V', 0) << 3)

    def _restore_flags(self, saved):
        self.flags['Z'] = saved & 1
        self.flags['C'] = (saved >> 1) & 1
        self.flags['N'] = (saved >> 2) & 1
        self.flags['V'] = (saved >> 3) & 1

    def step(self):
        if not self.running: return
        memory = self.memory
        registers = self.registers
        pc = self.pc
        instr = (memory[pc & MEM_MASK] << 8) | memory[(pc + 1) & MEM_MASK]
        self.pc = pc + 2

        # 5-Bit Opcode Decoding
        opcode = (instr >> 11) & 0x1F
        rd = (instr >> 8) & 0x7
        rs1 = (instr >> 5) & 0x7
        rs2 = (instr >> 2) & 0x7
        imm = instr & 0xFF

        if opcode == OP_LDI: registers[rd] = imm
        elif opcode == OP_SHR and (instr & 0xF8FF) == 0xF81E:
            self._execute_muldiv(instr)
        elif opcode == OP_SHR and (instr & 0xF8FF) == 0xF81F:
            self._execute_alu(instr)
        elif opcode == OP_SHR and (instr & 0xF81F) == 0xF81B:
            condition = ((instr >> 8) & 0x7) | (((instr >> 5) & 0x7) << 3)
            target = self.read_16(self.pc) | (self.read_16(self.pc + 2) << 16)
            self.pc += 4
            if self._flag_condition(condition):
                self.pc = target
        elif opcode == OP_SHR and instr in (0xF81D, 0xF91D, 0xFA1D, 0xFB1D):
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
        elif opcode == OP_LDWS:
            addr = self.sp + imm
            registers[rd] = (memory[addr & MEM_MASK] << 8) | memory[(addr + 1) & MEM_MASK]
        elif opcode == OP_ADD:
            a = registers[rs1]
            b = registers[rs2]
            res = a + b
            registers[rd] = res
            flags = self.flags
            flags['Z'] = 1 if res == 0 else 0
            flags['C'] = 1 if res > 0xFF else 0
            r16 = res & 0xFFFF
            flags['N'] = r16 >> 15
            flags['V'] = 1 if ((a ^ r16) & (b ^ r16) & 0x8000) else 0
        elif opcode == OP_LDI_H: registers[rd] = (imm << 8) | registers[rd]
        elif opcode == OP_PUSH:
            sp = self.sp - 2
            self.sp = sp
            addr = sp & MEM_MASK
            if GPU_BASE - 1 <= addr <= GPU_BASE:
                self.write_16(sp, registers[rd])
            else:
                value = registers[rd]
                memory[addr] = (value >> 8) & 0xFF
                memory[(addr + 1) & MEM_MASK] = value & 0xFF
        elif opcode == OP_ADJSP:
            offset = imm if imm < 0x80 else imm - 0x100
            self.sp = (self.sp + offset) & (MEM_SIZE - 1)
        elif opcode == OP_MOV: registers[rd] = registers[rs1]
        elif opcode == OP_CMP:
            a = registers[rs1]
            b = registers[rs2]
            res = a - b
            flags = self.flags
            flags['Z'] = 1 if res == 0 else 0
            flags['C'] = 1 if res < 0 else 0
            r16 = res & 0xFFFF
            flags['N'] = r16 >> 15
            flags['V'] = 1 if ((a ^ b) & (a ^ r16) & 0x8000) else 0
        elif opcode == OP_RET: self.pc = self.pop()
        elif opcode == OP_AND: registers[rd] = registers[rs1] & registers[rs2]
        elif opcode == OP_LDW:
            addr = registers[rs1]
            registers[rd] = (memory[addr & MEM_MASK] << 8) | memory[(addr + 1) & MEM_MASK]
        elif opcode == OP_STWS:
            addr = (self.sp + imm) & MEM_MASK
            if GPU_BASE - 1 <= addr <= GPU_BASE:
                self.write_16(addr, registers[rd])
            else:
                value = registers[rd]
                memory[addr] = (value >> 8) & 0xFF
                memory[(addr + 1) & MEM_MASK] = value & 0xFF
        elif opcode == OP_BZ:
            if self.flags['Z']: self.pc = instr & 0x7FF
        elif opcode == OP_BC:
            if self.flags['C']: self.pc = instr & 0x7FF
        elif opcode == OP_JMP: self.pc = instr & 0x7FF
        elif opcode == OP_LD: registers[rd] = self.read(registers[rs1])
        elif opcode == OP_ST: self.write(registers[rs1], registers[rd])
        elif opcode == OP_STW: self.write_16(registers[rs1], registers[rd])
        elif opcode == OP_OR:  registers[rd] = registers[rs1] | registers[rs2]
        elif opcode == OP_SUB:
            a = registers[rs1]
            b = registers[rs2]
            res = a - b
            registers[rd] = res
            flags = self.flags
            flags['Z'] = 1 if res == 0 else 0
            flags['C'] = 1 if res < 0 else 0
            r16 = res & 0xFFFF
            flags['N'] = r16 >> 15
            flags['V'] = 1 if ((a ^ b) & (a ^ r16) & 0x8000) else 0
        elif opcode == OP_XOR: registers[rd] = registers[rs1] ^ registers[rs2]
        elif opcode == OP_INC: registers[rd] = registers[rd] + 1
        elif opcode == OP_DEC: registers[rd] = registers[rd] - 1
        elif opcode == OP_JZ:
            if registers[rd] == 0: self.pc = imm
        elif opcode == OP_CALL:
            self.push(self.pc)
            self.pc = instr & 0x7FF
        elif opcode == OP_POP: registers[rd] = self.pop() & 0xFFFF
        elif opcode == OP_NOT: registers[rd] = (~registers[rs1]) & 0xFFFF
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
        elif opcode == OP_HALT: self.running = False
        elif opcode == OP_RTI:
            self._restore_flags(self.pop())
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
        self.push(self._pack_flags())
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

    # Mouse button bits: held (MOUSE_ADDR+4) bit0 left, bit1 right.
    # Events (MOUSE_ADDR+5), valid for one frame: bit0/1 left/right press, bit2/3 left/right double click,
    # bit4/5 left/right release (drop). Drag = button held while the position changes.
    def set_mouse_position(self, x, y):
        x = max(0, min(SCREEN_WIDTH - 1, x))
        y = max(0, min(SCREEN_HEIGHT - 1, y))
        self.memory[MOUSE_ADDR:MOUSE_ADDR + 4] = bytes((x >> 8, x & 0xFF, y >> 8, y & 0xFF))

    def mouse_button(self, button, down, x=0, y=0, now_ms=0):
        """Record a press or release of button 0 (left) or 1 (right); presses raise click events."""
        bit = 1 << button
        if not down:
            self.memory[MOUSE_ADDR + 4] &= ~bit
            self.memory[MOUSE_ADDR + 5] |= bit << 4
            return
        self.memory[MOUSE_ADDR + 4] |= bit
        self.memory[MOUSE_ADDR + 5] |= bit
        last_time, last_x, last_y = self._last_click[button]
        if now_ms - last_time <= DOUBLE_CLICK_MS and abs(x - last_x) <= 4 and abs(y - last_y) <= 4:
            self.memory[MOUSE_ADDR + 5] |= bit << 2
            self._last_click[button] = (-10**9, 0, 0)
        else:
            self._last_click[button] = (now_ms, x, y)

    def begin_mouse_frame(self):
        px, py = pygame.mouse.get_pos()
        self.set_mouse_position(px // WINDOW_SCALE, py // WINDOW_SCALE)
        node = getattr(self, 'net', None)
        if node is not None:
            node.poll()

    def end_mouse_frame(self):
        self.memory[MOUSE_ADDR + 5] = 0

    def press_keys(self, mask):
        self.write(KEYS_ADDR, self.read(KEYS_ADDR) | (mask & 0xFF))
        self.write(KEYS_ADDR + 1, self.read(KEYS_ADDR + 1) | (mask >> 8))

    def release_keys(self, mask):
        self.write(KEYS_ADDR, self.read(KEYS_ADDR) & ~(mask & 0xFF))
        self.write(KEYS_ADDR + 1, self.read(KEYS_ADDR + 1) & ~(mask >> 8))

    def reset(self):
        print("SC-16 starting...")
        self.registers = [0]*8
        self.sp = 0x2FFFF
        self.pc = self.entry_point
        self.running = True

    def run(self):
        self.reset()
        self.window_open = True
        deferred_release = 0
        while self.window_open:
            fresh_keys = 0
            for event in pygame.event.get():
                if event.type == pygame.QUIT:
                    self.window_open = False
                if event.type in (pygame.MOUSEBUTTONDOWN, pygame.MOUSEBUTTONUP) and event.button in (1, 3):
                    mx, my = event.pos[0] // WINDOW_SCALE, event.pos[1] // WINDOW_SCALE
                    self.set_mouse_position(mx, my)
                    self.mouse_button(0 if event.button == 1 else 1, event.type == pygame.MOUSEBUTTONDOWN,
                                      mx, my, pygame.time.get_ticks())
                if event.type == pygame.KEYUP and event.key in KEY_BITS:
                    # A tap shorter than one frame stays visible for that frame.
                    if KEY_BITS[event.key] & fresh_keys:
                        deferred_release |= KEY_BITS[event.key]
                    else:
                        self.release_keys(KEY_BITS[event.key])
                if event.type == pygame.KEYDOWN:
                    if not self.running:
                        self.window_open = False
                        continue
                    if event.key in KEY_BITS:
                        fresh_keys |= KEY_BITS[event.key]
                        deferred_release &= ~KEY_BITS[event.key]
                        self.press_keys(KEY_BITS[event.key])
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
            self.begin_mouse_frame()
            self.update_rtc()
            self.execute_frame()
            self.end_mouse_frame()
            if deferred_release:
                self.release_keys(deferred_release)
                deferred_release = 0
            self.update_display()
            self.clock.tick(DISPLAY_FPS)

def main():
    parser = argparse.ArgumentParser(description="Run the SC-16 emulator.")
    parser.add_argument(
        "program_file",
        nargs="?",
        help="optional .asm, .c, or flat .bin program to load",
    )
    parser.add_argument("--net-port", type=int, default=DEFAULT_PORT,
                        help="UDP port for LAN games (all players must use the same port)")
    args = parser.parse_args()

    try:
        program = None
        if args.program_file:
            program = load_program_file(args.program_file)
    except (AssemblyError, CCompilerError, OSError, ValueError) as exc:
        parser.error(str(exc))

    sc16 = SuomiCompute16()
    sc16.net_port = args.net_port
    if program is not None:
        sc16.load_program(program)
    try:
        sc16.run()
    finally:
        if getattr(sc16, 'net', None) is not None:
            sc16.net.close()
        pygame.quit()

if __name__ == "__main__":
    main()
