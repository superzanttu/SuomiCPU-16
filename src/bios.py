from src.lib_disk import DiskDeviceError
from src.SuomiCPU import BIOS_BASE, CONSOLE_DATA, DISK_BASE, MEM_SIZE


class BIOSError(Exception):
    pass


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
        cpu.disk.load_disk(disk_id)
    except DiskDeviceError as exc:
        raise BIOSError(str(exc)) from exc
    cpu.memory[BIOS_BASE + 15] = 0


def select_data_disk(cpu):
    import pygame

    try:
        disks = cpu.disk.list_disks()
        message = ""
    except DiskDeviceError as exc:
        disks = []
        message = str(exc)
    selected = 0
    scroll = 0
    title_font = pygame.font.Font(None, 42)
    item_font = pygame.font.Font(None, 30)
    clock = pygame.time.Clock()

    while True:
        cpu.screen.fill((10, 16, 26))
        title = title_font.render("SuomiCPU-16 BIOS", True, (225, 235, 245))
        cpu.screen.blit(title, (48, 40))
        prompt = item_font.render("Select a data disk and press Enter", True, (170, 195, 215))
        cpu.screen.blit(prompt, (48, 105))
        if disks:
            visible_rows = max(1, (cpu.screen.get_height() - 250) // 36)
            if selected < scroll:
                scroll = selected
            elif selected >= scroll + visible_rows:
                scroll = selected - visible_rows + 1
            for row, index in enumerate(range(scroll, min(len(disks), scroll + visible_rows))):
                disk_id = disks[index]
                color = (255, 235, 140) if index == selected else (205, 215, 225)
                prefix = "> " if index == selected else "  "
                item = item_font.render(prefix + disk_id, True, color)
                cpu.screen.blit(item, (64, 155 + row * 36))
        else:
            item = item_font.render("No virtual disks found in the disks folder.", True, (235, 170, 150))
            cpu.screen.blit(item, (64, 155))
        footer = item_font.render(
            "N: continue without a disk    L: game launcher    Esc: quit",
            True,
            (145, 165, 185),
        )
        cpu.screen.blit(footer, (48, cpu.screen.get_height() - 64))
        if message:
            notice = item_font.render(message, True, (255, 150, 130))
            cpu.screen.blit(notice, (48, cpu.screen.get_height() - 105))
        pygame.display.flip()

        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                return None
            if event.type != pygame.KEYDOWN:
                continue
            if event.key == pygame.K_ESCAPE:
                return None
            if event.key == pygame.K_n:
                return "none"
            if event.key == pygame.K_l:
                return "launcher"
            if event.key == pygame.K_UP and disks:
                selected = (selected - 1) % len(disks)
            elif event.key == pygame.K_DOWN and disks:
                selected = (selected + 1) % len(disks)
            elif event.key == pygame.K_RETURN and disks:
                try:
                    mount_data_disk(cpu, disks[selected])
                    return disks[selected]
                except BIOSError as exc:
                    message = str(exc)
        clock.tick(30)
