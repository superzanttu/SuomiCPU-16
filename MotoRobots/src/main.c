#include "../include/game.h"
#include <stdlib.h>

int main() {
    GameState state;
    game_init(&state);
    
    // We can't use printf easily in the SuomiCPU-16 environment 
    // if stdio.h is missing. The SuomiCPU-16 runtime likely handles 
    // output via its own API or a specific memory region.
    // For now, we'll just return 0 to ensure it compiles.
    
    return 0;
}
