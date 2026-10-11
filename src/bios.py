from dataclasses import dataclass
from pathlib import Path
import time

from src.lib_disk import DiskDeviceError
from src.SuomiCPU import (
    BIOS_BASE,
    CONSOLE_DATA,
    DISK_BASE,
    DOUBLE_CLICK_MS,
    MEM_SIZE,
)


class BIOSError(Exception):
    pass


@dataclass(frozen=True)
class BIOSSelection:
    module_path: Path | None
    image: object | None
    disk_id: str | None
    module_name: str | None = None


@dataclass(frozen=True)
class BootModule:
    path: Path | None
    name: str
    description: str
    error: str | None = None


def initialize_bios(cpu):
    descriptor = bytearray(16)
    descriptor[0:4] = b"SC16"
    descriptor[4:6] = bytes((1, 0))
    descriptor[6:9] = MEM_SIZE.to_bytes(3, "big")
    descriptor[9:12] = CONSOLE_DATA.to_bytes(3, "big")
    descriptor[12:15] = DISK_BASE.to_bytes(3, "big")
    cpu.memory[BIOS_BASE:BIOS_BASE + len(descriptor)] = descriptor


def mount_data_disk(cpu, disk_id):
    try:
        if not hasattr(cpu, "disk"):
            from src.lib_disk import DiskDevice

            cpu.disk = DiskDevice()
        if disk_id not in cpu.disk.list_disks():
            raise DiskDeviceError(f"virtual data disk {disk_id!r} does not exist")
        cpu.disk.load_disk(disk_id)
    except DiskDeviceError as exc:
        cpu.memory[BIOS_BASE + 15] = 1
        if hasattr(cpu, "console_log"):
            cpu.console_log(f"ERROR: Cannot mount data disk: {exc}")
        raise BIOSError(str(exc)) from exc
    cpu.memory[BIOS_BASE + 15] = 0
    if hasattr(cpu, "console_log"):
        cpu.console_log(f"Data disk mounted: {disk_id}")


def discover_boot_modules(modules_dir=None):
    root = Path(__file__).resolve().parent.parent
    directory = Path(modules_dir or root / "modules")
    if not directory.is_dir():
        raise FileNotFoundError(f"Modules folder does not exist: {directory}")

    modules = []
    for module_dir in sorted(
        (path for path in directory.iterdir() if path.is_dir()),
        key=lambda item: item.name.casefold(),
    ):
        source_files = sorted(
            (
                path
                for path in module_dir.iterdir()
                if path.is_file() and path.suffix.casefold() == ".c"
            ),
            key=lambda item: item.name.casefold(),
        )
        error = None
        if len(source_files) == 1:
            source_path = source_files[0]
        elif len(source_files) > 1:
            source_path = next(
                (path for path in source_files if path.name.casefold() == "main.c"),
                None,
            )
            if source_path is None:
                error = "Multiple .c files found; expected main.c."
        else:
            source_path = None
            error = "No .c files found in module folder."

        description = error or "No description"
        if source_path is not None:
            lines = source_path.read_text(encoding="utf-8-sig").splitlines()
            description = lines[0].strip() if lines else ""
        if source_path is not None and description.startswith("//"):
            description = description[2:].strip()
        elif source_path is not None and description.startswith("/*"):
            description = description[2:].removesuffix("*/").strip().lstrip("*").strip()
        modules.append(
            BootModule(
                path=source_path,
                name=module_dir.name,
                description=description or "No description",
                error=error,
            )
        )
    return modules


