#include "../include/game.h"
#include "../lib/suomi_gfx.h"
#include <string.h>

void game_init(GameState* state) {
    memset(state, 0, sizeof(GameState));
    map_init(&state->map);
    
    state->camera_x = 0;
    state->camera_y = 0;
    state->zoom = 1;

    // Start with a base
    state->buildings[state->building_count++] = (Building){
        .type = BUILDING_BASE,
        .position = {0, 0},
        .inventory_type = RES_NONE,
        .inventory_count = 0
    };
}

void game_update(GameState* state, float delta_time) {
    // Handle Input
    unsigned int keys = gfx_keys();
    if (keys & KEY_LEFT) state->camera_x -= 2;
    if (keys & KEY_RIGHT) state->camera_x += 2;
    if (keys & KEY_UP) state->camera_y -= 2;
    if (keys & KEY_DOWN) state->camera_y += 2;
}

void game_draw(GameState* state) {
    gfx_clear(BLACK);

    // Draw Map (Simplified)
    // We use gfx_rect to draw tiles for now as gfx_tilemap requires a specific buffer
    for (int x = 0; x < 20; x++) {
        for (int y = 0; y < 20; y++) {
            TileType t = map_get_tile(&state->map, x + (state->camera_x / 16), y + (state->camera_y / 16));
            unsigned char color = GRAY;
            if (t == TILE_WATER) color = BLUE;
            else if (t == TILE_ROCK) color = DARK_GRAY;
            else color = GREEN;
            
            gfx_rect(x * 16 - (state->camera_x % 16), y * 16 - (state->camera_y % 16), 15, 15, color);
        }
    }

    // Draw Buildings
    for (int i = 0; i < state->building_count; i++) {
        Building* b = &state->buildings[i];
        unsigned char color = WHITE;
        if (b->type == BUILDING_BASE) color = YELLOW;
        else if (b->type == BUILDING_MINE) color = RED;
        else if (b->type == BUILDING_FACTORY) color = BLUE;
        else if (b->type == BUILDING_STORAGE) color = CYAN;

        gfx_rect(b->position.x * 16 - (state->camera_x % 16), b->position.y * 16 - (state->camera_y % 16), 15, 15, color);
    }

    // Draw HUD
    gfx_text(10, 10, "MotoRobots Prototype", WHITE);
    gfx_text(10, 25, "Use WASD to move camera", WHITE);
}
