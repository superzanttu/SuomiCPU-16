from collections import Counter
from collections.abc import Callable
from pathlib import Path
import re
import runpy
import sys

import pygame

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools.c_compiler import compile_source


GAME_DIR = ROOT / "modules" / "Tank-16"
EDITOR_PATH = GAME_DIR / "map_editor.py"
MAIN_PATH = GAME_DIR / "main.c"
RUNTIME_TILE_DATA_PATH = GAME_DIR / "disks" / "assets" / "TANK16_TILES.json"
RUNTIME_PALETTE_BASE = 32
RUNTIME_PALETTE_CAPACITY = 256 - RUNTIME_PALETTE_BASE
OBJECT_REMOVAL_TYPES = ("fixed", "explosion", "disappear")


def object_tile_coordinates(map_data: dict) -> list[tuple[int, int]]:
    coordinates = {(3, 2), (4, 2), (5, 2)}
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
    logger: Callable[[str], None] | None = None,
) -> tuple[
    dict, list[list[int]], list[tuple[int, int, int]],
    tuple[int, int, int],
]:
    report = logger if logger is not None else (lambda message: None)
    report(f"[Tank-16 assets] Reading map source: {Path(map_path).resolve()}")
    if tile_size is None:
        report(f"[Tank-16 assets] Reading tile-size setting from {MAIN_PATH}")
        tile_size = map_tile_pixel_size()
    if not 1 <= tile_size <= 64:
        raise ValueError("Map tile pixel size must be between 1 and 64")
    report(f"[Tank-16 assets] Loading map editor: {EDITOR_PATH}")
    editor = runpy.run_path(str(EDITOR_PATH))
    map_data = editor["load_map"](Path(map_path))
    tileset_path = GAME_DIR / "gfx" / map_data["tileset"]
    report(f"[Tank-16 assets] Loading terrain tileset: {tileset_path}")
    tileset, columns, rows = editor["load_tileset"](tileset_path)
    sources = [
        tileset.subsurface((tile_x * 64, tile_y * 64, 64, 64))
        for tile_y in range(rows) for tile_x in range(columns)
    ]
    object_coordinates = object_tile_coordinates(map_data)
    if object_coordinates:
        objects_path = GAME_DIR / "gfx" / map_data["objects_tileset"]
        report(
            f"[Tank-16 assets] Loading {len(object_coordinates)} used object "
            f"tiles from {objects_path}"
        )
        objects, _, _ = editor["load_tileset"](
            objects_path, pad_edges=True)
        sources.extend(
            objects.subsurface((x * 64, y * 64, 64, 64))
            for x, y in object_coordinates
        )
    if len(sources) > 256:
        raise ValueError("Terrain and used object tiles exceed 256 GPU tile slots")
    report(
        f"[Tank-16 assets] Scaling {len(sources)} tiles from "
        f"{map_data['tileset']} to {tile_size}x{tile_size} pixels"
    )
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
    report(
        f"[Tank-16 assets] Building runtime palette from "
        f"{len(histogram)} opaque colors"
    )
    atlas_palette = _median_cut_palette(histogram, RUNTIME_PALETTE_CAPACITY)
    palette = [(0, 0, 0), *atlas_palette]
    color_indices = {}
    for color_number, color in enumerate(histogram, start=1):
        if color_number == 1 or color_number % 512 == 0:
            report(
                f"[Tank-16 assets] Mapping source colors: "
                f"{color_number}/{len(histogram)}"
            )
        color_indices[color] = min(
            range(len(atlas_palette)),
            key=lambda index: sum(
                (color[channel] - atlas_palette[index][channel]) ** 2
                for channel in range(3)),
        ) + RUNTIME_PALETTE_BASE
    report(f"[Tank-16 assets] Converting {len(scaled_tiles)} tiles to palette indices")
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


