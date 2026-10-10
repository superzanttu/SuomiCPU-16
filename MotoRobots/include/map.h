#include "types.h"

#define CHUNK_SIZE 16

typedef struct {
    TileType tiles[CHUNK_SIZE][CHUNK_SIZE];
} Chunk;

typedef struct {
    // Simplified map for prototype: fixed size or dynamic chunk management
    // For now, we'll use a limited number of chunks
    Chunk chunks[10][10]; 
    int width;
    int height;
} GameMap;

void map_init(GameMap* map);
TileType map_get_tile(GameMap* map, int x, int y);
