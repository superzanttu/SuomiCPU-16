import argparse
import json
import math
from pathlib import Path

import pygame

from map_editor import apply_water_depth_tiles, write_runtime_map


ROOT = Path(__file__).resolve().parents[2]
GAME_DIR = ROOT / "modules" / "Tank-16"
ATLAS_PATH = GAME_DIR / "gfx" / "tiles_terrain.png"
TILE_SIZE = 64
WATER_TILE_BASE = 40
SOURCE_BASE = "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/"
COUNTRIES_SOURCE = SOURCE_BASE + "ne_10m_admin_0_countries.geojson"
LAKES_SOURCE = SOURCE_BASE + "ne_10m_lakes.geojson"
ROADS_SOURCE = SOURCE_BASE + "ne_10m_roads.geojson"
RAILROADS_SOURCE = SOURCE_BASE + "ne_10m_railroads.geojson"
POPULATED_PLACES_SOURCE = SOURCE_BASE + "ne_10m_populated_places.geojson"
ROAD_TILE_BY_CONNECTIONS = {
    1: 19, 2: 9, 3: 15, 4: 1, 5: 1, 6: 13, 7: 3,
    8: 2, 9: 16, 10: 2, 11: 6, 12: 14, 13: 4, 14: 5, 15: 11,
}
FALLBACK_RAIL_ROUTES = (
    ((24.94, 60.17), (23.76, 61.50), (22.84, 62.79),
     (25.75, 62.24), (27.68, 62.89), (27.73, 64.23),
     (25.47, 65.01), (25.72, 66.50)),
    ((24.94, 60.17), (25.66, 60.98), (26.70, 60.87),
     (28.19, 61.05), (29.76, 62.60)),
    ((23.76, 61.50), (21.79, 61.49), (21.62, 63.10)),
)
FALLBACK_FINNISH_CITIES = (
    ("Helsinki", 24.94, 60.17, 1_000_000),
    ("Tampere", 23.76, 61.50, 300_000),
    ("Turku", 22.27, 60.45, 250_000),
    ("Oulu", 25.47, 65.01, 200_000),
    ("Jyväskylä", 25.75, 62.24, 150_000),
    ("Kuopio", 27.68, 62.89, 120_000),
    ("Lahti", 25.66, 60.98, 120_000),
    ("Joensuu", 29.76, 62.60, 100_000),
    ("Rovaniemi", 25.72, 66.50, 85_000),
    ("Vaasa", 21.62, 63.10, 70_000),
    ("Kouvola", 26.70, 60.87, 70_000),
    ("Lappeenranta", 28.19, 61.05, 70_000),
    ("Seinäjoki", 22.84, 62.79, 65_000),
    ("Kajaani", 27.73, 64.23, 40_000),
    ("Kemi", 24.57, 65.74, 35_000),
)
OBJECT_BUILDING_CELLS = (
    (0, 0), (1, 0), (2, 0),
    (0, 1), (1, 1), (2, 1),
    (0, 2), (1, 2), (2, 2),
)


def project(longitude: float, latitude: float) -> tuple[float, float]:
    latitude = math.radians(latitude)
    delta = math.radians(longitude - 26)
    center = math.radians(64.5)
    scale = math.sqrt(2 / (
        1 + math.sin(center) * math.sin(latitude) +
        math.cos(center) * math.cos(latitude) * math.cos(delta)))
    return (
        6371.0088 * scale * math.cos(latitude) * math.sin(delta),
        6371.0088 * scale * (
            math.cos(center) * math.sin(latitude) -
            math.sin(center) * math.cos(latitude) * math.cos(delta)),
    )


def polygons(geometry: dict) -> list:
    if geometry["type"] == "Polygon":
        return [geometry["coordinates"]]
    if geometry["type"] == "MultiPolygon":
        return geometry["coordinates"]
    raise ValueError("Expected Polygon or MultiPolygon geometry")


def line_geometries(geometry: dict) -> list[list[list[float]]]:
    if geometry["type"] == "LineString":
        return [geometry["coordinates"]]
    if geometry["type"] == "MultiLineString":
        return geometry["coordinates"]
    return []