def build_map_header(
    map_path: Path, round_map_paths: list[Path] | None = None,
) -> str:
    map_data, tile_data, palette, dimensions = load_map_assets(map_path)
    columns, _, tile_size = dimensions
    map_values = [
        tile_y * columns + tile_x
        for row in map_data["tiles"]
        for tile_x, tile_y in row
    ]
    editor = runpy.run_path(str(EDITOR_PATH))
    round_maps = [map_data]
    for round_map_path in round_map_paths or ():
        round_map = editor["load_map"](Path(round_map_path))
        if (round_map["width"], round_map["height"], round_map["tileset"]) != (
            map_data["width"], map_data["height"], map_data["tileset"],
        ):
            raise ValueError(
                f"Round map {round_map_path} must match the selected map's "
                "dimensions and terrain tileset"
            )
        if (
            object_tile_coordinates(round_map) and
            round_map["objects_tileset"] != map_data["objects_tileset"]
        ):
            raise ValueError(
                f"Round map {round_map_path} uses a different object tileset"
            )
        if not set(object_tile_coordinates(round_map)).issubset(
            object_tile_coordinates(map_data)
        ):
            raise ValueError(
                f"Round map {round_map_path} uses object tiles not included "
                "by the selected map"
            )
        round_maps.append(round_map)

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
    map_object_capacity = max(
        1,
        *(sum(cell is not None for row in round_map["objects"] for cell in row)
          for round_map in round_maps),
    )
    layer_header = (
        "#define MAP_VARIANT_COUNT %d\n"
        "#define MAP_OBJECT_COUNT %d\n"
        "#define MAP_OBJECT_CAPACITY %d\n"
        "#define POWERUP_HEALTH_TILE %d\n"
        "#define POWERUP_DAMAGE_TILE %d\n"
        "#define POWERUP_SPEED_TILE %d\n"
        "#define OBJECT_FIXED 0\n"
        "#define OBJECT_EXPLOSION 1\n"
        "#define OBJECT_DISAPPEAR 2\n"
    ) % (
        len(round_maps), len(objects), map_object_capacity,
        object_ids[(3, 2)], object_ids[(4, 2)], object_ids[(5, 2)],
    )
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
    if len(round_maps) > 1:
        variant_map_values = []
        variant_obstacles = []
        variant_objects = {name: [] for name in fields}
        variant_object_counts = []
        for round_map in round_maps:
            variant_map_values.append([
                tile_y * columns + tile_x
                for row in round_map["tiles"]
                for tile_x, tile_y in row
            ])
            variant_obstacles.append([
                0 if cell is None else object_ids[tuple(cell)]
                for row in round_map["obstacles"] for cell in row
            ])
            round_objects = [
                (x, y, cell)
                for y, row in enumerate(round_map["objects"])
                for x, cell in enumerate(row) if cell is not None
            ]
            variant_object_counts.append(len(round_objects))
            round_fields = {
                "x": [x for x, _, _ in round_objects],
                "y": [y for _, y, _ in round_objects],
                "tile": [
                    object_ids[tuple(cell["tile"])]
                    for _, _, cell in round_objects
                ],
                "hp": [cell["hp_change"] for _, _, cell in round_objects],
                "removal": [
                    OBJECT_REMOVAL_TYPES.index(cell["removal"])
                    for _, _, cell in round_objects
                ],
            }
            for name, values in round_fields.items():
                variant_objects[name].append(
                    values + [0] * (map_object_capacity - len(values))
                )
        layer_header += (
            "unsigned char map_variant_data[MAP_VARIANT_COUNT][MAP_CELLS] = {\n%s\n};\n"
            % ",\n".join(
                "    {%s}" % format_array(values, 16).replace("\n", "\n     ")
                for values in variant_map_values
            )
        )
        layer_header += (
            "unsigned char map_variant_obstacles[MAP_VARIANT_COUNT][MAP_CELLS] = {\n%s\n};\n"
            % ",\n".join(
                "    {%s}" % format_array(values, 16).replace("\n", "\n     ")
                for values in variant_obstacles
            )
        )
        layer_header += (
            "int map_variant_object_count[MAP_VARIANT_COUNT] = {%s};\n"
            % ", ".join(str(count) for count in variant_object_counts)
        )
        for name, values in variant_objects.items():
            layer_header += (
                "int map_variant_object_%s[MAP_VARIANT_COUNT][MAP_OBJECT_CAPACITY] = {\n%s\n};\n"
                % (
                    name,
                    ",\n".join(
                        "    {%s}" % format_array(row, 16).replace(
                            "\n", "\n     ")
                        for row in values
                    ),
                )
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


def build_runtime_assets_header(
    logger: Callable[[str], None] | None = None,
    tile_json_path: Path = RUNTIME_TILE_DATA_PATH,
) -> str:
    report = logger if logger is not None else (lambda message: None)
    editor = runpy.run_path(str(EDITOR_PATH))
    json_path = Path(tile_json_path)
    editor["convert_tilesets_to_json"](json_path, logger=logger)
    header = (
        "#define RUNTIME_MAP_LOADING 1\n"
        "#define MAP_COLUMNS map_columns\n"
        "#define MAP_ROWS map_rows\n"
        "#define MAP_OBJECT_COUNT 0\n"
        "#define MAP_OBJECT_CAPACITY 1024\n"
        "#define OBJECT_FIXED 0\n"
        "#define OBJECT_EXPLOSION 1\n"
        "#define OBJECT_DISAPPEAR 2\n"
        "#define TERRAIN_COLUMNS runtime_terrain_columns\n"
        "#define TERRAIN_ROWS runtime_terrain_rows\n"
        "#define TERRAIN_TILE_COUNT (runtime_terrain_columns * runtime_terrain_rows)\n"
        "#define OBJECT_TILE_BASE TERRAIN_TILE_COUNT\n"
        "#define OBJECT_COLUMNS runtime_object_columns\n"
        "#define OBJECT_ROWS runtime_object_rows\n"
        "#define RUNTIME_TILE_SIZE runtime_tile_size\n"
        "#define POWERUP_HEALTH_TILE (OBJECT_TILE_BASE + 2 * OBJECT_COLUMNS + 3)\n"
        "#define POWERUP_DAMAGE_TILE (OBJECT_TILE_BASE + 2 * OBJECT_COLUMNS + 4)\n"
        "#define POWERUP_SPEED_TILE (OBJECT_TILE_BASE + 2 * OBJECT_COLUMNS + 5)\n"
        "#define RUNTIME_TILE_COUNT runtime_tile_count\n"
        "#define TILE_PIXEL_COUNT (RUNTIME_TILE_SIZE * RUNTIME_TILE_SIZE)\n"
        "#define MAP_PALETTE_COUNT runtime_palette_count\n"
        f"#define MAP_PALETTE_BASE {RUNTIME_PALETTE_BASE}\n"
        '#define RUNTIME_TILE_DATA_PATH "assets/TANK16_TILES.json"\n'
        '#define RUNTIME_TILE_BINARY_PATH "assets/TANK16_TILES.bin"\n'
        "int map_columns;\n"
        "int map_rows;\n"
        "int runtime_terrain_columns;\n"
        "int runtime_terrain_rows;\n"
        "int runtime_object_columns;\n"
        "int runtime_object_rows;\n"
        "int runtime_tile_size;\n"
        "int runtime_tile_count;\n"
        "int runtime_palette_count;\n"
        "int runtime_tile_data_ready;\n"
        "int runtime_tile_binary;\n"
        "int runtime_tile_loaded[256];\n"
        "unsigned char *map_data;\n"
        "unsigned char *map_obstacles;\n"
        "volatile unsigned char map_json_chunk[4096];\n"
        "volatile char map_path_buffer[129];\n"
        "char map_selected_path[129];\n"
        "int map_object_x[MAP_OBJECT_CAPACITY];\n"
        "int map_object_y[MAP_OBJECT_CAPACITY];\n"
        "int map_object_tile[MAP_OBJECT_CAPACITY];\n"
        "int map_object_hp[MAP_OBJECT_CAPACITY];\n"
        "int map_object_removal[MAP_OBJECT_CAPACITY];\n"
        "unsigned char runtime_tile_pixels[4096];\n"
    )
    report(
        f"[Tank-16 assets] Generated runtime metadata header for {json_path}: "
        f"{len(header.encode('utf-8'))} bytes"
    )
    return header


def compile_runtime_tank16(
    logger: Callable[[str], None] | None = None,
):
    report = logger if logger is not None else (lambda message: None)
    report(f"[Tank-16 loader] Reading C source file: {MAIN_PATH}")
    source = MAIN_PATH.read_text(encoding="utf-8")
    include = '#include "map_assets.h"'
    if source.count(include) != 1:
        raise ValueError("Tank-16 main.c must include map_assets.h exactly once")
    report(f"[Tank-16 loader] Generating runtime graphics from {GAME_DIR / 'gfx'}")
    runtime_header = build_runtime_assets_header(logger)
    report(f"[Tank-16 loader] Injecting generated assets into {MAIN_PATH}")
    source = source.replace(include, runtime_header, 1)
    report(f"[Tank-16 loader] Compiling {MAIN_PATH} with generated assets")
    image = compile_source(source, str(MAIN_PATH), logger=logger)
    image_size = sum(len(segment.data) for segment in image.segments)
    report(
        f"[Tank-16 loader] Compilation finished: {image_size} bytes across "
        f"{len(image.segments)} segments"
    )
    return image


def discover_round_maps(map_path: Path) -> list[Path]:
    editor = runpy.run_path(str(EDITOR_PATH))
    selected_map = editor["load_map"](Path(map_path))
    selected_object_tiles = set(object_tile_coordinates(selected_map))
    compatible_maps = []
    for candidate in sorted((GAME_DIR / "disks" / "maps").glob("*.json")):
        if candidate.resolve() == Path(map_path).resolve():
            continue
        with candidate.open(encoding="utf-8") as map_file:
            header = map_file.read(1024)
        width = re.search(r'"width"\s*:\s*(\d+)', header)
        height = re.search(r'"height"\s*:\s*(\d+)', header)
        tileset = re.search(r'"tileset"\s*:\s*"([^"]+)"', header)
        if width is None or height is None or tileset is None:
            continue
        if (
            int(width.group(1)) != selected_map["width"] or
            int(height.group(1)) != selected_map["height"] or
            tileset.group(1) != selected_map["tileset"]
        ):
            continue
        candidate_map = editor["load_map"](candidate)
        if (
            (
                not object_tile_coordinates(candidate_map) or
                candidate_map["objects_tileset"] ==
                selected_map["objects_tileset"]
            ) and
            set(object_tile_coordinates(candidate_map)).issubset(
                selected_object_tiles
            )
        ):
            compatible_maps.append(candidate)
    return compatible_maps


def compile_map_file(map_path: Path, randomize_rounds: bool = False):
    source = MAIN_PATH.read_text(encoding="utf-8")
    include = '#include "map_assets.h"'
    if source.count(include) != 1:
        raise ValueError("Tank-16 main.c must include map_assets.h exactly once")
    round_map_paths = (
        discover_round_maps(Path(map_path)) if randomize_rounds else []
    )
    source = source.replace(
        include,
        build_map_header(Path(map_path), round_map_paths),
        1,
    )
    return compile_source(source, str(MAIN_PATH))