def select_boot_program(cpu, modules_dir=None, initial_disk=None):
    import pygame

    try:
        disks = cpu.disk.list_disks()
        message = ""
    except DiskDeviceError as exc:
        disks = []
        message = str(exc)
        if hasattr(cpu, "console_log"):
            cpu.console_log(f"ERROR: Cannot list data disks: {exc}")
    try:
        modules = discover_boot_modules(modules_dir)
    except (OSError, UnicodeError) as exc:
        modules = []
        message = f"Cannot read modules/: {exc}"
        if hasattr(cpu, "console_log"):
            cpu.console_log(f"ERROR: Cannot read modules/: {exc}")

    selected = 0
    scroll = 0
    title_font = pygame.font.Font(None, 42)
    item_font = pygame.font.Font(None, 30)
    small_font = pygame.font.Font(None, 22)
    clock = pygame.time.Clock()
    disk_options = [None, *disks]
    selected_disk = initial_disk if initial_disk in disks else None
    last_clicked_module = None
    list_top = 184
    if initial_disk is not None and selected_disk is None:
        message = f"Data disk {initial_disk!r} was not found."
        if hasattr(cpu, "console_log"):
            cpu.console_log(f"ERROR: {message}")

    def load_selected_module():
        nonlocal message
        module = modules[selected]
        cpu.module_name = module.name
        if hasattr(cpu, "console_log"):
            cpu.console_log(f"Loading module: {module.name}")
        if module.error is not None or module.path is None:
            error = module.error or "No .c files found in module folder."
            if hasattr(cpu, "console_log"):
                cpu.console_log(f"ERROR loading {module.name}: {error}")
            message = f"{module.name}: {error}"
            return None
        try:
            from src.SuomiCPU import CCompilerError, load_program_file
            from tools.assembler import AssemblyError

            logger = getattr(cpu, "console_progress", None)
            if logger is None:
                logger = getattr(cpu, "console_log", None)
            image = load_program_file(module.path, logger)
            return BIOSSelection(module.path, image, selected_disk, module.name)
        except (
            AssemblyError,
            CCompilerError,
            OSError,
            UnicodeError,
            ValueError,
        ) as exc:
            if hasattr(cpu, "console_log"):
                cpu.console_log(f"ERROR loading {module.name}: {exc}")
            message = f"{module.name}: {exc}"
            return None

    def scroll_modules(amount, visible_rows):
        nonlocal scroll, selected
        max_scroll = max(0, len(modules) - visible_rows)
        scroll = max(0, min(max_scroll, scroll + amount))
        if selected < scroll:
            selected = scroll
        elif selected >= scroll + visible_rows:
            selected = scroll + visible_rows - 1

    while True:
        if getattr(cpu, "bios_reset_requested", False):
            cpu.module_name = None
            cpu.program_loaded = False
            cpu.reset()
            cpu._prepared_for_run = False
            cpu.boot_time = time.monotonic()
            cpu._log_startup("System reset in BIOS")
        if getattr(cpu, "reboot_requested", False):
            cpu.reset()
            cpu.boot_time = time.monotonic()
            cpu._log_startup("SC-16 system rebooted")
        console_window = getattr(cpu, "console_window", None)
        if console_window is not None:
            console_window.pump()
            console_window.refresh(
                getattr(cpu, "module_name", None),
                getattr(getattr(cpu, "disk", None), "current_disk", None),
                getattr(cpu, "boot_time", 0),
                getattr(cpu, "paused", False),
            )
        cpu.screen.fill((10, 16, 26))
        title = title_font.render("SuomiCPU-16 BIOS", True, (225, 235, 245))
        cpu.screen.blit(title, (48, 40))
        prompt = item_font.render("Select a program from modules/", True, (170, 195, 215))
        cpu.screen.blit(prompt, (48, 105))
        disk_name = selected_disk or "None"
        disk_label = small_font.render(
            f"Data disk: {disk_name}  (click or press D to change)",
            True,
            (145, 200, 220),
        )
        cpu.screen.blit(disk_label, (48, 135))
        visible_rows = max(1, (cpu.screen.get_height() - 300) // 44)
        if modules:
            if selected < scroll:
                scroll = selected
            elif selected >= scroll + visible_rows:
                scroll = selected - visible_rows + 1
            if scroll:
                indicator = small_font.render(
                    f"^ {scroll} more module(s) above",
                    True,
                    (105, 190, 210),
                )
                cpu.screen.blit(indicator, (64, 158))
            for row, index in enumerate(range(scroll, min(len(modules), scroll + visible_rows))):
                module = modules[index]
                color = (255, 235, 140) if index == selected else (205, 215, 225)
                prefix = "> " if index == selected else "  "
                item = item_font.render(prefix + module.name, True, color)
                y = list_top + row * 44
                cpu.screen.blit(item, (64, y))
                description = small_font.render(module.description, True, (145, 165, 185))
                cpu.screen.blit(description, (96, y + 21))
            remaining = len(modules) - scroll - visible_rows
            if remaining > 0:
                indicator = small_font.render(
                    f"v {remaining} more module(s) below",
                    True,
                    (105, 190, 210),
                )
                cpu.screen.blit(
                    indicator,
                    (64, list_top + visible_rows * 44),
                )
        else:
            item = item_font.render("No modules found in modules/.", True, (235, 170, 150))
            cpu.screen.blit(item, (64, list_top))
        footer = small_font.render(
            "Wheel: scroll modules    Click: select; double-click: load    "
            "D/click disk: change    Esc: quit",
            True,
            (145, 165, 185),
        )
        cpu.screen.blit(footer, (48, cpu.screen.get_height() - 48))
        if message:
            notice = small_font.render(message, True, (255, 150, 130))
            cpu.screen.blit(notice, (48, cpu.screen.get_height() - 78))
        pygame.display.flip()

        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                return BIOSSelection(None, None, None)
            if event.type == pygame.MOUSEWHEEL and modules:
                mouse_x = getattr(event, "mouse_x", None)
                mouse_y = getattr(event, "mouse_y", None)
                if mouse_x is None or mouse_y is None:
                    mouse_x, mouse_y = pygame.mouse.get_pos()
                if (
                    48 <= mouse_x < cpu.screen.get_width() - 48
                    and list_top <= mouse_y < list_top + visible_rows * 44
                ):
                    scroll_modules(-event.y * 3, visible_rows)
                    last_clicked_module = None
                continue
            if (
                event.type == pygame.MOUSEBUTTONDOWN
                and event.button in (4, 5)
                and modules
            ):
                mouse_x, mouse_y = event.pos
                if (
                    48 <= mouse_x < cpu.screen.get_width() - 48
                    and list_top <= mouse_y < list_top + visible_rows * 44
                ):
                    scroll_modules(-3 if event.button == 4 else 3, visible_rows)
                    last_clicked_module = None
                continue
            if event.type == pygame.MOUSEMOTION and modules:
                mouse_x, mouse_y = event.pos
                row = (mouse_y - list_top) // 44
                index = scroll + row
                if (
                    48 <= mouse_x < cpu.screen.get_width() - 48
                    and list_top <= mouse_y < list_top + visible_rows * 44
                    and index < len(modules)
                ):
                    selected = index
                continue
            if event.type == pygame.MOUSEBUTTONDOWN and event.button == 1:
                mouse_x, mouse_y = event.pos
                if 48 <= mouse_x < cpu.screen.get_width() - 48 and 130 <= mouse_y < 164:
                    disk_index = disk_options.index(selected_disk)
                    selected_disk = disk_options[(disk_index + 1) % len(disk_options)]
                    message = ""
                    last_clicked_module = None
                    continue
                row = (mouse_y - list_top) // 44
                index = scroll + row
                if (
                    modules
                    and 48 <= mouse_x < cpu.screen.get_width() - 48
                    and list_top <= mouse_y < list_top + visible_rows * 44
                    and index < len(modules)
                ):
                    selected = index
                    now = pygame.time.get_ticks()
                    if (
                        last_clicked_module is not None
                        and last_clicked_module[0] == index
                        and now - last_clicked_module[1] <= DOUBLE_CLICK_MS
                    ):
                        selection = load_selected_module()
                        if selection is not None:
                            return selection
                        last_clicked_module = None
                    else:
                        last_clicked_module = (index, now)
                    continue
            if event.type != pygame.KEYDOWN:
                continue
            if event.key == pygame.K_F2 and console_window is not None:
                console_window.toggle()
                continue
            if event.key == pygame.K_ESCAPE:
                return BIOSSelection(None, None, None)
            if event.key == pygame.K_d:
                disk_index = disk_options.index(selected_disk)
                selected_disk = disk_options[(disk_index + 1) % len(disk_options)]
                message = ""
            elif event.key == pygame.K_UP and modules:
                selected = (selected - 1) % len(modules)
            elif event.key == pygame.K_DOWN and modules:
                selected = (selected + 1) % len(modules)
            elif event.key in (pygame.K_RETURN, pygame.K_KP_ENTER) and modules:
                selection = load_selected_module()
                if selection is not None:
                    return selection
        clock.tick(30)