def connection_mask(pixels: bytes, width: int, height: int,
                    x: int, y: int) -> int:
    position = y * width + x
    return (
        (1 if y > 0 and pixels[position - width] else 0) |
        (2 if x + 1 < width and pixels[position + 1] else 0) |
        (4 if y + 1 < height and pixels[position + width] else 0) |
        (8 if x > 0 and pixels[position - 1] else 0)
    )


def bridge_tile_id(connections: int, *, rail: bool = False,
                   variant: int = 0) -> int:
    horizontal = bool(connections & (2 | 8))
    orientation = 0 if horizontal else 1
    if rail:
        return 86 + orientation + (2 if variant % 2 else 0)
    terminal = connections.bit_count() == 1
    stone = variant % 3 == 0
    if terminal:
        return (66 if stone else 62) + orientation
    if stone:
        return 64 + orientation
    return 68 + orientation if variant % 3 == 1 else 60 + orientation


def water_tile(neighbors: int, variant: int = 0) -> pygame.Surface:
    scale = 3
    size = TILE_SIZE * scale
    tile = pygame.Surface((size, size), pygame.SRCALPHA)

    def shoreline_offset(along: float) -> float:
        return (
            3.2
            + 2.2 * math.sin(along * math.tau / 27 + variant * 0.7)
            + 0.95 * math.sin(along * math.tau / 12 + variant * 1.9)
            + 0.45 * math.sin(along * math.tau / 6.7 + variant * 2.3)
        )

    for y in range(size):
        for x in range(size):
            px, py = x / scale, y / scale
            wave = math.sin(px * math.tau / 32 + math.sin(py * math.tau / 64))
            wave += math.sin(py * math.tau / 16 + variant) * 0.6
            noise = ((int(px) * 73 + int(py) * 151) % 11 - 5) * 0.25
            land_distances = []
            if neighbors & 1:
                land_distances.append(py - shoreline_offset(px))
            if neighbors & 2:
                land_distances.append(63 - px - shoreline_offset(py))
            if neighbors & 4:
                land_distances.append(63 - py - shoreline_offset(px))
            if neighbors & 8:
                land_distances.append(px - shoreline_offset(py))
            distance = min(land_distances, default=64)
            if distance < 1.2:
                color = (50, 185, 116, 255)
            elif distance < 5.5:
                color = (224, 203, 150, 255)
            elif distance < 9:
                color = (186, 222, 215, 255)
            else:
                shallow = max(0, 1 - distance / 26)
                color = (
                    int(36 + shallow * 28 + wave * 2 + noise),
                    int(105 + shallow * 48 + wave * 4 + noise),
                    int(157 + shallow * 24 + wave * 5 + noise),
                    255,
                )
            tile.set_at((x, y), color)
    for index in range(3):
        x = (11 + index * 21 + variant * 7) % 57 + 3
        y = (15 + index * 19 + variant * 11) % 57 + 3
        if not neighbors:
            pygame.draw.arc(tile, (91, 157, 192, 255),
                            ((x - 3) * scale, y * scale, 8 * scale, 2 * scale),
                            0.15, 2.8, scale)
    tile = pygame.transform.smoothscale(tile, (TILE_SIZE, TILE_SIZE))
    tile.fill((0, 0, 0, 255), special_flags=pygame.BLEND_RGBA_MAX)
    return tile


