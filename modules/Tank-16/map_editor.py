import json
import re
from pathlib import Path

import pygame


ROOT = Path(__file__).resolve().parent
MAPS_DIR = ROOT / "maps"
TILESET_PATH = ROOT / "gfx" / "tiles_terrain.png"
DEFAULT_TILESET_FILENAME = TILESET_PATH.name
TILE_SIZE = 64
MAX_MAP_NAME_LENGTH = 8
MAX_MAP_DIMENSION = 128
SIDEBAR_WIDTH = 180
HEADER_HEIGHT = 40
BUTTON_HEIGHT = 34
BUTTON_GAP = 6
MAP_DEFAULT_WIDTH = 10
MAP_DEFAULT_HEIGHT = 8
TILE_SIZE_OPTIONS = (64, 32)


def load_tileset(path: Path = TILESET_PATH) -> tuple[pygame.Surface, int, int]:
    try:
        tileset = pygame.image.load(str(path))
    except (OSError, pygame.error) as error:
        raise ValueError("Could not load tileset %s: %s" %
                         (path.name, error)) from error
    columns = tileset.get_width() // TILE_SIZE
    rows = tileset.get_height() // TILE_SIZE
    if columns == 0 or rows == 0:
        raise ValueError("Tileset must contain at least one 64x64 tile")
    if (tileset.get_width() % TILE_SIZE or
            tileset.get_height() % TILE_SIZE):
        raise ValueError("Tileset dimensions must be multiples of 64 pixels")
    if columns * rows > 256:
        raise ValueError("Tileset cannot contain more than 256 tiles")
    return tileset, columns, rows


def validate_map_name(name: str) -> str:
    if not isinstance(name, str):
        raise ValueError("Map name must be text")
    name = name.strip()
    if not 1 <= len(name) <= MAX_MAP_NAME_LENGTH:
        raise ValueError("Map name must be 1 to 8 characters")
    if not re.fullmatch(r"[A-Za-z0-9_-]+", name):
        raise ValueError("Map name can contain letters, numbers, _ and -")
    return name.upper()


def validate_tileset_filename(filename: str) -> str:
    if not isinstance(filename, str) or not re.fullmatch(
            r"tiles_[A-Za-z0-9_-]+\.png", filename):
        raise ValueError("Tileset must be a tiles_*.png filename")
    return filename


def new_map(name: str, width: int, height: int,
            tile_x: int = 0, tile_y: int = 0) -> dict:
    name = validate_map_name(name)
    if (not isinstance(width, int) or isinstance(width, bool) or
            not 1 <= width <= MAX_MAP_DIMENSION):
        raise ValueError("Map width must be between 1 and 128 tiles")
    if (not isinstance(height, int) or isinstance(height, bool) or
            not 1 <= height <= MAX_MAP_DIMENSION):
        raise ValueError("Map height must be between 1 and 128 tiles")
    return {
        "format_version": 1,
        "name": name,
        "width": width,
        "height": height,
        "tile_size": TILE_SIZE,
        "tileset": DEFAULT_TILESET_FILENAME,
        "tiles": [
            [[tile_x, tile_y] for _ in range(width)]
            for _ in range(height)
        ],
    }


def set_map_tile(map_data: dict, x: int, y: int, tile_x: int, tile_y: int,
                 tileset_columns: int, tileset_rows: int) -> None:
    if not 0 <= x < map_data["width"] or not 0 <= y < map_data["height"]:
        raise ValueError("Map coordinates are outside the map")
    if not 0 <= tile_x < tileset_columns or not 0 <= tile_y < tileset_rows:
        raise ValueError("Tile coordinates are outside the tileset")
    map_data["tiles"][y][x] = [tile_x, tile_y]


