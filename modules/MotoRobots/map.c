#include "map.h"
#include "../../lib/suomi_gfx.h"

#define RESOURCE_PATCHES 2
#define TERRAIN_PATCHES 8
#define MAP_BASE_X 24
#define MAP_BASE_Y 24
#define CELL_TILE_MASK 3
#define CELL_RESOURCE_SHIFT 2
#define CELL_RESOURCE_MASK 28

static unsigned char cell_tile(unsigned char cell) {
    return cell & CELL_TILE_MASK;
}

static unsigned char cell_resource(unsigned char cell) {
    return (cell & CELL_RESOURCE_MASK) >> CELL_RESOURCE_SHIFT;
}

static void set_cell(GameMap* map, int x, int y, TileType tile,
                     ResourceType resource) {
    map->chunks[x / CHUNK_SIZE][y / CHUNK_SIZE]
        .cells[x % CHUNK_SIZE][y % CHUNK_SIZE] =
        (unsigned char)(tile | (resource << CELL_RESOURCE_SHIFT));
}

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
                set_cell(map, x, y, tile, RES_NONE);
            } else if (map_get_tile(map, x, y) == TILE_GROUND) {
                set_cell(map, x, y, TILE_GROUND, resource);
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
                    map->chunks[cx][cy].cells[x][y] = TILE_GROUND;
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
                .cells[x % CHUNK_SIZE][y % CHUNK_SIZE] = TILE_GROUND;
        }
    }
}

TileType map_get_tile(GameMap* map, int x, int y) {
    if (x < 0 || x >= map->width || y < 0 || y >= map->height) return TILE_ROCK;
    return (TileType)cell_tile(map->chunks[x / CHUNK_SIZE][y / CHUNK_SIZE]
        .cells[x % CHUNK_SIZE][y % CHUNK_SIZE]);
}

ResourceType map_get_resource(GameMap* map, int x, int y) {
    if (x < 0 || x >= map->width || y < 0 || y >= map->height) return RES_NONE;
    return (ResourceType)cell_resource(map->chunks[x / CHUNK_SIZE][y / CHUNK_SIZE]
        .cells[x % CHUNK_SIZE][y % CHUNK_SIZE]);
}
