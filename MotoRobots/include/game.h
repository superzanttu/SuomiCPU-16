#define MAX_BUILDINGS 100
#define MAX_ROBOTS 50

#include "types.h"
#include "map.h"

typedef struct {
    GameMap map;
    Building buildings[MAX_BUILDINGS];
    int building_count;
    Robot robots[MAX_ROBOTS];
    int robot_count;
    
    int total_resources[7]; // Indexed by ResourceType
    
    int camera_x;
    int camera_y;
    int zoom;
} GameState;

void game_init(GameState* state);
void game_update(GameState* state, float delta_time);
void game_draw(GameState* state);
