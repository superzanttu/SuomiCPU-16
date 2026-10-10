#include "../include/map.h"
#include <stdlib.h>

void map_init(GameMap* map) {
    map->width = 10 * CHUNK_SIZE;
    map->height = 10 * CHUNK_SIZE;
    
    for (int cx = 0; cx < 10; cx++) {
        for (int cy = 0; cy < 10; cy++) {
            for (int x = 0; x < CHUNK_SIZE; x++) {
                for (int y = 0; y < CHUNK_SIZE; y++) {
                    // Simple random generation for prototype
                    int r = rand() % 10;
                    if (r < 2) map->chunks[cx][cy].tiles[x][y] = TILE_WATER;
                    else if (r < 4) map->chunks[cx][cy].tiles[x][y] = TILE_ROCK;
                    else map->chunks[cx][cy].tiles[x][y] = TILE_GROUND;
                }
            }
        }
    }
}

TileType map_get_tile(GameMap* map, int x, int y) {
    if (x < 0 || x >= map->width || y < 0 || y >= map->height) return TILE_ROCK;
    return map->chunks[x / CHUNK_SIZE][y / CHUNK_SIZE].tiles[x % CHUNK_SIZE][y % CHUNK_SIZE];
}
