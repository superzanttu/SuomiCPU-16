from collections import Counter
from pathlib import Path
import re
import runpy

import pygame

from tools.c_compiler import compile_source


ROOT = Path(__file__).resolve().parents[1]
GAME_DIR = ROOT / "modules" / "Tank-16"
EDITOR_PATH = GAME_DIR / "map_editor.py"
MAIN_PATH = GAME_DIR / "main.c"
RUNTIME_PALETTE_BASE = 32
RUNTIME_PALETTE_CAPACITY = 256 - RUNTIME_PALETTE_BASE
OBJECT_REMOVAL_TYPES = ("fixed", "explosion", "disappear")


def object_tile_coordinates(map_data: dict) -> list[tuple[int, int]]:
    coordinates = set()
    for layer in ("obstacles", "objects"):
        for row in map_data[layer]:
            for cell in row:
                if cell is not None:
                    tile = cell if layer == "obstacles" else cell["tile"]
                    coordinates.add(tuple(tile))
    return sorted(coordinates, key=lambda tile: (tile[1], tile[0]))


def map_tile_pixel_size() -> int:
    source = MAIN_PATH.read_text(encoding="utf-8")
    match = re.search(
        r"(?m)^\s*#define\s+MAP_TILE_PIXEL_SIZE\s+(\d+)\s*$", source)
    if match is None:
        raise ValueError("main.c must define MAP_TILE_PIXEL_SIZE")
    size = int(match.group(1))
    if not 1 <= size <= 64:
        raise ValueError("MAP_TILE_PIXEL_SIZE must be between 1 and 64")
    return size


