#include "game.c"
#include "map.c"

GameState state;

int main() {
    game_init(&state);
    
    while (1) {
        float dt;
        dt = 0.016f;
        game_update(&state, dt);
        game_draw(&state);
        gfx_present();
    }
    
    return 0;
}
