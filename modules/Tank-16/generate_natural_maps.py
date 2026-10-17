import argparse
import json
import math
from pathlib import Path

from generate_finland_map import (
    OBJECT_BUILDING_CELLS,
    ROAD_TILE_BY_CONNECTIONS,
    bridge_tile_id,
)
from map_editor import write_runtime_map


ROOT = Path(__file__).resolve().parents[2]
GAME_DIR = ROOT / "modules" / "Tank-16"
MAP_WIDTH = 48
MAP_HEIGHT = 32
MAPS = (
    ("ARCHIPEL", 11, "islands"),
    ("RIVER-01", 23, "river"),
    ("LAKESIDE", 37, "lakes"),
    ("DELTA-01", 41, "delta"),
    ("COAST-01", 53, "coast"),
    ("ISLANDS", 67, "islands"),
    ("FJORD-01", 79, "fjord"),
    ("MARSH-01", 83, "marsh"),
    ("HIGHLND", 97, "highland"),
    ("ESTUARY", 109, "estuary"),
)


def _distance_to_line(x: float, y: float, axis: str, center: float,
                      amplitude: float, phase: float,
                      frequency: float) -> float:
    if axis == "vertical":
        line_x = center + amplitude * math.sin(y * frequency + phase)
        return abs(x - line_x)
    line_y = center + amplitude * math.sin(x * frequency + phase)
    return abs(y - line_y)


def _ellipse_contains(
    x: float, y: float, center_x: float, center_y: float,
    radius_x: float, radius_y: float, noise: float,
) -> bool:
    dx = (x - center_x) / radius_x
    dy = (y - center_y) / radius_y
    return dx * dx + dy * dy < 1 + noise * 0.08


def _is_water(kind: str, x: int, y: int, seed: int) -> bool:
    noise = (
        math.sin(x * 0.31 + seed) * 0.8
        + math.sin(y * 0.27 + seed * 1.7) * 0.7
        + math.cos((x - y) * 0.13 + seed * 0.37) * 0.6
    )
    if kind == "islands":
        islands = (
            (11, 13, 11, 8), (29, 10, 8, 6),
            (35, 23, 8, 7), (16, 26, 7, 5),
        )
        return not any(
            _ellipse_contains(
                x, y, center_x, center_y, radius_x, radius_y, noise)
            for center_x, center_y, radius_x, radius_y in islands
        )
    if kind == "river":
        return _distance_to_line(
            x, y, "vertical", 24, 5, seed * 0.17, 0.21) < 2.4 + noise * 0.3
    if kind == "lakes":
        lakes = (
            (13, 10, 5, 4), (34, 8, 6, 4),
            (32, 23, 8, 5), (12, 24, 5, 4),
        )
        lake = any(
            _ellipse_contains(
                x, y, center_x, center_y, radius_x, radius_y, noise)
            for center_x, center_y, radius_x, radius_y in lakes
        )
        river = _distance_to_line(
            x, y, "horizontal", 16, 8, seed * 0.31, 0.11) < 0.7
        return lake or river
    if kind == "delta":
        main_river = _distance_to_line(
            x, y, "vertical", 23, 3, seed * 0.11, 0.15) < 1.8
        branches = (
            _distance_to_line(
                x, y, "horizontal", 21, 8, seed * 0.23, 0.12) < 1.3
            if y > 15 else False
        )
        sea = y > 27 + noise * 0.8
        return main_river or branches or sea
    if kind == "coast":
        coast_x = 8 + 3 * math.sin(y * 0.19 + seed * 0.2) + noise
        bay = _ellipse_contains(x, y, 20, 8, 9, 5, noise)
        return x < coast_x or bay
    if kind == "fjord":
        inlet = _distance_to_line(
            x, y, "vertical", 24, 9, seed * 0.29, 0.12)
        branches = _distance_to_line(
            x, y, "vertical", 12, 4, seed * 0.13, 0.18)
        return inlet < 2.2 + noise * 0.25 or (
            y < 14 and branches < 1.6)
    if kind == "marsh":
        ponds = (
            _ellipse_contains(x, y, 10, 9, 4, 3, noise)
            or _ellipse_contains(x, y, 36, 22, 5, 4, noise)
            or _ellipse_contains(x, y, 22, 25, 4, 3, noise)
        )
        channels = (
            _distance_to_line(
                x, y, "horizontal", 16, 5, seed * 0.41, 0.17) < 1
            or _distance_to_line(
                x, y, "vertical", 29, 3, seed * 0.33, 0.14) < 0.8
        )
        return ponds or channels
    if kind == "highland":
        lakes = (
            _ellipse_contains(x, y, 15, 9, 3, 3, noise)
            or _ellipse_contains(x, y, 34, 22, 4, 3, noise)
        )
        river = _distance_to_line(
            x, y, "vertical", 24, 4, seed * 0.27, 0.2) < 1.2
        return lakes or river
    coast_x = 38 + 2.5 * math.sin(y * 0.18 + seed)
    river = _distance_to_line(
        x, y, "horizontal", 15, 5, seed * 0.19, 0.13) < 1.8
    return x > coast_x or river


