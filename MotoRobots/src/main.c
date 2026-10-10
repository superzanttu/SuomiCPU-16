#include <stdio.h>
#include "game.h"

int main() {
    GameState state;
    game_init(&state);
    
    printf("MotoRobots Prototype Initialized!\n");
    printf("Map size: %dx%d\n", state.map.width, state.map.height);
    
    return 0;
}
