"""Mouse-driven launcher for C modules in the SC-16 modules directory."""

from dataclasses import dataclass
from pathlib import Path

import pygame

from src.SuomiCPU import (
    CCompilerError,
    SCREEN_HEIGHT,
    SCREEN_WIDTH,
    WINDOW_SCALE,
    SuomiCompute16,
    load_program_file,
)
from tools.assembler import AssemblyError
from tools.sc16net import DEFAULT_PORT


BACKGROUND = (10, 18, 31)
PANEL = (18, 32, 49)
PANEL_SELECTED = (25, 58, 76)
ACCENT = (40, 220, 190)
TEXT = (226, 239, 241)
MUTED = (133, 164, 176)
ERROR = (255, 125, 112)

ROW_HEIGHT = 42
LIST_TOP = 48
LIST_BOTTOM = 204


@dataclass(frozen=True)
class Module:
    path: Path
    name: str
    description: str


def _description_text(first_line: str) -> str:
    """Remove common C comment wrappers while keeping the first-line text."""
    text = first_line.strip().lstrip("\ufeff")
    if text.startswith("//"):
        return text[2:].strip()
    if text.startswith("/*"):
        text = text[2:]
    if text.endswith("*/"):
        text = text[:-2]
    return text.strip().lstrip("*").strip()


def discover_modules(modules_dir: str | Path) -> list[Module]:
    """Return C source modules, ordered by their filename."""
    directory = Path(modules_dir)
    if not directory.is_dir():
        raise FileNotFoundError(f"Modules folder does not exist: {directory}")
    modules = []
    for path in sorted(directory.glob("*.c"), key=lambda item: item.name.casefold()):
        with path.open("r", encoding="utf-8-sig") as source:
            first_line = source.readline().strip()
        modules.append(
            Module(
                path=path,
                name=path.stem,
                description=_description_text(first_line) or "No description",
            )
        )
    return modules


