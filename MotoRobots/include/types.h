#ifndef MOTOROBOTS_TYPES_H
#define MOTOROBOTS_TYPES_H

#include <stdint.h>

typedef enum {
    TILE_GROUND,
    TILE_ROCK,
    TILE_WATER
} TileType;

typedef enum {
    RES_NONE,
    RES_COAL,
    RES_IRON_ORE,
    RES_COPPER_ORE,
    RES_IRON_PLATE,
    RES_COPPER_PLATE,
    RES_STEEL
} ResourceType;

typedef struct {
    int x;
    int y;
} Vec2i;

typedef struct {
    float x;
    float y;
} Vec2f;

typedef enum {
    BUILDING_NONE,
    BUILDING_MINE,
    BUILDING_FACTORY,
    BUILDING_STORAGE,
    BUILDING_BASE
} BuildingType;

typedef struct {
    BuildingType type;
    Vec2i position;
    ResourceType resource_type; // Only used for Mines
    int inventory_count;
    ResourceType inventory_type;
} Building;

typedef struct {
    int id;
    Vec2f position;
    int carrying_amount;
    ResourceType carrying_type;
    Vec2i source_pos;
    Vec2i target_pos;
    int is_active; // Using int instead of bool
} Robot;

#endif // MOTOROBOTS_TYPES_H
