#ifndef MOTOROBOTS_GAME_H
#define MOTOROBOTS_GAME_H

#define MAX_BUILDINGS 100
#define MAX_ROBOTS 8
#define TILE_PIXELS 16
#define MAP_VIEW_TOP 36
#define MAP_VIEW_BOTTOM 202

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
    int zoom;
    unsigned int pan_keys_was_down;
    float pan_repeat_timer;
    BuildingType build_tool;
    int selected_building;
    int tab_was_down;
    int mouse_last_x;
    int mouse_last_y;
    int right_dragging;
    int right_drag_moved;
    int route_source;
    int path_queue[MAP_SIZE * MAP_SIZE];
    unsigned char path_visited[(MAP_SIZE * MAP_SIZE + 7) / 8];
} GameState;

void game_init(GameState* state);
void game_update(GameState* state, float delta_time);
void game_draw(GameState* state);

#endif
