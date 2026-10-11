#ifndef MOTOROBOTS_GAME_H
#define MOTOROBOTS_GAME_H

#define MAX_BUILDINGS 100
#define MAX_ROBOTS 50
#define TILE_PIXELS 16
#define MAP_VIEW_TOP 32
#define MAP_VIEW_BOTTOM 220

#include "types.h"
#include "map.h"

typedef struct {
    GameMap map;
    Building buildings[MAX_BUILDINGS];
    int building_count;
    Robot robots[MAX_ROBOTS];
    int robot_count;

    int total_resources[7];

    int camera_x;
    int camera_y;
    BuildingType build_tool;
    int selected_building;
    int tab_was_down;
    char* notification;
    float notification_time;
} GameState;

void game_init(GameState* state);
void game_update(GameState* state, float delta_time);
void game_draw(GameState* state);

#endif
