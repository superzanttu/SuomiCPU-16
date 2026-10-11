from collections import deque
import time


class ConsoleDevice:
    RX_CAPACITY = 4096
    OUTPUT_CAPACITY = 65536
    COLUMNS = 80
    ROWS = 25
    SCROLLBACK_ROWS = 1000

    def __init__(self):
        self.reset()

    def reset(self, preserve_history=False):
        self.receive_buffer = deque()
        if not preserve_history:
            self.output = bytearray()
            self.lines = [bytearray(b" " * self.COLUMNS) for _ in range(self.ROWS)]
            self.scrollback = deque(maxlen=self.SCROLLBACK_ROWS)
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
            self.scrollback.append(self.lines.pop(0))
            self.lines.append(bytearray(b" " * self.COLUMNS))
            self.row = self.ROWS - 1

    def clear_overrun(self):
        self.overrun = False

    def text_snapshot(self):
        return tuple(
            bytes(line).decode("ascii", errors="replace")
            for line in self.lines
        )

    def scrollback_snapshot(self):
        return tuple(
            bytes(line).decode("ascii", errors="replace")
            for line in (*self.scrollback, *self.lines)
        )

    def write_text(self, text):
        for value in text.encode("ascii", errors="replace") + b"\n":
            self.write_data(value)

    def render(self, surface, font, color, origin=(0, 0), line_height=16):
        x, y = origin
        for line_number, line in enumerate(self.lines):
            text = bytes(line).decode("ascii", errors="replace").rstrip()
            if text:
                surface.blit(font.render(text, True, color), (x, y + line_number * line_height))


class ConsoleWindow:
    def __init__(self, cpu):
        import tkinter as tk

        self.cpu = cpu
        self.root = tk.Tk()
        self.root.title("SuomiCPU-16 Console")
        self.root.resizable(False, False)
        self.root.protocol("WM_DELETE_WINDOW", self.hide)

        controls = tk.Frame(self.root)
        controls.pack(fill="x", padx=6, pady=(6, 2))
        tk.Button(controls, text="REBOOT", command=cpu.request_reboot).pack(
            side="left", padx=(0, 4)
        )
        tk.Button(
            controls,
            text="RESET",
            command=cpu.request_bios_reset,
        ).pack(side="left", padx=(0, 4))
        tk.Button(
            controls,
            text="COPY",
            command=self.copy_text,
        ).pack(side="left", padx=(0, 4))
        self.pause_button = tk.Button(
            controls,
            text="PAUSE",
            command=cpu.toggle_pause,
        )
        self.pause_button.pack(side="left")

        self.status_var = tk.StringVar()
        tk.Label(
            self.root,
            textvariable=self.status_var,
            anchor="w",
            font=("Consolas", 8),
        ).pack(fill="x", padx=7, pady=(1, 4))

        self.text = tk.Text(
            self.root,
            width=ConsoleDevice.COLUMNS,
            height=ConsoleDevice.ROWS,
            wrap="none",
            font=("Consolas", 10),
            background="#101820",
            foreground="#d4e3d4",
            insertbackground="#ffffff",
            padx=4,
            pady=4,
            undo=False,
        )
        self.text.pack(padx=6, pady=(0, 6))
        self.text.bind("<KeyPress>", self._on_key_press)
        self.text.bind("<MouseWheel>", self._on_mousewheel)
        self.text.bind("<Button-4>", self._on_mousewheel)
        self.text.bind("<Button-5>", self._on_mousewheel)
        self.scroll_offset = 0
        self.last_snapshot = None
        self.visible = True
        self.closed = False

    def _on_key_press(self, event):
        if (
            getattr(event, "state", 0) & 0x04
            and event.keysym.lower() == "c"
        ):
            return self.copy_text()
        if event.keysym in ("Return", "KP_Enter"):
            data = b"\n"
        elif event.keysym == "BackSpace":
            data = b"\x08"
        elif event.char and event.char.isprintable():
            data = event.char.encode("utf-8")
        else:
            data = b""
        if data:
            self.cpu._console_device().receive(data)
        return "break"

    def copy_text(self):
        selection = self.text.tag_ranges("sel")
        if selection:
            copied = self.text.get("sel.first", "sel.last")
        else:
            copied = self.text.get("1.0", "end-1c")
            copied = "\n".join(line.rstrip() for line in copied.splitlines()).strip("\n")
        if copied:
            self.root.clipboard_clear()
            self.root.clipboard_append(copied)
        return "break"

    def _on_mousewheel(self, event):
        if getattr(event, "num", None) == 4:
            rows = 3
        elif getattr(event, "num", None) == 5:
            rows = -3
        else:
            delta = getattr(event, "delta", 0)
            if delta > 0:
                rows = max(1, abs(delta) // 120)
            elif delta < 0:
                rows = -max(1, abs(delta) // 120)
            else:
                rows = 0
        snapshot = self.cpu._console_device().scrollback_snapshot()
        max_offset = max(0, len(snapshot) - ConsoleDevice.ROWS)
        self.scroll_offset = max(0, min(max_offset, self.scroll_offset + rows))
        self.last_snapshot = None
        return "break"

    def hide(self):
        self.root.withdraw()
        self.visible = False

    def show(self):
        if not self.closed:
            self.root.deiconify()
            self.root.lift()
            self.visible = True

    def toggle(self):
        if self.visible:
            self.hide()
        else:
            self.show()

    def pump(self):
        if self.closed:
            return
        self.root.update()

    def refresh(self, module_name, disk_name, boot_time, paused):
        now = time.monotonic()
        elapsed = max(0, int(now - boot_time))
        hours, remainder = divmod(elapsed, 3600)
        minutes, seconds = divmod(remainder, 60)
        uptime = f"{hours:02}:{minutes:02}:{seconds:02}"
        self.status_var.set(
            f"TIME {time.strftime('%H:%M:%S')}  UP {uptime}  "
            f"DISK {disk_name or 'none'}  MODULE {module_name or 'unnamed'}"
        )
        self.pause_button.configure(
            text="CONTINUE" if paused else "PAUSE",
            state="normal" if getattr(self.cpu, "program_loaded", False) else "disabled"
        )

        snapshot = self.cpu._console_device().scrollback_snapshot()
        if snapshot != self.last_snapshot:
            end = len(snapshot) - self.scroll_offset
            start = max(0, end - ConsoleDevice.ROWS)
            visible = list(snapshot[start:end])
            visible = [" " * ConsoleDevice.COLUMNS] * (ConsoleDevice.ROWS - len(visible)) + visible
            self.text.delete("1.0", "end")
            self.text.insert("1.0", "\n".join(visible))
            self.text.mark_set("insert", "end-1c")
            self.last_snapshot = snapshot

    def close(self):
        if not self.closed:
            self.closed = True
            self.root.destroy()