def bridge_tile(orientation: str, style: str = "wood",
                terminal: bool = False, variant: int = 0) -> pygame.Surface:
    if orientation not in ("horizontal", "vertical"):
        raise ValueError("Bridge orientation must be horizontal or vertical")
    if style not in ("wood", "stone"):
        raise ValueError("Bridge style must be wood or stone")
    tile = water_tile(0, variant)
    shadow = (25, 48, 57)
    if style == "wood":
        deck = (144, 99, 58)
        plank = (175, 127, 78)
        alternate = (126, 84, 49)
        rail = (82, 57, 42)
        highlight = (205, 158, 102)
    else:
        deck = (139, 145, 137)
        plank = (177, 181, 167)
        alternate = (119, 128, 124)
        rail = (77, 91, 91)
        highlight = (212, 211, 190)

    pygame.draw.rect(tile, shadow, (0, 15, 64, 35))
    pygame.draw.rect(tile, rail, (0, 16, 64, 32))
    pygame.draw.rect(tile, deck, (0, 20, 64, 24))
    for x in range(1, 64, 8):
        color = plank if ((x // 8 + variant) % 2 == 0) else alternate
        pygame.draw.rect(tile, color, (x, 21, 7, 22))
        pygame.draw.line(tile, highlight, (x, 21), (x, 21))
        pygame.draw.line(tile, rail, (x + 7, 22), (x + 7, 42))
    pygame.draw.rect(tile, rail, (0, 16, 64, 4))
    pygame.draw.rect(tile, highlight, (0, 17, 64, 1))
    pygame.draw.rect(tile, rail, (0, 44, 64, 4))
    pygame.draw.rect(tile, highlight, (0, 45, 64, 1))
    for x in range(4, 64, 16):
        pygame.draw.rect(tile, deck, (x, 14, 4, 6))
        pygame.draw.rect(tile, deck, (x, 44, 4, 6))
    if terminal:
        pygame.draw.rect(tile, rail, (0, 19, 3, 26))
        pygame.draw.rect(tile, highlight, (1, 20, 1, 24))
        pygame.draw.rect(tile, rail, (61, 19, 3, 26))
        pygame.draw.rect(tile, highlight, (62, 20, 1, 24))
    if orientation == "vertical":
        tile = pygame.transform.rotate(tile, 90)
    return tile


def _rail_path(tile: pygame.Surface, points: list[tuple[float, float]],
               variant: int = 0) -> None:
    points = [(round(x), round(y)) for x, y in points]
    pygame.draw.lines(tile, (91, 88, 75), False, points, 19)
    pygame.draw.lines(tile, (126, 121, 102), False, points, 15)
    lengths = []
    total = 0.0
    for index, (start, end) in enumerate(zip(points, points[1:])):
        length = math.dist(start, end)
        if length:
            lengths.append((total, length, index, start, end))
        total += length
    spacing = 8
    distance = 3 + variant * 2
    while distance < total - 2:
        offset, length, index, start, end = next(
            entry for entry in lengths
            if entry[0] <= distance <= entry[0] + entry[1])
        ratio = (distance - offset) / length
        x = start[0] + (end[0] - start[0]) * ratio
        y = start[1] + (end[1] - start[1]) * ratio
        before = points[max(0, index - 1)]
        after = points[min(len(points) - 1, index + 2)]
        tangent_x = after[0] - before[0]
        tangent_y = after[1] - before[1]
        tangent_length = math.hypot(tangent_x, tangent_y) or 1
        normal = (-tangent_y / tangent_length, tangent_x / tangent_length)
        pygame.draw.line(
            tile, (88, 62, 43),
            (round(x - normal[0] * 10), round(y - normal[1] * 10)),
            (round(x + normal[0] * 10), round(y + normal[1] * 10)), 3)
        pygame.draw.line(
            tile, (154, 117, 78),
            (round(x - normal[0] * 9), round(y - normal[1] * 9)),
            (round(x + normal[0] * 9), round(y + normal[1] * 9)), 1)
        distance += spacing

    for offset in (-5.5, 5.5):
        rail_points = []
        for index, point in enumerate(points):
            before = points[max(0, index - 1)]
            after = points[min(len(points) - 1, index + 1)]
            tangent_x = after[0] - before[0]
            tangent_y = after[1] - before[1]
            tangent_length = math.hypot(tangent_x, tangent_y) or 1
            normal = (-tangent_y / tangent_length, tangent_x / tangent_length)
            rail_points.append((
                round(point[0] + normal[0] * offset),
                round(point[1] + normal[1] * offset),
            ))
        pygame.draw.lines(tile, (50, 54, 54), False, rail_points, 4)
        pygame.draw.lines(tile, (191, 197, 186), False, rail_points, 2)


def _track_curve(start: tuple[float, float],
                 first_control: tuple[float, float],
                 second_control: tuple[float, float],
                 end: tuple[float, float]) -> list[tuple[float, float]]:
    points = []
    for step in range(25):
        t = step / 24
        inverse = 1 - t
        points.append((
            inverse ** 3 * start[0]
            + 3 * inverse ** 2 * t * first_control[0]
            + 3 * inverse * t ** 2 * second_control[0]
            + t ** 3 * end[0],
            inverse ** 3 * start[1]
            + 3 * inverse ** 2 * t * first_control[1]
            + 3 * inverse * t ** 2 * second_control[1]
            + t ** 3 * end[1],
        ))
    return points


def train_track_tile(connections: int, variant: int = 0) -> pygame.Surface:
    if not isinstance(connections, int) or not 0 <= connections <= 15:
        raise ValueError("Track connections must be a 4-bit direction mask")
    tile = pygame.Surface((TILE_SIZE, TILE_SIZE))
    tile.fill((45, 177, 103))
    for y in range(TILE_SIZE):
        for x in range(TILE_SIZE):
            texture = (x * 73 + y * 151 + variant * 31) % 23
            if texture == 0:
                tile.set_at((x, y), (37, 162, 92))
            elif texture == 1:
                tile.set_at((x, y), (57, 190, 113))

    if connections == 0:
        return tile

    directions = (
        (1, (32, 0)), (2, (64, 32)),
        (4, (32, 64)), (8, (0, 32)),
    )
    present = [(bit, point) for bit, point in directions
               if connections & bit]
    if len(present) == 2 and connections in (3, 6, 12, 9):
        curves = {
            3: ((32, -2), (32, 16), (48, 32), (66, 32)),
            6: ((66, 32), (48, 32), (32, 48), (32, 66)),
            12: ((32, 66), (32, 48), (16, 32), (-2, 32)),
            9: ((-2, 32), (16, 32), (32, 16), (32, -2)),
        }
        _rail_path(tile, _track_curve(*curves[connections]), variant)
    elif len(present) == 2:
        start, end = present[0][1], present[1][1]
        _rail_path(tile, [start, end], variant)
    else:
        center = (32, 32)
        for _, point in present:
            _rail_path(tile, [point, center], variant)
    return tile


def rail_bridge_tile(orientation: str, style: str) -> pygame.Surface:
    tile = bridge_tile(orientation, style)
    if orientation == "horizontal":
        points = [(0, 32), (64, 32)]
    elif orientation == "vertical":
        points = [(32, 0), (32, 64)]
    else:
        raise ValueError("Bridge orientation must be horizontal or vertical")
    _rail_path(tile, points)
    return tile


def deep_water_tile() -> pygame.Surface:
    source = water_tile(0, 1)
    tile = pygame.Surface((TILE_SIZE, TILE_SIZE), pygame.SRCALPHA)
    for y in range(TILE_SIZE):
        for x in range(TILE_SIZE):
            red, green, blue, alpha = source.get_at((x, y))
            variation = max(0, min(255, blue - 145))
            tile.set_at((x, y), (
                5 + variation // 20,
                18 + variation // 3,
                52 + variation * 3 // 4,
                alpha,
            ))
    return tile


def extend_atlas(path: Path = ATLAS_PATH) -> None:
    original = pygame.image.load(str(path))
    if original.get_size() not in (
            (640, 256), (640, 384), (640, 448), (640, 576), (640, 640)):
        raise ValueError(
            "Expected a 640x256, 640x384, 640x448, 640x576 or 640x640 terrain atlas")
    atlas = pygame.Surface((640, 640), pygame.SRCALPHA)
    atlas.blit(original, (0, 0))
    for index in range(20):
        tile_id = WATER_TILE_BASE + index
        tile = water_tile(index if index < 16 else 0, max(0, index - 15))
        atlas.blit(tile, ((tile_id % 10) * TILE_SIZE, (tile_id // 10) * TILE_SIZE))
    for index in range(10):
        bridge_id = 60 + index
        orientation = "horizontal" if index % 2 == 0 else "vertical"
        style = "stone" if 4 <= index <= 7 else "wood"
        terminal = 2 <= index <= 3 or 6 <= index <= 7
        variant = 1 if index >= 8 else 0
        tile = bridge_tile(orientation, style, terminal, variant)
        atlas.blit(tile, ((bridge_id % 10) * TILE_SIZE,
                          (bridge_id // 10) * TILE_SIZE))
    for connections in range(16):
        track_id = 70 + connections
        tile = train_track_tile(connections)
        atlas.blit(tile, ((track_id % 10) * TILE_SIZE,
                          (track_id // 10) * TILE_SIZE))
    for index, (orientation, style) in enumerate((
            ("horizontal", "wood"), ("vertical", "wood"),
            ("horizontal", "stone"), ("vertical", "stone"))):
        tile_id = 86 + index
        tile = rail_bridge_tile(orientation, style)
        atlas.blit(tile, ((tile_id % 10) * TILE_SIZE,
                          (tile_id // 10) * TILE_SIZE))
    atlas.blit(deep_water_tile(), (0, 9 * TILE_SIZE))
    pygame.image.save(atlas, str(path))


def generate_map(countries_path: Path, lakes_path: Path,
                 output: Path, size: int = 1000,
                 preview: Path | None = None,
                 roads_path: Path | None = None,
                 railways_path: Path | None = None,
                 cities_path: Path | None = None) -> dict:
    if not 1 <= size <= 100:
        raise ValueError("Map size must be between 1 and 100 tiles for runtime loading")
    countries = json.loads(countries_path.read_text(encoding="utf-8"))
    lakes = json.loads(lakes_path.read_text(encoding="utf-8"))
    roads = (
        json.loads(roads_path.read_text(encoding="utf-8"))
        if roads_path is not None else None
    )
    railways = (
        json.loads(railways_path.read_text(encoding="utf-8"))
        if railways_path is not None else None
    )
    populated_places = (
        json.loads(cities_path.read_text(encoding="utf-8"))
        if cities_path is not None else None
    )
    road_source = ROADS_SOURCE if roads is not None else None
    finland = next(feature for feature in countries["features"]
                   if feature["properties"].get("ADMIN") == "Finland")
    points = [project(*point[:2])
              for polygon in polygons(finland["geometry"])
              for ring in polygon for point in ring]
    minimum_x = min(point[0] for point in points)
    maximum_x = max(point[0] for point in points)
    minimum_y = min(point[1] for point in points)
    maximum_y = max(point[1] for point in points)
    span = max(maximum_x - minimum_x, maximum_y - minimum_y) * 1.06
    left = (minimum_x + maximum_x - span) / 2
    top = (minimum_y + maximum_y + span) / 2

    def pixel(point: list) -> tuple[int, int]:
        x, y = project(*point[:2])
        return round((x - left) / span * (size - 1)), round((top - y) / span * (size - 1))

    def draw_geometry(target: pygame.Surface, geometry: dict, value: int) -> None:
        for polygon in polygons(geometry):
            exterior = polygon[0]
            if (max(point[0] for point in exterior) < 5 or
                    min(point[0] for point in exterior) > 47 or
                    max(point[1] for point in exterior) < 55 or
                    min(point[1] for point in exterior) > 74):
                continue
            projected = [pixel(point) for point in exterior]
            if (max(point[0] for point in projected) < 0 or
                    min(point[0] for point in projected) >= size or
                    max(point[1] for point in projected) < 0 or
                    min(point[1] for point in projected) >= size):
                continue
            pygame.draw.polygon(target, value, projected)
            for hole in polygon[1:]:
                pygame.draw.polygon(target, 0, [pixel(point) for point in hole])

    land = pygame.Surface((size, size), depth=8)
    national = pygame.Surface((size, size), depth=8)
    water = pygame.Surface((size, size), depth=8)
    for feature in countries["features"]:
        draw_geometry(land, feature["geometry"], 1)
    draw_geometry(national, finland["geometry"], 1)
    for feature in lakes["features"]:
        draw_geometry(water, feature["geometry"], 1)
    def draw_lines(target: pygame.Surface, features: list[dict]) -> int:
        feature_count = 0
        for feature in features:
            feature_drawn = False
            for line in line_geometries(feature["geometry"]):
                if (len(line) < 2 or
                        max(point[0] for point in line) < 18 or
                        min(point[0] for point in line) > 34 or
                        max(point[1] for point in line) < 59 or
                        min(point[1] for point in line) > 72):
                    continue
                projected = [pixel(point) for point in line]
                if (
                    max(point[0] for point in projected) < 0 or
                    min(point[0] for point in projected) >= size or
                    max(point[1] for point in projected) < 0 or
                    min(point[1] for point in projected) >= size
                ):
                    continue
                pygame.draw.lines(target, 1, False, projected, 1)
                feature_drawn = True
            feature_count += int(feature_drawn)
        return feature_count

    road_cells = pygame.Surface((size, size), depth=8)
    road_feature_count = draw_lines(
        road_cells, roads["features"] if roads is not None else [])
    rail_cells = pygame.Surface((size, size), depth=8)
    if railways is not None:
        rail_features = railways["features"]
    else:
        rail_features = [
            {"geometry": {"type": "LineString", "coordinates": list(route)}}
            for route in FALLBACK_RAIL_ROUTES
        ]
    railway_feature_count = draw_lines(rail_cells, rail_features)

    if populated_places is None:
        city_candidates = list(FALLBACK_FINNISH_CITIES)
    else:
        city_candidates = []
        for feature in populated_places["features"]:
            properties = feature.get("properties", {})
            if feature["geometry"]["type"] != "Point":
                continue
            country = properties.get(
                "SOV0NAME", properties.get("ADM0NAME", ""))
            if country != "Finland":
                continue
            name = properties.get("NAME_EN") or properties.get("NAME") or "City"
            coordinates = feature["geometry"]["coordinates"]
            city_candidates.append((
                name, coordinates[0], coordinates[1],
                properties.get("POP_MAX", 0) or 0,
            ))
    city_candidates.sort(key=lambda city: city[3], reverse=True)
    city_candidates = city_candidates[:15]

    land_pixels = pygame.image.tobytes(land, "P")
    finland_pixels = pygame.image.tobytes(national, "P")
    lake_pixels = pygame.image.tobytes(water, "P")
    road_pixels = pygame.image.tobytes(road_cells, "P")
    rail_pixels = pygame.image.tobytes(rail_cells, "P")
    dry = bytes(1 if ground and not lake else 0
                for ground, lake in zip(land_pixels, lake_pixels))
    finland_roads = bytes(
        1 if road and ground and national else 0
        for road, ground, national in zip(road_pixels, dry, finland_pixels)
    )
    finland_railways = bytes(
        1 if rail and ground and national else 0
        for rail, ground, national in zip(rail_pixels, dry, finland_pixels)
    )
    water_roads = bytes(
        1 if road and not ground else 0
        for road, ground in zip(road_pixels, dry)
    )
    water_railways = bytes(
        1 if rail and not ground else 0
        for rail, ground in zip(rail_pixels, dry)
    )

    objects = [[None for _ in range(size)] for _ in range(size)]
    obstacles = [[None for _ in range(size)] for _ in range(size)]
    city_cells = bytearray(size * size)
    cities = []
    for name, longitude, latitude, population in city_candidates:
        city_x, city_y = pixel([longitude, latitude])
        if not (0 <= city_x < size and 0 <= city_y < size):
            continue
        placements = []
        for radius in range(6):
            for city_top in range(city_y - 1 - radius, city_y + radius):
                for left_edge in range(city_x - 1 - radius,
                                       city_x + radius):
                    cells = [
                            (left_edge + dx, city_top + dy)
                        for dy in range(3) for dx in range(3)
                    ]
                    if any(
                        not (0 <= x < size and 0 <= y < size) or
                        not finland_pixels[y * size + x] or
                        city_cells[y * size + x]
                        for x, y in cells
                    ):
                        continue
                    placements.append((
                        (left_edge - city_x) ** 2 +
                        (city_top - city_y) ** 2,
                        left_edge, city_top, cells,
                    ))
            if placements:
                break
        if not placements:
            continue
        _, left_edge, city_top, cells = min(
            placements, key=lambda item: item[0])
        for index, (x, y) in enumerate(cells):
            city_cells[y * size + x] = 1
            objects[y][x] = {
                "tile": list(OBJECT_BUILDING_CELLS[index]),
                "hp_change": 0,
                "removal": "fixed",
            }
        cities.append({
            "name": name,
            "population": population,
            "tile": [left_edge + 1, city_top + 1],
        })

    forest_obstacles = 0
    rock_obstacles = 0
    for y in range(size):
        for x in range(size):
            position = y * size + x
            if (not finland_pixels[position] or not dry[position] or
                    city_cells[position] or finland_roads[position] or
                    finland_railways[position]):
                continue
            forest_field = (
                math.sin(x * 0.17 + math.sin(y * 0.09)) +
                math.cos(y * 0.19 - x * 0.035) +
                math.sin((x + y) * 0.11)
            )
            noise = (x * 73 + y * 151) % 29
            if forest_field > 0.2 and noise % 17 == 0:
                obstacles[y][x] = [noise % 6, 5]
                forest_obstacles += 1
            elif forest_field < -0.4 and noise % 17 == 1:
                obstacles[y][x] = [2 + noise % 4, 4]
                rock_obstacles += 1

    tiles = []
    counts = {
        "finland_land": 0, "neighbor_land": 0, "water": 0,
        "road_tiles": 0, "rail_tiles": 0, "road_bridges": 0,
        "rail_bridges": 0,
    }
    for y in range(size):
        row = []
        for x in range(size):
            position = y * size + x
            if dry[position]:
                if finland_pixels[position]:
                    counts["finland_land"] += 1
                    base = 0
                else:
                    counts["neighbor_land"] += 1
                    base = 20
                if base == 0 and any((
                    y > 0 and not dry[position - size],
                    x + 1 < size and not dry[position + 1],
                    y + 1 < size and not dry[position + size],
                    x > 0 and not dry[position - 1],
                )):
                    base = 20
                if finland_railways[position]:
                    connections = connection_mask(
                        finland_railways, size, size, x, y)
                    tile_id = 70 + connections
                    counts["rail_tiles"] += 1
                elif finland_roads[position]:
                    connections = connection_mask(
                        finland_roads, size, size, x, y)
                    road_tile = ROAD_TILE_BY_CONNECTIONS.get(connections)
                    if road_tile is not None:
                        tile_id = base + road_tile
                        counts["road_tiles"] += 1
                    else:
                        tile_id = base
                else:
                    tile_id = base
            else:
                counts["water"] += 1
                if water_railways[position]:
                    connections = connection_mask(
                        water_railways, size, size, x, y)
                    tile_id = bridge_tile_id(
                        connections, rail=True, variant=x + y)
                    counts["rail_bridges"] += 1
                elif water_roads[position]:
                    connections = connection_mask(
                        water_roads, size, size, x, y)
                    tile_id = bridge_tile_id(
                        connections, variant=x + y)
                    counts["road_bridges"] += 1
                else:
                    neighbors = (
                        (1 if y > 0 and dry[position - size] else 0) |
                        (2 if x + 1 < size and dry[position + 1] else 0) |
                        (4 if y + 1 < size and dry[position + size] else 0) |
                        (8 if x > 0 and dry[position - 1] else 0))
                    tile_id = WATER_TILE_BASE + neighbors
            tile = [tile_id % 10, tile_id // 10]
            row.append(tile)
        tiles.append(row)
    metadata = {
        "source": "Natural Earth 1:10m country boundaries, lakes, roads, railways, and populated places (public domain)",
        "source_urls": [
            COUNTRIES_SOURCE, LAKES_SOURCE,
            *([ROADS_SOURCE] if roads is not None else []),
            *([RAILROADS_SOURCE] if railways is not None else []),
            *([POPULATED_PLACES_SOURCE]
              if populated_places is not None else []),
        ],
        "projection": "Spherical Lambert azimuthal equal-area, center 26E 64.5N",
        "earth_radius_km": 6371.0088,
        "projected_bounds_km": [left, top - span, left + span, top],
        "north_up": True,
        "kilometers_per_tile": span / max(1, size - 1),
        "legend": {"finland_land": [0, 0], "neighbor_land": [0, 2],
                   "water_tile_ids": list(range(40, 60)),
                   "bridge_tile_ids": list(range(60, 70)),
                   "rail_connection_bits": {
                       "north": 1, "east": 2, "south": 4, "west": 8,
                   },
                   "train_track_tile_ids": list(range(70, 86)),
                   "rail_bridge_tile_ids": list(range(86, 90)),
                   "shore_land_neighbor_bits": {"north": 1, "east": 2, "south": 4, "west": 8}},
        "tile_counts": counts,
        "roads_source": road_source,
        "road_features": road_feature_count,
        "railways_source": (
            RAILROADS_SOURCE if railways is not None else "built-in Finnish routes"
        ),
        "railway_features": railway_feature_count,
        "cities_source": (
            POPULATED_PLACES_SOURCE if populated_places is not None
            else "built-in Finnish city locations"
        ),
        "cities": cities,
        "forest_obstacles": forest_obstacles,
        "rock_obstacles": rock_obstacles,
    }
    document = {
        "format_version": 1, "name": "FINLAND", "width": size, "height": size,
        "tile_size": 64, "tileset": "tiles_terrain.png",
        "objects_tileset": "tiles_objects.png", "geography": metadata,
        "obstacles": obstacles, "objects": objects, "tiles": tiles,
    }
    apply_water_depth_tiles(document)
    output.parent.mkdir(parents=True, exist_ok=True)
    write_runtime_map(document, output)
    if preview is not None:
        atlas = pygame.image.load(str(ATLAS_PATH))
        objects_atlas = pygame.image.load(
            str(GAME_DIR / "gfx" / "tiles_objects.png"))
        preview_scale = max(1, min(8, 800 // size))
        scaled_tiles = {}
        scaled_objects = {}
        image = pygame.Surface(
            (size * preview_scale, size * preview_scale))
        for tile_id in range(100):
            source_tile = atlas.subsurface(
                (tile_id % 10 * TILE_SIZE, tile_id // 10 * TILE_SIZE,
                 TILE_SIZE, TILE_SIZE))
            scaled_tiles[tile_id] = pygame.transform.scale(
                source_tile, (preview_scale, preview_scale))
        for row in (*obstacles, *objects):
            for cell in row:
                if cell is None:
                    continue
                tile_x, tile_y = (
                    cell if isinstance(cell, list) else cell["tile"])
                tile_id = tile_y * 6 + tile_x
                if tile_id not in scaled_objects:
                    source_tile = objects_atlas.subsurface((
                        tile_x * TILE_SIZE, tile_y * TILE_SIZE,
                        TILE_SIZE, TILE_SIZE))
                    scaled_objects[tile_id] = pygame.transform.scale(
                        source_tile, (preview_scale, preview_scale))
        for y, row in enumerate(tiles):
            for x, (tile_x, tile_y) in enumerate(row):
                image.blit(
                    scaled_tiles[tile_y * 10 + tile_x],
                    (x * preview_scale, y * preview_scale))
                for layer in (obstacles, objects):
                    cell = layer[y][x]
                    if cell is None:
                        continue
                    tile_x, tile_y = (
                        cell if isinstance(cell, list) else cell["tile"])
                    image.blit(
                        scaled_objects[tile_y * 6 + tile_x],
                        (x * preview_scale, y * preview_scale))
        pygame.image.save(image, str(preview))
    return document


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--countries", type=Path, required=True)
    parser.add_argument("--lakes", type=Path, required=True)
    parser.add_argument("--roads", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=GAME_DIR / "disks" / "maps" / "FINLAND.json")
    parser.add_argument("--runtime-output", type=Path)
    parser.add_argument("--size", type=int, default=100)
    parser.add_argument("--railways", type=Path)
    parser.add_argument("--cities", type=Path)
    parser.add_argument("--extend-atlas", action="store_true")
    parser.add_argument("--preview", type=Path)
    arguments = parser.parse_args()
    if arguments.extend_atlas:
        extend_atlas()
    document = generate_map(arguments.countries, arguments.lakes,
                            arguments.output, arguments.size, arguments.preview,
                            arguments.roads, arguments.railways,
                            arguments.cities)
    runtime_output = arguments.runtime_output
    if runtime_output is not None:
        runtime_output.parent.mkdir(parents=True, exist_ok=True)
        runtime_output.write_bytes(arguments.output.read_bytes())
    print(json.dumps({"path": str(arguments.output),
                      "runtime_path": (
                          str(runtime_output) if runtime_output is not None
                          else None),
                      "dimensions": [document["width"], document["height"]],
                      "counts": document["geography"]["tile_counts"]}))


if __name__ == "__main__":
    main()
