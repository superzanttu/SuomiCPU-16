from pathlib import Path

import pygame


ROOT = Path(__file__).resolve().parent
ATLAS_FILES = (
    ROOT / "gfx" / "tiles_objects.png",
    ROOT / "gfx" / "tiles_terrain.png",
)
SCALE = 2
HEADER_HEIGHT = 54
FOOTER_HEIGHT = 54
GRID = 16


def main():
    global SCALE

    pygame.init()
    atlases = [pygame.image.load(str(path)) for path in ATLAS_FILES]
    atlas_index = 0
    window = pygame.display.set_mode((1400, 920), pygame.RESIZABLE)
    pygame.display.set_caption("Tank-16 Sprite Viewer")
    font = pygame.font.Font(None, 18)
    small_font = pygame.font.Font(None, 13)
    dragging = False
    selection_start = None
    selection = None
    clock = pygame.time.Clock()
    running = True

    while running:
        image = atlases[atlas_index]
        width, height = window.get_size()
        image_width = image.get_width() * SCALE
        image_height = image.get_height() * SCALE
        image_x = max(0, (width - image_width) // 2)
        image_y = HEADER_HEIGHT
        mouse_x, mouse_y = pygame.mouse.get_pos()
        source_x = (mouse_x - image_x) // SCALE
        source_y = (mouse_y - image_y) // SCALE
        inside = (image_x <= mouse_x < image_x + image_width and
                  image_y <= mouse_y < image_y + image_height)

        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                running = False
            elif event.type == pygame.VIDEORESIZE:
                window = pygame.display.set_mode(event.size, pygame.RESIZABLE)
            elif event.type == pygame.KEYDOWN:
                if event.key == pygame.K_ESCAPE:
                    running = False
                elif event.key == pygame.K_TAB:
                    atlas_index = 1 - atlas_index
                    selection = None
                elif event.key in (pygame.K_EQUALS, pygame.K_PLUS):
                    SCALE = min(4, SCALE + 1)
                elif event.key == pygame.K_MINUS:
                    SCALE = max(1, SCALE - 1)
            elif event.type == pygame.MOUSEBUTTONDOWN and event.button == 1:
                if inside:
                    dragging = True
                    selection_start = (source_x // GRID * GRID,
                                       source_y // GRID * GRID)
            elif event.type == pygame.MOUSEBUTTONUP and event.button == 1:
                if dragging and selection_start is not None:
                    end_x = max(0, min(image.get_width(), source_x + 1))
                    end_y = max(0, min(image.get_height(), source_y + 1))
                    start_x, start_y = selection_start
                    left = min(start_x, end_x // GRID * GRID)
                    top = min(start_y, end_y // GRID * GRID)
                    right = max(start_x + GRID,
                                min(image.get_width(),
                                    ((end_x + GRID - 1) // GRID) * GRID))
                    bottom = max(start_y + GRID,
                                 min(image.get_height(),
                                     ((end_y + GRID - 1) // GRID) * GRID))
                    selection = (left, top, right - left, bottom - top)
                    columns = (image.get_width() + GRID - 1) // GRID
                    sprite_id = (top // GRID) * columns + left // GRID
                    print("atlas=%s id=%d x=%d y=%d width=%d height=%d" %
                          (ATLAS_FILES[atlas_index].name, sprite_id,
                           left, top, right - left, bottom - top))
                dragging = False

        window.fill((24, 27, 31))
        atlas_name = ATLAS_FILES[atlas_index].name
        title = font.render(
            "%s  %dx%d  |  Tab: switch atlas  +/-: zoom  |  Drag: select region"
            % (atlas_name, image.get_width(), image.get_height()),
            True, (245, 245, 245))
        window.blit(title, (12, 12))
        window.blit(pygame.transform.scale(image, (image_width, image_height)),
                    (image_x, image_y))

        columns = (image.get_width() + GRID - 1) // GRID
        for x in range(0, image.get_width() + 1, GRID):
            screen_x = image_x + x * SCALE
            pygame.draw.line(window, (0, 0, 0), (screen_x, image_y),
                             (screen_x, image_y + image_height), 1)
        for y in range(0, image.get_height() + 1, GRID):
            screen_y = image_y + y * SCALE
            pygame.draw.line(window, (0, 0, 0), (image_x, screen_y),
                             (image_x + image_width, screen_y), 1)
        for row in range((image.get_height() + GRID - 1) // GRID):
            for column in range(columns):
                sprite_id = row * columns + column
                label = small_font.render(str(sprite_id), True, (255, 255, 255))
                window.blit(label, (image_x + column * GRID * SCALE + 2,
                                    image_y + row * GRID * SCALE + 2))

        if inside:
            cell_x = source_x // GRID * GRID
            cell_y = source_y // GRID * GRID
            pygame.draw.rect(
                window, (255, 225, 40),
                (image_x + cell_x * SCALE, image_y + cell_y * SCALE,
                 GRID * SCALE, GRID * SCALE), 2)
        if dragging and selection_start is not None:
            start_x, start_y = selection_start
            pygame.draw.rect(
                window, (255, 80, 50),
                (image_x + start_x * SCALE, image_y + start_y * SCALE,
                 max(GRID, (source_x - start_x + 1) * SCALE),
                 max(GRID, (source_y - start_y + 1) * SCALE)), 2)
        if selection is None:
            detail = "Cell: x=%d y=%d" % (
                cell_x, cell_y) if inside else "Move over a sprite cell"
        else:
            sx, sy, sw, sh = selection
            sprite_id = (sy // GRID) * columns + sx // GRID
            detail = "Selection: ID %d  x=%d y=%d  %dx%d" % (
                sprite_id, sx, sy, sw, sh)
        window.blit(font.render(detail, True, (245, 245, 245)),
                    (12, height - FOOTER_HEIGHT + 10))
        pygame.display.flip()
        clock.tick(30)

    pygame.quit()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
