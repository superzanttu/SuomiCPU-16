#include <stdint.h>

#ifndef MOTOROBOTS_TYPES_H
#define MOTOROBOTS_TYPES_H

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
    RES_ROMBONENT
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
    BUILDING_BASE,
    BUILDING_ROUTE
} BuildingType;

typedef enum {
    ROBOT_IDLE,
    ROBOT_TO_SOURCE,
    ROBOT_LOADING,
    ROBOT_TO_TARGET,
    ROBOT_UNLOADING
} RobotState;

typedef struct {
    BuildingType type;
    Vec2i position;
    ResourceType resource_type; // Only used for Mines
    int inventory_count;
    ResourceType inventory_type;
    float production_timer;
} Building;

typedef struct {
    Vec2i position;
    int carrying_amount;
    ResourceType carrying_type;
    int source_building;
    int target_building;
    RobotState state;
    float timer;
} Robot;

#endif