class SCLauncher:
    def __init__(self, modules_dir: str | Path, net_port: int = DEFAULT_PORT):
        pygame.init()
        self.screen = pygame.display.set_mode(
            (SCREEN_WIDTH * WINDOW_SCALE, SCREEN_HEIGHT * WINDOW_SCALE)
        )
        pygame.display.set_caption("SC-16 Launcher")
        self.clock = pygame.time.Clock()
        self.canvas = pygame.Surface((SCREEN_WIDTH, SCREEN_HEIGHT))
        self.title_font = pygame.font.Font(None, 25)
        self.name_font = pygame.font.Font(None, 18)
        self.body_font = pygame.font.Font(None, 15)
        self.small_font = pygame.font.Font(None, 13)
        self.modules_dir = Path(modules_dir)
        self.net_port = net_port
        self.modules: list[Module] = []
        self.selected = 0
        self.scroll = 0
        self.error_message = ""
        self.running = True
        try:
            self.modules = discover_modules(self.modules_dir)
        except (OSError, UnicodeError) as exc:
            self.error_message = f"Cannot read modules folder: {exc}"

    @property
    def visible_rows(self) -> int:
        return (LIST_BOTTOM - LIST_TOP) // ROW_HEIGHT

    def _clamp_selection(self) -> None:
        if not self.modules:
            self.selected = 0
            self.scroll = 0
            return
        self.selected = max(0, min(self.selected, len(self.modules) - 1))
        self.scroll = max(0, min(self.scroll, len(self.modules) - self.visible_rows))
        if self.selected < self.scroll:
            self.scroll = self.selected
        elif self.selected >= self.scroll + self.visible_rows:
            self.scroll = self.selected - self.visible_rows + 1

    def _scroll_by(self, amount: int) -> None:
        max_scroll = max(0, len(self.modules) - self.visible_rows)
        self.scroll = max(0, min(max_scroll, self.scroll + amount))
        if self.selected < self.scroll:
            self.selected = self.scroll
        elif self.selected >= self.scroll + self.visible_rows:
            self.selected = self.scroll + self.visible_rows - 1
        if self.modules:
            self._clamp_selection()

    def _draw_text(self, text: str, font: pygame.font.Font, color, position) -> None:
        self.canvas.blit(font.render(text, True, color), position)

    def _draw(self) -> None:
        self.canvas.fill(BACKGROUND)
        pygame.draw.rect(self.canvas, PANEL, (0, 0, SCREEN_WIDTH, 37))
        pygame.draw.rect(self.canvas, ACCENT, (0, 36, SCREEN_WIDTH, 2))
        self._draw_text("SC-LAUNCHER", self.title_font, ACCENT, (11, 7))
        count_label = f"{len(self.modules)} MODULES"
        count_surface = self.small_font.render(count_label, True, MUTED)
        self.canvas.blit(count_surface, (SCREEN_WIDTH - count_surface.get_width() - 11, 13))

        if not self.modules:
            self._draw_text("No C modules found in modules/", self.body_font, MUTED, (15, 72))
        else:
            for row in range(self.visible_rows):
                index = self.scroll + row
                if index >= len(self.modules):
                    break
                module = self.modules[index]
                y = LIST_TOP + row * ROW_HEIGHT
                selected = index == self.selected
                pygame.draw.rect(
                    self.canvas,
                    PANEL_SELECTED if selected else PANEL,
                    (9, y, SCREEN_WIDTH - 18, ROW_HEIGHT - 3),
                    border_radius=3,
                )
                if selected:
                    pygame.draw.rect(self.canvas, ACCENT, (9, y, 2, ROW_HEIGHT - 3))

                # This 16x16 frame is reserved for a future per-module icon.
                pygame.draw.rect(self.canvas, (6, 13, 24), (17, y + 10, 16, 16))
                pygame.draw.rect(self.canvas, (65, 98, 111), (17, y + 10, 16, 16), 1)
                name = module.name
                while name and self.name_font.size(name)[0] > 263:
                    name = name[:-1]
                if name != module.name and len(name) > 3:
                    name = name[:-3].rstrip() + "..."
                self._draw_text(name, self.name_font, TEXT, (41, y + 5))
                description = module.description
                while description and self.body_font.size(description)[0] > 263:
                    description = description[:-1]
                if description != module.description and len(description) > 3:
                    description = description[:-3].rstrip() + "..."
                self._draw_text(description, self.body_font, MUTED, (41, y + 23))

        pygame.draw.rect(self.canvas, PANEL, (0, 211, SCREEN_WIDTH, 29))
        if self.error_message:
            message = self.error_message
            while message and self.small_font.size(message)[0] > 195:
                message = message[:-1]
            self._draw_text(message, self.small_font, ERROR, (10, 220))
        else:
            self._draw_text("Select a module", self.small_font, MUTED, (10, 220))

        button = pygame.Rect(217, 215, 93, 20)
        enabled = bool(self.modules)
        pygame.draw.rect(
            self.canvas,
            ACCENT if enabled else (55, 75, 83),
            button,
            border_radius=3,
        )
        label = self.small_font.render("LOAD MODULE", True, BACKGROUND)
        self.canvas.blit(
            label,
            (button.centerx - label.get_width() // 2, button.centery - label.get_height() // 2),
        )

        scaled = pygame.transform.scale(
            self.canvas, (SCREEN_WIDTH * WINDOW_SCALE, SCREEN_HEIGHT * WINDOW_SCALE)
        )
        self.screen.blit(scaled, (0, 0))
        pygame.display.flip()

    def _launch_selected(self) -> None:
        if not self.modules:
            return
        module = self.modules[self.selected]
        try:
            image = load_program_file(module.path)
        except (AssemblyError, CCompilerError, OSError, ValueError) as exc:
            self.error_message = f"{module.name}: {exc}"
            return

        emulator = SuomiCompute16()
        emulator.net_port = self.net_port
        try:
            emulator.load_program(image)
            emulator.run()
        finally:
            network = getattr(emulator, "net", None)
            if network is not None:
                network.close()
        if pygame.display.get_init():
            self.screen = pygame.display.set_mode(
                (SCREEN_WIDTH * WINDOW_SCALE, SCREEN_HEIGHT * WINDOW_SCALE)
            )
            pygame.display.set_caption("SC-16 Launcher")
            self.clock = pygame.time.Clock()
        self.error_message = ""

    def _handle_click(self, position: tuple[int, int]) -> None:
        x, y = position[0] // WINDOW_SCALE, position[1] // WINDOW_SCALE
        if 217 <= x < 310 and 215 <= y < 235:
            self._launch_selected()
            return
        if LIST_TOP <= y < LIST_BOTTOM and 9 <= x < SCREEN_WIDTH - 9:
            row = (y - LIST_TOP) // ROW_HEIGHT
            index = self.scroll + row
            row_bottom = LIST_TOP + row * ROW_HEIGHT + ROW_HEIGHT - 3
            if row < self.visible_rows and y < row_bottom and index < len(self.modules):
                self.selected = index
                self.error_message = ""

    def run(self) -> None:
        while self.running:
            for event in pygame.event.get():
                if event.type == pygame.QUIT:
                    self.running = False
                elif event.type == pygame.MOUSEBUTTONDOWN:
                    if event.button == 1:
                        self._handle_click(event.pos)
                    elif event.button == 4:
                        self._scroll_by(-1)
                    elif event.button == 5:
                        self._scroll_by(1)
                elif event.type == pygame.MOUSEWHEEL:
                    self._scroll_by(-event.y)
                elif event.type == pygame.KEYDOWN:
                    if event.key == pygame.K_ESCAPE:
                        self.running = False
                    elif event.key == pygame.K_UP:
                        self.selected -= 1
                        self._clamp_selection()
                    elif event.key == pygame.K_DOWN:
                        self.selected += 1
                        self._clamp_selection()
                    elif event.key in (pygame.K_RETURN, pygame.K_KP_ENTER):
                        self._launch_selected()
            if not self.running:
                break
            self._draw()
            self.clock.tick(30)


def run_launcher(
    modules_dir: str | Path | None = None,
    net_port: int = DEFAULT_PORT,
) -> None:
    root = Path(__file__).resolve().parent.parent
    launcher = SCLauncher(modules_dir or root / "modules", net_port)
    launcher.run()
