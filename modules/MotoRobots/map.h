#ifndef MOTOROBOTS_MAP_H
#define MOTOROBOTS_MAP_H

#include "types.h"

#define CHUNK_SIZE 16
#define MAP_CHUNKS 10

typedef struct {
    TileType tiles[CHUNK_SIZE][CHUNK_SIZE];
    ResourceType resources[CHUNK_SIZE][CHUNK_SIZE];
} Chunk;

typedef struct {
    Chunk chunks[MAP_CHUNKS][MAP_CHUNKS];
    int width;
    int height;
} GameMap;

void map_init(GameMap* map);
TileType map_get_tile(GameMap* map, int x, int y);
ResourceType map_get_resource(GameMap* map, int x, int y);

#endif