def validate_map(map_data: dict, tileset_columns: int,
                 tileset_rows: int) -> dict:
    if not isinstance(map_data, dict):
        raise ValueError("Map file must contain a JSON object")
    if map_data.get("format_version") != 1:
        raise ValueError("Unsupported map file format")
    name = validate_map_name(map_data.get("name", ""))
    width = map_data.get("width")
    height = map_data.get("height")
    if (not isinstance(width, int) or isinstance(width, bool) or
            not 1 <= width <= MAX_MAP_DIMENSION):
        raise ValueError("Map width must be between 1 and 128 tiles")
    if (not isinstance(height, int) or isinstance(height, bool) or
            not 1 <= height <= MAX_MAP_DIMENSION):
        raise ValueError("Map height must be between 1 and 128 tiles")
    if map_data.get("tile_size") != TILE_SIZE:
        raise ValueError("Map tile size must be 64 pixels")
    tileset = validate_tileset_filename(
        map_data.get("tileset", DEFAULT_TILESET_FILENAME))

    tiles = map_data.get("tiles")
    if not isinstance(tiles, list) or len(tiles) != height:
        raise ValueError("Map tile rows do not match the map height")
    normalized_tiles = []
    for row in tiles:
        if not isinstance(row, list) or len(row) != width:
            raise ValueError("Map tile columns do not match the map width")
        normalized_row = []
        for tile in row:
            if (not isinstance(tile, list) or len(tile) != 2 or
                    any(not isinstance(value, int) or isinstance(value, bool)
                        for value in tile)):
                raise ValueError("Each map tile must be an [x, y] pair")
            tile_x, tile_y = tile
            if (not 0 <= tile_x < tileset_columns or
                    not 0 <= tile_y < tileset_rows):
                raise ValueError("Map contains a tile outside the tileset")
            normalized_row.append([tile_x, tile_y])
        normalized_tiles.append(normalized_row)

    return {
        "format_version": 1,
        "name": name,
        "width": width,
        "height": height,
        "tile_size": TILE_SIZE,
        "tileset": tileset,
        "tiles": normalized_tiles,
    }


def save_map(map_data: dict, maps_dir: Path = MAPS_DIR) -> Path:
    if not isinstance(map_data, dict):
        raise ValueError("Map file must contain a JSON object")
    filename = validate_tileset_filename(
        map_data.get("tileset", DEFAULT_TILESET_FILENAME))
    _, columns, rows = load_tileset(ROOT / "gfx" / filename)
    normalized = validate_map(map_data, columns, rows)
    maps_dir.mkdir(parents=True, exist_ok=True)
    path = maps_dir / (normalized["name"] + ".json")
    path.write_text(
        json.dumps(normalized, indent=2) + "\n", encoding="utf-8")
    return path


def load_map(path: Path, tileset_columns: int | None = None,
             tileset_rows: int | None = None) -> dict:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError("Could not read map: %s" % error) from error
    if not isinstance(document, dict):
        raise ValueError("Map file must contain a JSON object")
    tileset_value = document.get("tileset", DEFAULT_TILESET_FILENAME)
    if not isinstance(tileset_value, str):
        raise ValueError("Tileset must be a tiles_*.png filename")
    filename = validate_tileset_filename(tileset_value)
    _, actual_columns, actual_rows = load_tileset(ROOT / "gfx" / filename)
    columns = actual_columns if tileset_columns is None else tileset_columns
    rows = actual_rows if tileset_rows is None else tileset_rows
    if (columns, rows) != (actual_columns, actual_rows):
        raise ValueError("Tileset dimensions do not match the map tileset")
    return validate_map(document, actual_columns, actual_rows)


def tile_surface(tileset: pygame.Surface, tile_x: int,
                 tile_y: int) -> pygame.Surface:
    return tileset.subsurface(
        (tile_x * TILE_SIZE, tile_y * TILE_SIZE,
         TILE_SIZE, TILE_SIZE))


def _button_rects() -> dict[str, pygame.Rect]:
    names = ("NEW", "SAVE", "LOAD", "TILESET")
    return {
        name: pygame.Rect(
            10, HEADER_HEIGHT + 8 + index * (BUTTON_HEIGHT + BUTTON_GAP),
            SIDEBAR_WIDTH - 20, BUTTON_HEIGHT)
        for index, name in enumerate(names)
    }


def _map_panel(window: pygame.Surface) -> pygame.Rect:
    return pygame.Rect(
        SIDEBAR_WIDTH, HEADER_HEIGHT,
        window.get_width() - SIDEBAR_WIDTH,
        window.get_height() - HEADER_HEIGHT,
    )


def _fit_cell_size(map_data: dict, panel: pygame.Rect) -> float:
    return min(panel.width / map_data["width"],
               panel.height / map_data["height"])


def _clamp_scroll(scroll: list[float], map_data: dict,
                  panel: pygame.Rect, cell_size: float) -> None:
    map_width = map_data["width"] * cell_size
    map_height = map_data["height"] * cell_size
    scroll[0] = min(0, max(panel.width - map_width, scroll[0]))
    scroll[1] = min(0, max(panel.height - map_height, scroll[1]))


