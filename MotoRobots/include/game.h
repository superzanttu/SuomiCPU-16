#ifndef MOTOROBOTS_GAME_H
#define MOTOROBOTS_GAME_H

#include "types.h"
#include "map.h"

#define MAX_BUILDINGS 100
#define MAX_ROBOTS 50

typedef struct {
    GameMap map;
    Building buildings[MAX_BUILDINGS];
    int building_count;
    Robot robots[MAX_ROBOTS];
    int robot_count;
    
    int total_resources[7]; // Indexed by ResourceType
} GameState;

void game_init(GameState* state);
void game_update(GameState* state, float delta_time);

#endif // MOTOROBOTS_GAME_H