def _median_cut_palette(
    histogram: Counter[tuple[int, int, int]], limit: int,
) -> list[tuple[int, int, int]]:
    if len(histogram) <= limit:
        return sorted(histogram)

    boxes = [list(histogram.items())]
    while len(boxes) < limit:
        candidate_index = -1
        candidate_channel = 0
        candidate_score = -1
        for index, box in enumerate(boxes):
            if len(box) < 2:
                continue
            ranges = [
                max(color[channel] for color, _ in box) -
                min(color[channel] for color, _ in box)
                for channel in range(3)
            ]
            channel = max(range(3), key=ranges.__getitem__)
            score = ranges[channel] * sum(count for _, count in box)
            if score > candidate_score:
                candidate_index = index
                candidate_channel = channel
                candidate_score = score
        if candidate_index < 0:
            break

        box = boxes.pop(candidate_index)
        box.sort(key=lambda item: item[0][candidate_channel])
        midpoint = (sum(count for _, count in box) + 1) // 2
        accumulated = 0
        split = 1
        for split, (_, count) in enumerate(box, start=1):
            accumulated += count
            if accumulated >= midpoint:
                break
        split = min(max(split, 1), len(box) - 1)
        boxes.extend((box[:split], box[split:]))

    palette: list[tuple[int, int, int]] = []
    for box in boxes:
        weight = sum(count for _, count in box)
        red = (sum(color[0] * count for color, count in box) + weight // 2) // weight
        green = (sum(color[1] * count for color, count in box) + weight // 2) // weight
        blue = (sum(color[2] * count for color, count in box) + weight // 2) // weight
        palette.append((red, green, blue))
    return palette


def load_map_assets(
    map_path: Path, tile_size: int | None = None,
) -> tuple[
    dict, list[list[int]], list[tuple[int, int, int]],
    tuple[int, int, int],
]:
    if tile_size is None:
        tile_size = map_tile_pixel_size()
    if not 1 <= tile_size <= 64:
        raise ValueError("Map tile pixel size must be between 1 and 64")
    editor = runpy.run_path(str(EDITOR_PATH))
    map_data = editor["load_map"](Path(map_path))
    tileset_path = GAME_DIR / "gfx" / map_data["tileset"]
    tileset, columns, rows = editor["load_tileset"](tileset_path)
    sources = [
        tileset.subsurface((tile_x * 64, tile_y * 64, 64, 64))
        for tile_y in range(rows) for tile_x in range(columns)
    ]
    object_coordinates = object_tile_coordinates(map_data)
    if object_coordinates:
        objects, _, _ = editor["load_tileset"](
            GAME_DIR / "gfx" / map_data["objects_tileset"], pad_edges=True)
        sources.extend(
            objects.subsurface((x * 64, y * 64, 64, 64))
            for x, y in object_coordinates
        )
    if len(sources) > 256:
        raise ValueError("Terrain and used object tiles exceed 256 GPU tile slots")
    scaled_tiles = []
    histogram: Counter[tuple[int, int, int]] = Counter()
    for source_tile in sources:
        scaled = pygame.transform.smoothscale(
            source_tile, (tile_size, tile_size))
        pixels = pygame.image.tostring(scaled, "RGBA")
        scaled_tiles.append(pixels)
        for offset in range(0, len(pixels), 4):
            if pixels[offset + 3] >= 128:
                rgb = (
                    pixels[offset], pixels[offset + 1], pixels[offset + 2],
                )
                histogram[rgb] += 1

    if not histogram:
        raise ValueError("Tileset contains no opaque pixels")
    atlas_palette = _median_cut_palette(histogram, RUNTIME_PALETTE_CAPACITY)
    palette = [(0, 0, 0), *atlas_palette]
    color_indices = {
        color: min(
            range(len(atlas_palette)),
            key=lambda index: sum(
                (color[channel] - atlas_palette[index][channel]) ** 2
                for channel in range(3)),
        ) + RUNTIME_PALETTE_BASE
        for color in histogram
    }
    tiles = []
    for pixels in scaled_tiles:
        tile_pixels = []
        for offset in range(0, len(pixels), 4):
            if pixels[offset + 3] < 128:
                tile_pixels.append(0)
            else:
                rgb = (
                    pixels[offset], pixels[offset + 1], pixels[offset + 2],
                )
                tile_pixels.append(color_indices[rgb])
        tiles.append(tile_pixels)
    return map_data, tiles, palette, (columns, rows, tile_size)


def build_map_header(map_path: Path) -> str:
    map_data, tile_data, palette, dimensions = load_map_assets(map_path)
    columns, _, tile_size = dimensions
    map_values = [
        tile_y * columns + tile_x
        for row in map_data["tiles"]
        for tile_x, tile_y in row
    ]

    def format_array(values: list[int], width: int) -> str:
        return ",\n".join(
            "    " + ", ".join(str(value) for value in values[start:start + width])
            for start in range(0, len(values), width)
        )

    if len(map_values) <= 32768:
        map_data_header = (
            "unsigned char map_data[MAP_CELLS] = {\n%s\n};\n"
            % format_array(map_values, 16)
        )
    else:
        map_page_size = 32768
        map_data_header = ""
        for page, start in enumerate(range(0, len(map_values), map_page_size)):
            values = map_values[start:start + map_page_size]
            map_data_header += (
                "unsigned char map_data_%d[%d] = {\n%s\n};\n"
                % (page, len(values), format_array(values, 16))
            )
        map_data_header += "#define map_data map_data_0\n"

    tile_arrays = ",\n".join(
        "    {" + format_array(values, 16).replace("\n", "\n     ") + "}"
        for values in tile_data
    )
    upload_tiles = "\n".join(
        "    gfx_tile_upload(%d, terrain_tiles[%d]);" % (index, index)
        for index in range(len(tile_data))
    )
    coordinates = object_tile_coordinates(map_data)
    terrain_count = dimensions[0] * dimensions[1]
    object_ids = {
        tile: terrain_count + index for index, tile in enumerate(coordinates)
    }
    obstacle_tiles = [
        0 if cell is None else object_ids[tuple(cell)]
        for row in map_data["obstacles"] for cell in row
    ]
    objects = [
        (x, y, cell)
        for y, row in enumerate(map_data["objects"])
        for x, cell in enumerate(row) if cell is not None
    ]
    layer_header = (
        "#define MAP_OBJECT_COUNT %d\n"
        "#define MAP_OBJECT_CAPACITY %d\n"
        "#define OBJECT_FIXED 0\n"
        "#define OBJECT_EXPLOSION 1\n"
        "#define OBJECT_DISAPPEAR 2\n"
    ) % (len(objects), max(1, len(objects)))
    if len(obstacle_tiles) <= 32768:
        layer_header += (
            "unsigned char map_obstacles[MAP_CELLS] = {\n%s\n};\n"
            % format_array(obstacle_tiles, 16)
        )
    elif not any(obstacle_tiles):
        layer_header += "#define MAP_OBSTACLES_EMPTY 1\n"
    else:
        page_rows = 32
        page_capacity = page_rows * map_data["width"]
        page_count = (map_data["height"] + page_rows - 1) // page_rows
        for page in range(page_count):
            start = page * page_capacity
            end = min(start + page_capacity, len(obstacle_tiles))
            layer_header += (
                "unsigned char map_obstacles_%d[%d] = {\n%s\n};\n"
                % (page, page_capacity,
                   format_array(obstacle_tiles[start:end], 16))
            )
    fields = {
        "x": [x for x, _, _ in objects],
        "y": [y for _, y, _ in objects],
        "tile": [object_ids[tuple(cell["tile"])] for _, _, cell in objects],
        "hp": [cell["hp_change"] for _, _, cell in objects],
        "removal": [OBJECT_REMOVAL_TYPES.index(cell["removal"])
                    for _, _, cell in objects],
    }
    for name, values in fields.items():
        layer_header += (
            "int map_object_%s[MAP_OBJECT_CAPACITY] = {\n%s\n};\n"
            % (name, format_array(values or [0], 16))
        )
    return (
        "#define MAP_COLUMNS %d\n"
        "#define MAP_ROWS %d\n"
        "#define MAP_CELLS (MAP_COLUMNS * MAP_ROWS)\n"
        "#define MAP_TERRAIN_TILE_COUNT %d\n"
        "#define TILE_COUNT %d\n"
        "#define TILE_PIXEL_COUNT %d\n"
        "#define MAP_PALETTE_COUNT %d\n"
        "#define MAP_PALETTE_BASE %d\n"
        "#define MAP_TILESET_FILENAME \"%s\"\n"
        "unsigned char map_palette[MAP_PALETTE_COUNT][3] = {\n"
        "%s\n"
        "};\n"
        "unsigned char terrain_tiles[TILE_COUNT][TILE_PIXEL_COUNT] = {\n"
        "%s\n"
        "};\n"
        "void upload_terrain_tiles(void) {\n"
        "%s\n"
        "}\n"
        "%s"
        "%s"
    ) % (
        map_data["width"], map_data["height"], terrain_count, len(tile_data),
        tile_size * tile_size, len(palette), RUNTIME_PALETTE_BASE,
        map_data["tileset"],
        ",\n".join(
            "    {%s}" % ", ".join(str(channel) for channel in color)
            for color in palette
        ),
        tile_arrays, upload_tiles, map_data_header, layer_header,
    )


def compile_map_file(map_path: Path):
    source = MAIN_PATH.read_text(encoding="utf-8")
    include = '#include "map_assets.h"'
    if source.count(include) != 1:
        raise ValueError("Tank-16 main.c must include map_assets.h exactly once")
    source = source.replace(include, build_map_header(Path(map_path)), 1)
    return compile_source(source, str(MAIN_PATH))
