from collections import deque


class ConsoleDevice:
    RX_CAPACITY = 4096
    OUTPUT_CAPACITY = 65536
    COLUMNS = 80
    ROWS = 25

    def __init__(self):
        self.reset()

    def reset(self):
        self.receive_buffer = deque()
        self.output = bytearray()
        self.lines = [bytearray(b" " * self.COLUMNS) for _ in range(self.ROWS)]
        self.column = 0
        self.row = 0
        self.overrun = False
        self.control = 0x03

    @property
    def status(self):
        return int(bool(self.receive_buffer)) | 0x02 | (0x04 if self.overrun else 0)

    def receive(self, data):
        if not self.control & 0x02:
            return
        for value in data:
            if len(self.receive_buffer) == self.RX_CAPACITY:
                self.overrun = True
                break
            self.receive_buffer.append(value & 0xFF)

    def read_data(self):
        if not self.receive_buffer:
            return 0
        value = self.receive_buffer.popleft()
        if self.control & 0x01:
            self.write_data(value)
        return value

    def write_data(self, value):
        value &= 0xFF
        self.output.append(value)
        if len(self.output) > self.OUTPUT_CAPACITY:
            del self.output[:self.OUTPUT_CAPACITY // 4]

        if value == 10:
            self.column = 0
            self.row += 1
        elif value == 13:
            self.column = 0
        elif value == 8:
            self.column = max(0, self.column - 1)
            self.lines[self.row][self.column] = 32
        elif value == 9:
            self.column = min(self.COLUMNS, (self.column // 8 + 1) * 8)
        elif value >= 32:
            if self.column >= self.COLUMNS:
                self.column = 0
                self.row += 1
            self.lines[self.row][self.column] = value
            self.column += 1

        if self.row >= self.ROWS:
            self.lines.pop(0)
            self.lines.append(bytearray(b" " * self.COLUMNS))
            self.row = self.ROWS - 1

    def clear_overrun(self):
        self.overrun = False

    def render(self, surface, font, color, origin=(0, 0), line_height=16):
        x, y = origin
        for line_number, line in enumerate(self.lines):
            text = bytes(line).decode("ascii", errors="replace").rstrip()
            if text:
                surface.blit(font.render(text, True, color), (x, y + line_number * line_height))
