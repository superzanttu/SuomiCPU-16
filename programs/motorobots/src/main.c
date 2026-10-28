#include "game.c"
#include "map.c"

GameState state;

int main() {
    gfx_mode(GFX_MODE_640_480);
    gfx_speed(SPEED_MAX);
    gfx_vsync(1);
    gfx_layer_reset(1 << LAYER_BACKGROUND);
    game_init(&state);
    
    while (1) {
        int dt;
        dt = GAME_TIME_UNITS_PER_SECOND / 30;
        game_update(&state, dt);
        game_draw(&state);
        gfx_present();
    }
    
    return 0;
}
