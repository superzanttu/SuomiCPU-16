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
- [x] Add short muzzle flashes, shot-impact sparks, damage flashes, destruction bursts, and shooting/hit/healing sounds.
- [x] Show reload progress below health bars, warn on low health, and separate the two camera views with a divider.
- [x] Show round results and the next-round countdown inside each player's viewport.
- [x] Use opaque-pixel collision masks for rotated tanks, obstacles, map objects, and ammunition; sweep moving shots to detect thin obstacles.
- [x] Draw larger 7x7 ammunition with a white core and yellow outline, using the same shape for collision detection.
- [x] Emit moving sparks and rising smoke from explosions and impacts, and flames/smoke from destroyed tanks during the round countdown.

## Collision and particle effects

Collision masks use the actual uploaded sprite/tile pixels: palette index zero is
transparent. Map obstacles block tanks and ammunition only where opaque pixels
overlap. Tank rotation is rejected if the new orientation overlaps an obstacle or
the other tank. Ground tiles and cosmetic tracks, bursts, and particles are not
colliders.

Ammunition collides with opposing tanks, obstacles, active map objects, and other
ammunition. Shooting an object consumes the shot; fixed objects remain, explosion
objects are removed with an explosion, and disappear objects are removed without
an object explosion. Object HP changes still apply only on tank contact.

The particle system uses a fixed pool of `MAX_PARTICLES` (48) entries. New emissions
replace the oldest entries when full. Sparks slow and fade through yellow, orange,
and dark red; smoke rises, drifts, and expands. Particles expire after
`PARTICLE_FRAMES` (24) updates or on leaving the map. Wrecks emit fire and smoke every
`FIRE_EMISSION_INTERVAL` (3) updates, including during round-end countdowns.
Starting a round clears all particles. Both camera views clip effects to their
viewport and the map boundaries. Round-result banners sit near the bottom of
each viewport so they do not cover the centered wreck fires.

The graphics API exposes `gfx_mask_overlap(first, first_x, first_y, second,
second_x, second_y)` with top-left world coordinates. Sprite mask IDs are 0-63;
tile masks are `GFX_MASK_TILE_BASE + tile_id`; `GFX_MASK_AMMO` selects the 7x7
projectile diamond. The result is 1 only for overlapping nonzero pixels.
`gfx_world_ammo(x, y)` draws the same diamond centered at world coordinates on the
selected non-background layer through both cameras.

## Remaining tasks
- [ ] Add timed power-ups, such as increased speed, stronger shots, and health restoration.
- [ ] Add tank movement sounds.

## Questions for the user
Q: Which object-atlas sprites should be used for mines and obstacles?
A: No sprite choices have been specified yet. Use `sprite_viewer.py` to find the 64×64 cell ID and coordinates, then record them in the game code.

## 64x64 atlas cells

Run `sprite_viewer.py` to view either atlas. Use Tab to switch atlases, +/- to zoom, and click a cell to print its atlas name, ID, and coordinates. IDs are numbered from zero, left-to-right and then top-to-bottom within the selected atlas. The viewer pads incomplete edge cells for display.

The Python helpers `sprite_id_at(image, x, y)`, `sprite_bounds(image, sprite_id)`, and `extract_sprite(image, sprite_id)` map between coordinates, IDs, and 64×64 images. The ID is the source cell index; C code still has to upload the selected pixel data to the GPU's tile or sprite slot.

In `main.c`, `TILE_*` defines name the tile IDs stored in the map, while `TANK_SPRITE_*_BASE` defines the sprite-slot ranges used by the two players' rotated tanks. Use these names instead of raw IDs when referring to the game's tile and sprite slots.

Terrain atlas cells are 64×64 and align to the grid. The object atlas currently has packed artwork and is padded by the viewer to a 7×7 grid; align its art to the grid before using those IDs as final sprite references. The current game API stores runtime terrain tiles as 16×16 and sprite images up to 64×64.
