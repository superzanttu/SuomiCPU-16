#ifndef MOTOROBOTS_GAME_H
#define MOTOROBOTS_GAME_H

#define MAX_BUILDINGS 100
#define MAX_ROBOTS 8
#define TILE_PIXELS 32
#define MAP_VIEW_TOP 72
#define MAP_VIEW_BOTTOM 404

#include "map.h"
#include "types.h"

typedef struct {
    GameMap map;
    Building buildings[MAX_BUILDINGS];
    int building_count;
    int next_building_id;
    Robot robots[MAX_ROBOTS];
    int robot_count;

    int total_resources[7];

    int camera_x;
    int camera_y;
    int zoom;
    int map_layer_camera_x;
    int map_layer_camera_y;
    int map_layer_zoom;
    int map_layer_building_count;
    int map_layer_selected_building;
    BuildingType map_layer_build_tool;
    int map_layer_route_source;
    int map_layer_toolbar_pressed;
    int map_layer_toolbar_hovered;
    int map_layer_valid;
    unsigned int pan_keys_was_down;
    float pan_repeat_timer;
    BuildingType build_tool;
    int toolbar_pressed;
    int toolbar_hovered;
    int mouse_left_was_down;
    int mouse_right_was_down;
    int selected_building;
    int tab_was_down;
    int mouse_last_x;
    int mouse_last_y;
    int right_dragging;
    int right_drag_moved;
    int route_source;
    int path_queue[MAP_SIZE * MAP_SIZE];
    unsigned char path_visited[(MAP_SIZE * MAP_SIZE + 7) / 8];
    unsigned char path_parent_direction[MAP_SIZE * MAP_SIZE];
    unsigned char robot_paths[MAX_ROBOTS][MAP_SIZE * MAP_SIZE];
    int pathfinding_robot;
    int path_queue_head;
    int path_queue_tail;
} GameState;

void game_init(GameState* state);
void game_update(GameState* state, float delta_time);
void game_draw(GameState* state);

#endif
