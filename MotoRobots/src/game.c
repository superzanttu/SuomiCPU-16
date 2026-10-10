#include "game.h"
#include <string.h>

void game_init(GameState* state) {
    memset(state, 0, sizeof(GameState));
    map_init(&state->map);
    
    // Start with a base
    state->buildings[state->building_count++] = (Building){
        .type = BUILDING_BASE,
        .position = {0, 0},
        .inventory_type = RES_NONE,
        .inventory_count = 0
    };
}

void game_update(GameState* state, float delta_time) {
    // Update logic will go here (Robot movement, production, etc.)
}
