from dataclasses import dataclass
from pathlib import Path

from src.lib_disk import DiskDeviceError
from src.SuomiCPU import BIOS_BASE, CONSOLE_DATA, DISK_BASE, MEM_SIZE


class BIOSError(Exception):
    pass


@dataclass(frozen=True)
class BIOSSelection:
    module_path: Path | None
    image: object | None
    disk_id: str | None


@dataclass(frozen=True)
class BootModule:
    path: Path
    name: str
    description: str


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
        raise BIOSError(str(exc)) from exc
    cpu.memory[BIOS_BASE + 15] = 0


def discover_boot_modules(modules_dir=None):
    root = Path(__file__).resolve().parent.parent
    directory = Path(modules_dir or root / "modules")
    if not directory.is_dir():
        raise FileNotFoundError(f"Modules folder does not exist: {directory}")

    modules = []
    for path in sorted(directory.glob("*.c"), key=lambda item: item.name.casefold()):
        lines = path.read_text(encoding="utf-8-sig").splitlines()
        description = lines[0].strip() if lines else ""
        if description.startswith("//"):
            description = description[2:].strip()
        elif description.startswith("/*"):
            description = description[2:].removesuffix("*/").strip().lstrip("*").strip()
        modules.append(
            BootModule(
                path=path,
                name=path.stem,
                description=description or "No description",
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
    try:
        modules = discover_boot_modules(modules_dir)
    except (OSError, UnicodeError) as exc:
        modules = []
        message = f"Cannot read modules/: {exc}"

    selected = 0
    scroll = 0
    title_font = pygame.font.Font(None, 42)
    item_font = pygame.font.Font(None, 30)
    small_font = pygame.font.Font(None, 22)
    clock = pygame.time.Clock()
    disk_options = [None, *disks]
    selected_disk = initial_disk if initial_disk in disks else None
    if initial_disk is not None and selected_disk is None:
        message = f"Data disk {initial_disk!r} was not found."

    while True:
        cpu.screen.fill((10, 16, 26))
        title = title_font.render("SuomiCPU-16 BIOS", True, (225, 235, 245))
        cpu.screen.blit(title, (48, 40))
        prompt = item_font.render("Select a program from modules/", True, (170, 195, 215))
        cpu.screen.blit(prompt, (48, 105))
        disk_name = selected_disk or "None"
        disk_label = small_font.render(
            f"Data disk: {disk_name}  (press D to change)",
            True,
            (145, 200, 220),
        )
        cpu.screen.blit(disk_label, (48, 135))
        if modules:
            visible_rows = max(1, (cpu.screen.get_height() - 300) // 44)
            if selected < scroll:
                scroll = selected
            elif selected >= scroll + visible_rows:
                scroll = selected - visible_rows + 1
            for row, index in enumerate(range(scroll, min(len(modules), scroll + visible_rows))):
                module = modules[index]
                color = (255, 235, 140) if index == selected else (205, 215, 225)
                prefix = "> " if index == selected else "  "
                item = item_font.render(prefix + module.name, True, color)
                y = 172 + row * 44
                cpu.screen.blit(item, (64, y))
                description = small_font.render(module.description, True, (145, 165, 185))
                cpu.screen.blit(description, (96, y + 21))
        else:
            item = item_font.render("No C programs found in modules/.", True, (235, 170, 150))
            cpu.screen.blit(item, (64, 172))
        footer = item_font.render(
            "Enter: load module    D: data disk    Esc: quit",
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
            if event.type != pygame.KEYDOWN:
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
                try:
                    from src.SuomiCPU import CCompilerError, load_program_file
                    from tools.assembler import AssemblyError

                    image = load_program_file(modules[selected].path)
                    return BIOSSelection(
                        modules[selected].path,
                        image,
                        selected_disk,
                    )
                except (
                    AssemblyError,
                    CCompilerError,
                    OSError,
                    UnicodeError,
                    ValueError,
                ) as exc:
                    message = f"{modules[selected].name}: {exc}"
        clock.tick(30)
