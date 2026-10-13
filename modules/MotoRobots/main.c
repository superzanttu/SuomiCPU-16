#include "game.c"
#include "map.c"

GameState state;

int main() {
    gfx_speed(SPEED_MAX);
    gfx_vsync(1);
    game_init(&state);
    
    while (1) {
        float dt;
        dt = 0.03333f;
        game_update(&state, dt);
        game_draw(&state);
        gfx_present();
    }
    
    return 0;
}
