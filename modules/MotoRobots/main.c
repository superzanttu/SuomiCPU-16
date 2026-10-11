#include "../include/game.h"
#include "../lib/suomi_gfx.h"
#include <stdlib.h>

int main() {
    GameState state;
    game_init(&state);
    
    while (1) {
        // Basic game loop
        float dt = 0.016f; // Assume ~60fps for prototype
        game_update(&state, dt);
        game_draw(&state);
        gfx_present();
    }
    
    return 0;
}
