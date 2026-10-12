#include "map.h"
#include "../../lib/suomi_gfx.h"

static int absolute(int value) {
    return value < 0 ? -value : value;
}

static void add_feature(GameMap* map, int x, int y, int radius, int seed,
                        TileType tile, ResourceType resource) {
    MapFeature* feature;
    if (map->feature_count >= MAP_MAX_FEATURES) return;
    feature = &map->features[map->feature_count++];
    feature->center_x = x;
    feature->center_y = y;
    feature->radius = radius;
    feature->seed = seed;
    feature->tile = tile;
    feature->resource = resource;
}

static int feature_row(MapFeature* feature, int y, int* center, int* radius) {
    int offset;
    int row;
    int spread;

    offset = y - feature->center_y;
    if (offset < -feature->radius || offset > feature->radius) return 0;
    row = absolute(offset);
    spread = feature->radius - (row * 2) / 3;
    if (spread < 1) spread = 1;
    *center = feature->center_x + ((offset + feature->seed) & 1) - 1;
    *radius = spread;
    return 1;
}

static int in_feature(MapFeature* feature, int x, int y) {
    int center;
    int radius;
    if (!feature_row(feature, y, &center, &radius)) return 0;
    return x >= center - radius && x <= center + radius;
}

void map_init(GameMap* map) {
    int y;
    int river_x;
    int river_change_y;
    int river_segment;
    int variation;

    map->width = MAP_SIZE;
    map->height = MAP_SIZE;
    map->feature_count = 0;
    variation = (int)(gfx_random() & 1);
    add_feature(map, 10 + variation, 9, 5, 1 + variation,
                TILE_ROCK, RES_NONE);
    add_feature(map, 37 - variation, 38, 5, 2 + variation,
                TILE_ROCK, RES_NONE);
    add_feature(map, 39, 10, 4, variation, TILE_WATER, RES_NONE);
    add_feature(map, 15 + variation, 19, 3, variation,
                TILE_GROUND, RES_COAL);
    add_feature(map, 33 - variation, 17, 3, 1 + variation,
                TILE_GROUND, RES_IRON_ORE);
    add_feature(map, 28 + variation, 36, 3, 2 + variation,
                TILE_GROUND, RES_COPPER_ORE);

    river_x = 5 + variation;
    river_change_y = 11;
    river_segment = 0;
    for (y = 0; y < MAP_SIZE; y++) {
        if (y == river_change_y) {
            if ((river_segment & 2) == 0) river_x++;
            else river_x--;
            if (river_x < 4) river_x = 4;
            if (river_x > 9) river_x = 9;
            river_segment++;
            river_change_y += 6;
        }
        map->river_x[y] = (unsigned char)river_x;
    }
}

TileType map_get_tile(GameMap* map, int x, int y) {
    int i;
    MapFeature* feature;

    if (x < 0 || y < 0 || x >= map->width || y >= map->height)
        return TILE_ROCK;
    if (x >= 22 && x <= 26 && y >= 22 && y <= 26) return TILE_GROUND;
    if (y >= 5 && y < 43) {
        i = (int)map->river_x[y];
        if (absolute(x - i) <= 1) return TILE_WATER;
    }
    for (i = 0; i < map->feature_count; i++) {
        feature = &map->features[i];
        if (feature->resource == RES_NONE &&
            in_feature(feature, x, y)) return feature->tile;
    }
    return TILE_GROUND;
}

ResourceType map_get_resource(GameMap* map, int x, int y) {
    int i;
    MapFeature* feature;

    if (map_get_tile(map, x, y) != TILE_GROUND) return RES_NONE;
    for (i = 0; i < map->feature_count; i++) {
        feature = &map->features[i];
        if (feature->resource != RES_NONE &&
            in_feature(feature, x, y)) return feature->resource;
    }
    return RES_NONE;
}

static unsigned char feature_color(MapFeature* feature) {
    if (feature->resource == RES_COAL) return DARK_GRAY;
    if (feature->resource == RES_IRON_ORE) return ORANGE;
    if (feature->resource == RES_COPPER_ORE) return CYAN;
    if (feature->tile == TILE_WATER) return BLUE;
    return DARK_GRAY;
}

void map_draw(GameMap* map, int camera_x, int camera_y, int tile_size,
              int view_y, int view_height) {
    int first_y;
    int last_y;
    int map_y;
    int river_y;
    int river_start;
    int river_end;
    int screen_y;
    int center;
    int radius;
    int i;
    MapFeature* feature;

    gfx_rect(0, view_y, SCREEN_W, view_height, GREEN);
    first_y = camera_y / tile_size;
    last_y = (camera_y + view_height) / tile_size + 1;
    if (last_y > map->height) last_y = map->height;
    for (river_y = 5; river_y < 43; river_y += 6) {
        river_start = river_y;
        river_end = river_y + 6;
        if (river_start < first_y) river_start = first_y;
        if (river_end > 43) river_end = 43;
        if (river_end > last_y) river_end = last_y;
        if (river_end <= river_start) continue;
        center = (int)map->river_x[(river_start + river_end) / 2];
        gfx_rect((center - 1) * tile_size - camera_x,
                 view_y + river_start * tile_size - camera_y,
                 3 * tile_size, (river_end - river_start) * tile_size, BLUE);
    }
    for (map_y = first_y; map_y < last_y; map_y++) {
        screen_y = view_y + map_y * tile_size - camera_y;
        for (i = 0; i < map->feature_count; i++) {
            feature = &map->features[i];
            if (feature_row(feature, map_y, &center, &radius)) {
                gfx_rect((center - radius) * tile_size - camera_x,
                         screen_y, (radius * 2 + 1) * tile_size,
                         tile_size, feature_color(feature));
            }
        }
    }
}

void map_draw_minimap(GameMap* map, int screen_x, int screen_y, int size) {
    int map_y;
    int map_x;
    int river_map_x;
    int i;
    MapFeature* feature;

    gfx_rect(screen_x, screen_y, size, size, GREEN);
    river_map_x = (int)map->river_x[24] * size / MAP_SIZE;
    gfx_rect(screen_x + river_map_x - 1, screen_y + 3, 3, size - 6, BLUE);
    for (i = 0; i < map->feature_count; i++) {
        feature = &map->features[i];
        map_x = feature->center_x * size / MAP_SIZE;
        map_y = feature->center_y * size / MAP_SIZE;
        gfx_rect(screen_x + map_x - 2, screen_y + map_y - 2,
                 5, 5, feature_color(feature));
    }
}
