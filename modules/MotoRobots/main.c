#include "game.c"
#include "map.c"

GameState state;

int main() {
    game_init(&state);
    gfx_speed(1);
    gfx_vsync(1);
    
    while (1) {
        float dt;
        dt = 0.03333f;
        game_update(&state, dt);
        game_draw(&state);
        gfx_present();
    }
    
    return 0;
}