def _event_to_cell(position: tuple[int, int], panel: pygame.Rect,
                   scroll: list[float], cell_size: float) -> tuple[int, int]:
    return (
        int((position[0] - panel.x - scroll[0]) / cell_size),
        int((position[1] - panel.y - scroll[1]) / cell_size),
    )


def _draw_button(window: pygame.Surface, font: pygame.font.Font,
                 rect: pygame.Rect, label: str, enabled: bool = True) -> None:
    background = (65, 95, 115) if enabled else (65, 65, 65)
    pygame.draw.rect(window, background, rect, border_radius=4)
    pygame.draw.rect(window, (145, 165, 175), rect, 1, border_radius=4)
    text = font.render(label, True, (245, 245, 245))
    window.blit(text, text.get_rect(center=rect.center))


def _draw_sidebar(window: pygame.Surface, fonts: tuple, tileset: pygame.Surface,
                  columns: int, rows: int, selected_tile: tuple[int, int],
                  tile_scroll: int, buttons: dict[str, pygame.Rect],
                  status: str) -> None:
    font, small_font = fonts
    sidebar = pygame.Rect(0, 0, SIDEBAR_WIDTH, window.get_height())
    pygame.draw.rect(window, (32, 36, 42), sidebar)
    for name, rect in buttons.items():
        _draw_button(window, font, rect, name)

    list_top = buttons["TILESET"].bottom + 14
    footer = 44
    list_bottom = max(list_top, window.get_height() - footer)
    viewport = pygame.Rect(8, list_top, SIDEBAR_WIDTH - 16,
                          list_bottom - list_top)
    pygame.draw.rect(window, (23, 26, 30), viewport)
    tile_stride = TILE_SIZE + 25
    visible_count = max(1, (viewport.height + 24) // tile_stride)
    max_scroll = max(0, columns * rows - visible_count)
    tile_scroll = min(tile_scroll, max_scroll)
    for item in range(tile_scroll, min(columns * rows,
                                       tile_scroll + visible_count)):
        tile_x = item % columns
        tile_y = item // columns
        top = viewport.top + (item - tile_scroll) * tile_stride
        tile_rect = pygame.Rect(viewport.centerx - TILE_SIZE // 2,
                                top, TILE_SIZE, TILE_SIZE)
        window.blit(tile_surface(tileset, tile_x, tile_y), tile_rect)
        if (tile_x, tile_y) == selected_tile:
            pygame.draw.rect(window, (255, 221, 70), tile_rect, 3)
        else:
            pygame.draw.rect(window, (120, 130, 140), tile_rect, 1)
        label = small_font.render(
            "%d-%d" % (tile_x, tile_y), True, (245, 245, 245))
        window.blit(label, label.get_rect(
            center=(viewport.centerx, top + TILE_SIZE + 12)))

    if status:
        status_text = small_font.render(status[:23], True, (235, 210, 130))
        window.blit(status_text, (8, window.get_height() - footer + 3))


def _draw_map(window: pygame.Surface, map_data: dict,
              tileset: pygame.Surface, panel: pygame.Rect,
              scroll: list[float], cell_size: float) -> None:
    previous_clip = window.get_clip()
    window.set_clip(panel)
    window.fill((18, 20, 24), panel)
    first_column = max(0, int(-scroll[0] // cell_size))
    first_row = max(0, int(-scroll[1] // cell_size))
    last_column = min(
        map_data["width"],
        int((panel.width - scroll[0]) // cell_size) + 1)
    last_row = min(
        map_data["height"],
        int((panel.height - scroll[1]) // cell_size) + 1)

    for y in range(first_row, last_row):
        top = panel.y + round(scroll[1] + y * cell_size)
        bottom = panel.y + round(scroll[1] + (y + 1) * cell_size)
        for x in range(first_column, last_column):
            left = panel.x + round(scroll[0] + x * cell_size)
            right = panel.x + round(scroll[0] + (x + 1) * cell_size)
            tile_x, tile_y = map_data["tiles"][y][x]
            tile = tile_surface(tileset, tile_x, tile_y)
            destination = pygame.Rect(left, top, right - left, bottom - top)
            if destination.width > 0 and destination.height > 0:
                if destination.size != tile.get_size():
                    tile = pygame.transform.scale(tile, destination.size)
                window.blit(tile, destination)
                pygame.draw.rect(window, (0, 0, 0), destination, 1)
    window.set_clip(previous_clip)


def _draw_dialog(window: pygame.Surface, font: pygame.font.Font,
                 title: str, lines: list[str]) -> None:
    overlay = pygame.Surface(window.get_size(), pygame.SRCALPHA)
    overlay.fill((0, 0, 0, 150))
    window.blit(overlay, (0, 0))
    box = pygame.Rect(0, 0, min(520, window.get_width() - 40), 260)
    box.center = window.get_rect().center
    pygame.draw.rect(window, (42, 47, 54), box, border_radius=8)
    pygame.draw.rect(window, (160, 175, 185), box, 1, border_radius=8)
    window.blit(font.render(title, True, (255, 255, 255)),
                (box.x + 20, box.y + 16))
    for index, line in enumerate(lines):
        window.blit(font.render(line, True, (225, 225, 225)),
                    (box.x + 20, box.y + 56 + index * 30))


def main() -> int:
    pygame.init()
    pygame.key.set_repeat(180, 35)
    tileset, tile_columns, tile_rows = load_tileset()
    MAPS_DIR.mkdir(parents=True, exist_ok=True)
    map_data = new_map("UNTITLED", MAP_DEFAULT_WIDTH, MAP_DEFAULT_HEIGHT)
    window = pygame.display.set_mode((1120, 760), pygame.RESIZABLE)
    pygame.display.set_caption("Tank-16 Map Editor")
    font = pygame.font.Font(None, 22)
    small_font = pygame.font.Font(None, 18)
    button_font = pygame.font.Font(None, 22)
    clock = pygame.time.Clock()
    buttons = _button_rects()
    selected_tile = (0, 0)
    tile_scroll = 0
    zoom_level = 1
    scroll = [0.0, 0.0]
    status = ""
    dialog = None
    new_fields = ["UNTITLED", str(MAP_DEFAULT_WIDTH),
                  str(MAP_DEFAULT_HEIGHT)]
    active_field = 0
    field_edited = [False, False, False]
    load_files: list[Path] = []
    load_scroll = 0
    running = True

    while running:
        panel = _map_panel(window)
        cell_size = (float(TILE_SIZE) if zoom_level == 1 else
                     TILE_SIZE_OPTIONS[1] if zoom_level == 2 else
                     _fit_cell_size(map_data, panel))
        _clamp_scroll(scroll, map_data, panel, cell_size)
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                running = False
            elif event.type == pygame.VIDEORESIZE:
                window = pygame.display.set_mode(event.size, pygame.RESIZABLE)
            elif dialog == "new" and event.type == pygame.KEYDOWN:
                if event.key == pygame.K_ESCAPE:
                    dialog = None
                elif event.key == pygame.K_TAB:
                    active_field = (active_field + 1) % len(new_fields)
                    field_edited[active_field] = False
                elif event.key == pygame.K_BACKSPACE:
                    if not field_edited[active_field]:
                        new_fields[active_field] = ""
                        field_edited[active_field] = True
                    new_fields[active_field] = new_fields[active_field][:-1]
                elif event.key == pygame.K_RETURN:
                    try:
                        map_data = new_map(
                            new_fields[0], int(new_fields[1]),
                            int(new_fields[2]))
                        tileset, tile_columns, tile_rows = load_tileset()
                        selected_tile = (0, 0)
                        tile_scroll = 0
                        scroll = [0.0, 0.0]
                        zoom_level = 1
                        status = "New map created"
                        dialog = None
                    except ValueError as error:
                        status = str(error)
                elif event.unicode and event.unicode.isprintable():
                    current = new_fields[active_field]
                    max_length = MAX_MAP_NAME_LENGTH if active_field == 0 else 3
                    if not field_edited[active_field]:
                        current = ""
                        new_fields[active_field] = ""
                        field_edited[active_field] = True
                    if len(current) < max_length:
                        new_fields[active_field] += event.unicode
            elif dialog == "load" and event.type == pygame.KEYDOWN:
                if event.key == pygame.K_ESCAPE:
                    dialog = None
                elif event.key in (pygame.K_UP, pygame.K_DOWN):
                    load_scroll = max(
                        0, min(max(0, len(load_files) - 6),
                               load_scroll + (
                                   -1 if event.key == pygame.K_UP else 1)))
            elif dialog == "tileset" and event.type == pygame.KEYDOWN:
                if event.key in (pygame.K_ESCAPE, pygame.K_RETURN):
                    dialog = None
            elif event.type == pygame.MOUSEWHEEL:
                mouse_x, _ = pygame.mouse.get_pos()
                if mouse_x < SIDEBAR_WIDTH:
                    tile_stride = TILE_SIZE + 25
                    viewport_height = max(
                        1, window.get_height() -
                        (buttons["TILESET"].bottom + 14) - 44)
                    visible_count = max(
                        1, (viewport_height + 24) // tile_stride)
                    tile_scroll = min(
                        max(0, tile_columns * tile_rows - visible_count),
                        max(0, tile_scroll - event.y))
                elif zoom_level != 3:
                    scroll[1] += event.y * 48
                    _clamp_scroll(scroll, map_data, panel, cell_size)
            elif event.type == pygame.KEYDOWN and dialog is None:
                if event.key == pygame.K_ESCAPE:
                    running = False
                elif event.key == pygame.K_1:
                    zoom_level = 1
                elif event.key == pygame.K_2:
                    zoom_level = 2
                elif event.key == pygame.K_3:
                    zoom_level = 3
                    scroll = [0.0, 0.0]
                elif event.key in (pygame.K_EQUALS, pygame.K_PLUS):
                    zoom_level = max(1, zoom_level - 1)
                elif event.key == pygame.K_MINUS:
                    zoom_level = min(3, zoom_level + 1)
                elif zoom_level != 3 and event.key in (
                        pygame.K_w, pygame.K_a, pygame.K_s, pygame.K_d):
                    pan = max(24, int(cell_size / 2))
                    if event.key == pygame.K_w:
                        scroll[1] += pan
                    elif event.key == pygame.K_s:
                        scroll[1] -= pan
                    elif event.key == pygame.K_a:
                        scroll[0] += pan
                    else:
                        scroll[0] -= pan
                    _clamp_scroll(scroll, map_data, panel, cell_size)
                elif event.key == pygame.K_n:
                    new_fields = [
                        "UNTITLED", str(MAP_DEFAULT_WIDTH),
                        str(MAP_DEFAULT_HEIGHT),
                    ]
                    active_field = 0
                    field_edited = [False, False, False]
                    dialog = "new"
            elif event.type == pygame.MOUSEBUTTONDOWN:
                if event.button == 1:
                    position = event.pos
                    if dialog == "new":
                        box = pygame.Rect(0, 0, 520, 260)
                        box.center = window.get_rect().center
                        field_rects = [
                            pygame.Rect(box.x + 150, box.y + 88 + i * 42,
                                        280, 32)
                            for i in range(3)
                        ]
                        for index, rect in enumerate(field_rects):
                            if rect.collidepoint(position):
                                active_field = index
                                field_edited[index] = False
                                break
                    elif dialog == "load":
                        box = pygame.Rect(0, 0, 520, 260)
                        box.center = window.get_rect().center
                        visible_files = load_files[load_scroll:
                                                   load_scroll + 6]
                        for index, path in enumerate(visible_files):
                            rect = pygame.Rect(box.x + 20, box.y + 52 +
                                               index * 28, box.width - 40, 26)
                            if rect.collidepoint(position):
                                try:
                                    loaded_map = load_map(path)
                                    loaded_tileset, columns, rows = load_tileset(
                                        ROOT / "gfx" / loaded_map["tileset"])
                                    map_data = loaded_map
                                    tileset = loaded_tileset
                                    tile_columns, tile_rows = columns, rows
                                    selected_tile = (0, 0)
                                    tile_scroll = 0
                                    scroll = [0.0, 0.0]
                                    zoom_level = 1
                                    dialog = None
                                    status = "Loaded " + map_data["name"]
                                except ValueError as error:
                                    status = str(error)
                                break
                    elif dialog == "tileset":
                        dialog = None
                    elif dialog is None:
                        action = next(
                            (name for name, rect in buttons.items()
                             if rect.collidepoint(position)), None)
                        if action == "NEW":
                            new_fields = [
                                "UNTITLED", str(MAP_DEFAULT_WIDTH),
                                str(MAP_DEFAULT_HEIGHT),
                            ]
                            active_field = 0
                            field_edited = [False, False, False]
                            dialog = "new"
                        elif action == "SAVE":
                            try:
                                path = save_map(map_data)
                                status = "Saved " + path.name
                            except (OSError, ValueError) as error:
                                status = "Save failed: %s" % error
                        elif action == "LOAD":
                            load_files = sorted(
                                MAPS_DIR.glob("*.json"),
                                key=lambda path: path.name.lower())
                            load_scroll = 0
                            dialog = "load"
                        elif action == "TILESET":
                            dialog = "tileset"
                        elif position[0] < SIDEBAR_WIDTH:
                            list_top = buttons["TILESET"].bottom + 14
                            if position[1] >= list_top:
                                tile_stride = TILE_SIZE + 25
                                tile_index = tile_scroll + max(
                                    0, (position[1] - list_top) // tile_stride)
                                if tile_index < tile_columns * tile_rows:
                                    selected_tile = (
                                        tile_index % tile_columns,
                                        tile_index // tile_columns)
                        elif panel.collidepoint(position):
                            map_x, map_y = _event_to_cell(
                                position, panel, scroll, cell_size)
                            if (0 <= map_x < map_data["width"] and
                                    0 <= map_y < map_data["height"]):
                                set_map_tile(
                                    map_data, map_x, map_y,
                                    selected_tile[0], selected_tile[1],
                                    tile_columns, tile_rows)
                                status = "Painted %d,%d" % (map_x, map_y)
                elif event.button == 4:
                    if event.pos[0] < SIDEBAR_WIDTH:
                        tile_scroll = max(0, tile_scroll - 1)
                elif event.button == 5:
                    if event.pos[0] < SIDEBAR_WIDTH:
                        tile_scroll += 1

        panel = _map_panel(window)
        cell_size = (float(TILE_SIZE) if zoom_level == 1 else
                     TILE_SIZE_OPTIONS[1] if zoom_level == 2 else
                     _fit_cell_size(map_data, panel))
        _clamp_scroll(scroll, map_data, panel, cell_size)
        window.fill((18, 20, 24))
        _draw_map(window, map_data, tileset, panel, scroll, cell_size)
        pygame.draw.rect(window, (100, 110, 120), panel, 1)
        name_text = font.render(
            "%s  %dx%d" % (map_data["name"], map_data["width"],
                           map_data["height"]),
            True, (245, 245, 245))
        window.blit(name_text, (SIDEBAR_WIDTH + 10, 10))
        zoom_text = small_font.render(
            "Zoom %d/3  (1-3)" % zoom_level, True, (220, 220, 220))
        window.blit(zoom_text, (window.get_width() - 120, 12))
        _draw_sidebar(
            window, (button_font, small_font), tileset,
            tile_columns, tile_rows, selected_tile, tile_scroll,
            buttons, status)

        if dialog == "new":
            _draw_dialog(
                window, font, "New map",
                ["Enter creates; Tab changes field; Esc cancels"])
            box = pygame.Rect(0, 0, 520, 260)
            box.center = window.get_rect().center
            field_labels = ("Name (max 8)", "Width in tiles",
                            "Height in tiles")
            for index, value in enumerate(new_fields):
                field_y = box.y + 88 + index * 42
                window.blit(font.render(
                    field_labels[index], True, (230, 230, 230)),
                    (box.x + 20, field_y + 7))
                rect = pygame.Rect(box.x + 150, field_y,
                                   280, 32)
                pygame.draw.rect(
                    window, (65, 76, 88) if active_field == index
                    else (30, 34, 39), rect)
                text = font.render(value, True, (255, 255, 255))
                window.blit(text, (rect.x + 8, rect.y + 6))
            if status:
                window.blit(font.render(
                    status, True, (235, 190, 130)),
                    (box.x + 20, box.bottom - 28))
        elif dialog == "load":
            visible_files = load_files[load_scroll:load_scroll + 6]
            _draw_dialog(
                window, font, "Load map",
                ([path.stem for path in visible_files] or ["No saved maps"]) +
                ([status] if status else []),
            )
            box = pygame.Rect(0, 0, 520, 260)
            box.center = window.get_rect().center
            for index, path in enumerate(visible_files):
                rect = pygame.Rect(box.x + 20, box.y + 52 + index * 28,
                                   box.width - 40, 26)
                pygame.draw.rect(window, (68, 82, 96), rect)
                window.blit(font.render(
                    path.stem, True, (255, 255, 255)),
                    (rect.x + 8, rect.y + 4))
            window.blit(small_font.render(
                "Up/Down scroll; click a map; Esc cancels",
                True, (220, 220, 220)),
                (box.x + 20, box.bottom - 26))
        elif dialog == "tileset":
            _draw_dialog(
                window, font, "Map tileset",
                ["gfx/" + map_data["tileset"],
                 "%d x %d tiles; each tile is 64x64" %
                 (tile_columns, tile_rows),
                 "Maps record the tileset used by the game.",
                 "Click or press Enter/Esc to close"])

        pygame.display.flip()
        clock.tick(30)

    pygame.quit()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
