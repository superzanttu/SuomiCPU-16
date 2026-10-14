# Tank-16 TODO List

## Completed
### Gameplay
- [x] Implement the game in SC-16 C in `main.c` for a 640×480 display.
- [x] Support two players in a top-down tank battle; start Player 1 on the left and Player 2 on the right.
- [x] Keep tanks within screen bounds and let players drive, avoid obstacles, and fire projectiles.
- [x] Map controls: Player 1 uses A/D to turn, W/S to move, and C to fire; Player 2 uses J/L to turn, I/K to move, and period to fire.
- [x] Give each player one life and 100 health; apply 10 damage for an ammunition hit, 25 for a mine, and 5 for a tank collision.
- [x] End a round when a player's health reaches zero.
- [x] Build the screen-sized map with 16×16 terrain tiles generated from `gfx/tiles_terrain.png`.
- [x] Inspect `gfx/tiles_objects.png` and use the tank sprite at atlas coordinates (124, 68), source size 48×56.
- [x] Provide `sprite_viewer.py` to inspect sprite IDs, atlas coordinates, and selected-region dimensions.
- [x] Precalculate 32 tank sprite rotations and projectile directions.
- [x] Show both players' health and the game timer and win count in the HUD.
- [x] Limit each round to five minutes; decide the winner by remaining health and declare a draw on equal health.
- [x] Start a new round automatically after each round and retain wins; reset the win count when the game restarts.

## Remaining tasks
- [ ] Use `sprite_viewer.py` to choose mine and obstacle sprites from the object atlas.
- [ ] Define the chosen sprite coordinates in the game code and use those sprites instead of the current simple mine and obstacle drawings.
- [ ] Add destructible and indestructible obstacles.
- [ ] Add timed power-ups, such as increased speed, stronger shots, and health restoration.
- [ ] Add sound effects for shooting, tank movement, and explosions.

## Questions for the user
Q: Which object-atlas sprites should be used for mines and obstacles?
A: No sprite choices have been specified yet. Choose them with `sprite_viewer.py`; then record their atlas coordinates in the game code.
