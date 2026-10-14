# Tank-16 TODO

## Core game
- [ ] Use `main.c` as the game's main file. Game is programmed in SC-16's C-language.
- [ ] Build a two-player, top-down 2D tank battle for a 640x480 screen.
- [ ] Keep both tanks inside the screen boundaries.
- [ ] Implement Player 1 controls: A/D to turn, W/S to move forward/backward, and C to fire.
- [ ] Implement Player 2 controls: J/L to turn, I/K to move forward/backward, and . (period) to fire.
- [ ] Start Player 1 on the left side of the map and Player 2 on the right.
- [ ] Let players drive, avoid obstacles, and hit the opposing tank with projectiles.
- [ ] Give each player one life and 100 health points; update health when they take damage.
- [ ] Reduce health by 10 points for an ammunition hit, 25 points for a mine, and 5 points for a tank collision.
- [ ] End a round when a player's health reaches zero.

## Graphics and map
- [ ] Use `/Tank-16/gfx/tiles_terrain.png` for the terrain, scaling its 64x64 source tiles to 16x16 pixels.
- [ ] Inspect `/Tank-16/gfx/tiles_objects.png` and document the available sprites, their dimensions, and positions in the image.
- [ ] Create an external `sprite_viewer.py` tool to inspect available sprites and show their IDs, if IDs are used, and coordinates for hard-coding selected sprites into the game.
- [ ] Create a screen-sized map using terrain tiles and place suitable objects from the object graphics.
- [ ] Precalculate tank and projectile rotations to keep turning and firing animations smooth.

## HUD and match flow
- [ ] Show Player 1's health at the top left and Player 2's health at the top right; update both in real time.
- [ ] Show the game timer at the top center, with the players' win count (for example, `1-2`) next to it.
- [ ] Limit each match to five minutes. When time expires, determine the winner from the remaining health.
- [ ] Treat equal health when time expires as a draw.
- [ ] Automatically start a new round after each round ends and preserve the win count between rounds.
- [ ] Reset the win count when the game is restarted.

## Optional enhancements
- [ ] Later enhancement: add destructible and indestructible obstacles.
- [ ] Later enhancement: add timed power-ups, such as increased speed, stronger shots, and health restoration.
- [ ] Later enhancement: add sound effects for shooting, tank movement, and explosions.
