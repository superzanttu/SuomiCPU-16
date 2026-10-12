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

#define ROBOT_PATH_NONE 0
#define ROBOT_PATH_QUEUED 1
#define ROBOT_PATH_SEARCHING 2
#define ROBOT_PATH_READY 3
#define ROBOT_PATH_FAILED 4
#define FACTORY_STATUS_EMPTY 1
#define FACTORY_STATUS_FULL 2

typedef struct {
    int id;
    BuildingType type;
    Vec2i position;
    ResourceType resource_type; // Only used for Mines
    int inventory_count;
    ResourceType inventory_type;
    int factory_iron_plate_input;
    int factory_copper_plate_input;
    int factory_robot_rombonent_input;
    int factory_status;
    float production_timer;
    int processing_batch;
} Building;

typedef struct {
    Vec2i position;
    int carrying_amount;
    ResourceType carrying_type;
    int source_building;
    int target_building;
    RobotState state;
    float timer;
    int path_length;
    int path_position;
    int path_status;
    int route_valid;
    int route_direction;
    float path_retry_timer;
} Robot;

#endif
