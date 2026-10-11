#include "map.h"
#include "../../lib/suomi_gfx.h"

#define RESOURCE_PATCHES 8
#define TERRAIN_PATCHES 24
#define MAP_BASE_X 80
#define MAP_BASE_Y 80

static void map_paint_patch(GameMap* map, int center_x, int center_y,
                            int radius, TileType tile, ResourceType resource) {
    int x;
    int y;
    int dx;
    int dy;

    for (x = center_x - radius; x <= center_x + radius; x++) {
        for (y = center_y - radius; y <= center_y + radius; y++) {
            dx = x - center_x;
            dy = y - center_y;
            if (dx * dx + dy * dy > radius * radius) continue;
            if (x < 0 || x >= map->width || y < 0 || y >= map->height) continue;
            if (tile != TILE_GROUND) {
                map->chunks[x / CHUNK_SIZE][y / CHUNK_SIZE]
                    .tiles[x % CHUNK_SIZE][y % CHUNK_SIZE] = tile;
                map->chunks[x / CHUNK_SIZE][y / CHUNK_SIZE]
                    .resources[x % CHUNK_SIZE][y % CHUNK_SIZE] = RES_NONE;
            } else if (map->chunks[x / CHUNK_SIZE][y / CHUNK_SIZE]
                           .tiles[x % CHUNK_SIZE][y % CHUNK_SIZE] == TILE_GROUND) {
                map->chunks[x / CHUNK_SIZE][y / CHUNK_SIZE]
                    .resources[x % CHUNK_SIZE][y % CHUNK_SIZE] = resource;
            }
        }
    }
}

void map_init(GameMap* map) {
    int cx;
    int cy;
    int x;
    int y;
    int i;
    int center_x;
    int center_y;
    int radius;
    ResourceType resource;

    map->width = MAP_CHUNKS * CHUNK_SIZE;
    map->height = MAP_CHUNKS * CHUNK_SIZE;

    for (cx = 0; cx < MAP_CHUNKS; cx++) {
        for (cy = 0; cy < MAP_CHUNKS; cy++) {
            for (x = 0; x < CHUNK_SIZE; x++) {
                for (y = 0; y < CHUNK_SIZE; y++) {
                    map->chunks[cx][cy].tiles[x][y] = TILE_GROUND;
                    map->chunks[cx][cy].resources[x][y] = RES_NONE;
                }
            }
        }
    }

    for (i = 0; i < TERRAIN_PATCHES; i++) {
        center_x = gfx_random() % map->width;
        center_y = gfx_random() % map->height;
        radius = 2 + gfx_random() % 5;
        map_paint_patch(map, center_x, center_y, radius,
                        (gfx_random() & 1) ? TILE_ROCK : TILE_WATER, RES_NONE);
    }

    for (i = 0; i < RESOURCE_PATCHES * 3; i++) {
        center_x = gfx_random() % map->width;
        center_y = gfx_random() % map->height;
        radius = 2 + gfx_random() % 3;
        if (i < RESOURCE_PATCHES) resource = RES_COAL;
        else if (i < RESOURCE_PATCHES * 2) resource = RES_IRON_ORE;
        else resource = RES_COPPER_ORE;
        map_paint_patch(map, center_x, center_y, radius, TILE_GROUND, resource);
    }

    for (x = MAP_BASE_X - 4; x <= MAP_BASE_X + 4; x++) {
        for (y = MAP_BASE_Y - 4; y <= MAP_BASE_Y + 4; y++) {
            map->chunks[x / CHUNK_SIZE][y / CHUNK_SIZE]
                .tiles[x % CHUNK_SIZE][y % CHUNK_SIZE] = TILE_GROUND;
            map->chunks[x / CHUNK_SIZE][y / CHUNK_SIZE]
                .resources[x % CHUNK_SIZE][y % CHUNK_SIZE] = RES_NONE;
        }
    }
}

TileType map_get_tile(GameMap* map, int x, int y) {
    if (x < 0 || x >= map->width || y < 0 || y >= map->height) return TILE_ROCK;
    return map->chunks[x / CHUNK_SIZE][y / CHUNK_SIZE]
        .tiles[x % CHUNK_SIZE][y % CHUNK_SIZE];
}

ResourceType map_get_resource(GameMap* map, int x, int y) {
    if (x < 0 || x >= map->width || y < 0 || y >= map->height) return RES_NONE;
    return map->chunks[x / CHUNK_SIZE][y / CHUNK_SIZE]
        .resources[x % CHUNK_SIZE][y % CHUNK_SIZE];
}
