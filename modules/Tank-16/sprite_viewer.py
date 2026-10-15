from pathlib import Path

import pygame


ROOT = Path(__file__).resolve().parent
ATLAS_FILES = (
    ROOT / "gfx" / "tiles_objects.png",
    ROOT / "gfx" / "tiles_terrain.png",
)
SPRITE_SIZE = 64
SCALE = 1
HEADER_HEIGHT = 54
FOOTER_HEIGHT = 54


def padded_atlas(image: pygame.Surface) -> pygame.Surface:
    """Return an atlas padded to whole 64x64 cells."""
    width, height = image.get_size()
    padded_size = (
        ((width + SPRITE_SIZE - 1) // SPRITE_SIZE) * SPRITE_SIZE,
        ((height + SPRITE_SIZE - 1) // SPRITE_SIZE) * SPRITE_SIZE,
    )
    if image.get_size() == padded_size:
        return image

    padded = pygame.Surface(padded_size, pygame.SRCALPHA, 32)
    padded.blit(image, (0, 0))
    return padded


def load_atlas(path: Path) -> pygame.Surface:
    return padded_atlas(pygame.image.load(str(path)))


def _grid_size(image: pygame.Surface) -> tuple[int, int]:
    width, height = image.get_size()
    columns = (width + SPRITE_SIZE - 1) // SPRITE_SIZE
    rows = (height + SPRITE_SIZE - 1) // SPRITE_SIZE
    return columns, rows


def sprite_id_at(image: pygame.Surface, x: int, y: int) -> int:
    """Return the row-major ID of the 64x64 cell containing (x, y)."""
    width, height = image.get_size()
    if not 0 <= x < width or not 0 <= y < height:
        raise ValueError("Coordinates are outside the atlas")

    columns, _ = _grid_size(image)
    return (y // SPRITE_SIZE) * columns + x // SPRITE_SIZE


def sprite_bounds(image: pygame.Surface, sprite_id: int) -> pygame.Rect:
    """Return the 64x64 rectangle for a row-major atlas sprite ID."""
    columns, rows = _grid_size(image)
    if not 0 <= sprite_id < columns * rows:
        raise ValueError("Sprite ID is outside the atlas")

    return pygame.Rect(
        (sprite_id % columns) * SPRITE_SIZE,
        (sprite_id // columns) * SPRITE_SIZE,
        SPRITE_SIZE,
        SPRITE_SIZE,
    )


def extract_sprite(image: pygame.Surface, sprite_id: int) -> pygame.Surface:
    """Copy one 64x64 sprite from an atlas, padding an incomplete edge cell."""
    bounds = sprite_bounds(image, sprite_id)
    normalized = padded_atlas(image)
    return normalized.subsurface(bounds).copy()


def main() -> int:
    global SCALE

    pygame.init()
    atlases = [load_atlas(path) for path in ATLAS_FILES]
    atlas_index = 0
    selected_sprite = None
    window = pygame.display.set_mode((1400, 920), pygame.RESIZABLE)
    pygame.display.set_caption("Tank-16 Sprite Viewer")
    font = pygame.font.Font(None, 18)
    small_font = pygame.font.Font(None, 16)
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
        hovered_sprite = (
            sprite_id_at(image, source_x, source_y) if inside else None)
        columns, rows = _grid_size(image)

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
                    selected_sprite = None
                elif event.key in (pygame.K_EQUALS, pygame.K_PLUS):
                    SCALE = min(4, SCALE + 1)
                elif event.key == pygame.K_MINUS:
                    SCALE = max(1, SCALE - 1)
            elif event.type == pygame.MOUSEBUTTONDOWN and event.button == 1:
                event_x, event_y = event.pos
                click_x = (event_x - image_x) // SCALE
                click_y = (event_y - image_y) // SCALE
                if (image_x <= event_x < image_x + image_width and
                        image_y <= event_y < image_y + image_height):
                    selected_sprite = sprite_id_at(
                        image, click_x, click_y)
                    bounds = sprite_bounds(image, selected_sprite)
                    print(
                        "atlas=%s id=%d x=%d y=%d width=%d height=%d"
                        % (ATLAS_FILES[atlas_index].name, selected_sprite,
                           bounds.x, bounds.y, SPRITE_SIZE, SPRITE_SIZE)
                    )

        window.fill((24, 27, 31))
        atlas_name = ATLAS_FILES[atlas_index].name
        title = font.render(
            "%s  %dx%d  |  Cell: 64x64  Tab: switch atlas  +/-: zoom"
            "  |  Click: select sprite"
            % (atlas_name, image.get_width(), image.get_height()),
            True, (245, 245, 245))
        window.blit(title, (12, 12))
        window.blit(pygame.transform.scale(image, (image_width, image_height)),
                    (image_x, image_y))

        for row in range(rows):
            for column in range(columns):
                sprite_id = row * columns + column
                screen_rect = pygame.Rect(
                    image_x + column * SPRITE_SIZE * SCALE,
                    image_y + row * SPRITE_SIZE * SCALE,
                    SPRITE_SIZE * SCALE,
                    SPRITE_SIZE * SCALE,
                )
                pygame.draw.rect(window, (0, 0, 0), screen_rect, 1)
                label = small_font.render(
                    str(sprite_id), True, (255, 255, 255))
                window.blit(label, (screen_rect.x + 3, screen_rect.y + 3))

        for sprite_id, color in (
                (hovered_sprite, (255, 225, 40)),
                (selected_sprite, (80, 255, 150))):
            if sprite_id is not None:
                bounds = sprite_bounds(image, sprite_id)
                pygame.draw.rect(
                    window, color,
                    (image_x + bounds.x * SCALE,
                     image_y + bounds.y * SCALE,
                     SPRITE_SIZE * SCALE, SPRITE_SIZE * SCALE), 2)

        active_sprite = selected_sprite
        if active_sprite is None:
            active_sprite = hovered_sprite
        if active_sprite is None:
            detail = "Move over a sprite cell"
        else:
            bounds = sprite_bounds(image, active_sprite)
            detail = "Sprite ID %d  x=%d y=%d  %dx%d" % (
                active_sprite, bounds.x, bounds.y,
                SPRITE_SIZE, SPRITE_SIZE)
        window.blit(font.render(detail, True, (245, 245, 245)),
                    (12, height - FOOTER_HEIGHT + 10))
        pygame.display.flip()
        clock.tick(30)

    pygame.quit()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
