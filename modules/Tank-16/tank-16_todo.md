# Tank-16 TODO List

## Completed
### Gameplay
- [x] Implement the game in SC-16 C in `main.c` for a 640×480 display.
- [x] Support two players in a top-down tank battle; start Player 1 on the left and Player 2 on the right.
- [x] Keep tanks within screen bounds and let players drive, avoid obstacles, and fire projectiles.
- [x] Allow tanks to slide along obstacle edges when a diagonal movement is blocked.
- [x] Map controls: Player 1 uses A/D to turn, W/S to move, and C to fire; Player 2 uses J/L to turn, I/K to move, and period to fire.
- [x] Give each player one life and 100 health; apply 10 damage for an ammunition hit, 25 for a mine, and 5 for a tank collision.
- [x] End a round when a player's health reaches zero.
- [x] Build the screen-sized map from 16×16 runtime terrain tiles.
- [x] Load editor JSON maps and their named terrain tileset when Tank-16 is started with a map file.
- [x] Preserve the tileset's source RGB colors through per-program palette entries.
- [ ] Use 64×64 cells in `gfx/tiles_objects.png` and `gfx/tiles_terrain.png` as the source sprite/tile size.
- [x] Scale 64×64 terrain atlas cells to 16×16 runtime tiles when preparing map assets.
- [x] Provide `sprite_viewer.py` to inspect 64×64 cells, atlas coordinates, and row-major IDs.
- [x] Provide `gfx/tiles_template.png` as a transparent, numbered 64×64-cell atlas template.
- [x] Precalculate 32 tank sprite rotations and projectile directions.
- [x] Show both players' health and the game timer and win count in the HUD.
- [x] Limit each round to five minutes; decide the winner by remaining health and declare a draw on equal health.
- [x] Start a new round automatically after each round and retain wins; reset the win count when the game restarts.

## Remaining tasks
- [ ] Add timed power-ups, such as increased speed, stronger shots, and health restoration.
- [ ] Add sound effects for shooting, tank movement, and explosions.

## Questions for the user
Q: Which object-atlas sprites should be used for mines and obstacles?
A: No sprite choices have been specified yet. Use `sprite_viewer.py` to find the 64×64 cell ID and coordinates, then record them in the game code.

## 64x64 atlas cells

Run `sprite_viewer.py` to view either atlas. Use Tab to switch atlases, +/- to zoom, and click a cell to print its atlas name, ID, and coordinates. IDs are numbered from zero, left-to-right and then top-to-bottom within the selected atlas. The viewer pads incomplete edge cells for display.

The Python helpers `sprite_id_at(image, x, y)`, `sprite_bounds(image, sprite_id)`, and `extract_sprite(image, sprite_id)` map between coordinates, IDs, and 64×64 images. The ID is the source cell index; C code still has to upload the selected pixel data to the GPU's tile or sprite slot.

In `main.c`, `TILE_*` defines name the tile IDs stored in the map, while `TANK_SPRITE_*_BASE` defines the sprite-slot ranges used by the two players' rotated tanks. Use these names instead of raw IDs when referring to the game's tile and sprite slots.

Terrain atlas cells are 64×64 and align to the grid. The object atlas currently has packed artwork and is padded by the viewer to a 7×7 grid; align its art to the grid before using those IDs as final sprite references. The current game API stores runtime terrain tiles as 16×16 and sprite images up to 64×64.