def _trace_route(axis: str, center: float, amplitude: float,
                 phase: float, frequency: float,
                 width: int, height: int) -> set[tuple[int, int]]:
    steps = max(width, height) * 8
    cells = set()
    for step in range(steps + 1):
        progress = step / steps
        if axis == "horizontal":
            x = round(progress * (width - 1))
            y = round(center + amplitude * math.sin(
                progress * (width - 1) * frequency + phase))
        else:
            y = round(progress * (height - 1))
            x = round(center + amplitude * math.sin(
                progress * (height - 1) * frequency + phase))
        if 0 <= x < width and 0 <= y < height:
            cells.add((x, y))
    return cells


def _connections(cells: set[tuple[int, int]], x: int, y: int) -> int:
    return (
        (1 if (x, y - 1) in cells else 0)
        | (2 if (x + 1, y) in cells else 0)
        | (4 if (x, y + 1) in cells else 0)
        | (8 if (x - 1, y) in cells else 0)
    )


def generate_map(name: str, seed: int, kind: str) -> dict:
    width, height = MAP_WIDTH, MAP_HEIGHT
    water = [
        [_is_water(kind, x, y, seed) for x in range(width)]
        for y in range(height)
    ]
    dry = [[not water[y][x] for x in range(width)] for y in range(height)]
    roads = _trace_route(
        "horizontal", height * 0.51, 5, seed * 0.13, 0.14, width, height)
    railways = _trace_route(
        "vertical", width * 0.50, 7, seed * 0.27, 0.18, width, height)
    objects = [[None for _ in range(width)] for _ in range(height)]
    obstacles = [[None for _ in range(width)] for _ in range(height)]
    city_cells = set()
    cities = []

    for city_x, city_y in (
            (width // 2, height // 4),
            (width // 2, height * 3 // 4)):
        placement = None
        for radius in range(8):
            candidates = []
            for top in range(city_y - 1 - radius, city_y + radius):
                for left in range(city_x - 1 - radius,
                                  city_x + radius):
                    cells = [
                        (left + dx, top + dy)
                        for dy in range(3) for dx in range(3)
                    ]
                    if all(
                        0 <= x < width and 0 <= y < height
                        and dry[y][x] and (x, y) not in city_cells
                        for x, y in cells
                    ):
                        score = sum(
                            (x - city_x) ** 2 + (y - city_y) ** 2
                            for x, y in cells
                        )
                        candidates.append((score, left, top, cells))
            if candidates:
                _, left, top, cells = min(candidates)
                placement = (left, top, cells)
                break
        if placement is None:
            continue
        left, top, cells = placement
        for index, (x, y) in enumerate(cells):
            city_cells.add((x, y))
            objects[y][x] = {
                "tile": list(OBJECT_BUILDING_CELLS[index]),
                "hp_change": 0,
                "removal": "fixed",
            }
        cities.append([left + 1, top + 1])

    for y in range(height):
        for x in range(width):
            if not dry[y][x] or (x, y) in city_cells:
                continue
            if (x, y) in roads or (x, y) in railways:
                continue
            forest = (
                math.sin(x * 0.27 + seed) +
                math.cos(y * 0.31 - seed * 0.3) +
                math.sin((x + y) * 0.17 + seed * 0.2)
            )
            noise = (x * 73 + y * 151 + seed * 17) % 43
            if forest > 0.45 and noise < 2:
                obstacles[y][x] = [(x + seed) % 6, 5]
            elif forest < -0.8 and noise == 2:
                obstacles[y][x] = [2 + (y + seed) % 4, 4]

    tiles = []
    counts = {
        "water": 0, "roads": 0, "railways": 0,
        "road_bridges": 0, "rail_bridges": 0,
        "trees_and_rocks": sum(
            cell is not None for row in obstacles for cell in row),
        "city_tiles": len(cities) * len(OBJECT_BUILDING_CELLS),
    }
    for y in range(height):
        row = []
        for x in range(width):
            if water[y][x]:
                counts["water"] += 1
                if (x, y) in railways:
                    connection = _connections(railways, x, y)
                    tile_id = bridge_tile_id(
                        connection, rail=True, variant=x + y)
                    counts["rail_bridges"] += 1
                elif (x, y) in roads:
                    connection = _connections(roads, x, y)
                    tile_id = bridge_tile_id(connection, variant=x + y)
                    counts["road_bridges"] += 1
                else:
                    neighbors = (
                        (1 if y > 0 and dry[y - 1][x] else 0)
                        | (2 if x + 1 < width and dry[y][x + 1] else 0)
                        | (4 if y + 1 < height and dry[y + 1][x] else 0)
                        | (8 if x > 0 and dry[y][x - 1] else 0)
                    )
                    tile_id = 40 + neighbors
            else:
                near_water = any((
                    y > 0 and water[y - 1][x],
                    x + 1 < width and water[y][x + 1],
                    y + 1 < height and water[y + 1][x],
                    x > 0 and water[y][x - 1],
                ))
                tile_id = 20 if near_water else 0
                if (x, y) in railways:
                    tile_id = 70 + _connections(railways, x, y)
                    counts["railways"] += 1
                elif (x, y) in roads:
                    road_tile = ROAD_TILE_BY_CONNECTIONS.get(
                        _connections(roads, x, y))
                    if road_tile is not None:
                        tile_id = road_tile
                        counts["roads"] += 1
            row.append([tile_id % 10, tile_id // 10])
        tiles.append(row)

    return {
        "format_version": 1,
        "name": name,
        "width": width,
        "height": height,
        "tile_size": 64,
        "tileset": "tiles_terrain.png",
        "objects_tileset": "tiles_objects.png",
        "geography": {
            "source": "Deterministic natural landscape generator",
            "landscape": kind,
            "seed": seed,
            "tile_counts": counts,
            "city_centers": cities,
            "rail_connection_bits": {
                "north": 1, "east": 2, "south": 4, "west": 8,
            },
        },
        "obstacles": obstacles,
        "objects": objects,
        "tiles": tiles,
    }


def _write_map(path: Path, data: dict) -> None:
    write_runtime_map(data, path)


def generate_maps(maps_dir: Path = GAME_DIR / "disks" / "maps") -> list[Path]:
    generated = []
    for name, seed, kind in MAPS:
        data = generate_map(name, seed, kind)
        path = maps_dir / f"{name}.json"
        _write_map(path, data)
        generated.append(path)
    return generated


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--maps-dir", type=Path, default=GAME_DIR / "disks" / "maps")
    arguments = parser.parse_args()
    generated = generate_maps(arguments.maps_dir)
    print(json.dumps({
        "maps": [str(path) for path in generated],
        "count": len(generated),
    }))


if __name__ == "__main__":
    main()
