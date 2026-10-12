#ifndef MOTOROBOTS_MAP_H
#define MOTOROBOTS_MAP_H

#include "types.h"

#define MAP_SIZE 48
#define MAP_MAX_FEATURES 8

typedef struct {
    int center_x;
    int center_y;
    int radius;
    int seed;
    TileType tile;
    ResourceType resource;
} MapFeature;

typedef struct {
    MapFeature features[MAP_MAX_FEATURES];
    int feature_count;
    unsigned char river_x[MAP_SIZE];
    int width;
    int height;
} GameMap;

void map_init(GameMap* map);
TileType map_get_tile(GameMap* map, int x, int y);
ResourceType map_get_resource(GameMap* map, int x, int y);
void map_draw(GameMap* map, int camera_x, int camera_y, int tile_size,
              int view_y, int view_height);
void map_draw_minimap(GameMap* map, int screen_x, int screen_y, int size);

#endif
