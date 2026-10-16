import argparse
import json
import math
from pathlib import Path

import pygame


ROOT = Path(__file__).resolve().parents[1]
GAME_DIR = ROOT / "modules" / "Tank-16"
ATLAS_PATH = GAME_DIR / "gfx" / "tiles_terrain.png"
TILE_SIZE = 64
WATER_TILE_BASE = 40
SOURCE_BASE = "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/"
COUNTRIES_SOURCE = SOURCE_BASE + "ne_10m_admin_0_countries.geojson"
LAKES_SOURCE = SOURCE_BASE + "ne_10m_lakes.geojson"


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


def water_tile(neighbors: int, variant: int = 0) -> pygame.Surface:
    scale = 3
    size = TILE_SIZE * scale
    tile = pygame.Surface((size, size), pygame.SRCALPHA)
    for y in range(size):
        for x in range(size):
            px, py = x / scale, y / scale
            wave = math.sin(px * math.tau / 32 + math.sin(py * math.tau / 64))
            wave += math.sin(py * math.tau / 16 + variant) * 0.6
            noise = ((int(px) * 73 + int(py) * 151) % 11 - 5) * 0.25
            land_distances = []
            if neighbors & 1:
                land_distances.append(py)
            if neighbors & 2:
                land_distances.append(63 - px)
            if neighbors & 4:
                land_distances.append(63 - py)
            if neighbors & 8:
                land_distances.append(px)
            distance = min(land_distances, default=64)
            if distance < 2:
                color = (50, 185, 116, 255)
            elif distance < 5:
                color = (224, 203, 150, 255)
            elif distance < 7:
                color = (186, 222, 215, 255)
            else:
                shallow = max(0, 1 - distance / 22)
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


def extend_atlas(path: Path = ATLAS_PATH) -> None:
    original = pygame.image.load(str(path))
    if original.get_size() == (640, 384):
        return
    if original.get_size() != (640, 256):
        raise ValueError("Expected the original 640x256 or extended 640x384 terrain atlas")
    atlas = pygame.Surface((640, 384), pygame.SRCALPHA)
    atlas.blit(original, (0, 0))
    for index in range(20):
        tile_id = WATER_TILE_BASE + index
        tile = water_tile(index if index < 16 else 0, max(0, index - 15))
        atlas.blit(tile, ((tile_id % 10) * TILE_SIZE, (tile_id // 10) * TILE_SIZE))
    pygame.image.save(atlas, str(path))


def generate_map(countries_path: Path, lakes_path: Path,
                 output: Path, size: int = 1000,
                 preview: Path | None = None) -> dict:
    if not 1 <= size <= 1024:
        raise ValueError("Map size must be between 1 and 1024 tiles")
    countries = json.loads(countries_path.read_text(encoding="utf-8"))
    lakes = json.loads(lakes_path.read_text(encoding="utf-8"))
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
    land_pixels = pygame.image.tobytes(land, "P")
    finland_pixels = pygame.image.tobytes(national, "P")
    lake_pixels = pygame.image.tobytes(water, "P")
    dry = bytes(1 if ground and not lake else 0
                for ground, lake in zip(land_pixels, lake_pixels))
    tiles = []
    counts = {"finland_land": 0, "neighbor_land": 0, "water": 0}
    for y in range(size):
        row = []
        for x in range(size):
            position = y * size + x
            if dry[position]:
                if finland_pixels[position]:
                    tile = [0, 0]
                    counts["finland_land"] += 1
                else:
                    tile = [0, 2]
                    counts["neighbor_land"] += 1
            else:
                neighbors = (
                    (1 if y > 0 and dry[position - size] else 0) |
                    (2 if x + 1 < size and dry[position + 1] else 0) |
                    (4 if y + 1 < size and dry[position + size] else 0) |
                    (8 if x > 0 and dry[position - 1] else 0))
                tile_id = WATER_TILE_BASE + neighbors
                tile = [tile_id % 10, tile_id // 10]
                counts["water"] += 1
            row.append(tile)
        tiles.append(row)
    metadata = {
        "source": "Natural Earth 1:10m country boundaries and lakes (public domain)",
        "source_urls": [COUNTRIES_SOURCE, LAKES_SOURCE],
        "projection": "Spherical Lambert azimuthal equal-area, center 26E 64.5N",
        "earth_radius_km": 6371.0088,
        "projected_bounds_km": [left, top - span, left + span, top],
        "north_up": True,
        "kilometers_per_tile": span / max(1, size - 1),
        "legend": {"finland_land": [0, 0], "neighbor_land": [0, 2],
                   "water_tile_ids": list(range(40, 56)),
                   "shore_land_neighbor_bits": {"north": 1, "east": 2, "south": 4, "west": 8}},
        "tile_counts": counts,
    }
    document = {
        "format_version": 1, "name": "SUOMI", "width": size, "height": size,
        "tile_size": 64, "tileset": "tiles_terrain.png",
        "objects_tileset": "tiles_objects.png", "geography": metadata,
        "tiles": tiles,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", encoding="utf-8", newline="\n") as handle:
        handle.write("{\n")
        for name, value in document.items():
            if name == "tiles":
                continue
            handle.write("  " + json.dumps(name) + ": " + json.dumps(value) + ",\n")
        handle.write('  "tiles": [\n')
        for index, row in enumerate(tiles):
            handle.write("    " + json.dumps(row, separators=(",", ":")) +
                         (",\n" if index + 1 < size else "\n"))
        handle.write("  ]\n}\n")
    if preview is not None:
        image = pygame.Surface((size, size))
        colors = {"finland_land": (50, 188, 117), "neighbor_land": (224, 203, 150),
                  "water": (36, 105, 157)}
        pixels = pygame.PixelArray(image)
        for y in range(size):
            for x in range(size):
                position = y * size + x
                kind = ("finland_land" if finland_pixels[position] else "neighbor_land") if dry[position] else "water"
                pixels[x, y] = colors[kind]
        pixels.close()
        pygame.image.save(image, str(preview))
    return document


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--countries", type=Path, required=True)
    parser.add_argument("--lakes", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=GAME_DIR / "maps" / "suomi.json")
    parser.add_argument("--size", type=int, default=1000)
    parser.add_argument("--extend-atlas", action="store_true")
    parser.add_argument("--preview", type=Path)
    arguments = parser.parse_args()
    if arguments.extend_atlas:
        extend_atlas()
    document = generate_map(arguments.countries, arguments.lakes,
                            arguments.output, arguments.size, arguments.preview)
    print(json.dumps({"path": str(arguments.output),
                      "dimensions": [document["width"], document["height"]],
                      "counts": document["geography"]["tile_counts"]}))


if __name__ == "__main__":
    main()
